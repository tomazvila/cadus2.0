//! The three handlers of the placement diagnostic, each one ONE transaction
//! under the tenant's advisory lock.

use axum::Json;
use axum::http::StatusCode;
use cadus_core::curriculum::{AnswerKind, Course, Curriculum, Slug};
use cadus_core::diagnostic::{self, DiagState, PlacementResult};
use cadus_core::event::Slug as EventSlug;
use cadus_core::event::{
    DiagnosticAnswer, DiagnosticPlaced, Event, SchemaVersion, Secs, Timestamp, Weight,
};
use cadus_core::selector::{course_scope, frontier, mastered_set};
use cadus_store::state::{Projection, clear_diag_state, project_current};
use serde_json::{Value, json};

use super::{
    DIAG_TASK_ID, deal_probe, deterministic, load_diagnostic, no_diagnostic, save_diagnostic,
    topic_kind, topic_record,
};
use crate::error::ApiError;
use crate::grade::{deterministic_grade, measure_secs};
use crate::session::{Ready, Reply, enrolled_event, event_slug, reply_committed, unknown_course};
use crate::state::{Content, INVALID_REQUEST, ServedProblem, UNKNOWN_PROBLEM, WebState};

/// The enrolled course of one projection.
fn enrolled_course(projection: &Projection) -> Option<String> {
    projection.view.enrollment_stack.last().cloned()
}

/// The `course` field of a start body.
fn asked_course(body: Option<&Json<Value>>) -> Option<String> {
    body.and_then(|Json(value)| value.get("course"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

/// The entry course of the journey: the lowest `order`, then the lowest id.
fn entry_course(graph: &Curriculum) -> Result<&Course, ApiError> {
    graph
        .courses()
        .iter()
        .min_by_key(|course| (course.order, course.id.as_str().to_owned()))
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                super::NO_COURSE,
                "No course is available to diagnose; enroll or name a course.",
            )
        })
}

/// The course a start diagnoses: the one named, then the enrolled one, then
/// the entry course of the journey.
fn chosen_course(
    graph: &Curriculum,
    asked: Option<String>,
    enrolled: Option<String>,
) -> Result<&Course, ApiError> {
    let Some(id) = asked.or(enrolled) else {
        return entry_course(graph);
    };
    graph.course(&id).ok_or_else(|| unknown_course(&id))
}

/// The diagnostic of `course` with every probe this service cannot mark
/// dropped. The verdict is deterministic, so such a probe is never asked. See
/// the module note.
fn markable_diagnostic(content: &Content, course: &str) -> DiagState {
    let graph = &content.curriculum;
    let mut diag = diagnostic::init_session(graph, &content.cfg, Some(course));
    diag.probe_set
        .retain(|topic| topic_kind(graph, topic).is_some_and(deterministic));
    diag
}

// --------------------------------------------------------------------------- //
// POST /api/diag/start
// --------------------------------------------------------------------------- //

/// Open a placement diagnostic and serve its first probe.
///
/// The course is the body's `course`, then the enrolled course. A learner with
/// neither is on the first run: the entry course of the journey (the lowest
/// `order`) is enrolled first, because the placement, the frontier readout and
/// every session after them key off an enrolled course. That is the same append
/// `POST /api/enroll` makes.
///
/// A second start REPLACES the diagnostic in progress, which is what 1.0 does:
/// the balances go back to zero and the probe list starts again.
pub async fn start(req: Ready, body: Option<Json<Value>>) -> Reply {
    let asked = asked_course(body.as_ref());
    let input = req.input();
    let (mut tx, projection) = req.locked_projection(&input).await?;

    let enrolled = enrolled_course(&projection);
    let first_run = enrolled.is_none() && asked.is_none();
    let course = chosen_course(req.graph(), asked, enrolled)?;
    if first_run {
        // Bind the entry course, so the placement and the frontier readout after
        // it see one enrolled course.
        let session = projection.view.current_session.clone();
        let event = enrolled_event(req.now, session, event_slug(&course.id)?);
        req.append_and_fold(&mut tx, &event, &input).await?;
    }

    let diag = markable_diagnostic(&req.content, course.id.as_str());
    let mut scratch = req.read_state(&mut tx).await?;
    let probe = deal_probe(&diag, &req.content, &mut scratch, req.now);
    save_diagnostic(&req.state, &mut tx, req.user_id, &diag, &scratch).await?;

    let body = json!({
        "probe": probe,
        "asked": diag.answered.len(),
        "cap": req.content.cfg.diag.max_questions,
    });
    reply_committed(tx, body).await
}

