//! The serving-pool operations: batch insert, the D-O1 pop, the approved-template
//! read, and the A6 operator flags.
//!
//! Requirements: D-O1 (one transaction on the serve path), D-O4 (the redraw lives
//! in the worker), D7 (`FOR UPDATE SKIP LOCKED`), A6 (the fallback is visible to
//! an operator).
//!
//! # The division of labor
//!
//! | Piece | Where it lives |
//! |---|---|
//! | the draw, the render, the answer | `cadus_core::template` |
//! | the anti-repeat rule and the two windows | `cadus_core::pool::ring` |
//! | the row documents and the serving key | `cadus_core::pool::row` |
//! | the SQL of this table | this file |
//! | the refill job that calls both | `cadus_worker::refill` |
//!
//! # The serve is one transaction
//!
//! Specification section 7.2 writes it out: 1.0 needed two transactions because a
//! 7-12 s model call sat between them. 2.0 makes no model call (T1), so the whole
//! serve is one transaction and it must be one. [`pop_with_ring_tx`] is the pool
//! half of it:
//!
//! ```text
//! BEGIN;                                  -- begin_tenant sets app.user_id (C3)
//!   SELECT ... LEFT JOIN content_store    -- D7, the pop; C6 reads the status
//!     FOR UPDATE OF serving_pool SKIP LOCKED LIMIT 8;
//!   -- cadus_core::pool::pick skips every ring hit
//!   UPDATE serving_pool SET claimed_at = now() WHERE id = $1;
//!   -- the caller writes the D-S6 state row here, in this same transaction
//! COMMIT;
//! ```
//!
//! `SKIP LOCKED` is the whole concurrency argument: two serves of one
//! `(user, kp)` walk past each other's locked rows, so neither one waits and
//! neither one reads the row the other claims. `FOR UPDATE OF serving_pool`
//! keeps the lock on the pool row alone, so a serve never locks the shared
//! `content_store` row of a template and two learners of one template never
//! wait for each other.
//!
//! # The approval decides the serve, on every serve (C6)
//!
//! A pool row that names a `content_store` digest is servable only while that
//! digest is `approved`. The pop joins `content_store` and reads
//! `status`, so a revoked approval stops the serve of the rows the digest
//! already wrote. A row with NO digest is an exemplar rotation (A6): the
//! curriculum file is its authority, `content_store` holds no row for it, and
//! the pop serves it.
//!
//! The pop leaves the unapproved rows in place. [`retire_unapproved`] is the
//! second half: the D-O4 refill job claims them, writes the reason in the log,
//! and the pair then falls under its target depth and refills from the source
//! that IS approved.
//!
//! # A claimed row stays
//!
//! `claimed_at` marks the row served (`migrations/0005_content.sql`). The M4
//! decision keeps the row: it is the served-instance log A5 names, and the D5
//! ring is then a cache of it. A retention job belongs to M5.
//!
//! # No panic
//!
//! Every function returns [`StoreError`]. A row with a `source` value this build
//! does not know, and a document that does not read, both give an error and never
//! an unwrap.

use std::collections::BTreeSet;

use cadus_core::pool::{Avoid, Candidate, Pick, PoolAnswer, PoolProblem, Source, pick};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{StoreError, begin_tenant};

/// The count of rows one pop takes.
///
/// The value is [`cadus_core::pool::POP_CANDIDATES`]. The store repeats it as an
/// `i64`, because the `LIMIT` bind of the pop is an `i64`.
pub const POP_LIMIT: i64 = cadus_core::pool::POP_CANDIDATES as i64;

/// One instance on its way into the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewInstance {
    /// The source that produced it (A7).
    pub source: Source,
    /// The `content_store` digest, when the source names one.
    pub content_digest: Option<String>,
    /// The `problem` document.
    pub problem: PoolProblem,
    /// The `expected_answer` document.
    pub expected_answer: PoolAnswer,
    /// `problem_text_hash` of the rendered statement (A5).
    pub instance_hash: String,
}

