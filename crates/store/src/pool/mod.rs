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

mod pop;
mod refill;

use cadus_core::pool::{Candidate, Pick, PoolAnswer, PoolProblem, Source};
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

pub use pop::{pop_with_ring, pop_with_ring_tx, reclaim_exemplar_tx};
pub use refill::{
    approved_template, operator_flags, operator_flags_with_exhausted, refill_targets,
    refill_targets_skipping, retire_unapproved, unclaimed_depth,
};

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