// --------------------------------------------------------------------------- //
// POST /api/diag/answer
// --------------------------------------------------------------------------- //

/// The `problem_id` and the `answer` of an answer body.
fn answer_body(body: Option<Json<Value>>) -> Result<(String, String), ApiError> {
    let body = body.map(|Json(value)| value).unwrap_or_default();
    let field = |name: &str| {
        body.get(name)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let problem_id = field("problem_id");
    if problem_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "A diagnostic answer requires a problem_id.",
        ));
    }
    Ok((problem_id, field("answer")))
}

/// The live probe, when `problem_id` names it.
fn served_probe(scratch: &WebState, problem_id: &str) -> Result<ServedProblem, ApiError> {
    match scratch.served.get(DIAG_TASK_ID) {
        Some(served) if served.problem_id == problem_id => Ok(served.clone()),
        _ => Err(ApiError::new(
            StatusCode::NOT_FOUND,
            UNKNOWN_PROBLEM,
            format!("Probe {problem_id:?} was not served."),
        )),
    }
}

/// The topic of the live probe and its answer kind, when the open diagnostic
/// still asks it.
///
/// A probe with no topic, a topic outside the universe (a start for another
/// course ran between the serve and this answer), and a topic this service
/// cannot mark are all `409 no_diagnostic`.
fn probe_topic<'a>(
    diag: &DiagState,
    graph: &'a Curriculum,
    served: &ServedProblem,
) -> Result<(&'a Slug, AnswerKind), ApiError> {
    let topic = served.topic.as_deref().ok_or_else(no_diagnostic)?;
    if !diag.in_universe(topic) {
        return Err(no_diagnostic());
    }
    let record = topic_record(graph, topic)
        .filter(|record| deterministic(record.answer_kind))
        .ok_or_else(no_diagnostic)?;
    Ok((&record.id, record.answer_kind))
}

/// The verdict of one probe and its weight.
struct Marked {
    correct: bool,
    secs: i64,
    weight: f64,
}

/// Mark one probe against its expected answer.
fn mark_probe(
    graph: &Curriculum,
    topic: &str,
    kind: AnswerKind,
    served: &ServedProblem,
    submitted: &str,
    now_micros: i64,
) -> Marked {
    let expected_time = topic_record(graph, topic).map(|record| record.expected_time_secs);
    let (secs, _) = measure_secs(served.started_at, now_micros, expected_time);
    let grade = deterministic_grade(&served.expected.answer, submitted, kind);
    let weight = diagnostic::answer_weight(
        grade.correct,
        expected_time.unwrap_or_default() as f64,
        secs as f64,
    );
    Marked {
        correct: grade.correct,
        secs,
        weight,
    }
}

/// The `diagnostic_answer` event of one marked probe.
///
/// `measure_secs` never counts below zero and `answer_weight` stays inside
/// 0.0 to 1.0, so the two scalar fallbacks never fire.
fn answer_event(
    now: Timestamp,
    session: Option<String>,
    topic: EventSlug,
    marked: &Marked,
) -> Event {
    Event::DiagnosticAnswer(DiagnosticAnswer {
        ts: now,
        session,
        v: SchemaVersion,
        topic,
        correct: marked.correct,
        secs: Secs::new(marked.secs).unwrap_or_default(),
        weight: Weight::new(marked.weight).unwrap_or_default(),
    })
}

