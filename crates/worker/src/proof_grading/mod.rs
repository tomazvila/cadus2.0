//! The Amendment K proof-grading job (design point 6): background grading of
//! a written proof or a free explanation the deterministic checker left
//! UNGRADED.
//!
//! The shape of the equivalence job, with a hosted model and a checklist:
//!
//! 1. claim the oldest pending `proof_grading_jobs` row (`SKIP LOCKED`);
//! 2. read the payload; an unreadable one dead-letters at once;
//! 3. the per-learner daily cap (20, Amendment K) settles the row `capped`;
//! 4. ask the hosted model one forced-tool question ([`prompt`]), and put the
//!    bill of every HTTP attempt in `model_call_log` (T6) before the settle;
//! 5. CODE decides the verdict from the checks ([`prompt::verdict_of`]);
//! 6. land it in ONE transaction: a pass appends a `regraded` event (outcome
//!    correct, the owner's decision: a passed proof counts for mastery like a
//!    correct short answer) and refolds the learner model; both verdicts
//!    settle the row `done` with the result document the learner reads.
//!
//! A reply that does not parse, or a model that does not answer, returns the
//! row to `pending` until its [`MAX_ATTEMPTS`]th claim, then it fails; the
//! attempt stays ungraded and the admin recovery path still holds it.

pub mod prompt;

use std::sync::Arc;
use std::time::Duration;

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{
    AttemptOutcome, Event, Regraded, RegradedAttempt, SchemaVersion, Slug, Timestamp, WorkQuality,
};
use cadus_model_client::{Attempt, Client};
use cadus_store::proof_grading::{
    self, Claimed, Grading, JOB_CAPPED, JOB_DONE, JOB_FAILED, JOB_PENDING, JobPayload,
    PAYLOAD_VERSION,
};
use cadus_store::state::{append_event, project_and_save};
use cadus_store::{Db, bounded};
use serde_json::json;
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;
use sqlx::{Postgres, Transaction};

use crate::WorkerError;
use crate::model_log::{self, CallRecord, PURPOSE_PROOF_GRADING};

/// The per-learner daily cap of Amendment K: 20 background gradings a day.
pub const DAILY_CAP: i64 = 20;

/// How many claims (including the failing one) a row may use before it
/// dead-letters.
pub const MAX_ATTEMPTS: i32 = 3;

/// The default model of the job (`PROOF_GRADER_MODEL`).
pub const DEFAULT_MODEL: &str = "deepseek/deepseek-v4-pro";

/// The default output bound of one call (`PROOF_GRADER_OUTPUT_TOKENS`). A
/// checklist of up to 13 checks with quotes and the feedback fits in well
/// under half of it; the rest is room for the reasoning budget.
pub const DEFAULT_OUTPUT_TOKENS: u32 = 8000;

/// The default reasoning bound of one call
/// (`PROOF_GRADER_REASONING_MAX_TOKENS`).
pub const DEFAULT_REASONING_MAX_TOKENS: u32 = 4000;

/// The client-side bound of one HTTP attempt.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(180);

/// The lease of a claimed row: past two attempts at [`CALL_TIMEOUT`] and the
/// backoff; a row stuck `running` past it is reclaimed.
const LEASE: Duration = Duration::from_secs(900);

/// The client-side bound of every statement of this module.
const STATEMENT_BOUND_MS: u64 = 30_000;

/// The reason the fold's correction names.
pub const FOLD_REASON: &str = "the background proof grader passed the written proof";

/// The grader note the correction carries.
pub const FOLD_NOTE: &str = "proof grading (background model)";

/// The configured job: the model client and the curriculum the fold reads.
#[derive(Debug)]
pub struct ProofGradingJob {
    client: Client,
    curriculum: Arc<Curriculum>,
}

impl ProofGradingJob {
    /// Build the job over the deployment's curriculum.
    #[must_use]
    pub fn new(client: Client, curriculum: Arc<Curriculum>) -> Self {
        Self { client, curriculum }
    }

    /// The model id this job asks.
    #[must_use]
    pub fn model(&self) -> &str {
        &self.client.config().model
    }
}

/// The report of one pass.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// What the pass ended in.
    pub outcome: Outcome,
    /// The job id, when the pass claimed one.
    pub job_id: Option<Uuid>,
    /// The HTTP attempt records of the model call.
    pub attempts: Vec<Attempt>,
}

/// How one pass ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The queue held nothing.
    Idle,
    /// The proof passed; the fold appended the correction.
    Passed,
    /// The proof needs revision; the attempt stays ungraded.
    NeedsRevision,
    /// The daily cap refused the call.
    Capped,
    /// The call or its reply failed; the row returned to the queue or failed.
    Failed,
}

