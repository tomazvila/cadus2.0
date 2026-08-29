//! The session and dashboard routes of unit U6, and the event scans they read.
//!
//! Requirements: A3 (the verdict is local), C2 (the log is append-only), C3
//! (every statement runs inside `begin_tenant`), C4, R4 (local CPU and database
//! I/O only), L6 (no model client links here), D-S6.
//!
//! Spec: `docs/reference/web-service-1.0-spec.md` section 2 (the route table),
//! section 4 (the state row and the lock), and section 11 row U6.
//!
//! # What each route touches
//!
//! | Route | Lock | Events | `web_states` | `learner_models` |
//! |---|---|---|---|---|
//! | `GET /api/status` | no | none | no | read |
//! | `GET /api/graph` | no | none | no | read |
//! | `GET /api/modules` | no | none | no | read |
//! | `GET /api/export` | no | read all | no | no |
//! | `POST /api/enroll` | yes | append `enrolled` | clear | write |
//! | `POST /api/session/start` | yes | append `session_start` | bind | write |
//! | `POST /api/session/end` | yes | append `session_end` | clear | write |
//! | `GET /api/session/plan` | yes | none | READ ONLY | read |
//!
//! "none" in the Events column means the route reads no event row when the
//! cursor already stands at the head of the log: the cached model and the cached
//! [`SessionView`] answer it (F15, F18). `GET /api/export` is the one route that
//! reads every row, and section 8 of `docs/reference/l1-budget.md` gives it no
//! p95 for that reason.
//!
//! `GET /api/session/plan` writes NOTHING. Trap W3 states the rule: the plan
//! reports each task through [`crate::state::WebState::plan_progress`], a plain
//! lookup, never 1.0's `_progress_for`, which installs a row per task merely
//! listed.
//!
//! # The event scans
//!
//! `current_session`, `new_session_id`, `session_xp`, `enrollment_stack`,
//! `learned_at`, `last_drill_at`, `active_study_days`, and
//! `quiz_high_score_streak` are ports of the 1.0 service-layer scans
//! (`cadus/service.py:298-334`, `:1078-1165`). Each one is a pure fold over the
//! log, so each one has its own test with literal expected values.
//!
//! The FOLD itself moved to [`cadus_store::state::SessionView`] (M5 review 1,
//! findings F15 and F18). The document is cached beside the learner model and
//! folds forward from `through_seq`, so no route on a learner path reads the
//! whole log any more. The eight functions below stay as the named entry points
//! the tests hold, and each one is now one call of that ONE fold: two copies of
//! a fold drift apart, one copy cannot.

use std::collections::{BTreeMap, BTreeSet};

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, TopicIdx};
use cadus_core::event::{
    Enrolled, Event, SchemaVersion, SessionEnd, SessionStart, Slug, Timestamp,
};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::projector::ProjectionInput;
use cadus_core::selector::{
    SeededSampler, SessionContext, Task, compose_session, course_scope, due_reviews, frontier,
    is_course_complete, is_mastered, mastered_set, nearly_due, quiz_is_due, schedule_drills,
};
use cadus_store::state::{
    EventRow, SessionView, append_event, clear_web_state, load_events, load_web_state,
    lock_web_state, project_and_save, project_current, save_web_state,
};
use cadus_store::{Db, StoreError, begin_tenant, bounded};
use serde_json::{Value, json};
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};
use sqlx::{Postgres, Transaction};

use crate::AppState;
use crate::error::ApiError;
use crate::state::{
    CURRICULUM_UNAVAILABLE, Content, INVALID_REQUEST, NO_OPEN_SESSION, STATE_UNAVAILABLE, Tenant,
    UNKNOWN_COURSE, WebState,
};

/// The code of a failure the caller cannot fix.
pub const INTERNAL_ERROR: &str = "internal_error";

/// The media type of the JSONL export (`api.py:2264`).
pub const EXPORT_MEDIA_TYPE: &str = "application/x-ndjson";

