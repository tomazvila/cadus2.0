//! The queue statements: the sweep, the claim, the count, and the end states.

use cadus_store::diagnosis::{
    JOB_FAILED, JOB_PENDING, JOB_RUNNING, NOTIFY_CHANNEL, notify_payload,
};
use cadus_store::{Db, bounded};
use serde_json::Value;

use super::{Job, LEASE, MAX_JOB_ATTEMPTS, Outcome};
use crate::WorkerError;

/// Reclaim stale leases and dead-letter the rows that used their attempts.
///
/// The two statements are the whole recovery story of the queue: a worker that
/// died mid-call leaves a `running` row nobody holds, and a job that fails three
/// times must stop costing money. Both run as `cadus_admin`, across tenants.
///
/// Returns the number of rows the sweep reset and the number it dead-lettered.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when a statement fails or the bound expires.
pub async fn sweep(db: &Db) -> Result<(u64, u64), WorkerError> {
    let lease_secs = f64::from(u32::try_from(LEASE.as_secs()).unwrap_or(u32::MAX));

    let dead = sqlx::query!(
        r#"
        UPDATE diagnosis_jobs
           SET status = $1, finished_at = now()
         WHERE status = $2
           AND attempts >= $3
           AND claimed_at < now() - make_interval(secs => $4)
        "#,
        JOB_FAILED,
        JOB_RUNNING,
        MAX_JOB_ATTEMPTS,
        lease_secs,
    )
    .execute(db.pool());
    let dead = bounded(db, dead).await?.rows_affected();

    let reset = sqlx::query!(
        r#"
        UPDATE diagnosis_jobs
           SET status = $1, claimed_at = NULL
         WHERE status = $2
           AND claimed_at < now() - make_interval(secs => $3)
        "#,
        JOB_PENDING,
        JOB_RUNNING,
        lease_secs,
    )
    .execute(db.pool());
    let reset = bounded(db, reset).await?.rows_affected();

    Ok((reset, dead))
}

/// Take the oldest pending row (D-O5, spec section 6.1).
///
/// `FOR UPDATE SKIP LOCKED` inside one statement is the whole concurrency
/// contract: a second worker running the same statement at the same instant
/// steps over the locked row and takes the next one, so two workers never claim
/// one job and neither one waits on the other. The partial index
/// `diagnosis_jobs_pending (created_at) WHERE status = 'pending'` serves it.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
pub async fn claim(db: &Db) -> Result<Option<Job>, WorkerError> {
    let query = sqlx::query!(
        r#"
        WITH job AS (
          SELECT id FROM diagnosis_jobs
           WHERE status = $1
           ORDER BY created_at
           FOR UPDATE SKIP LOCKED
           LIMIT 1)
        UPDATE diagnosis_jobs d
           SET status = $2, claimed_at = now(), attempts = attempts + 1
          FROM job WHERE d.id = job.id
        RETURNING d.id AS "id!", d.user_id AS "user_id!", d.attempt_id AS "attempt_id!",
                  d.payload AS "payload!", d.attempts AS "attempts!"
        "#,
        JOB_PENDING,
        JOB_RUNNING,
    )
    .fetch_optional(db.pool());

    let row = bounded(db, query).await?;
    Ok(row.map(|row| Job {
        id: row.id,
        user_id: row.user_id,
        attempt_id: row.attempt_id,
        payload: row.payload,
        attempts: row.attempts,
    }))
}

/// How many calls this session already spent (T4).
///
/// The count reads the queue itself: every row that reached `running` bought an
/// attempt. A job with no session is uncapped, because the cap is per session.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails or the bound expires.
pub async fn calls_this_session(db: &Db, job: &Job, session: &str) -> Result<i64, WorkerError> {
    let query = sqlx::query_scalar!(
        r#"
        SELECT count(*) AS "count!"
          FROM diagnosis_jobs
         WHERE user_id = $1
           AND payload ->> 'session' = $2
           AND id <> $3
           AND status <> $4
        "#,
        job.user_id,
        session,
        job.id,
        JOB_PENDING,
    )
    .fetch_one(db.pool());
    Ok(bounded(db, query).await?)
}

/// Write the end state of one row and push the notice, in ONE transaction.
///
/// The NOTIFY runs inside the same transaction that writes the row, so a client
/// woken by the notice always finds the finished row (D7). A transaction that
/// rolls back sends no notice at all: `pg_notify` is transactional.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when a statement fails, and
/// [`WorkerError::Db`] when the transaction does not commit.
pub(super) async fn settle(
    db: &Db,
    job: &Job,
    status: &str,
    result: Option<&Value>,
) -> Result<(), WorkerError> {
    let mut tx = db.pool().begin().await?;
    sqlx::query!(
        r#"
        UPDATE diagnosis_jobs
           SET status = $1, result = COALESCE($2, result), finished_at = now()
         WHERE id = $3
        "#,
        status,
        result,
        job.id,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "SELECT pg_notify($1, $2)",
        NOTIFY_CHANNEL,
        notify_payload(job.id, job.user_id),
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Put a claimed row back on the queue for another worker.
///
/// No NOTIFY: nothing finished, and the client is still polling a `pending` id.
///
/// # Errors
///
/// Returns [`WorkerError::Store`] when the statement fails.
async fn release(db: &Db, job: &Job) -> Result<(), WorkerError> {
    let query = sqlx::query!(
        "UPDATE diagnosis_jobs SET status = $1, claimed_at = NULL WHERE id = $2",
        JOB_PENDING,
        job.id,
    )
    .execute(db.pool());
    bounded(db, query).await?;
    Ok(())
}

/// End one failed attempt: another try, or the dead letter (spec section 6.1).
///
/// # Errors
///
/// Returns the error of [`release`] or of [`settle`].
pub async fn fail(db: &Db, job: &Job, why: &str) -> Result<Outcome, WorkerError> {
    if job.attempts < MAX_JOB_ATTEMPTS {
        tracing::warn!(job = %job.id, attempts = job.attempts, reason = why,
                       "diagnosis: the attempt failed; the row goes back on the queue");
        release(db, job).await?;
        return Ok(Outcome::Retry);
    }
    tracing::warn!(job = %job.id, attempts = job.attempts, reason = why,
                   "diagnosis: the row dead-letters; the learner keeps the verdict");
    settle(db, job, JOB_FAILED, None).await?;
    Ok(Outcome::Failed)
}
