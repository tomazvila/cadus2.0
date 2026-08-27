//! The A4 diagnosis queue: the enqueue, the tenant-bound read, and the push
//! notice.
//!
//! Requirements: A4 (the prose arrives after the verdict), C3 (every tenant read
//! and write runs inside `begin_tenant`), D7 (push), D-M5-1 (SSE over
//! `LISTEN/NOTIFY` with a poll fallback), R2 (compile-time checked queries).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2.1 (the wire shape of
//! `diagnosis`), section 4.3 step 9 (the enqueue is inside the grade
//! transaction), section 6 (the worker), and row U9 of section 11.
//!
//! # Who writes what
//!
//! The request tier calls [`enqueue`] inside the grade transaction, so a grade
//! that rolls back leaves no job row. The worker (unit U10) claims the row,
//! writes `result`, and runs [`notify_payload`]'s `NOTIFY`. Nothing else writes
//! this table.
//!
//! # Why the payload of the NOTIFY holds ids only
//!
//! `LISTEN/NOTIFY` carries no row-level security and no tenant binding (trap
//! W14). Any process that listens on the channel reads every payload of every
//! tenant, so the payload names a job id and a user id and nothing else. The SSE
//! handler re-reads the row through `begin_tenant` before it writes a byte.
//!
//! # Why [`job`] takes no `user_id`
//!
//! The read is scoped by the `tenant_isolation` policy of
//! `migrations/0006_grants_rls.sql` alone. A second predicate on `user_id` would
//! make the tenant test pass with the policy removed, and the boot guard
//! [`assert_rls_enforced`](crate::assert_rls_enforced) already refuses a role
//! that escapes the policy. One guard, and a test that proves it.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{PgExecutor, Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

/// The `LISTEN/NOTIFY` channel the worker signals a finished job on (D7).
pub const NOTIFY_CHANNEL: &str = "diagnosis_done";

/// The separator between the job id and the user id of a notice payload.
pub const NOTIFY_SEPARATOR: char = ':';

/// `diagnosis_jobs.status` of a job no worker claimed yet.
pub const JOB_PENDING: &str = "pending";

/// `diagnosis_jobs.status` of a claimed job.
pub const JOB_RUNNING: &str = "running";

/// `diagnosis_jobs.status` of a job whose `result` stands.
pub const JOB_DONE: &str = "done";

/// `diagnosis_jobs.status` of a dead-lettered job (spec section 6.1).
pub const JOB_FAILED: &str = "failed";

/// `diagnosis_jobs.status` of a job a T4 cap refused (spec section 6.6).
pub const JOB_CAPPED: &str = "capped";

/// The version of the [`JobPayload`] document.
pub const PAYLOAD_VERSION: u32 = 1;

/// What the grade transaction hands the worker (spec section 6.3).
///
/// The document lives here, beside the [`enqueue`] that writes it, because the
/// request tier writes it and the worker claim reads it: it belongs to neither
/// caller. Every field the section 6.3 user message names is here, and nothing
/// else — the worker never re-reads the learner's log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobPayload {
    /// [`PAYLOAD_VERSION`].
    pub v: u32,
    /// The session the attempt belongs to, for the T4 per-session knob.
    pub session: Option<String>,
    /// The task the attempt belongs to.
    pub task_id: String,
    /// The topic of the problem.
    pub topic: String,
    /// The knowledge point of the problem, when the serve named one.
    #[serde(default)]
    pub kp: Option<String>,
    /// The statement the learner read.
    pub problem: String,
    /// The authored answer. The worker states it as the reference.
    pub expected: String,
    /// The answer grammar, in the checker's spelling.
    pub answer_kind: String,
    /// What the learner answered.
    pub given_answer: String,
    /// The learner's shown work, when the submission carried any.
    #[serde(default)]
    pub work: Option<String>,
}