/// Grade one payload with one model call: the forced tool, then the reader
/// and the verdict rule. The caller bills the attempts.
///
/// The live quality check calls this with the real prompt and the real model.
pub async fn grade(
    client: &Client,
    payload: &JobPayload,
) -> (Vec<Attempt>, Result<Grading, String>) {
    let call = client.call(&prompt::request(payload)).await;
    let model = client.config().model.clone();
    let result = match call.result {
        Ok(arguments) => prompt::grading_of(&arguments, &model),
        Err(err) => Err(err.to_string()),
    };
    (call.attempts, result)
}

/// A configured `Db` handle with the client-side bound of this module.
fn handle(db: &Db) -> Db {
    Db::new(db.pool().clone(), STATEMENT_BOUND_MS)
}

/// Reclaim stale leases and dead-letter the rows that used their attempts.
async fn sweep(db: &Db) -> Result<(u64, u64), WorkerError> {
    let lease_secs = f64::from(u32::try_from(LEASE.as_secs()).unwrap_or(u32::MAX));
    let dead = sqlx::query!(
        r#"
        UPDATE proof_grading_jobs
           SET status = $1, finished_at = now()
         WHERE status = $2 AND attempts >= $3
           AND claimed_at < now() - make_interval(secs => $4)
        "#,
        JOB_FAILED,
        "running",
        MAX_ATTEMPTS,
        lease_secs,
    )
    .execute(db.pool());
    let dead = bounded(db, dead).await?.rows_affected();
    let reset = sqlx::query!(
        r#"
        UPDATE proof_grading_jobs
           SET status = $1, claimed_at = NULL
         WHERE status = $2 AND claimed_at < now() - make_interval(secs => $3)
        "#,
        JOB_PENDING,
        "running",
        lease_secs,
    )
    .execute(db.pool());
    let reset = bounded(db, reset).await?.rows_affected();
    Ok((reset, dead))
}

/// Run one pass of the queue.
///
/// # Errors
///
/// Returns [`WorkerError`] when a database statement fails. A model failure is
/// not an error: the row returns to the queue (or fails on its last claim) and
/// the pass reports [`Outcome::Failed`].
pub async fn run_once(db: &Db, job: &ProofGradingJob) -> Result<Report, WorkerError> {
    let db = &handle(db);
    let (reset, dead) = sweep(db).await?;
    if reset > 0 || dead > 0 {
        tracing::info!(reset, dead, "proof grading: the sweep ran");
    }
    let Some(claimed) = proof_grading::claim(db).await? else {
        return Ok(quiet(Outcome::Idle, None));
    };
    // An unreadable payload is reproduced by every retry: dead-letter at once.
    let payload = match serde_json::from_value::<JobPayload>(claimed.payload.clone()) {
        Ok(payload) if payload.v == PAYLOAD_VERSION => payload,
        _ => {
            tracing::error!(job = %claimed.id, "proof grading: the payload does not read");
            settle_failed(db, &claimed, "the payload does not read").await?;
            return Ok(quiet(Outcome::Failed, Some(claimed.id)));
        }
    };

    // The per-learner daily cap counts the rows the learner wrote today, this
    // one included. Past the cap the attempt stays ungraded (Amendment K: it
    // never counts for mastery) and waits for the human recovery path.
    let spent = proof_grading::jobs_today(db.pool(), claimed.user_id).await?;
    if spent > DAILY_CAP {
        tracing::info!(job = %claimed.id, spent, cap = DAILY_CAP, "proof grading: the daily cap refused the call");
        proof_grading::settle(db.pool(), claimed.id, JOB_CAPPED, None).await?;
        return Ok(quiet(Outcome::Capped, Some(claimed.id)));
    }

    let (attempts, result) = grade(&job.client, &payload).await;
    let record = CallRecord {
        purpose: PURPOSE_PROOF_GRADING,
        user_id: Some(claimed.user_id),
        session_id: None,
    };
    if let Err(err) = model_log::write(db, &record, &attempts).await {
        tracing::error!(job = %claimed.id, error = %err, "proof grading: the model-call ledger did not write");
    }
    let grading = match result {
        Ok(grading) => grading,
        Err(reason) => {
            tracing::warn!(job = %claimed.id, model = job.model(), %reason, "proof grading: no usable grading");
            if claimed.attempts >= MAX_ATTEMPTS {
                settle_failed(db, &claimed, &reason).await?;
            } else {
                proof_grading::settle(db.pool(), claimed.id, JOB_PENDING, None).await?;
            }
            return Ok(Report {
                outcome: Outcome::Failed,
                job_id: Some(claimed.id),
                attempts,
            });
        }
    };
    let outcome = if grading.passed() {
        Outcome::Passed
    } else {
        Outcome::NeedsRevision
    };
    land(db, job, &claimed, &grading).await?;
    Ok(Report {
        outcome,
        job_id: Some(claimed.id),
        attempts,
    })
}

