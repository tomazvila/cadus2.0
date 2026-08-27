//! The A4 client surface: the `diagnosis` field, the poll route, and the stream.
//!
//! Requirements: A4 (the verdict ships at once and the prose follows), C3 (every
//! tenant read runs inside `begin_tenant`), D7 (push), L2/L3 (the learner is
//! never blocked on a model), R4 and T1 (this tier does local work and database
//! I/O only; it spends no model token).
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2.1 (the wire shape
//! and the push paragraph), section 6.2 (what skips the call), section 9 trap
//! W14, and row U9 of section 11. Ruling D-M5-1 is binding.
//!
//! # The three states of `diagnosis`
//!
//! - [`STATUS_NOT_OFFERED`] — a correct answer or a blank one. No job row.
//! - [`STATUS_READY`] — a pre-authored distractor of the M4 template matched the
//!   learner's answer, read from `content_store` kind `diagnosis` in the SAME
//!   transaction. **No job row**: that is the difference between a bill that
//!   scales with attempts and one that scales with distinct misconceptions
//!   (spec section 6.2).
//! - [`STATUS_PENDING`] — a `diagnosis_jobs` row went in inside the grade
//!   transaction, and `id` is its primary key.
//!
//! # Why the enqueue sits inside the grade transaction
//!
//! Spec section 4.3 step 9. A grade that rolls back must leave no job row, or
//! the worker pays for an attempt the log does not hold.
//!
//! # Why the stream re-reads the row
//!
//! `LISTEN/NOTIFY` carries no row-level security and no tenant binding (trap
//! W14). The notice holds a job id and a user id, the handler keeps only the
//! notices of ITS tenant, and it re-reads the row through `begin_tenant` before
//! it writes a byte. A payload alone never reaches a client.
//!
//! # What this unit does NOT do
//!
//! The T4 knobs of spec section 6.6 (`DIAGNOSIS_CALLS_PER_SESSION` and the two
//! token bounds) belong to the worker of unit U10, so nothing here writes
//! `status = 'capped'`; the routes report the state when the worker does. The
//! vocabulary filter of spec section 6.3 runs where the tag is read: unit U9
//! filters the PRE-AUTHORED tag, and unit U10 filters the model's tags before it
//! writes `result`, so the poll route reports the stored document as it stands.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::routing::get;
use cadus_core::answer::check::{Outcome, check};
use cadus_core::config::Config;
use cadus_core::curriculum::AnswerKind;
use cadus_core::pool::kp_key;
use cadus_core::template::Distractor;
use cadus_store::diagnosis::{
    JOB_CAPPED, JOB_DONE, JOB_FAILED, JobPayload, JobRow, Notice, PAYLOAD_VERSION, enqueue, job,
};
use cadus_store::{Db, begin_tenant};
use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{Stream, StreamExt};

use crate::AppState;
use crate::error::ApiError;
use crate::session::{bound, failed};
use crate::state::{ServedProblem, Tenant, WebState};

/// The code of a diagnosis id this tenant does not own.
pub const UNKNOWN_DIAGNOSIS: &str = "unknown_diagnosis";

/// The `content_store.kind` of an authored distractor set (A4).
pub const KIND_DIAGNOSIS: &str = "diagnosis";

/// No diagnosis is owed: the answer was right, or it was blank.
pub const STATUS_NOT_OFFERED: &str = "not_offered";

/// A job row stands and the prose is on its way.
pub const STATUS_PENDING: &str = "pending";

/// The prose stands now: a pre-authored distractor, or a finished job.
pub const STATUS_READY: &str = "ready";

/// The job will produce nothing. The learner already holds the verdict, the
/// solution and the re-solve instruction, so this costs prose and nothing else.
pub const STATUS_FAILED: &str = "failed";

/// A T4 cap refused the call (spec section 6.6). Unit U10 writes it.
pub const STATUS_CAPPED: &str = "capped";

/// The `event:` name of every server-sent diagnosis frame (spec section 2.1).
pub const SSE_EVENT: &str = "diagnosis";

/// The heartbeat comment interval of the stream, in seconds (spec section 2.1).
pub const HEARTBEAT_SECS: u64 = 15;

/// The poll interval the fallback client uses, in seconds (spec section 2.1).
pub const POLL_INTERVAL_SECS: i64 = 2;

/// A job still unfinished after this many seconds is reported
/// [`STATUS_FAILED`], never left open (spec section 2.1, last paragraph).
pub const PENDING_DEADLINE_SECS: i64 = 30;

/// How many notices the hub buffers for one slow subscriber.
///
/// A subscriber that falls further behind than this loses the notices in
/// between. It is not a lost diagnosis: the poll route is the required fallback
/// and `pending_diagnoses` holds the ids.
pub const STREAM_BACKLOG: usize = 256;

// --------------------------------------------------------------------------- //
// The push hub (D7, D-M5-1)
// --------------------------------------------------------------------------- //

/// The one fan-out point between the process `LISTEN` connection and the open
/// streams.
///
/// One connection listens; every stream subscribes here. A connection per
/// stream would give one idle learner one Postgres backend.
#[derive(Debug)]
pub struct DiagnosisHub {
    sender: broadcast::Sender<Notice>,
}

impl Default for DiagnosisHub {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosisHub {
    /// A hub with no listener attached. Publishing still works, so a test and a
    /// process that runs no listener behave the same way.
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(STREAM_BACKLOG);
        Self { sender }
    }

    /// Hand one notice to every open stream. The answer is the count of
    /// subscribers it reached.
    pub fn publish(&self, notice: Notice) -> usize {
        self.sender.send(notice).unwrap_or(0)
    }

    /// Open one subscription. A stream holds it for its whole life.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Notice> {
        self.sender.subscribe()
    }

    /// Read every notice the worker sends and publish it (spec section 2.1).
    ///
    /// The loop never ends by itself: `PgListener::recv` reconnects and
    /// re-`LISTEN`s after a lost connection. A payload the reader refuses is
    /// counted and dropped, because the channel carries no tenant binding and a
    /// guess would name the wrong learner (trap W14).
    ///
    /// # Errors
    ///
    /// Returns the sqlx error when the `LISTEN` cannot be established at all.
    pub async fn listen(&self, db: &Db) -> Result<(), sqlx::Error> {
        let mut listener = sqlx::postgres::PgListener::connect_with(db.pool()).await?;
        listener
            .listen(cadus_store::diagnosis::NOTIFY_CHANNEL)
            .await?;
        tracing::info!(
            channel = cadus_store::diagnosis::NOTIFY_CHANNEL,
            "cadus-web: the diagnosis listener is up"
        );
        loop {
            let notification = listener.recv().await?;
            match Notice::parse(notification.payload()) {
                Some(notice) => {
                    self.publish(notice);
                }
                None => tracing::warn!(
                    channel = cadus_store::diagnosis::NOTIFY_CHANNEL,
                    "cadus-web: a notice payload did not read; dropping it"
                ),
            }
        }
    }
}

