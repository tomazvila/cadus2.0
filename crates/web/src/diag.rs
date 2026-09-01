//! The placement diagnostic: `POST /api/diag/start`, `/api/diag/answer` and
//! `/api/diag/finish` (`api.py:1896-2091`, spec section 2).
//!
//! The three rows sat in the spec table marked `keep` while no unit owned them:
//! M5 built no route, and the M6 placement screen shipped against
//! `SPEC_ROUTES_ABSENT`. This module closes that gap. The pedagogy is
//! `cadus_core::diagnostic`; the work here is the transport, the scratch and the
//! two events.
//!
//! THE VERDICT IS DETERMINISTIC AND THIS ROUTE FAMILY CALLS NO MODEL (R4, L6,
//! spec section 2 "deterministic-only verdict"). `start` therefore drops from the
//! probe set every topic whose `answer_kind` has no deterministic checker, so the
//! diagnostic never asks a question it cannot mark. 1.0 sent such a probe to its
//! grading engine; this service asks no model on a request path, so the question
//! is not asked at all.
//!
//! ONE TRANSACTION PER CALL. 1.0 splits `answer` into three phases, because it
//! held the tenant lock across a model round trip. The verdict here is a local
//! `check`, so the scratch save, the fold and the appended event stay in one
//! transaction. A crash then leaves neither a folded balance with no event nor an
//! event with no balance (1.0 finding F-10-1).
//!
//! THE SENTINEL TASK ID. The diagnostic is a single-track task: the served probe
//! lives at `served["diag"]` of the D-S6 document, so a new probe replaces the
//! one before it and a stale `problem_id` answers `404 unknown_problem`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use cadus_core::config::Config;
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_core::diagnostic::{self, DiagState};
use cadus_core::event::{
    DiagnosticAnswer, DiagnosticPlaced, Enrolled, Event, SchemaVersion, Secs, Slug, Weight,
};
use cadus_core::pool::PoolAnswer;
use cadus_core::selector::{course_scope, frontier, mastered_set};
use cadus_store::state::{
    Projection, append_event, clear_diag_state, load_diag_state, lock_web_state, project_and_save,
    project_current, save_diag_state,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

use crate::AppState;
use crate::error::ApiError;
use crate::grade::{deterministic_grade, measure_secs};
use crate::serve::unix_seconds;
use crate::session::{
    begin, bound, content, failed, now_pair, projection_input, read_state, write_state,
};
use crate::state::{
    INVALID_REQUEST, ServedProblem, Tenant, UNKNOWN_COURSE, UNKNOWN_PROBLEM, WebState,
};

/// The code of a diagnostic call made with no diagnostic open.
pub const NO_DIAGNOSTIC: &str = "no_diagnostic";

/// The code of a start with no course to diagnose.
pub const NO_COURSE: &str = "no_course";

/// The task id the served probe hangs on.
const DIAG_TASK_ID: &str = "diag";

/// The text a topic with no exemplar shows. It matches 1.0 character for
/// character, because the placement screen renders whatever it is given.
const NO_EXEMPLAR: &str = "(no exemplar)";

/// Whether this answer kind has a deterministic checker.
const fn deterministic(kind: AnswerKind) -> bool {
    matches!(kind, AnswerKind::Numeric | AnswerKind::Expression)
}

/// The `answer_kind` string the D-S6 document carries.
const fn kind_name(kind: AnswerKind) -> &'static str {
    match kind {
        AnswerKind::Numeric => "numeric",
        AnswerKind::Expression => "expression",
        AnswerKind::MultiStep => "multi-step",
        AnswerKind::Proof => "proof",
    }
}

/// The answer kind of one topic, or `None` when the arena does not hold it.
fn topic_kind(graph: &Curriculum, topic: &str) -> Option<AnswerKind> {
    graph
        .idx_of(topic)
        .and_then(|idx| graph.topic(idx))
        .map(|record| record.answer_kind)
}

