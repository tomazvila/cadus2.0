//! The A4 diagnosis worker: claim, call, filter, write, notify (D-O5, T4, T5).
//!
//! Requirements: A4 (the prose follows the verdict), C2 (nothing here writes
//! `events`), D-O5 (a concurrent claim that blocks no other worker), D7
//! (`LISTEN/NOTIFY` pushes the finished row), R4 and T2 (the model call runs
//! here and nowhere else), T4 (the two knobs), T5 (the request defaults), T6
//! (one record per HTTP attempt).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 6 in full, and row U10
//! of section 11.
//!
//! # One pass
//!
//! ```text
//!   sweep()            -- a lease past 5 min goes back to pending;
//!      │                  a row at 3 attempts dead-letters
//!      ▼
//!   claim()            -- FOR UPDATE SKIP LOCKED: two workers never take one row
//!      │
//!      ▼
//!   the T4 count       -- over the per-session cap? -> status 'capped', NOTIFY
//!      │
//!      ▼
//!   Client::call()     -- T5 body, the retry contract of spec section 6.5
//!      │
//!      ├─ Ok  -> filter the tags -> status 'done', result, NOTIFY
//!      └─ Err -> attempts < 3 ? back to 'pending' : status 'failed', NOTIFY
//! ```
//!
//! # Why the claim needs no fencing token
//!
//! 1.0 claims with a conditional UPDATE and a fencing token, and guards every
//! completion on `status = 'sending' AND attempts = <the claimed value>`, because
//! dramatiq redelivers a message the worker already holds. Here the worker picks
//! its OWN row inside one transaction, and `FOR UPDATE SKIP LOCKED` gives that
//! row to nobody else, so there is no second holder to fence out. The `attempts`
//! counter stays, as the dead-letter rule and not as a fence.
//!
//! # Why the worker crosses tenants
//!
//! The worker connects as `cadus_admin`, which holds BYPASSRLS. The queue is one
//! queue for every learner, so a tenant policy would hide the rows it must
//! process. The NOTIFY payload therefore carries ids only, and the request tier
//! re-reads the row through `begin_tenant` before a byte reaches a client (trap
//! W14).
//!
//! # A failed diagnosis costs prose and nothing else
//!
//! The learner already holds the verdict, the worked solution and the stock
//! re-solve instruction, all deterministic (A3, L2). Nothing in this file is on
//! any learner's critical path.

use std::time::{Duration, Instant};

use cadus_model_client::{Attempt, ChatRequest, Client, ToolSpec};
use cadus_store::diagnosis::{
    JOB_CAPPED, JOB_DONE, JOB_FAILED, JOB_PENDING, JOB_RUNNING, JobPayload, NOTIFY_CHANNEL,
    PAYLOAD_VERSION, notify_payload,
};
use cadus_store::{Db, bounded};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use crate::WorkerError;

/// The attempts one job gets before it dead-letters (spec section 6.1).
pub const MAX_JOB_ATTEMPTS: i32 = 3;

/// How long a claimed row may stay `running` before the sweep reclaims it.
///
/// 1.0 leases for 15 minutes because a mail send is long; a diagnosis is one
/// bounded HTTP call, so 5 minutes is past every honest run (spec section 6.1).
pub const LEASE: Duration = Duration::from_secs(300);

/// The shortest gap between two sweeps.
///
/// 1.0 runs the sweep on a 5-minute cadence with a 2-minute debounce
/// (`cadus_worker/sweeps.py:72`). The worker tick here is 5 seconds, so the pass
/// keeps its own gap instead of running the sweep 12 times a minute.
pub const SWEEP_CADENCE: Duration = Duration::from_secs(300);

/// The environment variable of the per-session call cap (T4).
pub const CALLS_PER_SESSION_VAR: &str = "DIAGNOSIS_CALLS_PER_SESSION";

/// The per-session call cap when the environment names none.
///
/// O2 sets no cap, so the knob exists and defaults to `0 = unlimited`. A cap that
/// IS configured never withholds the verdict: the row goes to `capped` and the
/// learner keeps the deterministic verdict and the stock re-solve instruction
/// (spec section 6.6).
pub const DEFAULT_CALLS_PER_SESSION: u32 = 0;

/// The name of the forced tool (spec section 6.3).
pub const TOOL_NAME: &str = "emit_diagnosis";

/// The version of the document this worker writes into `diagnosis_jobs.result`.
pub const RESULT_VERSION: u32 = 1;