// --------------------------------------------------------------------------- //
// The pre-authored distractor lookup (spec section 6.2)
// --------------------------------------------------------------------------- //

/// The `content_store` body of a kind-`diagnosis` document.
///
/// The reader takes the `distractors` list and nothing else, so the same reader
/// serves a dedicated diagnosis row and a template document stored under this
/// kind. An absent list is an empty list, which matches nothing.
#[derive(Debug, Deserialize)]
struct DiagnosisDoc {
    #[serde(default)]
    distractors: Vec<Distractor>,
}

/// The pre-authored diagnosis of one wrong answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preauthored {
    /// The authored tag, after the section 5.3 vocabulary filter.
    pub error_tags: Vec<String>,
    /// The authored prose the learner reads.
    pub prose: Option<String>,
}

/// Find the authored distractor that names this answer (spec section 6.2).
///
/// The match runs through [`cadus_core::answer::check`], so `12` and `12.0`
/// name the same mistake, exactly as they name the same right answer. A
/// distractor the checker cannot decide — an unbound parameter expression, say —
/// simply never matches, so a template document under this kind is safe to read.
///
/// The authored tag goes through the vocabulary of spec section 5.3 first: a tag
/// the vocabulary lacks is dropped silently, which is the 1.0 lesson of
/// `prompts.py:529-536`. A hit with neither a surviving tag nor prose carries
/// nothing a learner can read, so it is NOT a hit and the caller enqueues.
#[must_use]
pub fn match_distractor(
    body: &Value,
    answer: &str,
    kind: AnswerKind,
    vocabulary: &[String],
) -> Option<Preauthored> {
    let doc: DiagnosisDoc = serde_json::from_value(body.clone()).ok()?;
    let hit = doc.distractors.iter().find(|distractor| {
        matches!(
            check(&distractor.answer, answer, kind),
            Outcome::Decided(verdict) if verdict.correct
        )
    })?;
    let error_tags: Vec<String> = vocabulary
        .iter()
        .filter(|tag| *tag == &hit.error_tag)
        .cloned()
        .collect();
    let prose = hit.note.clone().filter(|note| !note.trim().is_empty());
    if error_tags.is_empty() && prose.is_none() {
        return None;
    }
    Some(Preauthored { error_tags, prose })
}

