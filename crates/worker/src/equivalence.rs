//! The Amendment K equivalence job (steer note 114, decided by the owner).
//!
//! One claim, one local-model call, one end state — the A4 shape of the
//! diagnosis queue, with the fold between the model and the settle:
//!
//! 1. claim the oldest pending `equivalence_jobs` row (`SKIP LOCKED`);
//! 2. read the payload; an unreadable one dead-letters at once;
//! 3. the per-learner daily cap (200, note 114) refuses the call;
//! 4. ask the local model the one equivalence question, and put the bill of
//!    the call in `model_call_log` (T6) before the row settles;
//! 5. land the verdict in ONE transaction: the cache write (a repeat answers
//!    at once), the `regraded` event that folds the correct verdict into the
//!    learner's log for an EQUIVALENT answer (C2: the log is append-only;
//!    nothing is edited), the model refold, and the settle.
//!
//! A model failure is never an error of the process: it ends in `failed`
//! (after [`MAX_ATTEMPTS`] claims) and the learner keeps the deterministic
//! verdict. The OpenRouter fallback stays OFF: this job reaches the endpoint
//! the environment names, and nothing else.
//!
//! The fold runs as `cadus_admin` (BYPASSRLS): the worker reads the attempt
//! row, appends the correction, and refolds the model. The refold uses the
//! deployment's scheduler constants (the default config), so the refold's
//! config hash matches what the web tier builds and the next request resumes
//! from the cached model.

use std::sync::Arc;

use cadus_core::config::Config;
use cadus_core::curriculum::Curriculum;
use cadus_core::event::{
    AttemptOutcome, Event, Regraded, RegradedAttempt, SchemaVersion, Slug, Timestamp, WorkQuality,
};
use cadus_model_client::{Attempt, EquivalenceClient, Question, Usage};
use cadus_store::equivalence::{
    self, JOB_CAPPED, JOB_DONE, JOB_FAILED, JOB_PENDING, JobPayload, PAYLOAD_VERSION, Verdict,
};
use cadus_store::state::{append_event, project_and_save};
use cadus_store::{Db, bounded};
use serde_json::json;
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;
use sqlx::{Postgres, Transaction};

use crate::WorkerError;
use crate::model_log::{self, CallRecord, PURPOSE_EQUIVALENCE};

/// The per-learner daily cap of the note-114 design: 200 model verdicts a day.
///
/// The model is free; the cap bounds abuse.
pub const DAILY_CAP: i64 = 200;

/// How many claims (including the failing one) a row may use before it
/// dead-letters. The diagnosis queue gives three; this job gives the same.
pub const MAX_ATTEMPTS: i32 = 3;

/// The lease of a claimed row. The call budget is 60 s, so 5 minutes is past
/// every honest run; a row stuck `running` past the lease is reclaimed.
const LEASE: std::time::Duration = std::time::Duration::from_secs(300);

/// The client-side bound of every statement of this module.
const STATEMENT_BOUND_MS: u64 = 30_000;

/// The budget of one equivalence call.
///
/// The owner set no number; the research doc measures the local model at
/// seconds to a minute. A call past this bound failed, and the deterministic
/// verdict stands.
const CALL_BUDGET: std::time::Duration = std::time::Duration::from_secs(60);

/// The one reason the fold's correction names.
pub const FOLD_REASON: &str = "the local model accepted the answer as equivalent";

/// The one grader note the correction carries.
pub const FOLD_NOTE: &str = "equivalence (background model)";

/// The configured job: the model client, and the curriculum the fold reads.
#[derive(Debug)]
pub struct EquivalenceJob {
    client: EquivalenceClient,
    fallback: Option<EquivalenceClient>,
    curriculum: Arc<Curriculum>,
}

impl EquivalenceJob {
    /// Build the job over the deployment's curriculum.
    #[must_use]
    pub fn new(client: EquivalenceClient, curriculum: Arc<Curriculum>) -> Self {
        Self {
            client,
            fallback: None,
            curriculum,
        }
    }

