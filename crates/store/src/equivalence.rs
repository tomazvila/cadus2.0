//! The Amendment K equivalence queue and its cache: the enqueue, the
//! tenant-bound read, and the cache lookup.
//!
//! The A4 pattern of `diagnosis`, applied to the owner's note-114 design: the
//! grade transaction enqueues, the worker claims and settles, the answer sits
//! in a cache keyed by (item digest, normalized learner text) so a repeat
//! costs nothing.
//!
//! # Who writes what
//!
//! The request tier calls [`enqueue`] inside the grade transaction and reads
//! the cache with [`cache_hit`]. The worker claims with [`claim`], writes the
//! cache with [`cache_put`], and settles with [`settle`]. Nothing else writes
//! either table.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::types::Uuid;
use sqlx::{PgExecutor, Postgres, Transaction};

use crate::StoreError;

/// `equivalence_jobs.status` of a job no worker claimed yet.
pub const JOB_PENDING: &str = "pending";

/// `equivalence_jobs.status` of a claimed job.
pub const JOB_RUNNING: &str = "running";

/// `equivalence_jobs.status` of a job whose `result` stands.
pub const JOB_DONE: &str = "done";

/// `equivalence_jobs.status` of a job the worker gave up on.
pub const JOB_FAILED: &str = "failed";

/// `equivalence_jobs.status` of a job a per-learner daily cap refused.
pub const JOB_CAPPED: &str = "capped";

/// The version of the [`JobPayload`] document.
pub const PAYLOAD_VERSION: u32 = 1;

/// What the grade transaction hands the worker.
///
/// Every field the model prompt needs and the cache needs: the problem, the
/// stored key, the answer contract, the learner's text, and the item digest
/// the cache is keyed by. No session, no user id — the worker reads the
/// tenant from the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobPayload {
    /// [`PAYLOAD_VERSION`].
    pub v: u32,
    /// The task the attempt belongs to.
    pub task_id: String,
    /// The topic of the problem.
    pub topic: String,
    /// The digest of the served item — the cache key's first half.
    pub item_digest: String,
    /// The statement the learner read.
    pub problem: String,
    /// The stored key.
    pub expected: String,
    /// The answer contract, in the checker's spelling, or `None`.
    #[serde(default)]
    pub answer_contract: Option<String>,
    /// What the learner answered (the raw text; the cache key is normalized,
    /// the model sees this).
    pub given_answer: String,
}

/// One `equivalence_jobs` row, as the poll route reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    /// The primary key. It is the `id` the client polls.
    pub id: Uuid,
    /// The attempt the job checks.
    pub attempt_id: String,
    /// One of [`JOB_PENDING`], [`JOB_RUNNING`], [`JOB_DONE`], [`JOB_FAILED`],
    /// [`JOB_CAPPED`].
    pub status: String,
    /// The verdict document, `NULL` until the worker writes it.
    pub result: Option<Json>,
    /// When the grade transaction wrote the row.
    pub created_at: DateTime<Utc>,
}

/// One claimed row, as the worker reads it.
#[derive(Debug, Clone)]
pub struct Claimed {
    /// The primary key.
    pub id: Uuid,
    /// The tenant that submitted the attempt.
    pub user_id: Uuid,
    /// The attempt the job checks.
    pub attempt_id: String,
    /// The payload document.
    pub payload: Json,
    /// How many attempts this row already used.
    pub attempts: i32,
}

/// The verdict document the worker settles with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    /// Whether the model accepted the answer as equivalent.
    pub equivalent: bool,
    /// The model's one-line why.
    pub reason: String,
    /// The model id that produced the verdict.
    pub model: String,
}

/// Put one job on the queue, inside the caller's transaction.
///
/// The insert is idempotent on `(user_id, attempt_id)`: a retried grade meets
/// the standing row and answers with its id instead of a second one.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    attempt_id: &str,
    payload: &Json,
) -> Result<Uuid, StoreError> {
    let id = sqlx::query_scalar!(
        r#"
        WITH inserted AS (
            INSERT INTO equivalence_jobs (user_id, attempt_id, payload)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, attempt_id) DO NOTHING
            RETURNING id
        )
        SELECT id AS "id!" FROM inserted
        UNION ALL
        SELECT id AS "id!" FROM equivalence_jobs
         WHERE user_id = $1 AND attempt_id = $2
        LIMIT 1
        "#,
        user_id,
        attempt_id,
        payload,
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// Read one job by its primary key, under the caller's tenant binding.
///
/// The statement names no `user_id`: the `tenant_isolation` policy scopes it,
/// so another tenant's id returns no row.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn job<'e, E>(executor: E, id: Uuid) -> Result<Option<JobRow>, StoreError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query!(
        r#"
        SELECT id AS "id!", attempt_id AS "attempt_id!", status AS "status!",
               result, created_at AS "created_at!"
        FROM equivalence_jobs
        WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(executor)
    .await?;

    Ok(row.map(|row| JobRow {
        id: row.id,
        attempt_id: row.attempt_id,
        status: row.status,
        result: row.result,
        created_at: row.created_at,
    }))
}