/// Mark one probe and serve the next.
///
/// The reply carries the verdict and the next probe, and it carries NEITHER the
/// expected answer NOR a solution. A placement that shows the answer measures
/// nothing after the first probe.
pub async fn answer(req: Ready, body: Option<Json<Value>>) -> Reply {
    let graph = req.graph();
    let (problem_id, submitted) = answer_body(body)?;

    let mut tx = req.open_locked().await?;
    let mut diag = load_diagnostic(&req.state, &mut tx, req.user_id).await?;
    let mut scratch = req.read_state(&mut tx).await?;
    let served = served_probe(&scratch, &problem_id)?;
    let (topic, kind) = probe_topic(&diag, graph, &served)?;

    let marked = mark_probe(
        graph,
        topic.as_str(),
        kind,
        &served,
        &submitted,
        req.now.micros(),
    );
    diagnostic::apply_answer(
        &mut diag,
        graph,
        topic.as_str(),
        marked.correct,
        marked.weight,
        &req.content.cfg,
    );

    let projection = req
        .store(project_current(&mut tx, req.user_id, &req.input()))
        .await?;
    let session = projection.view.current_session.clone();
    let event = answer_event(req.now, session, event_slug(topic)?, &marked);
    req.append(&mut tx, &event).await?;

    let next = deal_probe(&diag, &req.content, &mut scratch, req.now);
    save_diagnostic(&req.state, &mut tx, req.user_id, &diag, &scratch).await?;

    let next_probe = next.unwrap_or_else(|| json!({ "done": true }));
    let body = json!({ "correct": marked.correct, "next_probe": next_probe });
    reply_committed(tx, body).await
}

// --------------------------------------------------------------------------- //
// POST /api/diag/finish
// --------------------------------------------------------------------------- //

/// The `diagnostic_placed` event of one placement. A conditional topic is a
/// non-empty id of the universe, so every one reads as a slug.
fn placed_event(now: Timestamp, session: Option<String>, result: &PlacementResult) -> Event {
    let conditional: Vec<EventSlug> = result
        .conditional
        .iter()
        .filter_map(|topic| EventSlug::new(topic.clone()).ok())
        .collect();
    Event::DiagnosticPlaced(DiagnosticPlaced {
        ts: now,
        session,
        v: SchemaVersion,
        balances: result.event_balances(),
        conditional,
        refresh: false,
    })
}

/// The frontier of the enrolled course after the placement, in id order.
fn frontier_readout<'a>(graph: &'a Curriculum, after: &Projection) -> Vec<&'a str> {
    let enrolled = enrolled_course(after);
    let scope = course_scope(graph, enrolled.as_deref());
    let mastered = mastered_set(&after.model.topics, graph);
    frontier(graph, &mastered)
        .sorted_ids(graph)
        .into_iter()
        .filter(|id| scope.contains_id(graph, id))
        .collect()
}

/// Close the diagnostic, place the learner, and report the frontier.
///
/// The append of `diagnostic_placed`, the fold, and the drop of the scratch are
/// ONE transaction. A crash between them would leave the diagnostic finishable a
/// second time, and a second `diagnostic_placed` folds into the model for good:
/// the event carries no `attempt_id`, so no index dedups it and the log is
/// append-only (1.0 BUG-2).
pub async fn finish(req: Ready) -> Reply {
    let input = req.input();
    let mut tx = req.open_locked().await?;
    let diag = load_diagnostic(&req.state, &mut tx, req.user_id).await?;
    let result = diagnostic::placement(&diag, &req.content.cfg);
    let projection = req
        .store(project_current(&mut tx, req.user_id, &input))
        .await?;
    let event = placed_event(req.now, projection.view.current_session.clone(), &result);
    req.append_and_fold(&mut tx, &event, &input).await?;
    req.store(clear_diag_state(&mut tx, req.user_id)).await?;

    let mut scratch = req.read_state(&mut tx).await?;
    scratch.served.remove(DIAG_TASK_ID);
    req.write_state(&mut tx, &scratch).await?;

    // The readout comes from the model this transaction just wrote.
    let after = req
        .store(project_current(&mut tx, req.user_id, &input))
        .await?;
    let body = json!({
        "placed": result.placed.keys().collect::<Vec<&String>>(),
        "conditional": result.conditional,
        "frontier": frontier_readout(req.graph(), &after),
    });
    reply_committed(tx, body).await
}
