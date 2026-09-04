//! The request plumbing every route of the crate shares: the query bound, the
//! tenant transaction, the projection, the D-S6 document, and the envelopes of
//! the refusals more than one route gives.

use axum::Json;
use axum::http::StatusCode;
use cadus_core::event::{Enrolled, Event, SchemaVersion, Slug, TaskType, Timestamp};
use cadus_core::projector::ProjectionInput;
use cadus_store::state::{
    EventRow, Projection, SessionView, append_event, load_events_after, load_web_state,
    lock_web_state, project_and_save, project_current, save_web_state,
};
use cadus_store::{Db, StoreError, begin_tenant, bounded};
use serde::Serialize;
use serde_json::Value;
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};

use super::INTERNAL_ERROR;
use crate::AppState;
use crate::error::ApiError;
use crate::state::{
    CURRICULUM_UNAVAILABLE, Content, INVALID_REQUEST, NO_OPEN_SESSION, STATE_UNAVAILABLE,
    UNKNOWN_COURSE, WebState,
};

/// A tenant transaction.
pub(crate) type Tx = Transaction<'static, Postgres>;

/// Run a store call under the client-side query bound of `db` (L1, R4).
pub(crate) async fn bound<T>(
    db: &Db,
    call: impl Future<Output = Result<T, StoreError>>,
) -> Result<T, StoreError> {
    bounded(db, async { Ok(call.await) }).await?
}

/// Map a store failure onto the envelope. The cause goes to the log, never to
/// the client: it names table and column text.
pub(crate) fn failed(err: &StoreError) -> ApiError {
    tracing::error!(error = %err, "cadus-web: a state operation failed");
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        INTERNAL_ERROR,
        "The server could not complete this request.",
    )
}

/// The envelope of a store failure.
fn store_failed(err: StoreError) -> ApiError {
    failed(&err)
}

/// The envelope of a commit or a rollback that failed.
pub(crate) fn db_failed(err: sqlx::Error) -> ApiError {
    failed(&err.into())
}

/// Run one store call under the client-side query bound (L1, R4), and map its
/// failure onto the envelope.
pub(crate) async fn store<T>(
    state: &AppState,
    call: impl Future<Output = Result<T, StoreError>>,
) -> Result<T, ApiError> {
    bound(&state.db, call).await.map_err(store_failed)
}

/// Commit the transaction and answer `body`.
pub(crate) async fn reply_committed(tx: Tx, body: Value) -> Result<Json<Value>, ApiError> {
    tx.commit().await.map(|()| Json(body)).map_err(db_failed)
}

/// Release a read-only transaction and answer `body`.
pub(crate) async fn reply_read(tx: Tx, body: Value) -> Result<Json<Value>, ApiError> {
    tx.rollback().await.map(|()| Json(body)).map_err(db_failed)
}

/// Open a tenant-bound transaction under the client-side query bound.
///
/// `begin_tenant` runs `set_config('app.user_id', $1, true)`, which is a
/// statement like any other, so it takes the bound of `DB_CLIENT_TIMEOUT_MS`
/// too (R4, L1).
pub(crate) async fn begin(state: &AppState, user_id: Uuid) -> Result<Tx, ApiError> {
    store(state, begin_tenant(state.db.pool(), user_id)).await
}

/// The curriculum of this process, or `503` when the binary loaded none.
pub(crate) fn content(state: &AppState) -> Result<&Content, ApiError> {
    state.content.as_deref().ok_or_else(curriculum_unavailable)
}

/// The `503` of a process with no curriculum.
fn curriculum_unavailable() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        CURRICULUM_UNAVAILABLE,
        "This deployment has no curriculum loaded.",
    )
}

/// The `now` of one request, as the core spells it.
pub(crate) fn now_pair() -> (DateTime<Utc>, Timestamp) {
    let now = Utc::now();
    (now, Timestamp::from_micros(now.timestamp_micros()))
}