    /// Ask `fallback` when the primary model does not answer (the owner
    /// approved a hosted fallback on 4 Oct, superseding note 114 point 5).
    #[must_use]
    pub fn with_fallback(mut self, fallback: EquivalenceClient) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// The model id this job asks.
    #[must_use]
    pub fn model(&self) -> &str {
        self.client.model()
    }
}

/// The report of one pass.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// What the pass ended in.
    pub outcome: Outcome,
    /// The job id, when the pass claimed one.
    pub job_id: Option<Uuid>,
    /// The HTTP attempt records of the model call, for the tests.
    pub attempts: Vec<Attempt>,
}

/// How one pass ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The queue held nothing.
    Idle,
    /// The model accepted the answer; the fold appended the correction.
    Accepted,
    /// The model refused the answer; the deterministic wrong stands.
    Refused,
    /// The daily cap refused the call.
    Capped,
    /// The row failed (an unreadable payload, or the last of its attempts).
    Failed,
}

/// A configured `Db` handle with the client-side bound of this module.
fn handle(db: &Db) -> Db {
    Db::new(db.pool().clone(), STATEMENT_BOUND_MS)
}

/// Reclaim stale leases and dead-letter the rows that used their attempts.
///
/// The two statements are the recovery story of the queue: a worker that died
/// mid-call leaves a `running` row nobody holds, and a row that fails its
/// attempts must stop costing calls.
async fn sweep(db: &Db) -> Result<(u64, u64), WorkerError> {
    let lease_secs = f64::from(u32::try_from(LEASE.as_secs()).unwrap_or(u32::MAX));
    let dead = sqlx::query!(
        r#"
        UPDATE equivalence_jobs
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
        UPDATE equivalence_jobs
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
/// not an error: it settles the row and the pass reports [`Outcome::Failed`].
pub async fn run_once(db: &Db, job: &EquivalenceJob) -> Result<Report, WorkerError> {
    let db = &handle(db);
    let (reset, dead) = sweep(db).await?;
    if reset > 0 || dead > 0 {
        tracing::info!(reset, dead, "equivalence: the sweep ran");
    }
    let Some(claimed) = equivalence::claim(db).await.map_err(WorkerError::from)? else {
        return Ok(Report {
            outcome: Outcome::Idle,
            job_id: None,
            attempts: Vec::new(),
        });
    };
    // An unreadable payload is reproduced by every retry, so it dead-letters
    // at once instead of spending its remaining claims on it.
    let Ok(payload) = serde_json::from_value::<JobPayload>(claimed.payload.clone()) else {
        tracing::error!(job = %claimed.id, "equivalence: the payload does not read");
        settle_failed(db, &claimed, "the payload does not read").await?;
        return Ok(quiet(Outcome::Failed, claimed.id));
    };
    if payload.v != PAYLOAD_VERSION {
        tracing::error!(job = %claimed.id, version = payload.v, "equivalence: unknown payload version");
        settle_failed(db, &claimed, "the payload version is unknown").await?;
        return Ok(quiet(Outcome::Failed, claimed.id));
    }

    // The per-learner daily cap. The count reads the rows the learner wrote
    // today, this one included; past the cap the row settles `capped` and the
    // learner keeps the deterministic verdict.
    let spent = equivalence::calls_today(db.pool(), claimed.user_id)
        .await
        .map_err(WorkerError::from)?;
    if spent > DAILY_CAP {
        tracing::info!(job = %claimed.id, spent, cap = DAILY_CAP, "equivalence: the daily cap refused the call");
        equivalence::settle(db.pool(), claimed.id, JOB_CAPPED, None)
            .await
            .map_err(WorkerError::from)?;
        return Ok(quiet(Outcome::Capped, claimed.id));
    }

    // The cache first: a verdict another pass already settled answers here
    // with no model call (note 114, point 2). The fold still runs — the
    // attempt this job names may not be corrected yet.
    let verdict = match equivalence::cache_hit(
        db.pool(),
        &payload.item_digest,
        &payload.given_answer,
    )
    .await
    {
        Ok(Some(verdict)) => {
            tracing::info!(job = %claimed.id, "equivalence: the cache answered; no model call");
            Some(verdict)
        }
        Ok(None) => None,
        Err(err) => return Err(err.into()),
    };
    let (attempts, verdict) = match verdict {
        Some(verdict) => (Vec::new(), Some(verdict)),
        None => {
            let (attempts, verdict) = ask(db, job, &claimed, &payload).await?;
            (attempts, verdict)
        }
    };
    let Some(verdict) = verdict else {
        // The call failed. A row on its last attempt dead-letters; the rest
        // return to `pending` and a later pass reclaims them.
        if claimed.attempts >= MAX_ATTEMPTS {
            settle_failed(db, &claimed, "the model call failed").await?;
        } else {
            equivalence::settle(db.pool(), claimed.id, JOB_PENDING, None)
                .await
                .map_err(WorkerError::from)?;
        }
        return Ok(quiet(Outcome::Failed, claimed.id));
    };

    let outcome = if verdict.equivalent {
        Outcome::Accepted
    } else {
        Outcome::Refused
    };
    land(db, job, &claimed, &payload, &verdict).await?;
    Ok(Report {
        outcome,
        job_id: Some(claimed.id),
        attempts,
    })
}

/// Ask the model one question, and put the bill of the call in the ledger
/// BEFORE the row settles (T6). `None` means the call failed.
async fn ask(
    db: &Db,
    job: &EquivalenceJob,
    claimed: &equivalence::Claimed,
    payload: &JobPayload,
) -> Result<(Vec<Attempt>, Option<Verdict>), WorkerError> {
    let question = Question {
        problem: &payload.problem,
        key: &payload.expected,
        contract: payload.answer_contract.as_deref(),
        learner: &payload.given_answer,
    };
    let mut attempts = Vec::new();
    for (index, client) in std::iter::once(&job.client)
        .chain(job.fallback.as_ref())
        .enumerate()
    {
        let (attempt, verdict) = ask_one(db, client, claimed, &question, index).await;
        attempts.push(attempt);
        if verdict.is_some() {
            return Ok((attempts, verdict));
        }
    }
    Ok((attempts, None))
}

/// One call to one model, billed in the ledger BEFORE the row settles (T6).
async fn ask_one(
    db: &Db,
    client: &EquivalenceClient,
    claimed: &equivalence::Claimed,
    question: &Question<'_>,
    index: usize,
) -> (Attempt, Option<Verdict>) {
    let started = std::time::Instant::now();
    let answer = client.ask(question, CALL_BUDGET).await;
    let latency_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
    let attempt = |status, latency_ms, usage| Attempt {
        index: u32::try_from(index).unwrap_or(u32::MAX),
        max_tokens: cadus_model_client::equivalence::MAX_OUTPUT_TOKENS,
        status,
        latency_ms,
        usage,
        model_id: client.model().to_owned(),
        provider: None,
        request_id: None,
        cost_usd: None,
    };
    let (attempt, verdict) = match &answer {
        Ok(call) => (
            attempt(200, call.latency_ms, call.usage),
            Some(Verdict {
                equivalent: call.reply.equivalent,
                reason: call.reply.reason.clone(),
                model: client.model().to_owned(),
            }),
        ),
        Err(err) => {
            tracing::warn!(job = %claimed.id, model = client.model(), error = %err, "equivalence: the model call failed");
            (attempt(0, latency_ms, Usage::default()), None)
        }
    };
    let record = CallRecord {
        purpose: PURPOSE_EQUIVALENCE,
        user_id: Some(claimed.user_id),
        session_id: None,
    };
    if let Err(err) = model_log::write(db, &record, std::slice::from_ref(&attempt)).await {
        tracing::error!(job = %claimed.id, error = %err, "equivalence: the model-call ledger did not write");
    }
    (attempt, verdict)
}

/// Land one verdict: the cache write, the fold, and the settle, all inside
/// ONE transaction.
///
/// The row never reads done while the learner's log still says wrong: a fold
/// that fails rolls the settle back too, and the sweep of a later pass
/// reclaims the row.
async fn land(
    db: &Db,
    job: &EquivalenceJob,
    claimed: &equivalence::Claimed,
    payload: &JobPayload,
    verdict: &Verdict,
) -> Result<(), WorkerError> {
    let now = Utc::now();
    let ts = Timestamp::from_micros(now.timestamp_micros());
    let mut tx: Transaction<'static, Postgres> =
        db.pool().begin().await.map_err(WorkerError::from)?;
    if let Err(err) = land_inner(&mut tx, job, claimed, payload, verdict, ts).await {
        let _ = tx.rollback().await;
        return Err(err);
    }
    tx.commit().await.map_err(WorkerError::from)
}

/// The statements of one [`land`], on the caller's transaction.
async fn land_inner(
    tx: &mut Transaction<'static, Postgres>,
    job: &EquivalenceJob,
    claimed: &equivalence::Claimed,
    payload: &JobPayload,
    verdict: &Verdict,
    ts: Timestamp,
) -> Result<(), WorkerError> {
    // The cache write. An existing row stands: the first verdict wins, so a
    // re-check cannot flip a settled answer.
    equivalence::cache_put(
        &mut **tx,
        &payload.item_digest,
        &payload.given_answer,
        verdict,
    )
    .await?;
    let document = serde_json::to_value(verdict).unwrap_or_else(|_| json!({}));
    if !verdict.equivalent {
        equivalence::settle(&mut **tx, claimed.id, JOB_DONE, Some(&document)).await?;
        return Ok(());
    }
    // The attempt names its task and topic; read them from the log.
    let target = sqlx::query!(
        r#"
        SELECT payload->>'task_id' AS "task_id!",
               payload->>'topic'   AS "topic!"
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
        // The attempt is gone (a reset). The verdict stands in the cache; the
        // correction has nothing to correct.
        tracing::info!(job = %claimed.id, "equivalence: the attempt is not in the log; no fold");
        equivalence::settle(&mut **tx, claimed.id, JOB_DONE, Some(&document)).await?;
        return Ok(());
    };
    let Ok(topic) = Slug::new(&target.topic) else {
        return Err(WorkerError::Config(format!(
            "equivalence: the attempt topic {:?} is not a slug",
            target.topic
        )));
    };
    let reason = format!("{}: {}", FOLD_REASON, verdict.reason);
    let event = Event::Regraded(Regraded {
        ts,
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
        reason,
    });
    // Append and refold in the same transaction. The fold is the projection
    // the web tier reads; it runs with the deployment's own curriculum view
    // and its default scheduler constants, so the refold's config hash
    // matches what the web tier builds.
    append_event(tx, claimed.user_id, &event, None).await?;
    let now = Timestamp::from_micros(Utc::now().timestamp_micros());
    let cfg = Config::default();
    let input = cadus_core::projector::ProjectionInput::new(&job.curriculum, &cfg, now);
    project_and_save(tx, claimed.user_id, &input, None).await?;
    equivalence::settle(&mut **tx, claimed.id, JOB_DONE, Some(&document)).await?;
    Ok(())
}

/// A report of a pass that did not accept.
fn quiet(outcome: Outcome, job_id: Uuid) -> Report {
    Report {
        outcome,
        job_id: Some(job_id),
        attempts: Vec::new(),
    }
}

/// Settle one row `failed` with the reason in its result.
async fn settle_failed(
    db: &Db,
    claimed: &equivalence::Claimed,
    reason: &str,
) -> Result<(), WorkerError> {
    let document = json!({ "error": reason });
    equivalence::settle(db.pool(), claimed.id, JOB_FAILED, Some(&document))
        .await
        .map_err(WorkerError::from)
}