/// A quiz scored at or above this counts as aced (`service.py:1096`).
///
/// The fold that reads it lives in the store, so the number is defined once.
pub const QUIZ_HIGH_SCORE: f64 = cadus_store::state::QUIZ_HIGH_SCORE;

// --------------------------------------------------------------------------- //
// The pure event scans
// --------------------------------------------------------------------------- //

/// The open session id: a `session_start` with no later `session_end`.
#[must_use]
pub fn current_session(events: &[EventRow]) -> Option<String> {
    SessionView::of_log(events).current_session
}

/// The next unused session id of the day: `s_<date><letter>`.
#[must_use]
pub fn new_session_id(events: &[EventRow], today: DateTime<Utc>) -> String {
    SessionView::of_log(events).new_session_id(today)
}

/// The XP the log credits inside one session, rounded to two places.
#[must_use]
pub fn session_xp(events: &[EventRow], session: &str) -> f64 {
    SessionView::of_log(events).xp_in_session(session)
}

/// The enrollment stack, base first and effective last (`service.py:1142-1165`).
///
/// A manual enroll resets the stack, a `gap-fill` switch pushes, and a
/// `gap-return` switch pops one level.
#[must_use]
pub fn enrollment_stack(events: &[EventRow]) -> Vec<String> {
    SessionView::of_log(events).enrollment_stack
}

/// Topic id to the instant it was FIRST passed (`service.py:1086-1091`).
#[must_use]
pub fn learned_at(events: &[EventRow]) -> BTreeMap<String, i64> {
    SessionView::of_log(events).learned_at
}

/// Topic id to the instant of its last served drill (`service.py:1120-1129`).
#[must_use]
pub fn last_drill_at(events: &[EventRow]) -> BTreeMap<String, i64> {
    SessionView::of_log(events).last_drill_at
}

/// The distinct UTC dates that carry an attempt (`service.py:1099-1103`).
#[must_use]
pub fn active_study_days(events: &[EventRow]) -> Vec<NaiveDate> {
    SessionView::of_log(events).study_days()
}

/// The trailing run of quizzes scored at or above [`QUIZ_HIGH_SCORE`].
#[must_use]
pub fn quiz_high_score_streak(events: &[EventRow]) -> i64 {
    SessionView::of_log(events).quiz_high_score_streak
}

/// The task ids a `review_result` already closed (`service.py:1261`).
#[must_use]
pub fn closed_task_ids(events: &[EventRow]) -> BTreeSet<String> {
    SessionView::of_log(events).closed_task_ids
}

// --------------------------------------------------------------------------- //
// Shared request plumbing
// --------------------------------------------------------------------------- //

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

/// Open a tenant-bound transaction under the client-side query bound.
///
/// `begin_tenant` runs `set_config('app.user_id', $1, true)`, which is a
/// statement like any other, so it takes the bound of `DB_CLIENT_TIMEOUT_MS`
/// too (R4, L1).
pub(crate) async fn begin(
    state: &AppState,
    user_id: sqlx::types::Uuid,
) -> Result<Transaction<'static, Postgres>, ApiError> {
    bound(&state.db, begin_tenant(state.db.pool(), user_id))
        .await
        .map_err(|err| failed(&err))
}

/// The curriculum of this process, or `503` when the binary loaded none.
pub(crate) fn content(state: &AppState) -> Result<&Content, ApiError> {
    state.content.as_deref().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            CURRICULUM_UNAVAILABLE,
            "This deployment has no curriculum loaded.",
        )
    })
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

