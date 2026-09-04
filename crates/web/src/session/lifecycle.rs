//! The three writes of unit U6: `POST /api/enroll`, `POST /api/session/start`
//! and `POST /api/session/end`. Each one is ONE transaction under the tenant's
//! advisory lock.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cadus_core::event::{Event, SchemaVersion, SessionEnd, SessionStart};
use cadus_store::state::{clear_web_state, project_and_save};
use serde_json::{Value, json};

use super::dashboard::due_counts;
use super::store::{
    RequestInput, append, append_and_fold, enrolled_event, event_slug, json_of, locked_projection,
    no_open_session, read_state, reply_committed, request_input, store, unknown_course,
    write_state,
};
use crate::AppState;
use crate::error::ApiError;
use crate::state::{INVALID_REQUEST, Tenant};

/// The `course` field of an enroll body, or an empty text when it has none.
fn asked_course(body: Option<&Json<Value>>) -> String {
    body.and_then(|Json(value)| value.get("course"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The `minutes` field of a session-end body.
fn asked_minutes(body: Option<&Json<Value>>) -> Option<f64> {
    body.and_then(|Json(value)| value.get("minutes"))
        .and_then(Value::as_f64)
}

// --------------------------------------------------------------------------- //
// POST /api/enroll
// --------------------------------------------------------------------------- //

/// Switch the enrolled course (`api.py:824-846`).
///
/// The append, the re-projection, and the scratch clear are ONE transaction
/// under the tenant's advisory lock, so a course switch never commits against a
/// stale learner model or a stale served problem.
pub async fn enroll(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let RequestInput {
        content,
        now,
        input,
        ..
    } = request_input(&state)?;
    let course = asked_course(body.as_ref());
    if course.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "enroll requires a course id.",
        ));
    }
    let graph = &content.curriculum;
    let Some(found) = graph.course(&course) else {
        return Err(unknown_course(&course));
    };

    let (mut tx, projection) = locked_projection(&state, user_id, &input).await?;
    let slug = event_slug(&found.id)?;
    let event = enrolled_event(now, projection.view.current_session.clone(), slug);
    append_and_fold(&state, &mut tx, user_id, &event, &input).await?;
    // A new course makes every in-flight served problem stale.
    store(&state, clear_web_state(&mut tx, user_id)).await?;

    let mut floor: Vec<&str> = graph
        .mastery_floor(&course)
        .unwrap_or_default()
        .into_iter()
        .map(|idx| graph.id_of(idx))
        .collect();
    floor.sort_unstable();
    let body = json!({
        "enrolled": course,
        "mastery_floor": floor,
        "floor_size": floor.len(),
    });
    reply_committed(tx, body).await
}

// --------------------------------------------------------------------------- //
// POST /api/session/start
// --------------------------------------------------------------------------- //

/// Open a session, or resume the open one (`api.py:860-881`).
///
/// The scan, the decision, and the append are ONE transaction under the tenant's
/// advisory lock. `session_start` carries no `attempt_id`, so the FR-14 index
/// cannot dedup a second append out of an append-only log: the lock is the whole
/// idempotence argument (1.0 BUG-2).
pub async fn session_start(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
) -> Result<Json<Value>, ApiError> {
    let RequestInput {
        content,
        wall,
        now,
        input,
    } = request_input(&state)?;
    let (mut tx, before) = locked_projection(&state, user_id, &input).await?;

    let open = before.view.current_session.clone();
    let reopened = open.is_some();
    let session = match open {
        Some(session) => session,
        None => {
            let session = before.view.new_session_id(wall);
            let event = Event::SessionStart(SessionStart {
                ts: now,
                session: Some(session.clone()),
                v: SchemaVersion,
            });
            append(&state, &mut tx, user_id, &event).await?;
            session
        }
    };
    let projection = store(&state, project_and_save(&mut tx, user_id, &input, None)).await?;

    // (Re)bind the scratch to this session. The bind resets it on a drift.
    let mut scratch = read_state(&state.db, &mut tx, user_id).await?;
    scratch.bind(&session);
    write_state(&state.db, &mut tx, user_id, &scratch).await?;

    let model = projection.model;
    let course = projection.view.enrollment_stack.last().map(String::as_str);
    let (frontier_count, due_count, _) = due_counts(
        &model,
        &content.curriculum,
        &content.cfg,
        now.micros(),
        course,
    );
    let body = json!({
        "session": session,
        "reopened": reopened,
        "xp": json_of(&model.xp),
        "frontier": frontier_count,
        "due_reviews": due_count,
    });
    reply_committed(tx, body).await
}

// --------------------------------------------------------------------------- //
// POST /api/session/end
// --------------------------------------------------------------------------- //

/// Close the open session (`api.py:884-925`). `409 no_open_session` when none is.
pub async fn session_end(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let RequestInput { now, input, .. } = request_input(&state)?;
    let asked = asked_minutes(body.as_ref());

    let (mut tx, before) = locked_projection(&state, user_id, &input).await?;
    let session = before
        .view
        .current_session
        .clone()
        .ok_or_else(no_open_session)?;

    let scratch = read_state(&state.db, &mut tx, user_id).await?;
    let minutes = asked.unwrap_or_else(|| (scratch.active_secs / 60.0 * 100.0).round() / 100.0);
    let xp_earned = before.view.xp_in_session(&session);
    let event = Event::SessionEnd(SessionEnd {
        ts: now,
        session: Some(session.clone()),
        v: SchemaVersion,
        xp_earned,
        minutes,
    });
    let projection = append_and_fold(&state, &mut tx, user_id, &event, &input).await?;
    store(&state, clear_web_state(&mut tx, user_id)).await?;

    let body = json!({
        "session": session,
        "xp_earned": xp_earned,
        "minutes": minutes,
        "xp": json_of(&projection.model.xp),
        // The Anki route family defers past M5 (spec section 2), so nothing
        // fills `anki_queue` yet and the count is 0.
        "anki": {"pending": 0},
    });
    reply_committed(tx, body).await
}