/// Serve the next probe, storing it as the live problem.
///
/// The return value is the `probe` object of the spec, or `None` when the probe
/// list is exhausted. The caller answers `{"probe": null}` or `{"done": true}`
/// from that, which the screen reads.
fn serve_probe(
    diag: &DiagState,
    graph: &Curriculum,
    cfg: &Config,
    scratch: &mut WebState,
    started_at: f64,
) -> Option<Value> {
    let topic = diagnostic::next_probe(diag, graph, cfg)?;
    let record = graph.idx_of(&topic).and_then(|idx| graph.topic(idx))?;
    let exemplar = record.diagnostic_exemplar.as_ref();
    let problem_id = Uuid::new_v4().simple().to_string();
    let text = exemplar.map_or_else(|| NO_EXEMPLAR.to_owned(), |item| item.problem.clone());
    scratch.served.insert(
        DIAG_TASK_ID.to_owned(),
        ServedProblem {
            problem_id: problem_id.clone(),
            task_id: DIAG_TASK_ID.to_owned(),
            topic: Some(topic.clone()),
            serve_topic: Some(topic.clone()),
            kp: None,
            answer_kind: Some(kind_name(record.answer_kind).to_owned()),
            text: text.clone(),
            expected: PoolAnswer {
                v: cadus_core::pool::POOL_ROW_VERSION,
                answer: exemplar.map(|item| item.answer.clone()).unwrap_or_default(),
            },
            solution_sketch: exemplar.and_then(|item| item.solution_sketch.clone()),
            started_at,
            hints_given: Vec::new(),
            index: diag.answered.len() as i64,
            rework: None,
        },
    );
    Some(json!({ "topic": topic, "problem_id": problem_id, "text": text }))
}

/// Read the open diagnostic, or answer `409 no_diagnostic`.
fn open_diagnostic(doc: Option<Value>) -> Result<DiagState, ApiError> {
    let Some(doc) = doc else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_DIAGNOSTIC,
            "No diagnostic is in progress; POST /api/diag/start.",
        ));
    };
    serde_json::from_value(doc).map_err(|err| {
        tracing::error!(error = %err, "cadus-web: the diagnostic document did not read");
        ApiError::new(
            StatusCode::CONFLICT,
            NO_DIAGNOSTIC,
            "No diagnostic is in progress; POST /api/diag/start.",
        )
    })
}

/// Write the diagnostic document.
fn diag_doc(diag: &DiagState) -> Result<Value, ApiError> {
    serde_json::to_value(diag).map_err(|err| {
        tracing::error!(error = %err, "cadus-web: the diagnostic document did not write");
        ApiError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            crate::state::STATE_UNAVAILABLE,
            "The diagnostic state could not be written.",
        )
    })
}

/// The enrolled course of one projection.
fn enrolled_course(projection: &Projection) -> Option<String> {
    projection.view.enrollment_stack.last().cloned()
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
pub async fn start(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let graph = &content.curriculum;
    let asked_course = body
        .as_ref()
        .and_then(|Json(value)| value.get("course"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    let (_, now) = now_pair();
    let input = projection_input(content, now);
    let mut tx = begin(&state, user_id).await?;
    bound(&state.db, lock_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;

    let enrolled = enrolled_course(&projection);
    let first_run = enrolled.is_none() && asked_course.is_none();
    let course = match asked_course.clone().or(enrolled) {
        Some(course) => course,
        None => graph
            .courses()
            .iter()
            .min_by_key(|course| (course.order, course.id.as_str().to_owned()))
            .map(|course| course.id.as_str().to_owned())
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    NO_COURSE,
                    "No course is available to diagnose; enroll or name a course.",
                )
            })?,
    };
    if graph.course(&course).is_none() {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            UNKNOWN_COURSE,
            format!("No course {course:?} is in the curriculum."),
        ));
    }

    if first_run {
        // Bind the entry course, so the placement and the frontier readout after
        // it see one enrolled course. Same append as `POST /api/enroll`.
        let slug = Slug::new(course.clone()).map_err(|err| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                INVALID_REQUEST,
                err.to_string(),
            )
        })?;
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
    }

    let mut diag = diagnostic::init_session(graph, &content.cfg, Some(&course));
    // The verdict is deterministic, so a probe this service cannot mark is never
    // asked. See the module note.
    diag.probe_set
        .retain(|topic| topic_kind(graph, topic).is_some_and(deterministic));

    let mut scratch = read_state(&state.db, &mut tx, user_id).await?;
    scratch.served.remove(DIAG_TASK_ID);
    let started_at = unix_seconds(now.micros());
    let probe = serve_probe(&diag, graph, &content.cfg, &mut scratch, started_at);

    let doc = diag_doc(&diag)?;
    bound(&state.db, save_diag_state(&mut tx, user_id, &doc))
        .await
        .map_err(|err| failed(&err))?;
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    Ok(Json(json!({
        "probe": probe,
        "asked": diag.answered.len(),
        "cap": content.cfg.diag.max_questions,
    })))
}

