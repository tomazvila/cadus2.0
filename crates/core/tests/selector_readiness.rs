//! The eligibility rule of D-F5 inside `compose_session`.
//!
//! A lesson is served only when its knowledge point is teachable, practicable
//! and assessable, where a knowledge point with decidable exemplars counts as
//! practicable and assessable for the lesson (note 103 b). A review, a quiz
//! question and a drill need `practicable`.
//! Every task the rule stops stands in `SessionPlan::blocked` with its reasons.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, Exemplar, KnowledgePoint};
use cadus_core::event::{Slug, TaskType, TopicStatus};
use cadus_core::instruction::{KIND_HINT_LADDER, KIND_TEACH};
use cadus_core::learner::{PendingRemediation, TopicState};
use cadus_core::readiness::{Blocker, KIND_TEMPLATE, MapContent, ReadinessIndex, ReadinessSet};
use cadus_core::selector::{SessionContext, SessionPlan, compose_session};

use common::selector::{cfg, learned, quiz_quiet, sampler, states_of, topic};
use common::{T_US, knowledge_point};

/// One knowledge point with three decidable exemplars and a fourth held out.
fn ready_kp(id: &str) -> KnowledgePoint {
    let mut point = knowledge_point(id, &[]);
    point.exemplars = (1..=4)
        .map(|n| Exemplar {
            answer_contract: None,
            problem: format!("{n} + {n}"),
            answer: (n * 2).to_string(),
            solution_sketch: Some("add".to_owned()),
        })
        .collect();
    point
}

/// One knowledge point with two exemplars: the Foundations shape of finding (i).
fn thin_kp(id: &str) -> KnowledgePoint {
    let mut point = knowledge_point(id, &[]);
    point.exemplars = (1..=2)
        .map(|n| Exemplar {
            answer_contract: None,
            problem: format!("{n} x {n}"),
            answer: (n * n).to_string(),
            solution_sketch: Some("multiply".to_owned()),
        })
        .collect();
    point
}

/// `rich` teaches, practices and assesses. `thin` has a short practice pool and
/// no held-out item: it serves a lesson from its exemplars (note 103 b) and no
/// review.
fn tree() -> Curriculum {
    common::selector::graph_of(
        vec![
            topic("rich").kps(vec![ready_kp("kp1")]).build(),
            topic("thin").kps(vec![thin_kp("kp1")]).build(),
        ],
        &[],
    )
}

/// The approved documents of both knowledge points, teach page included.
fn stocked() -> MapContent {
    let mut content = MapContent::default();
    for key in ["rich/kp1", "thin/kp1"] {
        content.insert(key, KIND_TEACH, 1);
        content.insert(key, KIND_HINT_LADDER, 1);
        content.insert(key, KIND_TEMPLATE, 0);
    }
    content
}

/// The readiness of `tree()` against `content`.
fn set_of(graph: &Curriculum, content: &MapContent) -> ReadinessSet {
    ReadinessIndex::build(graph).resolve(content)
}

/// The tree, the empty learner, and a store that approves no teach page for
/// `rich`.
fn untaught() -> (Curriculum, BTreeMap<String, TopicState>, MapContent) {
    let mut content = stocked();
    content.insert("rich/kp1", KIND_TEACH, 0);
    (tree(), BTreeMap::new(), content)
}

/// The topic ids of the served tasks of a plan, in serve order.
fn served_topics(plan: &SessionPlan) -> Vec<&str> {
    plan.tasks
        .iter()
        .filter_map(|task| task.topic.as_deref())
        .collect()
}

/// Compose over `graph` and `states` with the gate, at `T`.
fn compose(
    graph: &Curriculum,
    states: &BTreeMap<String, TopicState>,
    set: Option<&ReadinessSet>,
    config: &Config,
) -> SessionPlan {
    let gate = set.map(|found| found as &dyn cadus_core::readiness::ReadinessGate);
    let quiz = quiz_quiet();
    let ctx = SessionContext::default()
        .with_quiz_state(Some(&quiz))
        .with_readiness(gate);
    compose_session(states, graph, config, T_US, &mut sampler(1), &ctx)
}

#[test]
fn a_lesson_with_no_teach_page_leaves_the_plan_and_names_its_blockers() {
    let (graph, states, content) = untaught();
    let set = set_of(&graph, &content);
    let plan = compose(&graph, &states, Some(&set), &cfg());

    // `rich` has no teach page and leaves the plan; `thin` has its page and
    // serves from its two exemplars (note 103 b).
    assert_eq!(served_topics(&plan), ["thin"]);
    let blocked: Vec<(&str, Vec<Blocker>)> = plan
        .blocked
        .iter()
        .map(|held| (held.topic.as_str(), held.blockers.clone()))
        .collect();
    assert_eq!(blocked, [("rich", vec![Blocker::Teachable])]);
    assert!(
        plan.blocked
            .iter()
            .all(|held| held.task_type == TaskType::Lesson)
    );
    assert_eq!(plan.blocked[0].kp.as_deref(), Some("kp1"));
}

#[test]
fn the_plan_takes_the_next_ready_lesson() {
    let graph = tree();
    let states: BTreeMap<String, TopicState> = BTreeMap::new();
    let set = set_of(&graph, &stocked());
    let plan = compose(&graph, &states, Some(&set), &cfg());
    // Both points have a teach page; the thin pool of `thin` blocks no lesson
    // (note 103 b), so the plan takes both lessons in order.
    assert_eq!(served_topics(&plan), ["rich", "thin"]);
    assert!(plan.blocked.is_empty());
}

