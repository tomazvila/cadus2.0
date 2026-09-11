//! The D-O1 pop, the A6 exemplar rotation, and the row decode they share.

use cadus_core::pool::{Avoid, PoolAnswer, PoolProblem, Source, pick};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use super::{Claimed, GenerationContext, POP_LIMIT, PoolRow, Pop};
use crate::{StoreError, begin_tenant};

/// One `serving_pool` row as the two candidate reads return it.
struct RawRow {
    id: Uuid,
    source: String,
    content_digest: Option<String>,
    source_curriculum_digest: Option<String>,
    source_review_engine_digest: Option<String>,
    problem: String,
    expected: String,
    instance_hash: String,
}

impl RawRow {
    /// Read the row out of its two text columns.
    fn decode(self) -> Result<PoolRow, StoreError> {
        let Some(source) = Source::from_wire(&self.source) else {
            return Err(StoreError::PoolRow(format!(
                "serving_pool row {} carries source {:?}, which this build does not know",
                self.id, self.source
            )));
        };
        Ok(PoolRow {
            id: self.id,
            source,
            content_digest: self.content_digest,
            generation_context: self
                .source_curriculum_digest
                .zip(self.source_review_engine_digest)
                .map(
                    |(curriculum_digest, review_engine_digest)| GenerationContext {
                        curriculum_digest,
                        review_engine_digest,
                    },
                ),
            problem: PoolProblem::from_body(&self.problem)?,
            expected_answer: PoolAnswer::from_body(&self.expected)?,
            instance_hash: self.instance_hash,
        })
    }
}

/// One candidate read: the six columns of [`RawRow`] from `serving_pool AS
/// sp`, followed by the rest of the statement, with the bind arguments.
///
/// The two reads of this module differ in their `WHERE` and `ORDER BY` alone,
/// so the column list is written once. The macro expands to one
/// `sqlx::query_as!`, so the statement stays compile-time checked (R2).
macro_rules! candidate_read {
    ($tail:literal, $($arg:expr),+ $(,)?) => {
        sqlx::query_as!(
            RawRow,
            r#"
        SELECT sp.id AS "id!", sp.source AS "source!", sp.content_digest,
               sp.source_curriculum_digest, sp.source_review_engine_digest,
               sp.problem::text AS "problem!", sp.expected_answer::text AS "expected!",
               sp.instance_hash AS "instance_hash!""# + $tail,
            $($arg),+
        )
    };
}

mod finite;
pub use finite::{FiniteDraw, FiniteEligibility, pop_finite_tx};

/// Decode the rows of one candidate read.
///
/// The answer is the decoded candidates, in the order of the read, and the ids
/// of the rows that did not decode. Every refused row is logged with `note` and
/// the reason; the caller decides what a refused row means.
fn decode_rows(
    rows: Vec<RawRow>,
    user_id: Uuid,
    kp_id: &str,
    note: &'static str,
) -> (Vec<PoolRow>, Vec<Uuid>) {
    let mut candidates: Vec<PoolRow> = Vec::with_capacity(rows.len());
    let mut refused: Vec<Uuid> = Vec::new();
    for row in rows {
        let id = row.id;
        match row.decode() {
            Ok(candidate) => candidates.push(candidate),
            Err(err) => {
                tracing::warn!(
                    row_id = %id,
                    user_id = %user_id,
                    kp_id = %kp_id,
                    reason = %err,
                    "{note}"
                );
                refused.push(id);
            }
        }
    }
    (candidates, refused)
}