// --------------------------------------------------------------------------- //
// POST /api/diag/answer
// --------------------------------------------------------------------------- //

/// Mark one probe and serve the next.
///
/// The reply carries the verdict and the next probe, and it carries NEITHER the
/// expected answer NOR a solution. A placement that shows the answer measures
/// nothing after the first probe.
pub async fn answer(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
    body: Option<Json<Value>>,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let graph = &content.curriculum;
    let body = body.map(|Json(value)| value).unwrap_or_default();
    let problem_id = body
        .get("problem_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if problem_id.is_empty() {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            INVALID_REQUEST,
            "A diagnostic answer requires a problem_id.",
        ));
    }
    let submitted = body
        .get("answer")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let (_, now) = now_pair();
    let input = projection_input(content, now);
    let mut tx = begin(&state, user_id).await?;
    bound(&state.db, lock_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let doc = bound(&state.db, load_diag_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let mut diag = open_diagnostic(doc)?;

    let mut scratch = read_state(&state.db, &mut tx, user_id).await?;
    let served = match scratch.served.get(DIAG_TASK_ID) {
        Some(served) if served.problem_id == problem_id => served.clone(),
        _ => {
            return Err(ApiError::new(
                StatusCode::NOT_FOUND,
                UNKNOWN_PROBLEM,
                format!("Probe {problem_id:?} was not served."),
            ));
        }
    };
    let Some(topic) = served.topic.clone() else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_DIAGNOSTIC,
            "No diagnostic is in progress; POST /api/diag/start.",
        ));
    };
    if !diag.in_universe(&topic) {
        // The scope moved under an open diagnostic: a start for another course
        // ran between the serve and this answer.
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_DIAGNOSTIC,
            "No diagnostic is in progress; POST /api/diag/start.",
        ));
    }
    let Some(kind) = topic_kind(graph, &topic).filter(|kind| deterministic(*kind)) else {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            NO_DIAGNOSTIC,
            "No diagnostic is in progress; POST /api/diag/start.",
        ));
    };

    let expected_time = graph
        .idx_of(&topic)
        .and_then(|idx| graph.topic(idx))
        .map(|record| record.expected_time_secs);
    let (secs, _) = measure_secs(served.started_at, now.micros(), expected_time);
    let grade = deterministic_grade(&served.expected.answer, &submitted, kind);

    let weight = diagnostic::answer_weight(
        grade.correct,
        expected_time.unwrap_or_default() as f64,
        secs as f64,
    );
    diagnostic::apply_answer(
        &mut diag,
        graph,
        &topic,
        grade.correct,
        weight,
        &content.cfg,
    );

    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    let event = Event::DiagnosticAnswer(DiagnosticAnswer {
        ts: now,
        session: projection.view.current_session.clone(),
        v: SchemaVersion,
        topic: Slug::new(topic.clone()).map_err(|err| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                INVALID_REQUEST,
                err.to_string(),
            )
        })?,
        correct: grade.correct,
        secs: Secs::new(secs).map_err(|err| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                INVALID_REQUEST,
                err.to_string(),
            )
        })?,
        weight: Weight::new(weight).map_err(|err| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                INVALID_REQUEST,
                err.to_string(),
            )
        })?,
    });
    bound(&state.db, append_event(&mut tx, user_id, &event, None))
        .await
        .map_err(|err| failed(&err))?;

    scratch.served.remove(DIAG_TASK_ID);
    let started_at = unix_seconds(now.micros());
    let next = serve_probe(&diag, graph, &content.cfg, &mut scratch, started_at);

    let doc = diag_doc(&diag)?;
    bound(&state.db, save_diag_state(&mut tx, user_id, &doc))
        .await
        .map_err(|err| failed(&err))?;
    write_state(&state.db, &mut tx, user_id, &scratch).await?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    let next_probe = next.unwrap_or_else(|| json!({ "done": true }));
    Ok(Json(
        json!({ "correct": grade.correct, "next_probe": next_probe }),
    ))
}

