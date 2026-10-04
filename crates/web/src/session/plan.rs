//! `GET /api/session/plan`, and the ONE derivation of the ordered plan that
//! the serve, teach and hint routes of unit U7 look a task up in.

use std::collections::BTreeSet;

use cadus_core::curriculum::Curriculum;
use cadus_core::event::Timestamp;
use cadus_core::learner::LearnerModel;
use cadus_core::readiness::{Blocker, ReadinessSet};
use cadus_core::selector::{
    BlockedTask, SeededSampler, SessionContext, SessionPlan, Task, compose_session,
    gap_fill_chain_for_stack, is_course_complete, resolve_gap_fill_stack,
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
    let events = req
        .view_for_open_session(&mut tx, &mut view, &session)
        .await?;

    let graph = req.graph();
    let model = projection.model;
    let course = view.enrollment_stack.last().map(String::as_str);
    // The readiness of D-F5, read in the SAME transaction as the projection, so
    // the listed plan and the served plan cannot disagree.
    let readiness = req.readiness(&mut tx).await?;
    let mut plan = compose_plan(&req.content, &view, &model, &session, req.now, &readiness);
    crate::serve::restore_session_tasks(&mut plan, &scratch, &events, graph, &model);

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
    // A reload loses the grade reply that named a background proof grading;
    // the plan names it again so the session can show its result.
    let proof = crate::proof_grading::restore_field(&req.state, &mut tx, &events).await?;

    let mut body = json!({
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
    if let Some(proof) = proof {
        body["proof_grading"] = proof;
    }
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
/// The serving course and the cross-course gap fill of one composition
/// (PEDAGOGY 8, `selector.py:186-395`).
///
/// The enrolled course stays the RETURN course (`gap_return_to`); the stack
/// descends when the enrolled course's frontier cannot serve because
/// lower-course prerequisite topics are unknown, and the plan then serves the
/// chain's lessons until every blocker is known. Without this the wall of the
/// 2026-09-24 staging walks: linear-algebra consumes prerequisite topics of
/// precalculus, geometry, proofs and calculus-1 that the mastery floor never
/// credits, the course-restricted frontier empties, and the plan goes empty
/// with 38 topics still unpassed.
fn gap_context(
    content: &Content,
    model: &LearnerModel,
    enrolled: Option<&str>,
) -> (Option<String>, Option<BTreeSet<String>>) {
    let stack = resolve_gap_fill_stack(
        &model.topics,
        &content.curriculum,
        &content.cfg,
        0,
        enrolled,
    );
    let (tip, parents) = match stack.split_last() {
        Some(split) if !split.1.is_empty() => split,
        _ => return (enrolled.map(str::to_owned), None),
    };
    let Some(chain) = gap_fill_chain_for_stack(&model.topics, &content.curriculum, &stack, None)
    else {
        return (enrolled.map(str::to_owned), None);
    };
    let mut names = BTreeSet::new();
    for idx in chain.indices() {
        names.insert(content.curriculum.id_of(idx).to_owned());
    }
    if names.is_empty() {
        return (enrolled.map(str::to_owned), None);
    }
    (Some(tip.as_str().to_owned()), Some(names))
}

pub(crate) fn compose_plan(
    content: &Content,
    view: &SessionView,
    model: &LearnerModel,
    session: &str,
    now: Timestamp,
    readiness: &ReadinessSet,
) -> SessionPlan {
    let enrolled = view.enrollment_stack.last().map(String::as_str);
    let (serving_course, gap_chain) = gap_context(content, model, enrolled);
    let course = serving_course.as_deref();
    let days = view.study_days();
    let closed = &view.closed_task_ids;
    let no_test_prep: BTreeSet<String> = BTreeSet::new();
    let mut sampler = SeededSampler::new(session_seed(session));

    let mut ctx = SessionContext::default()
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
    if let Some(chain) = &gap_chain {
        ctx.gap_fill_chain = Some(chain);
        ctx.gap_return_to = enrolled;
    }
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

#[cfg(test)]
mod gap_tests {
    use super::*;
    use std::collections::BTreeMap;
    use cadus_core::config::Config;
    use crate::state::Content;
    use cadus_core::curriculum::{Curriculum, load_curriculum};
    use cadus_core::event::{TaskType, Timestamp, TopicStatus};
    use cadus_core::learner::TopicState;
    use cadus_core::selector::{is_course_complete, known_set};
    use cadus_store::state::SessionView;

    /// The learner of the 2026-09-24 staging walk at its stop: 37 passed
    /// linear-algebra topics, the mastery floor of foundations credited, every
    /// other topic untouched. The enrolled course's frontier is empty and 38
    /// topics stand unpassed — the plan composed empty (the walk's stop line).
    fn walker_model(graph: &Curriculum) -> LearnerModel {
        let passed = [
            "augmented-matrix-representation",
            "component-form-of-vectors",
            "back-substitution-triangular-systems",
            "vector-arithmetic",
            "elementary-row-operations",
            "computing-dot-products",
            "echelon-form-recognition",
            "linear-combinations-of-vectors",
            "matrix-addition-scalar-multiplication",
            "vector-norms-unit-vectors",
            "computing-matrix-vector-products",
            "transpose-of-a-matrix",
            "row-reduction-echelon-forms",
            "consistency-of-linear-systems",
            "matrix-multiplication",
            "matrix-vector-equations",
            "polynomial-curve-fitting",
            "identity-zero-matrices",
            "solution-sets-free-variables",
            "matrix-operations",
            "homogeneous-systems",
            "diagonal-triangular-matrices",
            "linear-systems-applications",
            "matrix-inverse-2x2-formula",
            "determinants-2x2",
            "transpose-symmetric-matrices",
            "matrix-powers",
            "determinants",
            "matrix-inverses",
            "characteristic-polynomial",
            "finding-eigenvectors",
            "elementary-matrices-invertibility",
            "determinant-properties-cramers-rule",
            "matrix-equations",
            "determinants-area-volume",
            "eigenvalues-eigenvectors",
            "determinants-row-reduction",
        ];
        let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
        for index in 0..graph.topic_count() {
            let idx = cadus_core::curriculum::TopicIdx::from_u32(index as u32);
            let id = graph.id_of(idx);
            let status = match graph.course_of(idx) {
                "linear-algebra" if passed.contains(&id) => TopicStatus::Learning,
                "foundations" => TopicStatus::Floor,
                _ => continue,
            };
            topics.insert(id.to_owned(), TopicState { status, ..TopicState::default() });
        }
        LearnerModel { topics, ..LearnerModel::default() }
    }

    fn content_of(graph: Curriculum) -> Content {
        Content::new(graph)
    }

    #[test]
    fn the_empty_frontier_descends_into_the_gap_course_and_serves_its_lessons() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
        let (curriculum, _findings) = load_curriculum(&root).expect("the tree loads");
        let model = walker_model(&curriculum);
        assert!(
            !is_course_complete(&model.topics, &curriculum, &Config::default(),
                Some("linear-algebra"), None),
            "topics stand unpassed"
        );
        // The enrolled course's frontier is empty (the wall): no unpassed
        // linear-algebra topic has every prerequisite known.
        let frontier = cadus_core::selector::frontier(
            &curriculum,
            &known_set(&model.topics, &curriculum),
        );
        let in_course = frontier.indices().filter(|idx| {
            curriculum.course_of(*idx) == "linear-algebra"
        }).count();
        assert_eq!(in_course, 0, "the frontier of the enrolled course is empty");
        let (serving, chain) = gap_context(&content_of(curriculum.clone()), &model, Some("linear-algebra"));
        let curriculum = &curriculum;
        let tip = serving.expect("the stack descends into a lower course");
        assert_ne!(tip.as_str(), "linear-algebra");
        let chain = chain.expect("the chain names the blocking topics");
        assert!(!chain.is_empty());
        // The composed plan serves the gap course's lessons.
        let view = SessionView {
            enrollment_stack: vec!["linear-algebra".to_owned()],
            ..SessionView::default()
        };
        let plan = compose_plan(
            &content_of(curriculum.clone()),
            &view,
            &model,
            "s_2026-09-24test",
            Timestamp::from_micros(1_784_031_400_000_000),
            &ReadinessSet::default(),
        );
        let outside: Vec<String> = plan
            .tasks
            .iter()
            .filter(|task| task.task_type == TaskType::Lesson)
            .filter_map(|task| task.topic.clone())
            .filter(|topic| {
                curriculum
                    .idx_of(topic)
                    .is_some_and(|idx| curriculum.course_of(idx) != "linear-algebra")
            })
            .collect();
        assert!(
            !outside.is_empty(),
            "the gap fill serves lessons where the old compose served none"
        );
    }

}