/// The cached verdict of `(item_digest, answer_key)`, or `None`.
///
/// `answer_key` is the NORMALIZED learner text, never the raw one: the cache
/// key only; the model sees the raw text.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn cache_hit<'e, E>(
    executor: E,
    item_digest: &str,
    answer_key: &str,
) -> Result<Option<Verdict>, StoreError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query!(
        r#"
        SELECT equivalent AS "equivalent!", reason AS "reason!", model AS "model!"
        FROM equivalence_cache
        WHERE item_digest = $1 AND answer_key = $2
        "#,
        item_digest,
        answer_key,
    )
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|row| Verdict {
        equivalent: row.equivalent,
        reason: row.reason,
        model: row.model,
    }))
}

/// Write one verdict into the cache. The worker is the only caller.
///
/// An existing row stands: the first verdict wins, so a re-check cannot flip
/// a settled answer.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn cache_put(
    executor: impl PgExecutor<'_>,
    item_digest: &str,
    answer_key: &str,
    verdict: &Verdict,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        INSERT INTO equivalence_cache (item_digest, answer_key, equivalent, reason, model)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (item_digest, answer_key) DO NOTHING
        "#,
        item_digest,
        answer_key,
        verdict.equivalent,
        verdict.reason,
        verdict.model,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Take the oldest pending row (`FOR UPDATE SKIP LOCKED`, the D-O5 pattern).
///
/// Two workers running the same statement at the same instant step over the
/// locked row and take the next one, so two workers never claim one job.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn claim(db: &crate::Db) -> Result<Option<Claimed>, StoreError> {
    let row = sqlx::query!(
        r#"
        WITH job AS (
          SELECT id FROM equivalence_jobs
           WHERE status = $1
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1)
        UPDATE equivalence_jobs d
           SET status = $2, claimed_at = now(), attempts = attempts + 1
          FROM job WHERE d.id = job.id
        RETURNING d.id AS "id!", d.user_id AS "user_id!", d.attempt_id AS "attempt_id!",
                  d.payload AS "payload!", d.attempts AS "attempts!"
        "#,
        JOB_PENDING,
        JOB_RUNNING,
    )
    .fetch_optional(db.pool())
    .await?;
    Ok(row.map(|row| Claimed {
        id: row.id,
        user_id: row.user_id,
        attempt_id: row.attempt_id,
        payload: row.payload,
        attempts: row.attempts,
    }))
}

/// Settle one claimed row into `status`, with `result` when it is done.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn settle(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    status: &str,
    result: Option<&Json>,
) -> Result<(), StoreError> {
    sqlx::query!(
        r#"
        UPDATE equivalence_jobs
           SET status = $2, result = $3, finished_at = now()
         WHERE id = $1
        "#,
        id,
        status,
        result,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// How many equivalence calls this learner spent since the UTC day began.
///
/// The per-learner daily cap of the note-114 design reads this count.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn calls_today<'e, E>(executor: E, user_id: Uuid) -> Result<i64, StoreError>
where
    E: PgExecutor<'e>,
{
    let count = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
          FROM equivalence_jobs
         WHERE user_id = $1 AND created_at >= date_trunc('day', now())
        "#,
        user_id,
    )
    .fetch_one(executor)
    .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::Verdict;

    /// The verdict document round-trips through its JSON spelling.
    #[test]
    fn a_verdict_round_trips() {
        let verdict = Verdict {
            equivalent: true,
            reason: "the same value in other words".to_owned(),
            model: "qwen-general-8bit".to_owned(),
        };
        let json = serde_json::to_value(&verdict).unwrap();
        assert_eq!(serde_json::from_value::<Verdict>(json).unwrap(), verdict);
    }
}