#[test]
fn a_review_needs_practicable_only() {
    let graph = tree();
    // Both topics are learned and due, so both owe a review.
    let states = states_of(vec![("rich", learned(0.2)), ("thin", learned(0.2))]);
    let mut content = stocked();
    // No teach page anywhere: a review does not need one.
    content.insert("rich/kp1", KIND_TEACH, 0);
    content.insert("thin/kp1", KIND_TEACH, 0);
    let set = set_of(&graph, &content);
    let plan = compose(&graph, &states, Some(&set), &cfg());
    let reviews: Vec<&str> = plan
        .tasks
        .iter()
        .filter(|task| task.task_type == TaskType::Review)
        .filter_map(|task| task.topic.as_deref())
        .collect();
    assert_eq!(reviews, ["rich"]);
    let held: Vec<&str> = plan
        .blocked
        .iter()
        .filter(|task| task.task_type == TaskType::Review)
        .map(|task| task.topic.as_str())
        .collect();
    assert_eq!(held, ["thin"]);
    assert_eq!(
        plan.blocked
            .iter()
            .find(|task| task.task_type == TaskType::Review)
            .map(|task| task.blockers.clone()),
        Some(vec![Blocker::Practicable])
    );
}

#[test]
fn the_rule_is_off_with_no_gate_and_off_when_the_config_says_so() {
    let (graph, states, content) = untaught();
    let set = set_of(&graph, &content);

    // No gate: the plan is the plan of every earlier unit.
    let open = compose(&graph, &states, None, &cfg());
    assert_eq!(open.tasks.len(), 2);
    assert!(open.blocked.is_empty());

    // A gate the config turns off changes nothing either.
    let mut off = cfg();
    off.readiness.enforce = false;
    let quiet = compose(&graph, &states, Some(&set), &off);
    assert_eq!(quiet.tasks.len(), 2);
    assert!(quiet.blocked.is_empty());
    assert!(cfg().readiness.enforce);
}

/// A failed confirmation queues a peel-back LESSON. When the content has no
/// approved teach page for that lesson, the remediation path must apply the
/// same `teachable` gate as ordinary planning: the lesson is NOT scheduled, and
/// a practice-only remediation review takes its place (ISSUE-4).
#[test]
fn a_failed_confirmation_with_no_teach_page_falls_back_to_practice_only() {
    let graph = common::selector::graph_of(
        vec![
            topic("p")
                .kps(vec![ready_kp("kp1"), ready_kp("kp2")])
                .build(),
        ],
        &[],
    );
    // The topic is PLACED (inferred, never practiced), the state a failed
    // confirmation leaves behind. No approved teach page exists.
    let placed = TopicState {
        status: TopicStatus::Placed,
        ..TopicState::default()
    };
    let states = states_of(vec![("p", placed)]);
    let set = set_of(&graph, &MapContent::default());
    let pending = vec![PendingRemediation {
        kind: "confirm_failed".to_owned(),
        targets: vec![Slug::new("p").expect("a slug")],
    }];
    let quiz = quiz_quiet();
    let ctx = SessionContext::default()
        .with_quiz_state(Some(&quiz))
        .with_pending_remediation(&pending)
        .with_readiness(Some(&set));
    let plan = compose_session(&states, &graph, &cfg(), T_US, &mut sampler(1), &ctx);

    // The peel-back LESSON is not served...
    assert!(
        plan.tasks
            .iter()
            .all(|task| !(task.task_type == TaskType::Lesson
                && task.topic.as_deref() == Some("p"))),
        "an unteachable remediation lesson was scheduled: {:?}",
        plan.tasks
    );
    // ...and the target is not dropped, but downgraded to a practice-only
    // remediation review.
    let fallback = plan
        .tasks
        .iter()
        .find(|task| task.is_remediation && task.topic.as_deref() == Some("p"))
        .expect("the remediation target was not scheduled at all");
    assert_eq!(fallback.task_type, TaskType::Review);
    assert!(
        fallback.why.contains("practice-only"),
        "the fallback does not name itself: {}",
        fallback.why
    );
}

/// A remediation target with no practicable knowledge point at all is dropped,
/// not turned into a review that could serve nothing.
#[test]
fn a_failed_confirmation_with_no_practicable_point_is_skipped() {
    let graph = common::selector::graph_of(vec![topic("p").kps(vec![thin_kp("kp1")]).build()], &[]);
    let placed = TopicState {
        status: TopicStatus::Placed,
        ..TopicState::default()
    };
    let states = states_of(vec![("p", placed)]);
    let set = set_of(&graph, &MapContent::default());
    let pending = vec![PendingRemediation {
        kind: "confirm_failed".to_owned(),
        targets: vec![Slug::new("p").expect("a slug")],
    }];
    let quiz = quiz_quiet();
    let ctx = SessionContext::default()
        .with_quiz_state(Some(&quiz))
        .with_pending_remediation(&pending)
        .with_readiness(Some(&set));
    let plan = compose_session(&states, &graph, &cfg(), T_US, &mut sampler(1), &ctx);
    assert!(
        plan.tasks.iter().all(|task| !task.is_remediation),
        "a remediation target with no practicable point was scheduled: {:?}",
        plan.tasks
    );
}