/// Stamp `claimed_at` on one unclaimed row and return the count of rows the
/// statement wrote.
async fn claim_unclaimed(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<u64, StoreError> {
    let claimed = sqlx::query!(
        "UPDATE serving_pool SET claimed_at = now() WHERE id = $1 AND claimed_at IS NULL",
        id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(claimed.rows_affected())
}

/// Pop up to [`POP_LIMIT`] unclaimed rows, skip every ring hit, and claim one.
///
/// The whole call runs inside the caller's transaction, so the claim and the
/// state write the caller makes next commit together (D-O1). The rows come back
/// in `created_at` order, so the pool serves oldest first and the pop is
/// deterministic on a tie.
///
/// `avoid` is the D5 view of the ring and the task memory. The rule of
/// [`cadus_core::pool::pick`] decides: the first unblocked row wins, and when
/// every popped row is blocked the LAST row wins and [`Pick::exhausted`] says so.
/// The pop never redraws; the redraw is the worker's job (D-O4).
///
/// # The approval is read on every serve (C6)
///
/// The statement joins `content_store` on `serving_pool.content_digest`, and a
/// row that names a digest is a candidate only while that digest carries
/// `status = 'approved'`. A row with NO digest is an exemplar rotation, whose
/// authority is the curriculum file (A6), so the pop serves it.
///
/// The old statement read `serving_pool` alone. An operator who read a wrong
/// answer in the refill log and set `content_store.status = 'rejected'` stopped
/// the NEXT refill and nothing else: the up to `target_depth` unclaimed rows the
/// digest had already written kept being served, one per serve, each with the
/// wrong `expected_answer`, and the M5 grade path wrote an append-only wrong
/// attempt for every one of them (M4 review 2, finding #4).
///
/// The join costs one index probe on the `content_store` primary key per
/// candidate row, and the pop reads at most [`POP_LIMIT`] rows.
/// `FOR UPDATE OF sp` keeps the row lock on `serving_pool`: the pop must not
/// lock the one `content_store` row that every learner of that template shares.
///
/// The pop does not retire the rows it walks past. [`retire_unapproved`] does
/// that, in the D-O4 refill job, so the serve path stays one transaction with no
/// extra write.
///
/// [`Pop::claimed`] is `None` when the pool held no servable row for this pair.
/// The caller then instantiates an exemplar in process and raises the A6 flag
/// (specification section 7.2); it must not generate.
///
/// # A row that does not decode is retired, not fatal
///
/// A row whose `problem`, `expected_answer`, or `source` this build refuses is
/// skipped, claimed with the reason in the log, and counted in
/// [`Pop::undecodable`]; the pop continues with the other candidates. The old
/// code returned the decode error, so ONE stale row denied every serve of that
/// pair forever, and the row itself was never retired. The module rule holds the
/// other way round: a repeat is a far smaller failure than no problem.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when a statement fails, and [`StoreError::PoolRow`]
/// when the claim writes no row.
pub async fn pop_with_ring_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Pop, StoreError> {
    pop_with_ring_context_tx(tx, user_id, kp_id, avoid, None).await
}

/// Pop only rows generated under the supplied current trusted context.
pub async fn pop_with_ring_current_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
    context: &GenerationContext,
) -> Result<Pop, StoreError> {
    pop_with_ring_context_tx(tx, user_id, kp_id, avoid, Some(context)).await
}

async fn pop_with_ring_context_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
    context: Option<&GenerationContext>,
) -> Result<Pop, StoreError> {
    let popped = candidate_read!(
        r#"
        FROM serving_pool AS sp
        LEFT JOIN content_store AS cs ON cs.digest = sp.content_digest
        WHERE sp.user_id = $1
          AND sp.kp_id = $2
          AND sp.claimed_at IS NULL
          AND (sp.content_digest IS NULL OR
               (cs.status = 'approved' AND cs.approved_policy_digest IS NULL
                AND ($4::text IS NULL OR
                     (cs.approved_curriculum_digest = $4
                      AND cs.approved_review_engine_digest = $5
                      AND sp.source_curriculum_digest = $4
                      AND sp.source_review_engine_digest = $5))))
        ORDER BY sp.created_at, sp.id
        FOR UPDATE OF sp SKIP LOCKED
        LIMIT $3
        "#,
        user_id,
        kp_id,
        POP_LIMIT,
        context.map(|value| value.curriculum_digest.as_str()),
        context.map(|value| value.review_engine_digest.as_str()),
    )
    .fetch_all(&mut **tx)
    .await?;

    let (mut candidates, refused) = decode_rows(
        popped,
        user_id,
        kp_id,
        "pool: a row this build cannot decode is claimed and skipped; the serve continues with \
         the rows behind it",
    );
    for id in &refused {
        retire_row(tx, *id).await?;
    }
    let undecodable = refused.len();

    let Some(chosen) = pick(&candidates, avoid) else {
        return Ok(Pop {
            claimed: None,
            undecodable,
        });
    };
    // `pick` names a position inside `candidates`, so the index is in bounds.
    let count = candidates.len();
    let row = candidates.swap_remove(chosen.index);

    if claim_unclaimed(tx, row.id).await? != 1 {
        return Err(StoreError::PoolRow(format!(
            "serving_pool row {} was claimed by another transaction; FOR UPDATE SKIP LOCKED must \
             make that impossible",
            row.id
        )));
    }

    Ok(Pop {
        claimed: Some(Claimed {
            row,
            pick: chosen,
            candidates: count,
        }),
        undecodable,
    })
}