impl NewInstance {
    /// Build a row from one instance, its source, and the seed of its batch.
    #[must_use]
    pub fn from_instance(
        instance: &cadus_core::template::Instance,
        source: Source,
        content_digest: Option<String>,
        seed: u64,
    ) -> Self {
        Self {
            source,
            content_digest,
            problem: PoolProblem::from_instance(instance, seed),
            expected_answer: PoolAnswer::from_instance(instance),
            instance_hash: instance.instance_hash.clone(),
        }
    }
}

/// One row the pop read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolRow {
    /// The primary key.
    pub id: Uuid,
    /// The source that produced it (A7).
    pub source: Source,
    /// The `content_store` digest, when the row has one.
    pub content_digest: Option<String>,
    /// The `problem` document.
    pub problem: PoolProblem,
    /// The `expected_answer` document.
    pub expected_answer: PoolAnswer,
    /// The anti-repeat digest (A5).
    pub instance_hash: String,
}

impl Candidate for PoolRow {
    fn instance_hash(&self) -> &str {
        &self.instance_hash
    }
}

/// The outcome of one pop: the claimed row and what the rule walked past.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claimed {
    /// The row the transaction claimed.
    pub row: PoolRow,
    /// The report of the candidate rule.
    ///
    /// [`Pick::exhausted`] means every popped row was inside the ring and the
    /// serve took a repeat. The caller counts that as `pool_exhausted`.
    pub pick: Pick,
    /// The count of rows the pop read.
    pub candidates: usize,
}

/// The outcome of one pop, including the rows it retired.
///
/// A pool row that this build cannot decode is skipped, claimed, and counted; the
/// pop then continues with the rows behind it. The claim is what retires the row:
/// it leaves the unclaimed set, so it never blocks a later serve of the same pair
/// and it never counts toward the refill depth again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pop {
    /// The row the transaction claimed and the report of the candidate rule.
    ///
    /// `None` means the pool held no servable row for this pair. The caller then
    /// instantiates an exemplar in process and raises the A6 flag; it must not
    /// generate.
    pub claimed: Option<Claimed>,
    /// The count of rows this pop retired because they did not decode.
    ///
    /// The caller counts it as
    /// [`PoolCounters::pool_row_undecodable`](cadus_core::pool::PoolCounters::pool_row_undecodable).
    pub undecodable: usize,
}

/// One approved template document of a knowledge point (C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovedTemplate {
    /// The content address the approval binds to (C6).
    pub digest: String,
    /// The document text, as `content_store.body` holds it.
    pub body: String,
}

/// The A6 operator row of one knowledge point.
///
/// Specification section 6.2: 1.0 has one Prometheus counter and no per-knowledge-point
/// view at all. A6 asks for the view, so this is the query behind it. M5 exposes
/// it on a read-only endpoint.
///
/// The counts read the rows the caller may read. The worker connects as
/// `cadus_admin`, which holds BYPASSRLS, so it sees every tenant. A request tier
/// inside a tenant transaction sees that tenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KpFlag {
    /// The serving key, `"<topic_id>/<kp_id>"`.
    pub kp_id: String,
    /// The count of approved template documents (C6).
    pub approved_templates: i64,
    /// The count of unclaimed pool rows.
    pub pool_depth: i64,
    /// The source of the newest claimed row, if the knowledge point served one.
    pub last_source: Option<Source>,
    /// When the knowledge point last fell back to an exemplar (A6).
    pub last_exemplar_at: Option<DateTime<Utc>>,
    /// Whether the knowledge point has no approved template.
    ///
    /// `true` is the A6 flag the dashboard shows: every serve of this knowledge
    /// point is an exemplar rotation.
    pub needs_template: bool,
    /// Whether a `(user, kp)` pair of this knowledge point ran its source dry.
    ///
    /// `true` says the source produced no NEW statement twice in a row, so the
    /// pool of that pair cannot grow: an exemplar list under the target depth,
    /// or a template whose whole space is already in the pool. The pair is on a
    /// one-hour backoff and the knowledge point needs more authored content
    /// (M4 review 2, finding #8).
    ///
    /// The state is the worker's, not the database's. `operator_flags` therefore
    /// reads `false` for every row; a caller that holds the refill state passes
    /// it to [`operator_flags_with_exhausted`].
    pub source_exhausted: bool,
}