// --------------------------------------------------------------------------- //
// The grade reply's `diagnosis` field (spec section 2.1)
// --------------------------------------------------------------------------- //

/// The submission the diagnosis is about.
///
/// It names THIS submission, never the stashed H3 attempt the log records: a
/// failed re-solve records the stashed correct answer with `correct` rewritten,
/// and the mistake to diagnose is the one the learner just made.
pub(crate) struct Miss<'a> {
    /// The `attempt_id` of the row the log holds. It is the enqueue's idempotency key.
    pub attempt_id: &'a str,
    /// The open session, for the T4 per-session knob of unit U10.
    pub session: Option<&'a str>,
    /// The task the attempt belongs to.
    pub task_id: &'a str,
    /// What the learner answered.
    pub answer: &'a str,
    /// The learner's shown work.
    pub work: Option<&'a str>,
    /// The deterministic verdict of THIS submission.
    pub correct: bool,
}

/// Everything one diagnosis decision reads about its own grade.
pub(crate) struct Pending<'a> {
    /// The scheduler config, for the section 5.3 vocabulary.
    pub cfg: &'a Config,
    /// The problem the learner answered.
    pub served: &'a ServedProblem,
    /// The answer grammar the checker read.
    pub kind: AnswerKind,
    /// The submission itself.
    pub miss: Miss<'a>,
    /// Whether this call may write. It is false on the replay path: a grade that
    /// appends nothing must leave no job row, so the replay looks the
    /// pre-authored answer up, reads the standing job id out of the D-S6
    /// document, and writes neither.
    pub write: bool,
}