/// Claim one row the pop cannot decode, so it leaves the unclaimed set.
///
/// The row stays in the table: a claimed row is the served-instance log of A5,
/// and a retention job of M5 owns the delete.
async fn retire_row(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<(), StoreError> {
    claim_unclaimed(tx, id).await?;
    Ok(())
}

/// Serve one exemplar row of this pair again, oldest serve first (A6).
///
/// The A6 fallback of the M5 serve path instantiates the knowledge point's
/// exemplars in process, writes them with [`insert_batch`], and pops. A pair
/// whose whole exemplar list is already claimed pops nothing, and the pool cannot
/// grow: the unique index `(user_id, kp_id, instance_hash)` refuses a second copy
/// of a statement the authored list already holds. This call is the rotation for
/// that pair.
///
/// It reads the CLAIMED exemplar rows of the pair, oldest serve first, lets the
/// D5 rule of [`pick`] choose among them, and re-stamps `claimed_at`. Two
/// consequences, both wanted:
///
/// - the learner keeps getting the least recently served exemplar, which is
///   1.0's `pool[index % len]` rotation with an anti-repeat rule on top
///   (specification section 6.1);
/// - `last_exemplar_at` of [`operator_flags`] tracks the newest fallback serve,
///   so the A6 dashboard shows a knowledge point that is STILL on exemplars and
///   not only the day it first fell back.
///
/// A row that this build cannot decode is skipped and left alone: it is claimed
/// already, so it is outside the unclaimed set and outside the refill depth, and
/// [`pop_with_ring_tx`] owns the retirement of an unclaimed one.
///
/// `Ok(None)` means the pair holds no exemplar row at all. The caller then has
/// nothing to serve for this knowledge point.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when a statement fails.
pub async fn reclaim_exemplar_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Option<PoolRow>, StoreError> {
    let rows = candidate_read!(
        r#"
        FROM serving_pool AS sp
        WHERE sp.user_id = $1
          AND sp.kp_id = $2
          AND sp.source = 'exemplar'
          AND sp.claimed_at IS NOT NULL
        ORDER BY sp.claimed_at, sp.id
        FOR UPDATE OF sp SKIP LOCKED
        LIMIT $3
        "#,
        user_id,
        kp_id,
        POP_LIMIT,
    )
    .fetch_all(&mut **tx)
    .await?;

    let (mut candidates, _) = decode_rows(
        rows,
        user_id,
        kp_id,
        "pool: an exemplar row this build cannot decode is skipped by the A6 rotation",
    );

    let Some(chosen) = pick(&candidates, avoid) else {
        return Ok(None);
    };
    // The rows come back oldest serve first, so the first UNBLOCKED row is the
    // one the learner saw longest ago. When every row is blocked — the steady
    // state of a knowledge point whose exemplar count is under the ring size —
    // [`pick`] takes the LAST candidate, which here is the row the learner saw
    // most recently. Take the first one instead: the repeat must be the oldest
    // one, or a two-exemplar knowledge point serves one statement forever.
    // `pick` names a position inside `candidates`, so both indexes are in
    // bounds.
    let index = if chosen.exhausted { 0 } else { chosen.index };
    let row = candidates.swap_remove(index);

    sqlx::query!(
        "UPDATE serving_pool SET claimed_at = now() WHERE id = $1",
        row.id,
    )
    .execute(&mut **tx)
    .await?;

    Ok(Some(row))
}

/// [`pop_with_ring_tx`] inside a tenant transaction of its own.
///
/// The M5 serve path uses [`pop_with_ring_tx`], because it writes the D-S6 state
/// row in the same transaction. This wrapper serves a caller that pops alone.
///
/// # Errors
///
/// Returns the error of [`pop_with_ring_tx`], and [`StoreError::Db`] when the
/// transaction does not start or does not commit.
pub async fn pop_with_ring(
    pool: &PgPool,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Pop, StoreError> {
    let mut tx = begin_tenant(pool, user_id).await?;
    let popped = pop_with_ring_tx(&mut tx, user_id, kp_id, avoid).await?;
    tx.commit().await?;
    Ok(popped)
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::RawRow;

    /// A source value outside the three of this build is refused with the row
    /// id in the message, and a document that does not read is refused too.
    #[test]
    fn a_row_with_an_unknown_source_or_a_broken_document_does_not_decode() {
        let raw = |source: &str, problem: &str| RawRow {
            id: Uuid::nil(),
            source: source.to_string(),
            content_digest: None,
            source_curriculum_digest: None,
            source_review_engine_digest: None,
            problem: problem.to_string(),
            expected: r#"{"v":1,"answer":"1"}"#.to_string(),
            instance_hash: "hash-1".to_string(),
        };
        let err = raw("oracle", r#"{"v":1,"text":"t","seed":0}"#)
            .decode()
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "serving pool error: serving_pool row 00000000-0000-0000-0000-000000000000 carries \
             source \"oracle\", which this build does not know"
        );
        assert!(raw("template", "not json").decode().is_err());
        let mut bad_answer = raw("template", r#"{"v":1,"text":"t","seed":0}"#);
        bad_answer.expected = "not json".to_string();
        assert!(bad_answer.decode().is_err());
        assert!(
            raw("template", r#"{"v":1,"text":"t","seed":0}"#)
                .decode()
                .is_ok()
        );
    }
}