/// One pool row the refill retired because its digest lost its approval (C6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetiredRow {
    /// The primary key of the retired row.
    pub id: Uuid,
    /// The learner the row belonged to.
    pub user_id: Uuid,
    /// The serving key of the row.
    pub kp_id: String,
    /// The `content_store` digest the row names.
    pub content_digest: String,
    /// The status that digest carries now: `pending` or `rejected`.
    pub status: String,
}

/// One `(user, kp)` pair whose unclaimed depth is under the target (D-O4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolTarget {
    /// The learner.
    pub user_id: Uuid,
    /// The serving key.
    pub kp_id: String,
    /// The count of unclaimed rows the pair holds now.
    pub depth: i64,
}

/// Insert a batch of instances and skip every digest the pool already holds.
///
/// The unique index `(user_id, kp_id, instance_hash)` is the pool invariant of
/// A5, so `ON CONFLICT DO NOTHING` on that index is the whole idempotency rule: a
/// refill that draws a statement the pool already carries inserts nothing and
/// reports the smaller count.
///
/// The call returns the count of rows the statement inserted.
///
/// # The order inside one batch
///
/// One call is one statement, so every row of a batch takes the same
/// `created_at`: `now()` is the transaction instant, not the row instant. The
/// pop orders by `created_at` and then by `id`, so the pool serves batch by
/// batch in age order and serves the rows INSIDE one batch in the order of their
/// random ids. That is not the order the source produced them in, and no caller
/// may depend on it: the anti-repeat rule of D5 decides which row a learner sees,
/// not the position of a row in its batch.
///
/// # Errors
///
/// Returns [`StoreError::Body`] when a document does not serialize, and
/// [`StoreError::Db`] when the statement fails.
pub async fn insert_batch<'e, E>(
    executor: E,
    user_id: Uuid,
    kp_id: &str,
    rows: &[NewInstance],
) -> Result<u64, StoreError>
where
    E: PgExecutor<'e>,
{
    if rows.is_empty() {
        return Ok(0);
    }

    let mut sources: Vec<String> = Vec::with_capacity(rows.len());
    let mut digests: Vec<Option<String>> = Vec::with_capacity(rows.len());
    let mut problems: Vec<String> = Vec::with_capacity(rows.len());
    let mut answers: Vec<String> = Vec::with_capacity(rows.len());
    let mut hashes: Vec<String> = Vec::with_capacity(rows.len());
    for row in rows {
        sources.push(row.source.as_str().to_string());
        digests.push(row.content_digest.clone());
        problems.push(row.problem.to_body()?);
        answers.push(row.expected_answer.to_body()?);
        hashes.push(row.instance_hash.clone());
    }

    let inserted = sqlx::query_scalar!(
        r#"
        INSERT INTO serving_pool
            (user_id, kp_id, source, content_digest, problem, expected_answer, instance_hash)
        SELECT $1, $2, batch.source, batch.digest, batch.problem::jsonb,
               batch.expected::jsonb, batch.instance_hash
        FROM unnest($3::text[], $4::text[], $5::text[], $6::text[], $7::text[])
             AS batch(source, digest, problem, expected, instance_hash)
        ON CONFLICT (user_id, kp_id, instance_hash) DO NOTHING
        RETURNING id AS "id!"
        "#,
        user_id,
        kp_id,
        &sources,
        digests.as_slice() as &[Option<String>],
        &problems,
        &answers,
        &hashes,
    )
    .fetch_all(executor)
    .await?;

    Ok(inserted.len() as u64)
}