/// Read the D-S6 document of this tenant. An absent row gives an empty one.
pub(crate) async fn read_state(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    user_id: sqlx::types::Uuid,
) -> Result<WebState, ApiError> {
    let doc = bound(db, load_web_state(tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let Some(doc) = doc else {
        return Ok(WebState::default());
    };
    WebState::from_doc(&doc).map_err(|reason| {
        tracing::error!(error = %reason, "cadus-web: the D-S6 document did not read");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            STATE_UNAVAILABLE,
            "The stored session state is not readable.",
        )
    })
}

/// Write the D-S6 document of this tenant, inside the caller's transaction.
pub(crate) async fn write_state(
    db: &Db,
    tx: &mut Transaction<'_, Postgres>,
    user_id: sqlx::types::Uuid,
    scratch: &WebState,
) -> Result<(), ApiError> {
    let doc = scratch.to_doc().map_err(|reason| {
        tracing::error!(error = %reason, "cadus-web: the D-S6 document did not write");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            STATE_UNAVAILABLE,
            "The session state could not be written.",
        )
    })?;
    bound(db, save_web_state(tx, user_id, &doc))
        .await
        .map_err(|err| failed(&err))
}

/// The `{id, name}` view of a course. `None` means the learner enrolled in none.
fn course_view(graph: &Curriculum, course: Option<&str>) -> Value {
    match course.and_then(|id| graph.course(id)) {
        Some(found) => json!({"id": found.id.as_str(), "name": found.name}),
        None => json!({"id": course, "name": Value::Null}),
    }
}

/// The ordered course journey with the enrolled one flagged (`api.py:784-787`).
fn journey(graph: &Curriculum, course: Option<&str>) -> Value {
    let mut courses: Vec<_> = graph.courses().iter().collect();
    courses.sort_by_key(|item| item.order);
    Value::Array(
        courses
            .into_iter()
            .map(|item| {
                json!({
                    "id": item.id.as_str(),
                    "name": item.name,
                    "current": Some(item.id.as_str()) == course,
                })
            })
            .collect(),
    )
}

/// The three dashboard counts (`due_counts`, `service.py:1382-1418`).
fn due_counts(
    model: &LearnerModel,
    graph: &Curriculum,
    cfg: &Config,
    t_us: i64,
    course: Option<&str>,
) -> (usize, usize, usize) {
    let states = &model.topics;
    let scope = course_scope(graph, course);
    let mastered = mastered_set(states, graph);
    let open_frontier = frontier(graph, &mastered).intersect(&scope);
    let no_test_prep: BTreeSet<String> = BTreeSet::new();
    let due = due_reviews(states, graph, cfg, t_us, &no_test_prep);
    let due_set: BTreeSet<&str> = due.iter().map(String::as_str).collect();
    let nearly = nearly_due(states, graph, cfg, t_us)
        .into_iter()
        .filter(|id| !due_set.contains(id.as_str()))
        .count();
    (open_frontier.indices().count(), due.len(), nearly)
}

// --------------------------------------------------------------------------- //
// GET /api/status
// --------------------------------------------------------------------------- //

/// The dashboard payload (`api.py:749-800`).
///
/// It is a pure read: no event is appended and no row is written.
pub async fn status(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let input = projection_input(content, now);

    let mut tx = begin(&state, user_id).await?;
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    tx.rollback().await.map_err(|err| failed(&err.into()))?;

    let model = projection.model;
    let view = projection.view;
    let stack = &view.enrollment_stack;
    let course = stack.last().map(String::as_str);
    let (frontier_count, due_count, nearly_count) = due_counts(
        &model,
        &content.curriculum,
        &content.cfg,
        now.micros(),
        course,
    );
    let placed = model.topics.values().any(|topic| {
        matches!(
            topic.status,
            cadus_core::event::TopicStatus::Placed | cadus_core::event::TopicStatus::Learning
        )
    }) || view.has_diagnostic;
    let quiz_due = quiz_is_due(
        Some(&model.quiz),
        &model.topics,
        &content.curriculum,
        &content.cfg,
        now.micros(),
        Some(&view.study_days()),
    );
    let drill_due = !schedule_drills(
        &model.topics,
        &content.curriculum,
        now.micros(),
        Some(&view.last_drill_at),
    )
    .is_empty();

    Ok(Json(json!({
        "course": course_view(&content.curriculum, course),
        "placed": placed,
        "courses": journey(&content.curriculum, course),
        // The F6 test-prep set lives on the `profiles` row, and no M5 unit
        // writes that row. The key stays on the wire with its "off" value.
        "test_prep": Value::Null,
        "xp": serde_json::to_value(&model.xp).unwrap_or(Value::Null),
        "velocity": serde_json::to_value(&model.velocity).unwrap_or(Value::Null),
        "quiz": serde_json::to_value(&model.quiz).unwrap_or(Value::Null),
        "pending_remediation":
            serde_json::to_value(&model.pending_remediation).unwrap_or(Value::Null),
        "quiz_due": quiz_due,
        "drill_due": drill_due,
        "frontier": frontier_count,
        "due_reviews": due_count,
        "nearly_due": nearly_count,
    })))
}