/// The projection inputs of one request.
pub(crate) fn projection_input<'a>(content: &'a Content, now: Timestamp) -> ProjectionInput<'a> {
    ProjectionInput::new(&content.curriculum, &content.cfg, now)
        .with_timezone(content.cfg.timezone.as_deref())
}

/// What one request derives before it opens its transaction.
pub(crate) struct RequestInput<'a> {
    /// The curriculum and the config of this process.
    pub content: &'a Content,
    /// The wall clock of the request.
    pub wall: DateTime<Utc>,
    /// The same instant, as the core spells it.
    pub now: Timestamp,
    /// The projection inputs of the request.
    pub input: ProjectionInput<'a>,
}

/// The curriculum, the clock, and the projection inputs of one request.
pub(crate) fn request_input(state: &AppState) -> Result<RequestInput<'_>, ApiError> {
    let content = content(state)?;
    let (wall, now) = now_pair();
    let input = projection_input(content, now);
    Ok(RequestInput {
        content,
        wall,
        now,
        input,
    })
}

/// Open the tenant transaction and take the tenant's advisory lock.
pub(crate) async fn open_locked(state: &AppState, user_id: Uuid) -> Result<Tx, ApiError> {
    let mut tx = begin(state, user_id).await?;
    store(state, lock_web_state(&mut tx, user_id)).await?;
    Ok(tx)
}

/// Open the tenant transaction, take the tenant's advisory lock, and fold the
/// model to the head of the log.
pub(crate) async fn locked_projection(
    state: &AppState,
    user_id: Uuid,
    input: &ProjectionInput<'_>,
) -> Result<(Tx, Projection), ApiError> {
    let mut tx = open_locked(state, user_id).await?;
    let projection = store(state, project_current(&mut tx, user_id, input)).await?;
    Ok((tx, projection))
}

/// Fold the model to the head of the log in a transaction of its own, and
/// release it. The fold is a pure read.
pub(crate) async fn read_projection(
    state: &AppState,
    user_id: Uuid,
    input: &ProjectionInput<'_>,
) -> Result<Projection, ApiError> {
    let mut tx = begin(state, user_id).await?;
    let projection = store(state, project_current(&mut tx, user_id, input)).await?;
    tx.rollback().await.map(|()| projection).map_err(db_failed)
}

/// Append one event inside the caller's transaction. The answer is the `seq`
/// the row took, or `None` when the FR-14 index made the INSERT a no-op.
pub(crate) async fn append(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
    event: &Event,
) -> Result<Option<i64>, ApiError> {
    store(state, append_event(tx, user_id, event, None)).await
}

/// Append one event and fold it into the saved model.
pub(crate) async fn append_and_fold(
    state: &AppState,
    tx: &mut Tx,
    user_id: Uuid,
    event: &Event,
    input: &ProjectionInput<'_>,
) -> Result<Projection, ApiError> {
    append(state, tx, user_id, event).await?;
    store(state, project_and_save(tx, user_id, input, None)).await
}

/// The `422 invalid_request` of a value the core refuses.
pub(crate) fn invalid(err: impl ToString) -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        INVALID_REQUEST,
        err.to_string(),
    )
}

/// The `409 no_open_session` of a route that needs an open session.
pub(crate) fn no_open_session() -> ApiError {
    ApiError::new(StatusCode::CONFLICT, NO_OPEN_SESSION, "No session is open.")
}

/// The `404 unknown_course` of a course id the curriculum does not hold.
pub(crate) fn unknown_course(course: &str) -> ApiError {
    ApiError::new(
        StatusCode::NOT_FOUND,
        UNKNOWN_COURSE,
        format!("No course {course:?} is in the curriculum."),
    )
}

/// The event-side slug of a curriculum id. A curriculum slug is trimmed and
/// non-empty, so the event slug always reads.
pub(crate) fn event_slug(id: &cadus_core::curriculum::Slug) -> Result<Slug, ApiError> {
    Slug::new(id.as_str()).map_err(invalid)
}