/// Land one grading: the fold of a pass and the settle, in ONE transaction.
///
/// The row never reads done while the learner's log still holds the
/// ungraded verdict of a passed proof: a fold that fails rolls the settle
/// back, and the sweep of a later pass reclaims the row.
async fn land(
    db: &Db,
    job: &ProofGradingJob,
    claimed: &Claimed,
    grading: &Grading,
) -> Result<(), WorkerError> {
    let mut tx: Transaction<'static, Postgres> = db.pool().begin().await?;
    if let Err(err) = land_inner(&mut tx, job, claimed, grading).await {
        let _ = tx.rollback().await;
        return Err(err);
    }
    tx.commit().await.map_err(WorkerError::from)
}

/// The statements of one [`land`], on the caller's transaction.
async fn land_inner(
    tx: &mut Transaction<'static, Postgres>,
    job: &ProofGradingJob,
    claimed: &Claimed,
    grading: &Grading,
) -> Result<(), WorkerError> {
    let document = serde_json::to_value(grading).unwrap_or_else(|_| json!({}));
    if grading.passed() {
        fold_pass(tx, job, claimed).await?;
    }
    proof_grading::settle(&mut **tx, claimed.id, JOB_DONE, Some(&document)).await?;
    Ok(())
}

/// Append the `regraded` correction of a passed proof and refold the model.
///
/// No fold when the attempt is gone (a reset) or when a correction of the
/// attempt already stands (a human regrade wins: the worker never supersedes
/// it).
async fn fold_pass(
    tx: &mut Transaction<'static, Postgres>,
    job: &ProofGradingJob,
    claimed: &Claimed,
) -> Result<(), WorkerError> {
    let target = sqlx::query!(
        r#"
        SELECT payload->>'task_id' AS "task_id!",
               payload->>'topic'   AS "topic!",
               EXISTS (
                 SELECT 1 FROM events r
                  WHERE r.user_id = $1 AND r.type = 'regraded'
                    AND r.payload->'attempts' @> jsonb_build_array(jsonb_build_object('attempt_id', $2::text))
               ) AS "corrected!"
          FROM events
         WHERE user_id = $1 AND attempt_id = $2 AND type = 'attempt'
         ORDER BY seq
         LIMIT 1
        "#,
        claimed.user_id,
        claimed.attempt_id,
    )
    .fetch_optional(&mut **tx)
    .await?;
    let Some(target) = target else {
        tracing::info!(job = %claimed.id, "proof grading: the attempt is not in the log; no fold");
        return Ok(());
    };
    if target.corrected {
        tracing::info!(job = %claimed.id, "proof grading: a correction already stands; no fold");
        return Ok(());
    }
    let Ok(topic) = Slug::new(&target.topic) else {
        return Err(WorkerError::Config(format!(
            "proof grading: the attempt topic {:?} is not a slug",
            target.topic
        )));
    };
    let now = Timestamp::from_micros(Utc::now().timestamp_micros());
    let event = Event::Regraded(Regraded {
        ts: now,
        session: None,
        v: SchemaVersion::current(),
        task_id: target.task_id,
        topic,
        attempts: vec![RegradedAttempt {
            attempt_id: claimed.attempt_id.clone(),
            outcome: Some(AttemptOutcome::Correct),
            work_quality: WorkQuality::NearlyPerfect,
            error_tags: Vec::new(),
            grader_note: Some(FOLD_NOTE.to_owned()),
        }],
        quality_tier: None,
        xp: None,
        reason: FOLD_REASON.to_owned(),
    });
    append_event(tx, claimed.user_id, &event, None).await?;
    let cfg = Config::default();
    let input = cadus_core::projector::ProjectionInput::new(&job.curriculum, &cfg, now);
    project_and_save(tx, claimed.user_id, &input, None).await?;
    Ok(())
}

/// A report with no HTTP attempts.
fn quiet(outcome: Outcome, job_id: Option<Uuid>) -> Report {
    Report {
        outcome,
        job_id,
        attempts: Vec::new(),
    }
}

/// Settle one row `failed` with the reason in its result.
async fn settle_failed(db: &Db, claimed: &Claimed, reason: &str) -> Result<(), WorkerError> {
    let document = json!({ "error": reason });
    proof_grading::settle(db.pool(), claimed.id, JOB_FAILED, Some(&document))
        .await
        .map_err(WorkerError::from)
}