// --------------------------------------------------------------------------- //
// GET /api/graph
// --------------------------------------------------------------------------- //

/// The `?scope=` query of `GET /api/graph`.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct GraphQuery {
    /// `all`, a course id, or absent for the enrolled course.
    pub scope: Option<String>,
}

/// The curriculum map joined with this learner's per-topic state (`api.py:803`).
///
/// It is a pure read. The scope is a VIEW FILTER only: the learner whose state
/// joins is always the request tenant, so a scope can never select another one.
pub async fn graph(
    State(state): State<AppState>,
    Query(query): Query<GraphQuery>,
    Tenant(user_id): Tenant,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (wall, now) = now_pair();
    let input = projection_input(content, now);

    let mut tx = begin(&state, user_id).await?;
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    tx.rollback().await.map_err(|err| failed(&err.into()))?;

    let graph = &content.curriculum;
    let stack = &projection.view.enrollment_stack;
    let enrolled = stack.last().map(String::as_str);
    let scope = query.scope.as_deref();
    let selected: Vec<TopicIdx> = match scope {
        Some("all") => course_scope(graph, None).indices().collect(),
        Some(course) => {
            if graph.course(course).is_none() {
                return Err(ApiError::new(
                    StatusCode::NOT_FOUND,
                    UNKNOWN_COURSE,
                    format!("No course {course:?} is in the curriculum."),
                ));
            }
            course_scope(graph, Some(course)).indices().collect()
        }
        None => course_scope(graph, enrolled).indices().collect(),
    };

    let inside: BTreeSet<TopicIdx> = selected.iter().copied().collect();
    let default = TopicState::default();
    let mut modules: Vec<&str> = Vec::new();
    let mut nodes: Vec<Value> = Vec::with_capacity(selected.len());
    let mut edges: Vec<Value> = Vec::new();
    let mut mastered = 0_usize;

    for idx in &selected {
        let id = graph.id_of(*idx);
        let module = graph.module_of(*idx);
        if !modules.contains(&module) {
            modules.push(module);
        }
        let topic_state = projection.model.topics.get(id).unwrap_or(&default);
        if is_mastered(topic_state) {
            mastered += 1;
        }
        nodes.push(json!({
            "id": id,
            "name": graph.topic(*idx).map(|topic| topic.name.as_str()),
            "module": module,
            "course": graph.course_of(*idx),
            "status": serde_json::to_value(topic_state.status).unwrap_or(Value::Null),
            "ability": topic_state.ability,
        }));
        for prereq in graph.prerequisites(*idx) {
            if inside.contains(&prereq) {
                edges.push(json!({"from": graph.id_of(prereq), "to": id}));
            }
        }
    }

    Ok(Json(json!({
        "now": wall.to_rfc3339(),
        "scope": scope,
        "courses": journey(graph, enrolled),
        "modules": modules,
        "counts": {"nodes": nodes.len(), "edges": edges.len(), "mastered": mastered},
        "nodes": nodes,
        "edges": edges,
    })))
}

// --------------------------------------------------------------------------- //
// GET /api/modules
// --------------------------------------------------------------------------- //