/// Insert a batch inside a tenant transaction of its own.
///
/// The tenant binding makes the insert legal for the runtime role too: the
/// `serving_pool` policy of `0006_grants_rls.sql` carries a `WITH CHECK`, and an
/// unbound connection writes no row. The worker connects as `cadus_admin` and
/// bypasses the policy, so the binding changes nothing for it.
///
/// # Errors
///
/// Returns the error of [`insert_batch`], and [`StoreError::Db`] when the
/// transaction does not start or does not commit.
pub async fn insert_batch_for_user(
    pool: &PgPool,
    user_id: Uuid,
    kp_id: &str,
    rows: &[NewInstance],
) -> Result<u64, StoreError> {
    let mut tx = begin_tenant(pool, user_id).await?;
    let inserted = insert_batch(&mut *tx, user_id, kp_id, rows).await?;
    tx.commit().await?;
    Ok(inserted)
}

/// Read one popped row out of its two text columns.
fn read_row(
    id: Uuid,
    source: &str,
    content_digest: Option<String>,
    problem: &str,
    expected: &str,
    instance_hash: String,
) -> Result<PoolRow, StoreError> {
    let Some(source) = Source::from_wire(source) else {
        return Err(StoreError::PoolRow(format!(
            "serving_pool row {id} carries source {source:?}, which this build does not know"
        )));
    };
    Ok(PoolRow {
        id,
        source,
        content_digest,
        problem: PoolProblem::from_body(problem)?,
        expected_answer: PoolAnswer::from_body(expected)?,
        instance_hash,
    })
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
/// when the candidate rule names a row the pop did not read.
pub async fn pop_with_ring_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Pop, StoreError> {
    let popped = sqlx::query!(
        r#"
        SELECT sp.id AS "id!",
               sp.source AS "source!",
               sp.content_digest,
               sp.problem::text AS "problem!",
               sp.expected_answer::text AS "expected!",
               sp.instance_hash AS "instance_hash!"
        FROM serving_pool AS sp
        LEFT JOIN content_store AS cs ON cs.digest = sp.content_digest
        WHERE sp.user_id = $1
          AND sp.kp_id = $2
          AND sp.claimed_at IS NULL
          AND (sp.content_digest IS NULL OR cs.status = 'approved')
        ORDER BY sp.created_at, sp.id
        FOR UPDATE OF sp SKIP LOCKED
        LIMIT $3
        "#,
        user_id,
        kp_id,
        POP_LIMIT,
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut candidates: Vec<PoolRow> = Vec::with_capacity(popped.len());
    let mut undecodable: usize = 0;
    for row in popped {
        let id = row.id;
        match read_row(
            id,
            &row.source,
            row.content_digest,
            &row.problem,
            &row.expected,
            row.instance_hash,
        ) {
            Ok(candidate) => candidates.push(candidate),
            Err(err) => {
                undecodable = undecodable.saturating_add(1);
                tracing::warn!(
                    row_id = %id,
                    user_id = %user_id,
                    kp_id = %kp_id,
                    reason = %err,
                    "pool: a row this build cannot decode is claimed and skipped; the serve \
                     continues with the rows behind it"
                );
                retire_row(tx, id).await?;
            }
        }
    }

    let Some(chosen) = pick(&candidates, avoid) else {
        return Ok(Pop {
            claimed: None,
            undecodable,
        });
    };
    let Some(row) = candidates.get(chosen.index).cloned() else {
        return Err(StoreError::PoolRow(format!(
            "the candidate rule chose index {} of {} popped rows",
            chosen.index,
            candidates.len()
        )));
    };

    let claimed = sqlx::query!(
        r#"
        UPDATE serving_pool
        SET claimed_at = now()
        WHERE id = $1 AND claimed_at IS NULL
        "#,
        row.id,
    )
    .execute(&mut **tx)
    .await?;

    if claimed.rows_affected() != 1 {
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
            candidates: candidates.len(),
        }),
        undecodable,
    })
}

