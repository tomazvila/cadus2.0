//! `GET /api/session/plan`, and the ONE derivation of the ordered plan that
//! the serve, teach and hint routes of unit U7 look a task up in.

use std::collections::BTreeSet;

use cadus_core::curriculum::Curriculum;
use cadus_core::event::Timestamp;
use cadus_core::learner::LearnerModel;
use cadus_core::readiness::{Blocker, ReadinessSet};
use cadus_core::selector::{
    BlockedTask, SeededSampler, SessionContext, SessionPlan, Task, compose_session,
    is_course_complete,
};
use cadus_store::state::SessionView;
use serde_json::{Value, json};
use sqlx::types::chrono::{DateTime, Utc};

use super::store::{Ready, Reply, no_open_session, reply_read};
use crate::state::{Content, WebState};

/// The ordered session plan (`api.py:928-957`). It WRITES NOTHING.
///
/// The plan and the per-task progress are read in ONE transaction, so the two
/// cannot disagree, and the progress comes from
/// [`crate::state::WebState::plan_progress`], a plain lookup (trap W3).
pub async fn session_plan(req: Ready) -> Reply {
    let (mut tx, projection) = req.locked_projection(&req.input()).await?;
    let session = projection
        .view
        .current_session
        .clone()
        .ok_or_else(no_open_session)?;
    let scratch = req.read_state(&mut tx).await?;
    let mut view = projection.view;
    // The listing composes from the SAME repaired view the serve route composes
    // from (V3, V9): a drill this session already serves keeps its place.
    req.view_for_open_session(&mut tx, &mut view, &session)
        .await?;

    let graph = req.graph();
    let model = projection.model;
    let course = view.enrollment_stack.last().map(String::as_str);
    // The readiness of D-F5, read in the SAME transaction as the projection, so
    // the listed plan and the served plan cannot disagree.
    let readiness = req.readiness(&mut tx).await?;
    let mut plan = compose_plan(&req.content, &view, &model, &session, req.now, &readiness);
    crate::serve::restore_feedback_tasks(&mut plan, &scratch);

    let tasks: Vec<Value> = plan
        .tasks
        .iter()
        .map(|task| {
            let mut value = trim_task(task, graph, &scratch);
            value["integrated_instruction_required"] =
                json!(crate::integrated::instruction::required(&req.content, task));
            if task
                .integrated_assessment_of
                .as_ref()
                .and_then(|source| model.integrated_journey.assessments.get(source))
                .is_some_and(|held| held.completed)
            {
                value["progress"]["done"] = json!(true);
            }
            value
        })
        .collect();
    let complete = is_course_complete(&model.topics, graph, &req.content.cfg, course, None);

    let body = json!({
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
        "blocked": plan.blocked.iter().map(blocked_json).collect::<Vec<Value>>(),
        "frontier_blocked_until": plan
            .frontier_blocked_until
            .and_then(|stamp| DateTime::<Utc>::from_timestamp_micros(stamp.micros()))
            .map(|stamp| stamp.to_rfc3339()),
    });
    reply_read(tx, body).await
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
    readiness: &ReadinessSet,
) -> SessionPlan {
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
        .with_multistep(i64::try_from(closed.len()).unwrap_or(i64::MAX), closed)
        .with_readiness(Some(readiness))
        // f19-retention: the delayed probe of D-F11. The state is the fold's, so
        // the schedule reads the probes that already ran and never repeats one.
        .with_retention(Some(&model.retention));
    let mut plan = compose_session(
        &model.topics,
        &content.curriculum,
        &content.cfg,
        now.micros(),
        &mut sampler,
        &ctx,
    );
    if let Some(task) = model
        .integrated_journey
        .task(&content.integrated, session, now)
    {
        plan.tasks.push(task);
    }
    plan
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

/// One blocked task, as the SPA reads it (D-F5).
///
/// The list names the topic, the knowledge point, and the conditions the
/// content does not meet. Nothing here is a served problem, so Hard Rule 1
/// stands: no statement, no answer, and no solution sketch.
fn blocked_json(task: &BlockedTask) -> Value {
    json!({
        "task_type": task.task_type.as_str(),
        "topic": task.topic,
        "kp": task.kp,
        "blockers": task
            .blockers
            .iter()
            .map(|blocker| Blocker::as_str(*blocker))
            .collect::<Vec<&str>>(),
    })
}

/// The client-safe view of one task (`_trim_task`, `api.py:988-1006`).
///
/// No exemplar and no expected answer ever reaches the client (Hard Rule, trap
/// W7). `progress` is the pure lookup of trap W3.
fn trim_task(task: &Task, graph: &Curriculum, scratch: &WebState) -> Value {
    let topic = task.topic.as_deref().and_then(|id| {
        graph.idx_of(id).map(|idx| {
            json!({
                "id": id,
                "name": graph.topic(idx).map(|found| found.name.as_str()),
                "module": graph.module_of(idx),
            })
        })
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
        // D-F6: the client labels a confirmation item, and it never reads a
        // scheduling decision back out of `why`.
        "confirm": task.confirm,
        "integrated_assessment": task.integrated_assessment_of.is_some(),
        "progress": {"answered": answered, "done": done},
    })
}