/// The error tags a MODEL may assign (spec section 5.3).
///
/// `blank-answer` is not here. It is server-assigned — the grade path stamps it
/// on a blank submission — so the prompt never invites the model to claim that
/// an answer was blank. `notation` and `timing-unreliable` are server-assigned
/// too, but 1.0 leaves both in the grader's vocabulary and dropping them here
/// would silently discard a tag the prompt invites, which is the exact 1.0
/// failure this list exists to prevent.
pub const MODEL_ERROR_TAGS: [&str; 11] = [
    "sign-error",
    "arithmetic-slip",
    "algebra-slip",
    "wrong-method",
    "formula-recall",
    "misread-problem",
    "incomplete",
    "notation",
    "units",
    "timing-unreliable",
    "blowoff",
];

/// One claimed row of `diagnosis_jobs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// The primary key; the id the client polls.
    pub id: Uuid,
    /// The tenant the row belongs to.
    pub user_id: Uuid,
    /// The attempt the job diagnoses.
    pub attempt_id: String,
    /// The document the grade transaction wrote.
    pub payload: Value,
    /// The value AFTER this claim raised it. 1 on the first claim.
    pub attempts: i32,
}

/// What one pass of [`run_once`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The queue held no pending row.
    Idle,
    /// The row carries a diagnosis now.
    Done,
    /// The row goes back on the queue for another attempt.
    Retry,
    /// The row dead-lettered.
    Failed,
    /// A T4 cap refused the call.
    Capped,
}

/// One pass, with the bill of its HTTP attempts (T6).
#[derive(Debug)]
pub struct Report {
    /// What the pass did.
    pub outcome: Outcome,
    /// The row the pass claimed, if any.
    pub job_id: Option<Uuid>,
    /// One record per HTTP attempt, in order. Unit U11 writes them to
    /// `model_call_log`.
    pub attempts: Vec<Attempt>,
}

/// The model client and the T4 knob of the running worker.
#[derive(Debug)]
pub struct DiagnosisJob {
    client: Client,
    calls_per_session: u32,
    swept: Option<Instant>,
}

impl DiagnosisJob {
    /// Build the job around a client.
    #[must_use]
    pub fn new(client: Client, calls_per_session: u32) -> Self {
        Self {
            client,
            calls_per_session,
            swept: None,
        }
    }

    /// Read the T4 call cap from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerError::Config`] when the value is not a whole number. A
    /// silent fallback to the default hides an operator mistake.
    pub fn calls_per_session_from_env() -> Result<u32, WorkerError> {
        let raw = std::env::var(CALLS_PER_SESSION_VAR).unwrap_or_default();
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(DEFAULT_CALLS_PER_SESSION);
        }
        raw.parse().map_err(|_| {
            WorkerError::Config(format!(
                "{CALLS_PER_SESSION_VAR} must be a whole number, not {raw:?}"
            ))
        })
    }

    /// Has the sweep waited out [`SWEEP_CADENCE`]? Record the run if it has.
    fn sweep_due(&mut self, now: Instant) -> bool {
        let due = self
            .swept
            .is_none_or(|last| now.duration_since(last) >= SWEEP_CADENCE);
        if due {
            self.swept = Some(now);
        }
        due
    }
}

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
async fn settle(
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

/// Keep only the tags of [`MODEL_ERROR_TAGS`], in the order the model gave them.
///
/// The 1.0 lesson (`prompts.py:529-536`): a tag the prompt invites and the filter
/// lacks is dropped silently, and the diagnosis is lost with no error anywhere.
/// [`system_prompt`] therefore renders its vocabulary from this same list, so the
/// prompt and the filter are one statement.
#[must_use]
pub fn filter_tags(tags: &Value) -> Vec<String> {
    let Some(list) = tags.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(Value::as_str)
        .filter(|tag| MODEL_ERROR_TAGS.contains(tag))
        .map(str::to_owned)
        .collect()
}

/// The system message (spec section 6.3).
///
/// It is 1.0's `GRADE_SYSTEM` minus what 2.0 already knows: the `correct`
/// paragraph, the timing paragraph, and the `work_quality` paragraphs. All three
/// are the VERDICT, and the verdict is decided locally before this call exists
/// (A3, D-M5-2). The mandatory unaided re-solve paragraph stays, and the
/// vocabulary is rendered from [`MODEL_ERROR_TAGS`].
#[must_use]
pub fn system_prompt() -> String {
    format!(
        "You are the grader for Cadus. The server already decided that the answer is WRONG. \
Name the misconception and write the diagnosis with the {TOOL_NAME} tool. Be honest and \
structural.\n\n\
SHOWN WORK IS OPTIONAL, and its absence is NOT a defect. The interface labels the working \
field 'optional'. Judge method only from work that IS shown. When no work is shown, judge on \
the answer alone and do NOT tag 'incomplete' merely because the field is empty.\n\n\
'error_tags' come ONLY from this controlled vocabulary: {}. Use [] when you cannot name the \
error. Do not invent tags; a tag outside this list is dropped.\n\n\
'prose' is 1-3 sentences, brisk and encouraging. Praise the specific STRATEGY or process the \
learner used, never raw ability. Put any math in $...$ LaTeX.\n\n\
Mandatory unaided re-solve (pp. 427, 431): a miss is NOT the end of the task. In 'prose', have \
the learner study the worked solution and then re-solve the ORIGINAL problem THEMSELVES, \
unaided and from memory, before moving on.\n\n\
'confidence' is 'high' when you can name the misconception and 'low' when you are guessing. A \
low-confidence diagnosis is stored and its tags are not shown.",
        MODEL_ERROR_TAGS.join(", ")
    )
}

/// The user message (spec section 6.3).
#[must_use]
pub fn user_message(payload: &JobPayload) -> String {
    let work = payload.work.as_deref().unwrap_or("(none provided)");
    format!(
        "Problem: {}\n\
Correct final answer (reference): {}\n\
Answer kind: {}\n\
Learner's answer: '{}'\n\
Learner's shown work: {work}\n\
The answer is WRONG; the server decided that. Do not restate the verdict.\n\
Name the misconception and write the diagnosis via the {TOOL_NAME} tool.",
        payload.problem, payload.expected, payload.answer_kind, payload.given_answer
    )
}

/// The forced tool of spec section 6.3, `additionalProperties: false`.
#[must_use]
pub fn tool_spec() -> ToolSpec {
    ToolSpec {
        name: TOOL_NAME.to_owned(),
        description: "Report the misconception behind one wrong answer.".to_owned(),
        parameters: json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["error_tags", "prose"],
            "properties": {
                "error_tags": {
                    "type": "array",
                    "items": {"type": "string", "enum": MODEL_ERROR_TAGS},
                },
                "prose": {"type": "string"},
                "confidence": {"type": "string", "enum": ["high", "low"]},
            },
        }),
    }
}

