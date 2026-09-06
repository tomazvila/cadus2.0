//! The four pure reads of unit U6: `GET /api/status`, `GET /api/graph`,
//! `GET /api/modules` and `GET /api/export`. No route here appends an event
//! or writes a row.

use std::collections::BTreeSet;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, TopicIdx};
use cadus_core::event::TopicStatus;
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_core::selector::{
    course_scope, due_reviews, frontier, is_mastered, mastered_set, nearly_due, quiz_is_due,
    schedule_drills,
};
use cadus_store::state::{EventRow, load_events, project_current};
use serde_json::{Value, json};

use super::EXPORT_MEDIA_TYPE;
use super::store::{Ready, Reply, begin, json_of, reply_read, store, unknown_course};
use crate::AppState;
use crate::error::ApiError;
use crate::state::Tenant;

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
pub(super) fn due_counts(
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

/// Whether one topic state counts as placed on the dashboard.
fn is_placed(topic: &TopicState) -> bool {
    matches!(topic.status, TopicStatus::Placed | TopicStatus::Learning)
}

// --------------------------------------------------------------------------- //
// GET /api/status
// --------------------------------------------------------------------------- //

/// The dashboard payload (`api.py:749-800`).
///
/// It is a pure read: no event is appended and no row is written.
pub async fn status(req: Ready) -> Reply {
    let input = req.input();
    let mut tx = req.begin().await?;
    let projection = req
        .store(project_current(&mut tx, req.user_id, &input))
        .await?;
    let model = projection.model;
    let mut view = projection.view;
    // The dashboard reads `drill_due` off the same repaired view the plan and
    // the serve read (V3, V9). A learner with no open session has no window to
    // read, and no drill of an open session to forget.
    if let Some(session) = view.current_session.clone() {
        req.view_for_open_session(&mut tx, &mut view, &session)
            .await?;
    }

    let graph = req.graph();
    let cfg = &req.content.cfg;
    let t_us = req.now.micros();
    let course = view.enrollment_stack.last().map(String::as_str);
    let (frontier_count, due_count, nearly_count) = due_counts(&model, graph, cfg, t_us, course);
    let placed = model.topics.values().any(is_placed) || view.has_diagnostic;
    let quiz_due = quiz_is_due(
        Some(&model.quiz),
        &model.topics,
        graph,
        cfg,
        t_us,
        Some(&view.study_days()),
    );
    let drill_due =
        !schedule_drills(&model.topics, graph, t_us, Some(&view.last_drill_at)).is_empty();

    let body = json!({
        "course": course_view(graph, course),
        "placed": placed,
        "courses": journey(graph, course),
        // The F6 test-prep set lives on the `profiles` row, and no M5 unit
        // writes that row. The key stays on the wire with its "off" value.
        "test_prep": Value::Null,
        "xp": json_of(&model.xp),
        "velocity": json_of(&model.velocity),
        "quiz": json_of(&model.quiz),
        "pending_remediation": json_of(&model.pending_remediation),
        "quiz_due": quiz_due,
        "drill_due": drill_due,
        "frontier": frontier_count,
        "due_reviews": due_count,
        "nearly_due": nearly_count,
    });
    reply_read(tx, body).await
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

/// The topics the `?scope=` query selects: the whole arena for `all`, a named
/// course, or the enrolled course.
fn scoped_topics(
    graph: &Curriculum,
    scope: Option<&str>,
    enrolled: Option<&str>,
) -> Result<Vec<TopicIdx>, ApiError> {
    let course = match scope {
        Some("all") => None,
        Some(course) => {
            if graph.course(course).is_none() {
                return Err(unknown_course(course));
            }
            Some(course)
        }
        None => enrolled,
    };
    Ok(course_scope(graph, course).indices().collect())
}

/// The nodes, the edges, the module list and the mastered count of one scope.
struct GraphView<'a> {
    modules: Vec<&'a str>,
    nodes: Vec<Value>,
    edges: Vec<Value>,
    mastered: usize,
}

/// Join the selected topics with the learner's per-topic state.
fn graph_view<'a>(
    graph: &'a Curriculum,
    model: &LearnerModel,
    selected: &[TopicIdx],
) -> GraphView<'a> {
    let inside: BTreeSet<TopicIdx> = selected.iter().copied().collect();
    let default = TopicState::default();
    let mut view = GraphView {
        modules: Vec::new(),
        nodes: Vec::with_capacity(selected.len()),
        edges: Vec::new(),
        mastered: 0,
    };
    for idx in selected {
        let id = graph.id_of(*idx);
        let module = graph.module_of(*idx);
        if !view.modules.contains(&module) {
            view.modules.push(module);
        }
        let topic_state = model.topics.get(id).unwrap_or(&default);
        if is_mastered(topic_state) {
            view.mastered += 1;
        }
        view.nodes.push(json!({
            "id": id,
            "name": graph.topic(*idx).map(|topic| topic.name.as_str()),
            "module": module,
            "course": graph.course_of(*idx),
            "status": json_of(&topic_state.status),
            "ability": topic_state.ability,
        }));
        for prereq in graph.prerequisites(*idx) {
            if inside.contains(&prereq) {
                view.edges
                    .push(json!({"from": graph.id_of(prereq), "to": id}));
            }
        }
    }
    view
}

/// The curriculum map joined with this learner's per-topic state (`api.py:803`).
///
/// It is a pure read. The scope is a VIEW FILTER only: the learner whose state
/// joins is always the request tenant, so a scope can never select another one.
pub async fn graph(req: Ready, Query(query): Query<GraphQuery>) -> Reply {
    let projection = req.read_projection(&req.input()).await?;
    let graph = req.graph();
    let enrolled = projection.view.enrollment_stack.last().map(String::as_str);
    let scope = query.scope.as_deref();
    let selected = scoped_topics(graph, scope, enrolled)?;
    let view = graph_view(graph, &projection.model, &selected);

    Ok(Json(json!({
        "now": req.wall.to_rfc3339(),
        "scope": scope,
        "courses": journey(graph, enrolled),
        "modules": view.modules,
        "counts": {
            "nodes": view.nodes.len(),
            "edges": view.edges.len(),
            "mastered": view.mastered,
        },
        "nodes": view.nodes,
        "edges": view.edges,
    })))
}

// --------------------------------------------------------------------------- //
// GET /api/modules
// --------------------------------------------------------------------------- //

/// The enrolled course's module names, in curriculum order (`api.py:2291-2305`).
pub async fn modules(req: Ready) -> Reply {
    let projection = req.read_projection(&req.input()).await?;
    let graph = req.graph();
    let course = projection.view.enrollment_stack.last().map(String::as_str);
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

/// The JSONL body of the log, one canonical event per line.
///
/// Every event of the log read through `Event::from_json`, and the canonical
/// writer serializes each variant of `Event` in full: every key is a string
/// and no value refuses. The default stands for a line that cannot occur.
fn export_body(events: &[EventRow]) -> String {
    let mut body = String::new();
    for row in events {
        body.push_str(&row.event.to_canonical_json().unwrap_or_default());
        body.push('\n');
    }
    body
}

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
    let events = store(&state, load_events(&mut tx, user_id)).await?;
    drop(tx);
    let body = export_body(&events);

    let disposition = format!("attachment; filename=\"cadus-export-{user_id}.jsonl\"");
    let mut response = (StatusCode::OK, body).into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(EXPORT_MEDIA_TYPE),
    );
    // A uuid is ASCII, so the disposition always reads as a header value.
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).unwrap_or(HeaderValue::from_static("attachment")),
    );
    Ok(response)
}
