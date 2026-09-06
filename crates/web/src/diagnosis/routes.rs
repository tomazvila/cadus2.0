//! The two A4 client routes: `GET /api/diagnosis/{id}`, the poll fallback,
//! and `GET /api/diagnosis/stream`, the push channel.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use cadus_store::diagnosis::{JOB_CAPPED, JOB_DONE, JOB_FAILED, JobRow, job};
use cadus_store::{Db, begin_tenant};
use serde_json::{Value, json};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::{Stream, StreamExt};

use super::{
    HEARTBEAT_SECS, PENDING_DEADLINE_SECS, SSE_EVENT, STATUS_CAPPED, STATUS_FAILED, STATUS_PENDING,
    STATUS_READY, UNKNOWN_DIAGNOSIS,
};
use crate::AppState;
use crate::error::ApiError;
use crate::path::ApiPath;
use crate::session::{bound, failed};
use crate::state::Tenant;

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
    ApiPath(id): ApiPath<String>,
) -> Result<Json<Value>, ApiError> {
    let Ok(id) = Uuid::parse_str(&id) else {
        return Err(unknown_diagnosis());
    };
    let mut tx = crate::session::begin(&state, user_id).await?;
    let row = bound(&state.db, job(&mut *tx, id))
        .await
        .map_err(|err| failed(&err))?;
    // The read changed nothing, so the transaction ends with the rollback its
    // drop runs.
    drop(tx);
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
    // The read changed nothing, so the transaction ends with the rollback its
    // drop runs.
    drop(tx);
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