/// Turn the model's arguments into the document `diagnosis_jobs.result` holds.
///
/// A low-confidence diagnosis is STORED and its tags are NOT shown (spec section
/// 6.3): `error_tags` goes out empty and the model's own list stays under
/// `withheld_error_tags`, where an operator reads it and no learner does.
#[must_use]
pub fn result_document(arguments: &Value, model_id: &str) -> Value {
    let tags = filter_tags(arguments.get("error_tags").unwrap_or(&Value::Null));
    let low = arguments.get("confidence").and_then(Value::as_str) == Some("low");
    let prose = arguments
        .get("prose")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut document = json!({
        "v": RESULT_VERSION,
        "error_tags": if low { Vec::new() } else { tags.clone() },
        "prose": prose,
        "model_id": model_id,
        "confidence": if low { "low" } else { "high" },
    });
    if low && let Some(map) = document.as_object_mut() {
        map.insert("withheld_error_tags".to_owned(), json!(tags));
    }
    document
}

/// Run one pass of the queue (spec section 6).
///
/// # Errors
///
/// Returns [`WorkerError::Store`] or [`WorkerError::Db`] when the database
/// refuses a statement. A model failure is never an error of this function: it
/// ends in [`Outcome::Retry`] or [`Outcome::Failed`], because the queue's own
/// recovery is the answer to it.
pub async fn run_once(db: &Db, job: &mut DiagnosisJob) -> Result<Report, WorkerError> {
    if job.sweep_due(Instant::now()) {
        let (reset, dead) = sweep(db).await?;
        if reset > 0 || dead > 0 {
            tracing::info!(reset, dead, "diagnosis: the sweep ran");
        }
    }

    let Some(claimed) = claim(db).await? else {
        return Ok(Report {
            outcome: Outcome::Idle,
            job_id: None,
            attempts: Vec::new(),
        });
    };

    let idle = |outcome| Report {
        outcome,
        job_id: Some(claimed.id),
        attempts: Vec::new(),
    };

    // A payload the worker cannot read is reproduced by every retry, so it
    // dead-letters at once instead of spending two more claims on it.
    let payload: JobPayload = match serde_json::from_value(claimed.payload.clone()) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::error!(job = %claimed.id, error = %err, "diagnosis: the payload does not read");
            settle(db, &claimed, JOB_FAILED, None).await?;
            return Ok(idle(Outcome::Failed));
        }
    };
    if payload.v != PAYLOAD_VERSION {
        tracing::error!(job = %claimed.id, version = payload.v, "diagnosis: unknown payload version");
        settle(db, &claimed, JOB_FAILED, None).await?;
        return Ok(idle(Outcome::Failed));
    }

    // T4: a configured cap refuses the call and still leaves the learner with
    // the verdict and the stock re-solve instruction.
    if job.calls_per_session > 0
        && let Some(session) = payload.session.as_deref()
    {
        let spent = calls_this_session(db, &claimed, session).await?;
        if spent >= i64::from(job.calls_per_session) {
            tracing::info!(job = %claimed.id, spent, cap = job.calls_per_session,
                           "diagnosis: the session cap refused the call");
            settle(db, &claimed, JOB_CAPPED, None).await?;
            return Ok(idle(Outcome::Capped));
        }
    }

    let request = ChatRequest {
        system: system_prompt(),
        user: user_message(&payload),
        tool: tool_spec(),
    };
    let call = job.client.call(&request).await;
    let attempts = call.attempts;

    match call.result {
        Ok(arguments) => {
            let document = result_document(&arguments, &job.client.config().model);
            settle(db, &claimed, JOB_DONE, Some(&document)).await?;
            Ok(Report {
                outcome: Outcome::Done,
                job_id: Some(claimed.id),
                attempts,
            })
        }
        Err(err) => {
            let outcome = fail(db, &claimed, &err.to_string()).await?;
            Ok(Report {
                outcome,
                job_id: Some(claimed.id),
                attempts,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MODEL_ERROR_TAGS, filter_tags, result_document, system_prompt};
    use serde_json::json;

    /// The vocabulary is the 11 tags of spec section 5.3, in 1.0 order.
    ///
    /// `blank-answer` is not one of them: the server stamps it, so the prompt
    /// never invites the model to claim that an answer was blank.
    #[test]
    fn the_model_vocabulary_is_the_eleven_tags() {
        assert_eq!(
            MODEL_ERROR_TAGS,
            [
                "sign-error",
                "arithmetic-slip",
                "algebra-slip",
                "wrong-method",
                "formula-recall",
                "misread-problem",
                "incomplete",
                "notation",
                "units",
                "timing-unreliable",
                "blowoff",
            ]
        );
    }

    /// A tag outside the vocabulary is dropped and the rest keep their order.
    #[test]
    fn a_tag_outside_the_vocabulary_is_dropped() {
        let tags = json!(["sign-error", "carelessness", "blank-answer", "units", 7]);
        assert_eq!(filter_tags(&tags), vec!["sign-error", "units"]);
        assert!(filter_tags(&json!("sign-error")).is_empty());
        assert!(filter_tags(&json!(null)).is_empty());
    }

    /// The prompt renders its vocabulary from the list the filter uses, so a tag
    /// the prompt invites can never be one the filter lacks.
    #[test]
    fn the_prompt_names_every_tag_the_filter_keeps() {
        let prompt = system_prompt();
        for tag in MODEL_ERROR_TAGS {
            assert!(prompt.contains(tag), "the prompt does not name {tag}");
        }
        assert!(!prompt.contains("blank-answer"));
    }

    /// A low-confidence diagnosis is stored and its tags are not shown.
    #[test]
    fn a_low_confidence_diagnosis_shows_no_tags() {
        let document = result_document(
            &json!({"error_tags": ["sign-error"], "prose": "Maybe the sign.",
                    "confidence": "low"}),
            "deepseek/deepseek-v4-pro",
        );
        assert_eq!(document["error_tags"], json!([]));
        assert_eq!(document["withheld_error_tags"], json!(["sign-error"]));
        assert_eq!(document["prose"], json!("Maybe the sign."));
        assert_eq!(document["model_id"], json!("deepseek/deepseek-v4-pro"));
    }

    /// A high-confidence diagnosis shows the filtered tags.
    #[test]
    fn a_high_confidence_diagnosis_shows_the_filtered_tags() {
        let document = result_document(
            &json!({"error_tags": ["sign-error", "made-up"], "prose": "Watch the sign."}),
            "qwen3.6",
        );
        assert_eq!(document["v"], json!(1));
        assert_eq!(document["error_tags"], json!(["sign-error"]));
        assert_eq!(document["confidence"], json!("high"));
        assert_eq!(document.get("withheld_error_tags"), None);
    }
}