/// Claim one row the pop cannot decode, so it leaves the unclaimed set.
///
/// The row stays in the table: a claimed row is the served-instance log of A5,
/// and a retention job of M5 owns the delete.
async fn retire_row(tx: &mut Transaction<'_, Postgres>, id: Uuid) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        UPDATE serving_pool
        SET claimed_at = now()
        WHERE id = $1 AND claimed_at IS NULL
        "#,
        id,
    )
    .execute(&mut **tx)
    .await?;
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
/// Returns [`StoreError::Db`] when a statement fails, and [`StoreError::PoolRow`]
/// when the candidate rule names a row the read did not return.
pub async fn reclaim_exemplar_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Option<PoolRow>, StoreError> {
    let rows = sqlx::query!(
        r#"
        SELECT sp.id AS "id!",
               sp.source AS "source!",
               sp.content_digest,
               sp.problem::text AS "problem!",
               sp.expected_answer::text AS "expected!",
               sp.instance_hash AS "instance_hash!"
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

    let mut candidates: Vec<PoolRow> = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.id;
        match read_row(
            id,
            &row.source,
            row.content_digest,
            &row.problem,
            &row.expected,
            row.instance_hash,
        ) {
            Ok(candidate) => candidates.push(candidate),
            Err(err) => tracing::warn!(
                row_id = %id,
                user_id = %user_id,
                kp_id = %kp_id,
                reason = %err,
                "pool: an exemplar row this build cannot decode is skipped by the A6 rotation"
            ),
        }
    }

    let Some(chosen) = pick(&candidates, avoid) else {
        return Ok(None);
    };
    // The rows come back oldest serve first, so the first UNBLOCKED row is the
    // one the learner saw longest ago. When every row is blocked — the steady
    // state of a knowledge point whose exemplar count is under the ring size —
    // [`pick`] takes the LAST candidate, which here is the row the learner saw
    // most recently. Take the first one instead: the repeat must be the oldest
    // one, or a two-exemplar knowledge point serves one statement forever.
    let index = if chosen.exhausted { 0 } else { chosen.index };
    let Some(row) = candidates.get(index).cloned() else {
        return Err(StoreError::PoolRow(format!(
            "the candidate rule chose index {index} of {} exemplar rows",
            candidates.len()
        )));
    };

    sqlx::query!(
        r#"
        UPDATE serving_pool
        SET claimed_at = now()
        WHERE id = $1
        "#,
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

/// Retire every unclaimed row whose digest is no longer approved (C6, D-O4).
///
/// The statement claims the row, exactly as a serve does: `claimed_at` leaves
/// the unclaimed set, so the row never reaches a pop again and never counts
/// toward the refill depth again. The row itself stays, because a claimed row is
/// the served-instance log of A5 and the retention job of M5 owns the delete.
///
/// A row with no digest is an exemplar rotation and is never retired here (A6).
///
/// The call returns one [`RetiredRow`] per claimed row, so the caller writes the
/// pair, the digest, and the status into the log. The D-O4 refill job is that
/// caller: it runs this before it reads the target list, so a pair whose rows
/// this statement took falls under its target depth in the SAME pass and refills
/// from the source that IS approved.
///
/// # The role decides how much this reaches
///
/// The statement carries no tenant bind, so it retires across every learner. The
/// worker connects as `cadus_admin`, which holds BYPASSRLS, so it sees them all.
/// A caller inside a tenant transaction retires that tenant's rows alone, and a
/// caller with no tenant bound retires nothing; neither one is an error.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn retire_unapproved<'e, E>(executor: E) -> Result<Vec<RetiredRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let rows = sqlx::query!(
        r#"
        UPDATE serving_pool AS sp
        SET claimed_at = now()
        FROM content_store AS cs
        WHERE cs.digest = sp.content_digest
          AND sp.claimed_at IS NULL
          AND cs.status <> 'approved'
        RETURNING sp.id AS "id!",
                  sp.user_id AS "user_id!",
                  sp.kp_id AS "kp_id!",
                  sp.content_digest AS "content_digest!",
                  cs.status AS "status!"
        "#
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| RetiredRow {
            id: row.id,
            user_id: row.user_id,
            kp_id: row.kp_id,
            content_digest: row.content_digest,
            status: row.status,
        })
        .collect())
}

/// The count of unclaimed rows of one `(user, kp)` pair.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn unclaimed_depth<'e, E>(
    executor: E,
    user_id: Uuid,
    kp_id: &str,
) -> Result<i64, StoreError>
where
    E: PgExecutor<'e>,
{
    let depth = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "depth!"
        FROM serving_pool
        WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
        "#,
        user_id,
        kp_id,
    )
    .fetch_one(executor)
    .await?;
    Ok(depth)
}

