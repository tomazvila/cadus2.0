//! `POST /api/quiz/round`: the learner starts a quiz round of a chosen length
//! and scope (D-QZ1).
//!
//! The route composes the round with the same strata and the same seeded draw as
//! the due quiz. It records the task in the scratch document and appends NO
//! event. `restore_session_tasks` puts the task back in every plan of the
//! session. The quiz keeps its rules: the server owns the clock, no feedback
//! shows before the batch reveal, and a score under 0.8 sets the retake wait.
//! No model call stands on this path.

use axum::Json;
use axum::http::StatusCode;
use cadus_core::selector::{
    QUIZ_LENGTHS, QuizRequest, SeededSampler, quiz_composer_for, quiz_retake_available_at,
    quiz_task_named, utc_date,
};
use serde_json::{Value, json};

use super::plan::{session_seed, trim_task};
use super::store::{Ready, Reply, no_open_session, reply_committed};
use crate::error::ApiError;
use crate::state::{INVALID_REQUEST, RecordedReview, RecordedRound, WebState};

/// The `length` field of the body, when it is 5, 10 or 20.
fn asked_length(body: Option<&Json<Value>>) -> Option<usize> {
    body.and_then(|Json(value)| value.get("length"))
        .and_then(Value::as_u64)
        .and_then(|length| usize::try_from(length).ok())
        .filter(|length| QUIZ_LENGTHS.contains(length))
}

/// The `scope` field of the body, when it is `course` or `all`.
fn asked_scope(body: Option<&Json<Value>>) -> Option<&'static str> {
    match body
        .and_then(|Json(value)| value.get("scope"))
        .and_then(Value::as_str)
    {
        Some("course") => Some("course"),
        Some("all") => Some("all"),
        _ => None,
    }
}

fn invalid(message: &'static str) -> ApiError {
    ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, INVALID_REQUEST, message)
}

/// The reply of one recorded round.
fn round_body(
    recorded: &RecordedRound,
    graph: &cadus_core::curriculum::Curriculum,
    scratch: &WebState,
    resumed: bool,
) -> Value {
    let task = &recorded.task.0;
    let mut body = json!({
        "task": trim_task(task, graph, scratch),
        "round": {
            "length": recorded.length,
            "n": task.n_problems,
            "scope": recorded.scope,
            "course_name": recorded.course_name,
            "target_secs": task.time_budget_secs,
        },
    });
    if resumed {
        body["resumed"] = json!(true);
    }
    body
}

/// Start a quiz round. `422 invalid_request` for a bad length or scope,
/// `409 quiz_retake_wait`, `409 no_course`, or `409 no_topics_for_round`.
pub async fn start_round(req: Ready, body: Option<Json<Value>>) -> Reply {
    let Some(length) = asked_length(body.as_ref()) else {
        return Err(invalid("length must be 5, 10 or 20."));
    };
    let Some(scope) = asked_scope(body.as_ref()) else {
        return Err(invalid("scope must be course or all."));
    };
    let input = req.input();
    let (mut tx, projection) = req.locked_projection(&input).await?;
    let session = projection
        .view
        .current_session
        .clone()
        .ok_or_else(no_open_session)?;
    let model = &projection.model;
    if let Some(at) = quiz_retake_available_at(Some(&model.quiz))
        && utc_date(req.now.micros()).is_some_and(|today| today < at)
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "quiz_retake_wait",
            format!("You can try another quiz on {at}. A quiz under 80% waits one day."),
        ));
    }
    let graph = req.graph();
    let mut scratch = req.read_state(&mut tx).await?;
    let prefix = format!("{session}-round-");
    let open: Option<String> = scratch
        .rounds
        .keys()
        .filter(|id| id.starts_with(&prefix))
        .find(|id| {
            let (answered, done) = scratch.plan_progress(id);
            !done && answered > 0
        })
        .cloned();
    if let Some(recorded) = open.and_then(|id| scratch.rounds.get(&id)) {
        let body = round_body(recorded, graph, &scratch, true);
        return reply_committed(tx, body).await;
    }
    // A round with no answer yet is replaced, and so is its progress.
    let stale: Vec<String> = scratch
        .rounds
        .keys()
        .filter(|id| id.starts_with(&prefix) && !scratch.plan_progress(id).1)
        .cloned()
        .collect();
    for id in &stale {
        scratch.rounds.remove(id);
        scratch.tasks.remove(id);
        scratch.served.remove(id);
        scratch.quizzes.remove(id);
    }
    let course_id = if scope == "course" {
        let Some(id) = projection.view.enrollment_stack.last() else {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "no_course",
                "Pick a course first.",
            ));
        };
        Some(id.as_str())
    } else {
        None
    };
    let course_name = course_id
        .and_then(|id| graph.course(id))
        .map(|course| course.name.clone());
    let round_no = scratch.rounds.len() + 1;
    let mut sampler =
        SeededSampler::new(session_seed(&session) ^ u64::try_from(round_no).unwrap_or(0));
    let plan = quiz_composer_for(
        &model.topics,
        graph,
        &req.content.cfg,
        req.now.micros(),
        &mut sampler,
        Some(&projection.view.learned_at),
        &QuizRequest {
            length,
            course: course_id,
        },
    );
    if plan.questions.is_empty() {
        let message = match &course_name {
            Some(name) => format!(
                "You have no answered topics in {name} yet. Answer the starting questions first, or choose All my courses."
            ),
            None => {
                "You have no answered topics yet. Answer the starting questions first.".to_owned()
            }
        };
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "no_topics_for_round",
            message,
        ));
    }
    let task = quiz_task_named(
        &plan,
        projection.view.quiz_high_score_streak,
        &format!("{prefix}{round_no}"),
    );
    let recorded = RecordedRound {
        task: RecordedReview(task),
        length: i64::try_from(length).unwrap_or(0),
        scope: scope.to_owned(),
        course: course_id.map(str::to_owned),
        course_name,
    };
    let id = recorded.task.0.task_id.clone();
    if scratch.session.is_none() {
        scratch.session = Some(session);
    }
    scratch.rounds.insert(id, recorded.clone());
    req.write_state(&mut tx, &scratch).await?;
    let body = round_body(&recorded, graph, &scratch, false);
    reply_committed(tx, body).await
}