/// The enrolled course's module names, in curriculum order (`api.py:2291-2305`).
pub async fn modules(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let input = projection_input(content, now);
    let mut tx = begin(&state, user_id).await?;
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    tx.rollback().await.map_err(|err| failed(&err.into()))?;

    let graph = &content.curriculum;
    let stack = &projection.view.enrollment_stack;
    let course = stack.last().map(String::as_str);
    let mut seen: Vec<&str> = Vec::new();
    for idx in course_scope(graph, course).indices() {
        let module = graph.module_of(idx);
        if !module.is_empty() && !seen.contains(&module) {
            seen.push(module);
        }
    }
    Ok(Json(json!({
        "course": course_view(graph, course),
        "modules": seen,
    })))
}

// --------------------------------------------------------------------------- //
// GET /api/export
// --------------------------------------------------------------------------- //

/// The learner's whole log as JSONL, one canonical event per line
/// (`api.py:2245-2269`).
///
/// The bytes are the ones `Event::from_json` reads, so the export round-trips
/// back through the event reader unchanged. Read-only: nothing is written.
pub async fn export(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
) -> Result<Response, ApiError> {
    let mut tx = begin(&state, user_id).await?;
    let events = bound(&state.db, load_events(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    tx.rollback().await.map_err(|err| failed(&err.into()))?;

    let mut body = String::new();
    for row in &events {
        let line = row.event.to_canonical_json().map_err(|err| {
            tracing::error!(error = %err, seq = row.seq, "cadus-web: an event did not serialize");
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                INTERNAL_ERROR,
                "The export could not be built.",
            )
        })?;
        body.push_str(&line);
        body.push('\n');
    }

    let disposition = format!("attachment; filename=\"cadus-export-{user_id}.jsonl\"");
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(EXPORT_MEDIA_TYPE),
    );
    if let Ok(value) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, value);
    }
    Ok(response)
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
    let content = content(&state)?;
    let course = body
        .as_ref()
        .and_then(|Json(value)| value.get("course"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if course.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "enroll requires a course id.",
        ));
    }
    let graph = &content.curriculum;
    if graph.course(&course).is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            UNKNOWN_COURSE,
            format!("No course {course:?} is in the curriculum."),
        ));
    }
    let slug = Slug::new(course.clone()).map_err(|err| {
        ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            err.to_string(),
        )
    })?;

    let (_, now) = now_pair();
    let input = projection_input(content, now);
    let mut tx = begin(&state, user_id).await?;
    bound(&state.db, lock_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    let event = Event::Enrolled(Enrolled {
        ts: now,
        session: projection.view.current_session.clone(),
        v: SchemaVersion,
        course: slug,
        reason: None,
        return_to: None,
    });
    bound(&state.db, append_event(&mut tx, user_id, &event, None))
        .await
        .map_err(|err| failed(&err))?;
    bound(&state.db, project_and_save(&mut tx, user_id, &input, None))
        .await
        .map_err(|err| failed(&err))?;
    // A new course makes every in-flight served problem stale.
    bound(&state.db, clear_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    let floor: Vec<&str> = {
        let mut ids: Vec<&str> = graph
            .mastery_floor(&course)
            .unwrap_or_default()
            .into_iter()
            .map(|idx| graph.id_of(idx))
            .collect();
        ids.sort_unstable();
        ids
    };
    Ok(Json(json!({
        "enrolled": course,
        "mastery_floor": floor,
        "floor_size": floor.len(),
    })))
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
    let content = content(&state)?;
    let (wall, now) = now_pair();
    let input = projection_input(content, now);

    let mut tx = begin(&state, user_id).await?;
    bound(&state.db, lock_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let before = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;

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
            bound(&state.db, append_event(&mut tx, user_id, &event, None))
                .await
                .map_err(|err| failed(&err))?;
            session
        }
    };

    let projection = bound(&state.db, project_and_save(&mut tx, user_id, &input, None))
        .await
        .map_err(|err| failed(&err))?;

    // (Re)bind the scratch to this session. The bind resets it on a drift.
    let mut scratch = read_state(&state.db, &mut tx, user_id).await?;
    scratch.bind(&session);
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    let model = projection.model;
    let stack = &projection.view.enrollment_stack;
    let (frontier_count, due_count, _) = due_counts(
        &model,
        &content.curriculum,
        &content.cfg,
        now.micros(),
        stack.last().map(String::as_str),
    );
    Ok(Json(json!({
        "session": session,
        "reopened": reopened,
        "xp": serde_json::to_value(&model.xp).unwrap_or(Value::Null),
        "frontier": frontier_count,
        "due_reviews": due_count,
    })))
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
    let content = content(&state)?;
    let (_, now) = now_pair();
    let input = projection_input(content, now);
    let asked_minutes = body
        .as_ref()
        .and_then(|Json(value)| value.get("minutes"))
        .and_then(Value::as_f64);

    let mut tx = begin(&state, user_id).await?;
    bound(&state.db, lock_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let before = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    let Some(session) = before.view.current_session.clone() else {
        tx.rollback().await.map_err(|err| failed(&err.into()))?;
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_OPEN_SESSION,
            "No session is open.",
        ));
    };

    let scratch = read_state(&state.db, &mut tx, user_id).await?;
    let minutes = match asked_minutes {
        Some(value) => value,
        None => (scratch.active_secs / 60.0 * 100.0).round() / 100.0,
    };
    let xp_earned = before.view.xp_in_session(&session);
    let event = Event::SessionEnd(SessionEnd {
        ts: now,
        session: Some(session.clone()),
        v: SchemaVersion,
        xp_earned,
        minutes,
    });
    bound(&state.db, append_event(&mut tx, user_id, &event, None))
        .await
        .map_err(|err| failed(&err))?;
    let projection = bound(&state.db, project_and_save(&mut tx, user_id, &input, None))
        .await
        .map_err(|err| failed(&err))?;
    bound(&state.db, clear_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    Ok(Json(json!({
        "session": session,
        "xp_earned": xp_earned,
        "minutes": minutes,
        "xp": serde_json::to_value(&projection.model.xp).unwrap_or(Value::Null),
        // The Anki route family defers past M5 (spec section 2), so nothing
        // fills `anki_queue` yet and the count is 0.
        "anki": {"pending": 0},
    })))
}