/// Every `(user, kp)` pair whose unclaimed depth is under `target_depth` (D-O4).
///
/// The pairs come from `serving_pool` itself, so a pair reaches this list after
/// its first row exists. A pair the pool never held is not a refill target: 2.0
/// has no table that enrolls a learner in a knowledge point, and the serve path
/// creates the first rows when it enqueues a refill on a miss (specification
/// section 7.2).
///
/// The shallowest pair comes first, so a small `limit` refills the pairs closest
/// to an empty pool.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn refill_targets<'e, E>(
    executor: E,
    target_depth: i64,
    limit: i64,
) -> Result<Vec<PoolTarget>, StoreError>
where
    E: PgExecutor<'e>,
{
    refill_targets_skipping(executor, target_depth, limit, &[]).await
}

/// [`refill_targets`] without the pairs of `skip` (D-O4).
///
/// A `(user, kp)` pair with no approved template and no decidable exemplar can
/// never gain a row. It stays at depth 0, and depth 0 sorts first, so such a pair
/// took the head of the list on every tick and the whole per-tick budget went to
/// pairs that could not use it. The worker holds those pairs in a backoff map and
/// passes them here, so the `LIMIT` counts pairs that CAN fill.
///
/// The exclusion runs before the grouping, so a skipped pair costs no slot at
/// all.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn refill_targets_skipping<'e, E>(
    executor: E,
    target_depth: i64,
    limit: i64,
    skip: &[(Uuid, String)],
) -> Result<Vec<PoolTarget>, StoreError>
where
    E: PgExecutor<'e>,
{
    let mut skip_users: Vec<Uuid> = Vec::with_capacity(skip.len());
    let mut skip_kps: Vec<String> = Vec::with_capacity(skip.len());
    for (user_id, kp_id) in skip {
        skip_users.push(*user_id);
        skip_kps.push(kp_id.clone());
    }

    let rows = sqlx::query!(
        r#"
        SELECT user_id AS "user_id!",
               kp_id AS "kp_id!",
               count(*) FILTER (WHERE claimed_at IS NULL) AS "depth!"
        FROM serving_pool
        WHERE NOT EXISTS (
                  SELECT 1
                  FROM unnest($3::uuid[], $4::text[]) AS skip(user_id, kp_id)
                  WHERE skip.user_id = serving_pool.user_id
                    AND skip.kp_id = serving_pool.kp_id
              )
        GROUP BY user_id, kp_id
        HAVING count(*) FILTER (WHERE claimed_at IS NULL) < $1
        ORDER BY "depth!", user_id, kp_id
        LIMIT $2
        "#,
        target_depth,
        limit,
        &skip_users,
        &skip_kps,
    )
    .fetch_all(executor)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PoolTarget {
            user_id: row.user_id,
            kp_id: row.kp_id,
            depth: row.depth,
        })
        .collect())
}

/// The newest approved template document of one knowledge point (C6).
///
/// `status = 'approved'` is the C6 gate: a `pending` row is never served, and the
/// approval binds to the digest, so an edited body is a different row that needs
/// its own approval.
///
/// The newest approval wins, and the digest breaks a tie, so two rows approved in
/// the same statement give one stable answer.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn approved_template<'e, E>(
    executor: E,
    kp_id: &str,
) -> Result<Option<ApprovedTemplate>, StoreError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query!(
        r#"
        SELECT digest AS "digest!", body::text AS "body!"
        FROM content_store
        WHERE kp_id = $1 AND kind = 'template' AND status = 'approved'
        ORDER BY approved_at DESC NULLS LAST, created_at DESC, digest
        LIMIT 1
        "#,
        kp_id,
    )
    .fetch_optional(executor)
    .await?;

    Ok(row.map(|row| ApprovedTemplate {
        digest: row.digest,
        body: row.body,
    }))
}