/// The `enrolled` event of `course`, the same append `POST /api/enroll` makes.
pub(crate) fn enrolled_event(now: Timestamp, session: Option<String>, course: Slug) -> Event {
    Event::Enrolled(Enrolled {
        ts: now,
        session,
        v: SchemaVersion,
        course,
        reason: None,
        return_to: None,
    })
}

/// The JSON of a model field, or `null` when it does not serialize.
pub(crate) fn json_of<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// The `500 state_unavailable` of a D-S6 document that did not read.
fn state_unreadable(reason: String) -> ApiError {
    tracing::error!(error = %reason, "cadus-web: the D-S6 document did not read");
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        STATE_UNAVAILABLE,
        "The stored session state is not readable.",
    )
}

/// Read the D-S6 document of this tenant. An absent row gives an empty one.
pub(crate) async fn read_state(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
) -> Result<WebState, ApiError> {
    let doc = bound(db, load_web_state(tx, user_id))
        .await
        .map_err(store_failed)?;
    let Some(doc) = doc else {
        return Ok(WebState::default());
    };
    WebState::from_doc(&doc).map_err(state_unreadable)
}

/// Write the D-S6 document of this tenant, inside the caller's transaction.
pub(crate) async fn write_state(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    scratch: &WebState,
) -> Result<(), ApiError> {
    bound(db, save_web_state(tx, user_id, &scratch.to_doc()))
        .await
        .map_err(store_failed)
}

// --------------------------------------------------------------------------- //
// The ONE view of the open session (M5 review 2, findings V3 and V9)
// --------------------------------------------------------------------------- //

/// Take the drills the OPEN session itself served out of the cadence map.
///
/// [`cadus_core::selector::schedule_drills`] drops a topic whose last drill is
/// inside the 3.5-day window, and [`crate::serve::record_first_serve`] stamps
/// that instant at the FIRST serve of the drill. Without this step the drill
/// task leaves the plan while the learner still works through its 20 questions.
///
/// The cadence therefore reads "no NEW drill of this topic for 3.5 days", and a
/// drill the open session already works on stays in that session's plan. The
/// rule is the queue stability of `_reserve_open_plan` (`selector.py:1630`): a
/// task the session already serves keeps its id and its place.
fn forget_own_drills(view: &mut SessionView, events: &[EventRow], session: &str) {
    for row in events {
        if let Event::TaskServed(body) = &row.event
            && body.session.as_deref() == Some(session)
            && body.task_type == TaskType::Drill
            && let Some(topic) = body.topic.as_ref()
        {
            view.last_drill_at.remove(topic.as_str());
        }
    }
}

/// Read the events of the OPEN SESSION and repair the drill cadence of `view`.
///
/// It is the ONE view every route composes from: `POST /api/task/{id}/serve`
/// (through [`crate::serve::open`]), `GET /api/session/plan`, and
/// `GET /api/status`. Before M5 review 2 the repair lived inside the serve
/// alone, so the first serve of a drill took that drill out of the plan listing
/// and turned `drill_due` false while the serve route still handed out its
/// questions 2 to 20 (findings V3 and V9). Three derivations of one view drift
/// apart; one derivation cannot.
///
/// The answer is the window itself, in `seq` order and `session_start` first,
/// because the serve route reads it again for the tasks this session served.
/// The window starts AT the `session_start` that opened the session, so the read
/// asks for the events after the line before it. It is NOT the whole log (F15,
/// F18): a task id is `{session}-{task_type}-{topic}`, so every event of a task
/// of the open session stands in this window.
pub(crate) async fn view_for_open_session(
    state: &AppState,
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    view: &mut SessionView,
    session: &str,
) -> Result<Vec<EventRow>, ApiError> {
    let after = view.session_start_seq.unwrap_or(0).saturating_sub(1);
    let events = store(state, load_events_after(tx, user_id, after)).await?;
    forget_own_drills(view, &events, session);
    Ok(events)
}