// --------------------------------------------------------------------------- //
// GET /api/session/plan
// --------------------------------------------------------------------------- //

/// The ordered session plan (`api.py:928-957`). It WRITES NOTHING.
///
/// The plan and the per-task progress are read in ONE transaction, so the two
/// cannot disagree, and the progress comes from
/// [`crate::state::WebState::plan_progress`], a plain lookup (trap W3).
pub async fn session_plan(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let (_, now) = now_pair();
    let input = projection_input(content, now);

    let mut tx = begin(&state, user_id).await?;
    bound(&state.db, lock_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    let Some(session) = projection.view.current_session.clone() else {
        tx.rollback().await.map_err(|err| failed(&err.into()))?;
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_OPEN_SESSION,
            "No session is open.",
        ));
    };
    let scratch = read_state(&state.db, &mut tx, user_id).await?;
    tx.rollback().await.map_err(|err| failed(&err.into()))?;

    let graph = &content.curriculum;
    let view = projection.view;
    let model = projection.model;
    let course = view.enrollment_stack.last().map(String::as_str);
    let plan = compose_plan(content, &view, &model, &session, now);

    let tasks: Vec<Value> = plan
        .tasks
        .iter()
        .map(|task| trim_task(task, graph, &scratch))
        .collect();
    let complete = is_course_complete(&model.topics, graph, course, None);

    Ok(Json(json!({
        "session": plan.session,
        "tasks": tasks,
        "quiz_due": plan.quiz_due,
        "constraints": {
            "lesson_ratio_ok": plan.constraints.lesson_ratio_ok,
            "lesson_ratio": plan.constraints.lesson_ratio,
            "throttle_ok": plan.constraints.throttle_ok,
            "reviews": plan.constraints.reviews,
            "lessons": plan.constraints.lessons,
        },
        "course_complete": complete,
        "frontier_blocked_until": plan
            .frontier_blocked_until
            .and_then(|stamp| DateTime::<Utc>::from_timestamp_micros(stamp.micros()))
            .map(|stamp| stamp.to_rfc3339()),
    })))
}