/// Decide the `diagnosis` field of one grade reply (spec section 2.1).
pub(crate) async fn decide(
    state: &AppState,
    tx: &mut Transaction<'static, Postgres>,
    user_id: Uuid,
    scratch: &mut WebState,
    about: &Pending<'_>,
) -> Result<Value, ApiError> {
    let Pending {
        cfg,
        served,
        kind,
        miss,
        write,
    } = about;
    let (kind, write) = (*kind, *write);
    if miss.correct || miss.answer.trim().is_empty() {
        return Ok(json!({ "status": STATUS_NOT_OFFERED }));
    }

    // Spec section 6.2: the pre-authored path first. A hit writes no job row.
    if let (Some(topic), Some(point)) = (served.topic.as_deref(), served.kp.as_deref()) {
        let key = kp_key(topic, point);
        let doc = bound(
            &state.db,
            cadus_store::content::approved_document(&mut **tx, &key, KIND_DIAGNOSIS),
        )
        .await
        .map_err(|err| failed(&err))?;
        if let Some(hit) =
            doc.and_then(|doc| match_distractor(&doc.body, miss.answer, kind, &cfg.error_tags))
        {
            return Ok(json!({
                "status": STATUS_READY,
                "error_tags": hit.error_tags,
                "prose": hit.prose,
            }));
        }
    }

    if !write {
        // The replay path. The job id of the first request stands in the D-S6
        // document, so a retried request names the same job and starts none.
        return Ok(match scratch.pending_diagnoses.get(miss.attempt_id) {
            Some(id) => json!({ "id": id, "status": STATUS_PENDING }),
            None => json!({ "status": STATUS_NOT_OFFERED }),
        });
    }

    let payload = JobPayload {
        v: PAYLOAD_VERSION,
        session: miss.session.map(str::to_string),
        task_id: miss.task_id.to_string(),
        topic: served.topic.clone().unwrap_or_default(),
        kp: served.kp.clone(),
        problem: served.text.clone(),
        expected: served.expected.answer.clone(),
        answer_kind: served.answer_kind.clone().unwrap_or_default(),
        given_answer: miss.answer.to_string(),
        work: miss.work.map(str::to_string),
    };
    let document = serde_json::to_value(&payload).map_err(|err| {
        tracing::error!(error = %err, "cadus-web: the diagnosis payload did not write");
        ApiError::internal("diagnosis payload")
    })?;
    let id = bound(&state.db, enqueue(tx, user_id, miss.attempt_id, &document))
        .await
        .map_err(|err| failed(&err))?;
    scratch
        .pending_diagnoses
        .insert(miss.attempt_id.to_string(), id.to_string());
    Ok(json!({ "id": id.to_string(), "status": STATUS_PENDING }))
}

// --------------------------------------------------------------------------- //
// The wire view of one job row
// --------------------------------------------------------------------------- //

/// The wire status of one row, and the body the client reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobView {
    /// One of [`STATUS_PENDING`], [`STATUS_READY`], [`STATUS_FAILED`],
    /// [`STATUS_CAPPED`].
    pub status: &'static str,
    /// The body of the poll answer and of the stream frame.
    pub body: Value,
}

/// Read one stored row into the wire shape of spec section 2.1.
///
/// The five stored values map onto four wire values: `running` reads as
/// [`STATUS_PENDING`], because the client waits either way. A row still
/// unfinished [`PENDING_DEADLINE_SECS`] after the grade is reported
/// [`STATUS_FAILED`] — the spec's rule, and the reason is L3: a client that
/// polls a job forever waits for prose that no longer costs it anything. A
/// status no build of this service writes reads as [`STATUS_FAILED`] too, so the
/// route fails closed and never leaves the client open.
#[must_use]
pub fn job_view(row: &JobRow, now: DateTime<Utc>) -> JobView {
    let status = match row.status.as_str() {
        JOB_DONE => STATUS_READY,
        JOB_CAPPED => STATUS_CAPPED,
        JOB_FAILED => STATUS_FAILED,
        _ if (now - row.created_at).num_seconds() > PENDING_DEADLINE_SECS => STATUS_FAILED,
        cadus_store::diagnosis::JOB_PENDING | cadus_store::diagnosis::JOB_RUNNING => STATUS_PENDING,
        _ => STATUS_FAILED,
    };
    let result = row.result.as_ref();
    let field = |name: &str| result.and_then(|value| value.get(name));
    let error_tags = match field("error_tags") {
        Some(Value::Array(tags)) if status == STATUS_READY => Value::Array(tags.clone()),
        _ => Value::Array(Vec::new()),
    };
    let mut body = json!({
        "id": row.id.to_string(),
        "status": status,
        "error_tags": error_tags,
    });
    if status == STATUS_READY
        && let Some(map) = body.as_object_mut()
    {
        if let Some(Value::String(prose)) = field("prose") {
            map.insert("prose".to_string(), json!(prose));
        }
        if let Some(Value::String(model)) = field("model_id") {
            map.insert("model_id".to_string(), json!(model));
        }
    }
    JobView { status, body }
}