/// The A6 operator view: one row per knowledge point (A6).
///
/// The row set is every knowledge point that holds a pool row or a template
/// document, so a knowledge point with a `pending` template appears with
/// `approved_templates = 0` and `needs_template = true`.
///
/// # Errors
///
/// Returns [`StoreError::PoolRow`] when a claimed row carries a `source` value
/// this build does not know, and [`StoreError::Db`] when the statement fails.
pub async fn operator_flags<'e, E>(executor: E) -> Result<Vec<KpFlag>, StoreError>
where
    E: PgExecutor<'e>,
{
    operator_flags_with_exhausted(executor, &[]).await
}

/// [`operator_flags`] with the exhausted knowledge points of the refill (A6).
///
/// `exhausted` holds the serving keys whose source ran dry: the refill filled the
/// pair twice in a row and inserted no new statement either time, so the pair is
/// on a one-hour backoff and the pool cannot grow (M4 review 2, finding #8). A
/// key in the slice sets [`KpFlag::source_exhausted`] on that row.
///
/// The backoff map lives in `cadus_worker::refill::RefillState`, in the worker
/// process. A caller in that process passes `RefillState::exhausted_kps()` here.
/// A caller in another process passes an empty slice and reads `false`, because
/// 2.0 has no table that carries the refill state across processes.
///
/// # Errors
///
/// Returns the errors of [`operator_flags`].
pub async fn operator_flags_with_exhausted<'e, E>(
    executor: E,
    exhausted: &[String],
) -> Result<Vec<KpFlag>, StoreError>
where
    E: PgExecutor<'e>,
{
    let exhausted: BTreeSet<&str> = exhausted.iter().map(String::as_str).collect();
    let rows = sqlx::query!(
        r#"
        WITH kps AS (
            SELECT kp_id FROM serving_pool
            UNION
            SELECT kp_id FROM content_store WHERE kind = 'template'
        ),
        approved AS (
            SELECT kp_id, count(*) AS approved
            FROM content_store
            WHERE kind = 'template' AND status = 'approved'
            GROUP BY kp_id
        ),
        depth AS (
            SELECT kp_id, count(*) AS depth
            FROM serving_pool
            WHERE claimed_at IS NULL
            GROUP BY kp_id
        ),
        last_served AS (
            SELECT DISTINCT ON (kp_id) kp_id, source, claimed_at
            FROM serving_pool
            WHERE claimed_at IS NOT NULL
            ORDER BY kp_id, claimed_at DESC, id
        ),
        last_exemplar AS (
            SELECT kp_id, max(claimed_at) AS at
            FROM serving_pool
            WHERE claimed_at IS NOT NULL AND source = 'exemplar'
            GROUP BY kp_id
        )
        SELECT kps.kp_id AS "kp_id!",
               coalesce(approved.approved, 0) AS "approved_templates!",
               coalesce(depth.depth, 0) AS "pool_depth!",
               last_served.source AS "last_source?",
               last_exemplar.at AS "last_exemplar_at?"
        FROM kps
        LEFT JOIN approved ON approved.kp_id = kps.kp_id
        LEFT JOIN depth ON depth.kp_id = kps.kp_id
        LEFT JOIN last_served ON last_served.kp_id = kps.kp_id
        LEFT JOIN last_exemplar ON last_exemplar.kp_id = kps.kp_id
        ORDER BY kps.kp_id
        "#
    )
    .fetch_all(executor)
    .await?;

    let mut flags = Vec::with_capacity(rows.len());
    for row in rows {
        let last_source = match row.last_source {
            None => None,
            Some(ref wire) => match Source::from_wire(wire) {
                Some(source) => Some(source),
                None => {
                    return Err(StoreError::PoolRow(format!(
                        "knowledge point {} last served source {wire:?}, which this build does not \
                         know",
                        row.kp_id
                    )));
                }
            },
        };
        let source_exhausted = exhausted.contains(row.kp_id.as_str());
        flags.push(KpFlag {
            kp_id: row.kp_id,
            approved_templates: row.approved_templates,
            pool_depth: row.pool_depth,
            last_source,
            last_exemplar_at: row.last_exemplar_at,
            needs_template: row.approved_templates == 0,
            source_exhausted,
        });
    }
    Ok(flags)
}