// --------------------------------------------------------------------------- //
// POST /api/diag/finish
// --------------------------------------------------------------------------- //

/// Close the diagnostic, place the learner, and report the frontier.
///
/// The append of `diagnostic_placed`, the fold, and the drop of the scratch are
/// ONE transaction. A crash between them would leave the diagnostic finishable a
/// second time, and a second `diagnostic_placed` folds into the model for good:
/// the event carries no `attempt_id`, so no index dedups it and the log is
/// append-only (1.0 BUG-2).
pub async fn finish(
    State(state): State<AppState>,
    Tenant(user_id): Tenant,
) -> Result<Json<Value>, ApiError> {
    let content = content(&state)?;
    let graph = &content.curriculum;

    let (_, now) = now_pair();
    let input = projection_input(content, now);
    let mut tx = begin(&state, user_id).await?;
    bound(&state.db, lock_web_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let doc = bound(&state.db, load_diag_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;
    let diag = open_diagnostic(doc)?;

    let result = diagnostic::placement(&diag, &content.cfg);
    let projection = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;

    let mut conditional: Vec<Slug> = Vec::with_capacity(result.conditional.len());
    for topic in &result.conditional {
        conditional.push(Slug::new(topic.clone()).map_err(|err| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                INVALID_REQUEST,
                err.to_string(),
            )
        })?);
    }
    let event = Event::DiagnosticPlaced(DiagnosticPlaced {
        ts: now,
        session: projection.view.current_session.clone(),
        v: SchemaVersion,
        balances: result.event_balances(),
        conditional,
        refresh: false,
    });
    bound(&state.db, append_event(&mut tx, user_id, &event, None))
        .await
        .map_err(|err| failed(&err))?;
    bound(&state.db, project_and_save(&mut tx, user_id, &input, None))
        .await
        .map_err(|err| failed(&err))?;
    bound(&state.db, clear_diag_state(&mut tx, user_id))
        .await
        .map_err(|err| failed(&err))?;

    let mut scratch = read_state(&state.db, &mut tx, user_id).await?;
    scratch.served.remove(DIAG_TASK_ID);
    write_state(&state.db, &mut tx, user_id, &scratch).await?;

    // The readout comes from the model this transaction just wrote.
    let after = bound(&state.db, project_current(&mut tx, user_id, &input))
        .await
        .map_err(|err| failed(&err))?;
    tx.commit().await.map_err(|err| failed(&err.into()))?;

    let enrolled = enrolled_course(&after);
    let scope = course_scope(graph, enrolled.as_deref());
    let mastered = mastered_set(&after.model.topics, graph);
    let front = frontier(graph, &mastered);
    let frontier_ids: Vec<&str> = front
        .sorted_ids(graph)
        .into_iter()
        .filter(|id| scope.contains_id(graph, id))
        .collect();

    Ok(Json(json!({
        "placed": result.placed.keys().collect::<Vec<&String>>(),
        "conditional": result.conditional,
        "frontier": frontier_ids,
    })))
}