// --------------------------------------------------------------------------- //
// GET /api/diagnosis/{id} — the poll fallback
// --------------------------------------------------------------------------- //

/// `404 unknown_diagnosis`. It is the ONE answer for an id this tenant does not
/// own and for an id no row carries, so a caller learns nothing about another
/// tenant's queue.
fn unknown_diagnosis() -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        UNKNOWN_DIAGNOSIS,
        "This diagnosis does not exist.",
    )
}

/// Read one diagnosis (spec section 2.1). The poll fallback is required: a proxy
/// that buffers server-sent events leaves the stream silent.
///
/// The read runs inside `begin_tenant`, and the statement names no `user_id`:
/// the `tenant_isolation` policy scopes it (C3).
pub async fn poll(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let Ok(id) = Uuid::parse_str(&id) else {
        return Err(unknown_diagnosis());
    };
    let mut tx = crate::session::begin(&state, user_id).await?;
    let row = bound(&state.db, job(&mut *tx, id))
        .await
        .map_err(|err| failed(&err))?;
    // The read changed nothing, so the transaction ends with a rollback.
    if let Err(err) = tx.rollback().await {
        tracing::warn!(error = %err, "cadus-web: the diagnosis read did not roll back");
    }
    let Some(row) = row else {
        return Err(unknown_diagnosis());
    };
    Ok(Json(job_view(&row, Utc::now()).body))
}

// --------------------------------------------------------------------------- //
// GET /api/diagnosis/stream — the push channel
// --------------------------------------------------------------------------- //

/// The stream frame of one finished job, or `None` when there is nothing to say.
///
/// The re-read is the tenant boundary. The notice named a user id, and the
/// handler already dropped every notice of another tenant, but the payload
/// crossed a channel with no row-level security at all (trap W14), so the row
/// itself is read again under this stream's OWN binding.
async fn finished_frame(db: &Db, user_id: Uuid, job_id: Uuid) -> Option<SseEvent> {
    let mut tx = begin_tenant(db.pool(), user_id).await.ok()?;
    let row = job(&mut *tx, job_id).await.ok().flatten();
    if let Err(err) = tx.rollback().await {
        tracing::warn!(error = %err, "cadus-web: the stream read did not roll back");
    }
    let view = job_view(&row?, Utc::now());
    if view.status == STATUS_PENDING {
        return None;
    }
    SseEvent::default()
        .event(SSE_EVENT)
        .json_data(view.body)
        .ok()
}

/// Stream this tenant's finished diagnoses (D-M5-1, D7).
///
/// The handler subscribes to the process hub, keeps the notices of its own
/// tenant, re-reads each row through `begin_tenant`, and writes one
/// `event: diagnosis` frame. A heartbeat comment every [`HEARTBEAT_SECS`]
/// seconds keeps an idle connection open through a proxy.
pub async fn stream(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let db = state.db.clone();
    let frames = BroadcastStream::new(state.diagnosis.subscribe())
        .filter_map(move |item| match item {
            // The tenant filter. A notice for another learner is dropped here,
            // and the re-read below is the second guard behind it.
            Ok(notice) if notice.user_id == user_id => Some(notice),
            Ok(_) => None,
            Err(BroadcastStreamRecvError::Lagged(missed)) => {
                tracing::warn!(
                    missed,
                    "cadus-web: a diagnosis stream fell behind; the client polls for the rest"
                );
                None
            }
        })
        .then(move |notice| {
            let db = db.clone();
            async move { finished_frame(&db, user_id, notice.job_id).await }
        })
        .filter_map(|frame| frame.map(Ok));
    Sse::new(frames).keep_alive(KeepAlive::new().interval(Duration::from_secs(HEARTBEAT_SECS)))
}

/// The two A4 client routes (spec section 2.1, "New in 2.0").
///
/// `/api/diagnosis/stream` is a static segment and `/api/diagnosis/{id}` is a
/// parameter, so the router matches the static one first and `stream` is never
/// read as an id.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/diagnosis/stream", get(stream))
        .route("/api/diagnosis/{id}", get(poll))
}
