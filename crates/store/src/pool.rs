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
//!   SELECT ... FOR UPDATE SKIP LOCKED LIMIT 8;   -- D7, the pop
//!   -- cadus_core::pool::pick skips every ring hit
//!   UPDATE serving_pool SET claimed_at = now() WHERE id = $1;
//!   -- the caller writes the D-S6 state row here, in this same transaction
//! COMMIT;
//! ```
//!
//! `SKIP LOCKED` is the whole concurrency argument: two serves of one
//! `(user, kp)` walk past each other's locked rows, so neither one waits and
//! neither one reads the row the other claims.
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
/// `Ok(None)` means the pool held no unclaimed row for this pair. The caller then
/// instantiates an exemplar in process and raises the A6 flag (specification
/// section 7.2); it must not generate.
///
/// # Errors
///
/// Returns [`StoreError::PoolRow`] when a row does not decode, and
/// [`StoreError::Db`] when a statement fails.
pub async fn pop_with_ring_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    kp_id: &str,
    avoid: &Avoid<'_>,
) -> Result<Option<Claimed>, StoreError> {
    let popped = sqlx::query!(
        r#"
        SELECT id AS "id!",
               source AS "source!",
               content_digest,
               problem::text AS "problem!",
               expected_answer::text AS "expected!",
               instance_hash AS "instance_hash!"
        FROM serving_pool
        WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
        ORDER BY created_at, id
        FOR UPDATE SKIP LOCKED
        LIMIT $3
        "#,
        user_id,
        kp_id,
        POP_LIMIT,
    )
    .fetch_all(&mut **tx)
    .await?;

    let mut candidates: Vec<PoolRow> = Vec::with_capacity(popped.len());
    for row in popped {
        candidates.push(read_row(
            row.id,
            &row.source,
            row.content_digest,
            &row.problem,
            &row.expected,
            row.instance_hash,
        )?);
    }

    let Some(chosen) = pick(&candidates, avoid) else {
        return Ok(None);
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

    Ok(Some(Claimed {
        row,
        pick: chosen,
        candidates: candidates.len(),
    }))
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
) -> Result<Option<Claimed>, StoreError> {
    let mut tx = begin_tenant(pool, user_id).await?;
    let claimed = pop_with_ring_tx(&mut tx, user_id, kp_id, avoid).await?;
    tx.commit().await?;
    Ok(claimed)
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
    let rows = sqlx::query!(
        r#"
        SELECT user_id AS "user_id!",
               kp_id AS "kp_id!",
               count(*) FILTER (WHERE claimed_at IS NULL) AS "depth!"
        FROM serving_pool
        GROUP BY user_id, kp_id
        HAVING count(*) FILTER (WHERE claimed_at IS NULL) < $1
        ORDER BY "depth!", user_id, kp_id
        LIMIT $2
        "#,
        target_depth,
        limit,
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
        flags.push(KpFlag {
            kp_id: row.kp_id,
            approved_templates: row.approved_templates,
            pool_depth: row.pool_depth,
            last_source,
            last_exemplar_at: row.last_exemplar_at,
            needs_template: row.approved_templates == 0,
        });
    }
    Ok(flags)
}
