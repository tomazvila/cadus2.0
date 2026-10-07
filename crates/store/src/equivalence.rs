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
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
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

/// The normalized cache key of one learner answer.
///
/// The normalization of the note-114 design: whitespace, case, unicode minus,
/// `x^2`/`x²`, decimal comma — the cache key only; the model still sees the
/// raw text.
#[must_use]
pub fn cache_key(learner: &str) -> String {
    // The char map of the note-114 normalization, one pass: minus signs to
    // `-`, superscripts to `^n`, the dot operators to `*`, the decimal comma
    // to `.` (a comma between digits is a decimal comma, the list separator
    // always carries a space).
    let mapped: String = learner
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '\u{2212}' | '\u{2013}' | '\u{2014}' => '-',
            '\u{00b2}' => '^',
            '\u{00b3}' => '$',
            '\u{221a}' => 'V',
            '\u{00b7}' | '\u{00d7}' => '*',
            ',' => '.',
            other => other,
        })
        .collect();
    mapped.split_whitespace().collect::<Vec<_>>().join(" ")
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

/// One learner-facing line of a job's progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Step {
    /// The line the learner reads.
    pub text: String,
}

/// What the poll route reads of a job beyond [`JobRow`]: the steps, the
/// payload, and the landing record.
#[derive(Debug, Clone, PartialEq)]
pub struct JobDetail {
    /// The lines the worker appended, in order.
    pub steps: Vec<Step>,
    /// The payload document the worker was handed.
    pub payload: Json,
    /// Whether the web tier already rewrote the stored attempt.
    pub landed: bool,
    /// The XP that rewrite awarded.
    pub landed_xp: Option<f64>,
}

/// Read the steps, payload and landing record of one job (tenant-scoped).
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn detail<'e, E>(executor: E, id: Uuid) -> Result<Option<JobDetail>, StoreError>
where
    E: PgExecutor<'e>,
{
    let row = sqlx::query_as::<_, (Json, Json, bool, Option<f64>)>(
        "SELECT steps, payload, landed_at IS NOT NULL, landed_xp \
           FROM equivalence_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(executor)
    .await?;
    Ok(row.map(|(steps, payload, landed, landed_xp)| JobDetail {
        steps: serde_json::from_value(steps).unwrap_or_default(),
        payload,
        landed,
        landed_xp,
    }))
}

/// Append one learner-facing line to a job. The worker is the only caller.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn add_step(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    text: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE equivalence_jobs \
            SET steps = steps || jsonb_build_array(jsonb_build_object('text', $2::text)) \
          WHERE id = $1 AND octet_length(steps::text) < 12000",
    )
    .bind(id)
    .bind(text)
    .execute(executor)
    .await?;
    Ok(())
}

/// Win the right to rewrite the stored attempt of a done job.
///
/// Returns `true` for exactly one caller per job. The caller runs the rewrite
/// in the same transaction, so a rewrite that rolls back gives the right back.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn claim_landing(executor: impl PgExecutor<'_>, id: Uuid) -> Result<bool, StoreError> {
    let done = sqlx::query(
        "UPDATE equivalence_jobs SET landed_at = now() \
          WHERE id = $1 AND status = 'done' AND landed_at IS NULL",
    )
    .bind(id)
    .execute(executor)
    .await?;
    Ok(done.rows_affected() == 1)
}

/// Record the XP the rewrite of a job awarded.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn set_landed_xp(
    executor: impl PgExecutor<'_>,
    id: Uuid,
    xp: f64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE equivalence_jobs SET landed_xp = $2 WHERE id = $1")
        .bind(id)
        .bind(xp)
        .execute(executor)
        .await?;
    Ok(())
}

/// One row of the overturn report: an item and how often a background check
/// overturned the first-pass fail on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Overturn {
    /// The item digest (the cache key's first half).
    pub item_digest: String,
    /// The topic of the item.
    pub topic: String,
    /// Settled background checks of the item.
    pub checks: i64,
    /// Checks that accepted the answer the first pass had refused.
    pub overturned: i64,
}

/// The items with at least `min_checks` settled checks, the highest overturn
/// rate first. Admin pool only: it reads every tenant.
///
/// # Errors
///
/// Returns [`StoreError::Db`] when the statement fails.
pub async fn overturn_rates(
    executor: impl PgExecutor<'_>,
    min_checks: i64,
    limit: i64,
) -> Result<Vec<Overturn>, StoreError> {
    let rows = sqlx::query_as::<_, (String, String, i64, i64)>(
        "SELECT payload->>'item_digest', coalesce(max(payload->>'topic'), ''), \
                count(*), count(*) FILTER (WHERE (result->>'equivalent')::boolean) \
           FROM equivalence_jobs \
          WHERE status = 'done' AND result ? 'equivalent' \
          GROUP BY payload->>'item_digest' \
         HAVING count(*) >= $1 \
          ORDER BY (count(*) FILTER (WHERE (result->>'equivalent')::boolean))::float8 \
                   / count(*) DESC, count(*) DESC \
          LIMIT $2",
    )
    .bind(min_checks)
    .bind(limit)
    .fetch_all(executor)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(item_digest, topic, checks, overturned)| Overturn {
            item_digest,
            topic,
            checks,
            overturned,
        })
        .collect())
}

/// The standard short form of an accepted answer whose form differed.
///
/// A multipart answer ("degree = 3; leading_coefficient = 4") has the bare
/// values ("3, 4") as its short form. The form is offered only when it is
/// shorter than what the learner typed. Any other contract has none.
#[must_use]
pub fn shorter_form(expected: &str, contract: Option<&str>, learner: &str) -> Option<String> {
    let doc: Json = serde_json::from_str(contract?).ok()?;
    if doc.get("kind").and_then(Json::as_str) != Some("multipart") {
        return None;
    }
    let values: Vec<&str> = expected
        .split([';', ','])
        .map(|piece| piece.rsplit('=').next().unwrap_or(piece).trim())
        .filter(|value| !value.is_empty())
        .collect();
    let form = values.join(", ");
    (!form.is_empty() && form.len() < learner.trim().len()).then_some(form)
}

/// Clip a learner or key text to a short quoted line for a step.
#[must_use]
pub fn clip(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 120 {
        let head: String = flat.chars().take(117).collect();
        format!("{head}...")
    } else {
        flat
    }
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

    /// The cache key normalizes case, whitespace, the unicode minus and the
    /// decimal comma, so the worker's write and the web lookup meet.
    #[test]
    fn the_cache_key_normalizes_the_answer() {
        assert_eq!(
            super::cache_key("  Not a  solution, 5 "),
            "not a solution. 5"
        );
        assert_eq!(super::cache_key("\u{2212}3"), "-3");
    }

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
