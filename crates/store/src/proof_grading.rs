//! The Amendment K proof-grading queue (design point 6): the enqueue, the
//! tenant-bound read, the claim, and the settle.
//!
//! The equivalence queue's shape, without a cache. The grade transaction
//! enqueues an UNGRADED written-proof attempt; the worker claims the row,
//! asks the hosted model for a per-check grading, and settles it.
//!
//! # Who writes what
//!
//! The request tier calls [`enqueue`] inside the grade transaction and reads
//! with [`job`]. The worker claims with [`claim`] and settles with [`settle`].
//! Nothing else writes the table.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{PgExecutor, Postgres, Transaction};

use crate::StoreError;

/// `proof_grading_jobs.status` of a job no worker claimed yet.
pub const JOB_PENDING: &str = "pending";

/// `proof_grading_jobs.status` of a claimed job.
pub const JOB_RUNNING: &str = "running";

/// `proof_grading_jobs.status` of a job whose `result` stands.
pub const JOB_DONE: &str = "done";

/// `proof_grading_jobs.status` of a job the worker gave up on.
pub const JOB_FAILED: &str = "failed";

/// `proof_grading_jobs.status` of a job the per-learner daily cap refused.
pub const JOB_CAPPED: &str = "capped";

/// The version of the [`JobPayload`] document.
pub const PAYLOAD_VERSION: u32 = 1;

/// The version of the [`Grading`] result document.
pub const RESULT_VERSION: u32 = 1;

/// The `verdict` of a proof the grading accepts.
pub const VERDICT_PASS: &str = "pass";

/// The `verdict` of a proof the grading sends back.
pub const VERDICT_NEEDS_REVISION: &str = "needs_revision";

/// What the grade transaction hands the worker.
///
/// Every field the grading prompt needs: the problem, the reference solution,
/// an authored rubric when the item carries one, and the learner's text. No
/// session and no user id: the worker reads the tenant from the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobPayload {
    /// [`PAYLOAD_VERSION`].
    pub v: u32,
    /// The task the attempt belongs to.
    pub task_id: String,
    /// The topic of the problem.
    pub topic: String,
    /// The digest of the served item.
    pub item_digest: String,
    /// The statement the learner read.
    pub problem: String,
    /// The reference solution (the worked solution sketch), or `None`.
    #[serde(default)]
    pub reference: Option<String>,
    /// The stored key, when it carries content (a proof item usually stores a
    /// placeholder such as "See the solution.").
    #[serde(default)]
    pub expected: Option<String>,
    /// The authored yes/no rubric of the item. Empty today: no item carries
    /// one yet. A non-empty rubric takes precedence over derived checks.
    #[serde(default)]
    pub rubric: Vec<String>,
    /// What the learner wrote, raw.
    pub given_answer: String,
}

/// One graded check of a [`Grading`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// `G1`..`G5` for the general checks, `S1`.. for the problem-specific
    /// ones, `R1`.. for authored rubric items.
    pub id: String,
    /// The yes/no question, in words.
    pub text: String,
    /// Whether the check is minor (one failed minor check still passes).
    pub minor: bool,
    /// Whether the learner text meets the check.
    pub met: bool,
    /// A short quote from the learner text, or "not found".
    pub evidence: String,
}

/// The result document of one graded proof (`proof_grading_jobs.result`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grading {
    /// [`RESULT_VERSION`].
    pub v: u32,
    /// [`VERDICT_PASS`] or [`VERDICT_NEEDS_REVISION`].
    pub verdict: String,
    /// The checks, in the order the model answered them.
    pub checks: Vec<Check>,
    /// Two to four sentences for the learner.
    pub feedback: String,
    /// The model id that graded.
    pub model: String,
}

impl Grading {
    /// Whether the verdict is a pass.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.verdict == VERDICT_PASS
    }
}

/// One `proof_grading_jobs` row, as the poll route reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    /// The primary key. It is the `id` the client polls.
    pub id: Uuid,
    /// The attempt the job grades.
    pub attempt_id: String,
    /// One of [`JOB_PENDING`], [`JOB_RUNNING`], [`JOB_DONE`], [`JOB_FAILED`],
    /// [`JOB_CAPPED`].
    pub status: String,
    /// The payload the grade transaction wrote.
    pub payload: Json,
    /// The result document, `NULL` until the worker writes it.
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
    /// The attempt the job grades.
    pub attempt_id: String,
    /// The payload document.
    pub payload: Json,
    /// How many claims this row already used, this one included.
    pub attempts: i32,
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
            INSERT INTO proof_grading_jobs (user_id, attempt_id, payload)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, attempt_id) DO NOTHING
            RETURNING id
        )
        SELECT id AS "id!" FROM inserted
        UNION ALL
        SELECT id AS "id!" FROM proof_grading_jobs
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
               payload AS "payload!", result, created_at AS "created_at!"
        FROM proof_grading_jobs
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
        payload: row.payload,
        result: row.result,
        created_at: row.created_at,
    }))
}

/// Take the oldest pending row (`FOR UPDATE SKIP LOCKED`, the D-O5 pattern).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn claim(db: &crate::Db) -> Result<Option<Claimed>, StoreError> {
    let row = sqlx::query!(
        r#"
        WITH job AS (
          SELECT id FROM proof_grading_jobs
           WHERE status = $1
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1)
        UPDATE proof_grading_jobs d
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

/// Settle one claimed row into `status`, with `result` when there is one.
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
        UPDATE proof_grading_jobs
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

/// How many proof-grading jobs this learner wrote since the UTC day began.
///
/// The per-learner daily cap of Amendment K (20) reads this count.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn jobs_today<'e, E>(executor: E, user_id: Uuid) -> Result<i64, StoreError>
where
    E: PgExecutor<'e>,
{
    let count = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
          FROM proof_grading_jobs
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
    #![allow(clippy::unwrap_used)]
    use super::{Check, Grading, JobPayload, VERDICT_PASS};

    /// The result document round-trips through its JSON spelling.
    #[test]
    fn a_grading_round_trips() {
        let grading = Grading {
            v: 1,
            verdict: VERDICT_PASS.to_owned(),
            checks: vec![Check {
                id: "G1".to_owned(),
                text: "The claim is stated.".to_owned(),
                minor: false,
                met: true,
                evidence: "Let n be odd".to_owned(),
            }],
            feedback: "A complete proof.".to_owned(),
            model: "deepseek/deepseek-v4-pro".to_owned(),
        };
        let json = serde_json::to_value(&grading).unwrap();
        assert_eq!(serde_json::from_value::<Grading>(json).unwrap(), grading);
        assert!(grading.passed());
    }

    /// A payload with no rubric, no reference and no key still reads.
    #[test]
    fn a_minimal_payload_reads() {
        let payload: JobPayload = serde_json::from_value(serde_json::json!({
            "v": 1, "task_id": "t", "topic": "x", "item_digest": "abc123def456",
            "problem": "Prove it.", "given_answer": "Proof."
        }))
        .unwrap();
        assert!(payload.rubric.is_empty());
        assert_eq!(payload.reference, None);
    }
}
