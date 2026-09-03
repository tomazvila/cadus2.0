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

mod prompt;
mod queue;

use std::time::{Duration, Instant};

use cadus_model_client::{Attempt, ChatRequest, Client};
use cadus_store::Db;
use cadus_store::diagnosis::{JOB_CAPPED, JOB_DONE, JOB_FAILED, JobPayload, PAYLOAD_VERSION};
use serde_json::Value;
use sqlx::types::Uuid;

pub use prompt::{filter_tags, result_document, system_prompt, tool_spec, user_message};
pub use queue::{calls_this_session, claim, fail, sweep};

use crate::WorkerError;
use crate::model_log::{self, CallRecord, PURPOSE_DIAGNOSIS};
use queue::settle;

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
        parse_calls_per_session(&std::env::var(CALLS_PER_SESSION_VAR).unwrap_or_default())
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

/// Parse one value of [`CALLS_PER_SESSION_VAR`].
///
/// An empty value gives [`DEFAULT_CALLS_PER_SESSION`]; a value that is not a
/// whole number is a configuration error.
fn parse_calls_per_session(raw: &str) -> Result<u32, WorkerError> {
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

/// A report of one claimed row that made no HTTP attempt.
fn quiet(outcome: Outcome, job_id: Uuid) -> Report {
    Report {
        outcome,
        job_id: Some(job_id),
        attempts: Vec::new(),
    }
}

/// Run the sweep when [`SWEEP_CADENCE`] has passed since the last one.
async fn sweep_if_due(db: &Db, job: &mut DiagnosisJob) -> Result<(), WorkerError> {
    if !job.sweep_due(Instant::now()) {
        return Ok(());
    }
    let (reset, dead) = sweep(db).await?;
    if reset > 0 || dead > 0 {
        tracing::info!(reset, dead, "diagnosis: the sweep ran");
    }
    Ok(())
}

/// Read the payload of one claimed row, or dead-letter the row.
///
/// A payload the worker cannot read is reproduced by every retry, so it
/// dead-letters at once instead of spending two more claims on it. So does a
/// payload of a version this worker does not know.
async fn read_payload(db: &Db, claimed: &Job) -> Result<Option<JobPayload>, WorkerError> {
    let payload: JobPayload = match serde_json::from_value(claimed.payload.clone()) {
        Ok(payload) => payload,
        Err(err) => {
            tracing::error!(job = %claimed.id, error = %err, "diagnosis: the payload does not read");
            settle(db, claimed, JOB_FAILED, None).await?;
            return Ok(None);
        }
    };
    if payload.v != PAYLOAD_VERSION {
        tracing::error!(job = %claimed.id, version = payload.v, "diagnosis: unknown payload version");
        settle(db, claimed, JOB_FAILED, None).await?;
        return Ok(None);
    }
    Ok(Some(payload))
}

/// T4: refuse the call when the session spent its cap, and say so.
///
/// A configured cap refuses the call and still leaves the learner with the
/// verdict and the stock re-solve instruction. A cap of 0 refuses nothing, and
/// so does a job with no session.
async fn capped(
    db: &Db,
    job: &DiagnosisJob,
    claimed: &Job,
    payload: &JobPayload,
) -> Result<bool, WorkerError> {
    let (true, Some(session)) = (job.calls_per_session > 0, payload.session.as_deref()) else {
        return Ok(false);
    };
    let spent = calls_this_session(db, claimed, session).await?;
    if spent < i64::from(job.calls_per_session) {
        return Ok(false);
    }
    tracing::info!(job = %claimed.id, spent, cap = job.calls_per_session,
                   "diagnosis: the session cap refused the call");
    settle(db, claimed, JOB_CAPPED, None).await?;
    Ok(true)
}

/// Make the model call, and put the bill of every HTTP attempt in the ledger
/// BEFORE the row settles (T6).
///
/// One row per HTTP attempt (spec section 7). A ledger write that fails is
/// logged with the record it did not write and stops nothing: the learner keeps
/// the diagnosis that is already paid for.
async fn call_model(
    db: &Db,
    client: &Client,
    claimed: &Job,
    payload: &JobPayload,
) -> (Vec<Attempt>, Result<Value, cadus_model_client::ModelError>) {
    let request = ChatRequest {
        system: system_prompt(),
        user: user_message(payload),
        tool: tool_spec(),
    };
    let call = client.call(&request).await;
    let record = CallRecord {
        purpose: PURPOSE_DIAGNOSIS,
        user_id: Some(claimed.user_id),
        session_id: payload.session.as_deref(),
    };
    if let Err(err) = model_log::write(db, &record, &call.attempts).await {
        tracing::error!(job = %claimed.id, error = %err, attempts = ?call.attempts,
                        "diagnosis: the model-call ledger did not write");
    }
    (call.attempts, call.result)
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
    sweep_if_due(db, job).await?;

    let Some(claimed) = claim(db).await? else {
        return Ok(Report {
            outcome: Outcome::Idle,
            job_id: None,
            attempts: Vec::new(),
        });
    };
    let Some(payload) = read_payload(db, &claimed).await? else {
        return Ok(quiet(Outcome::Failed, claimed.id));
    };
    if capped(db, job, &claimed, &payload).await? {
        return Ok(quiet(Outcome::Capped, claimed.id));
    }

    let (attempts, result) = call_model(db, &job.client, &claimed, &payload).await;
    let outcome = match result {
        Ok(arguments) => {
            let document = result_document(&arguments, &job.client.config().model);
            settle(db, &claimed, JOB_DONE, Some(&document)).await?;
            Outcome::Done
        }
        Err(err) => fail(db, &claimed, &err.to_string()).await?,
    };
    Ok(Report {
        outcome,
        job_id: Some(claimed.id),
        attempts,
    })
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_CALLS_PER_SESSION, DiagnosisJob, SWEEP_CADENCE, parse_calls_per_session};
    use cadus_model_client::{Client, ModelConfig};
    use std::time::{Duration, Instant};

    /// The T4 knob: an empty value is the default, a number is the number, and
    /// a word is a configuration error that names the variable.
    #[test]
    fn the_call_cap_reads_the_three_shapes() {
        assert_eq!(parse_calls_per_session("").unwrap(), 0);
        assert_eq!(
            parse_calls_per_session("  ").unwrap(),
            DEFAULT_CALLS_PER_SESSION
        );
        assert_eq!(parse_calls_per_session(" 3 ").unwrap(), 3);
        assert_eq!(
            parse_calls_per_session("many").unwrap_err().to_string(),
            "configuration error: DIAGNOSIS_CALLS_PER_SESSION must be a whole number, not \"many\""
        );
    }

    /// The sweep runs on the first pass, then once per cadence.
    #[test]
    fn the_sweep_is_due_once_per_cadence() {
        let cfg = ModelConfig {
            base_url: "http://127.0.0.1:1/v1".to_owned(),
            api_key: "test-key".to_owned(),
            model: "qwen3.6".to_owned(),
            output_tokens: 600,
            reasoning_max_tokens: 600,
            provider_order: Vec::new(),
            timeout: Duration::from_secs(5),
        };
        let mut job = DiagnosisJob::new(Client::new(cfg).unwrap(), 0);
        let start = Instant::now();
        assert!(job.sweep_due(start));
        assert!(!job.sweep_due(start + Duration::from_secs(1)));
        assert!(!job.sweep_due(start + SWEEP_CADENCE - Duration::from_secs(1)));
        assert!(job.sweep_due(start + SWEEP_CADENCE));
        assert!(!job.sweep_due(start + SWEEP_CADENCE + Duration::from_secs(1)));
    }
}