/// One `diagnosis_jobs` row, as the poll route and the SSE handler read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    /// The primary key. It is the `id` the client polls.
    pub id: Uuid,
    /// The attempt the job diagnoses.
    pub attempt_id: String,
    /// One of [`JOB_PENDING`], [`JOB_RUNNING`], [`JOB_DONE`], [`JOB_FAILED`],
    /// [`JOB_CAPPED`].
    pub status: String,
    /// The diagnosis document, `NULL` until the worker writes it.
    pub result: Option<Json>,
    /// When the grade transaction wrote the row.
    pub created_at: DateTime<Utc>,
}

/// Put one job on the queue, inside the caller's transaction (spec 4.3 step 9).
///
/// The insert is idempotent: `(user_id, attempt_id)` is unique, so a retried
/// grade meets the standing row and the call returns its id instead of a second
/// one. The caller therefore hands one client one job id for one attempt, and a
/// grade that rolls back leaves no row at all.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails, and when the standing
/// row disappears between the insert and the read — which the transaction's own
/// lock makes impossible.
pub async fn enqueue(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    attempt_id: &str,
    payload: &Json,
) -> Result<Uuid, StoreError> {
    let id = sqlx::query_scalar!(
        r#"
        WITH inserted AS (
            INSERT INTO diagnosis_jobs (user_id, attempt_id, payload)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, attempt_id) DO NOTHING
            RETURNING id
        )
        SELECT id AS "id!" FROM inserted
        UNION ALL
        SELECT id AS "id!" FROM diagnosis_jobs
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

/// Read one job by its primary key, under the caller's tenant binding (C3).
///
/// The statement names no `user_id`. The `tenant_isolation` policy scopes it, so
/// another tenant's id returns no row and the route answers `404`.
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
        FROM diagnosis_jobs
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

/// One `NOTIFY diagnosis_done` payload: a job id and the tenant it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Notice {
    /// The finished job.
    pub job_id: Uuid,
    /// The tenant that may read it.
    pub user_id: Uuid,
}

impl Notice {
    /// Read a payload the worker sent. An unreadable payload gives `None`.
    ///
    /// Anything at all can arrive on a `LISTEN` channel, so the reader refuses
    /// a payload with the wrong shape instead of guessing at it.
    #[must_use]
    pub fn parse(payload: &str) -> Option<Self> {
        let (job, user) = payload.split_once(NOTIFY_SEPARATOR)?;
        Some(Self {
            job_id: Uuid::parse_str(job.trim()).ok()?,
            user_id: Uuid::parse_str(user.trim()).ok()?,
        })
    }
}

/// Write the payload of a `NOTIFY diagnosis_done` (spec section 2.1, "Push").
#[must_use]
pub fn notify_payload(job_id: Uuid, user_id: Uuid) -> String {
    format!("{job_id}{NOTIFY_SEPARATOR}{user_id}")
}

#[cfg(test)]
mod tests {
    use super::{Notice, notify_payload};
    use uuid::Uuid;

    /// The payload round-trips, and every malformed shape is refused.
    ///
    /// The channel carries no tenant binding, so a payload the reader cannot
    /// parse must never become a notice for some other learner.
    #[test]
    fn a_notice_round_trips_and_a_broken_payload_is_refused() {
        let job = Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
        let user = Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap();

        assert_eq!(
            notify_payload(job, user),
            "11111111-1111-4111-8111-111111111111:22222222-2222-4222-8222-222222222222"
        );
        assert_eq!(
            Notice::parse(&notify_payload(job, user)),
            Some(Notice {
                job_id: job,
                user_id: user,
            })
        );

        for broken in [
            "",
            "11111111-1111-4111-8111-111111111111",
            "11111111-1111-4111-8111-111111111111:",
            ":22222222-2222-4222-8222-222222222222",
            "not-a-uuid:22222222-2222-4222-8222-222222222222",
            "11111111-1111-4111-8111-111111111111:not-a-uuid",
        ] {
            assert_eq!(Notice::parse(broken), None, "{broken:?}");
        }
    }
}