/// Compose the ordered plan of one open session (`api.py:928-945`).
///
/// It is the ONE derivation of the plan. `GET /api/session/plan` lists it and
/// the serve, teach, and hint routes of unit U7 look one task up in it, so a
/// second copy of this call order would let a listed task and a served task
/// disagree.
///
/// The call is PURE: it reads the session view, the learner model, and the
/// arena, and it writes nothing. Trap W3 makes that load-bearing for the plan
/// route. It reads NO event row: the six maps it needs are the cached
/// [`SessionView`] of the same `through_seq` as `model` (F15, F18).
pub(crate) fn compose_plan(
    content: &Content,
    view: &SessionView,
    model: &LearnerModel,
    session: &str,
    now: Timestamp,
) -> cadus_core::selector::SessionPlan {
    let course = view.enrollment_stack.last().map(String::as_str);
    let days = view.study_days();
    let closed = &view.closed_task_ids;
    let no_test_prep: BTreeSet<String> = BTreeSet::new();
    let mut sampler = SeededSampler::new(session_seed(session));

    let ctx = SessionContext::default()
        .with_session_id(session)
        .with_course(course)
        .with_pending_remediation(&model.pending_remediation)
        .with_quiz_state(Some(&model.quiz))
        .with_learned_at(Some(&view.learned_at))
        .with_last_drill_at(Some(&view.last_drill_at))
        .with_active_study_days(Some(&days))
        .with_quiz_streak(view.quiz_high_score_streak)
        .with_test_prep(&no_test_prep)
        .with_multistep(i64::try_from(closed.len()).unwrap_or(i64::MAX), closed);
    compose_session(
        &model.topics,
        &content.curriculum,
        &content.cfg,
        now.micros(),
        &mut sampler,
        &ctx,
    )
}

/// The quiz-sampler seed of one session (`service.py:1259`).
///
/// 1.0 reads the session id as one big-endian integer and takes it modulo 2^63.
/// The port folds the same bytes into 64 bits, so one session id always draws
/// the same quiz.
fn session_seed(session: &str) -> u64 {
    let mut seed: u64 = 0;
    for byte in session.as_bytes() {
        seed = seed.wrapping_mul(256).wrapping_add(u64::from(*byte));
    }
    seed
}

/// The client-safe view of one task (`_trim_task`, `api.py:988-1006`).
///
/// No exemplar and no expected answer ever reaches the client (Hard Rule, trap
/// W7). `progress` is the pure lookup of trap W3.
fn trim_task(task: &Task, graph: &Curriculum, scratch: &WebState) -> Value {
    let topic = task.topic.as_deref().and_then(|id| {
        let idx = graph.idx_of(id)?;
        Some(json!({
            "id": id,
            "name": graph.topic(idx).map(|found| found.name.as_str()),
            "module": graph.module_of(idx),
        }))
    });
    let (answered, done) = scratch.plan_progress(&task.task_id);
    json!({
        "task_id": task.task_id,
        "task_type": task.task_type.as_str(),
        "topic": topic,
        "kp": task.start_at_kp,
        "start_at_kp": task.start_at_kp,
        "n_problems": task.n_problems,
        "mix": task.mix,
        "component_topics": task.component_topics,
        "time_budget_secs": task.time_budget_secs,
        "difficulty_target": task.difficulty_target,
        "why": task.why,
        "progress": {"answered": answered, "done": done},
    })
}
