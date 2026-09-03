//! Selector part 3: the module interleaving, the course completion, the open plan and the remediation queue.
//!
//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use cadus_core::curriculum::Curriculum;
use cadus_core::event::{TaskType, Timestamp, TopicStatus};
use cadus_core::learner::{PendingRemediation, QuizState, TopicState};
use cadus_core::selector::{
    REMEDIATION_QUIZ_MISS, SessionContext, SessionPlan, Task, arrange_lessons, due_reviews,
    interleave, remediation_for_quiz_miss, remediation_for_repeat_fail,
};
use common::selector::{
    LearnedSpec, blocked_frontier, cfg, compose_quiet, compose_with, compose_with_remediation,
    graph_of, id_set, ids, kp, learned, plan_topics, quiz_quiet, states_of, topic,
};
use common::{DAY_US, T_US};

/// The context of session `s1` with a quiet quiz, and the plan it composes.
fn session_s1<'a>(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    quiz: &'a QuizState,
) -> (SessionContext<'a>, SessionPlan) {
    let base = SessionContext::default()
        .with_session_id("s1")
        .with_quiz_state(Some(quiz));
    let plan = compose_with(states, graph, base.clone());
    (base, plan)
}

// --------------------------------------------------------------------------- //
// Module interleaving (test_selector.py:335-353)
// --------------------------------------------------------------------------- //

#[test]
fn no_two_consecutive_lessons_from_the_same_module() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("a1").build(),
            topic("a2").build(),
            topic("z1").build(),
        ],
        &[
            ("root", "Numbers"),
            ("a1", "Numbers"),
            ("a2", "Numbers"),
            ("z1", "Fractions"),
        ],
    );
    let mut states = states_of(vec![(
        "root",
        LearnedSpec::new(0.9).status(TopicStatus::Floor).build(),
    )]);
    for tid in ["a1", "a2", "z1"] {
        states.insert(tid.to_owned(), TopicState::default());
    }
    let plan = compose_quiet(&states, &graph);
    let modules: Vec<&str> = plan
        .tasks
        .iter()
        .filter(|task| task.task_type == TaskType::Lesson)
        .filter_map(|task| task.topic.as_deref())
        .map(|tid| graph.module_of(graph.idx_of(tid).unwrap()))
        .collect();
    // `tests/test_selector.py:352-353`. The two Numbers lessons rank ahead of the
    // Fractions one, so a selector that ignored the last module used would serve
    // them back to back.
    assert_eq!(modules, vec!["Numbers", "Fractions", "Numbers"]);
}

#[test]
fn arrange_lessons_alternates_modules() {
    let graph = graph_of(
        vec![
            topic("na").build(),
            topic("nb").build(),
            topic("nc").build(),
            topic("fa").build(),
        ],
        &[
            ("na", "Numbers"),
            ("nb", "Numbers"),
            ("nc", "Numbers"),
            ("fa", "Fractions"),
        ],
    );
    // Three Numbers lessons and one Fractions lesson: the biggest module leads,
    // then the other module breaks the run.
    assert_eq!(
        arrange_lessons(&ids(&["na", "nb", "nc", "fa"]), &graph),
        ids(&["na", "fa", "nb", "nc"])
    );
}

#[test]
fn interleave_forces_a_lesson_after_three_reviews() {
    let cfg = cfg();
    let seq = interleave(
        &ids(&["r0", "r1", "r2", "r3", "r4"]),
        &ids(&["l0", "l1"]),
        &cfg,
    );
    let order: Vec<String> = seq.iter().map(|(_, tid)| tid.clone()).collect();
    assert_eq!(order, ids(&["r0", "r1", "r2", "l0", "r3", "r4", "l1"]));
}

// --------------------------------------------------------------------------- //
// Course complete and frontier blocked (test_selector.py:361-424)
// --------------------------------------------------------------------------- //

#[test]
fn course_complete_gives_an_empty_plan_and_the_flag() {
    let all: Vec<String> = (0..12).map(|index| format!("t{index:02}")).collect();
    let graph = graph_of(all.iter().map(|id| topic(id).build()).collect(), &[]);
    let states: BTreeMap<String, TopicState> = all
        .iter()
        .map(|id| (id.clone(), LearnedSpec::new(0.95).ability(0.99).build()))
        .collect();
    let quiz = quiz_quiet();
    let plan = compose_with(
        &states,
        &graph,
        SessionContext::default()
            .with_course(Some("c"))
            .with_quiz_state(Some(&quiz)),
    );
    // `tests/test_selector.py:389-398`.
    assert!(plan.course_complete);
    assert_eq!(plan.tasks, Vec::<Task>::new());
}

#[test]
fn frontier_blocked_serves_nearly_due_and_reports_until() {
    let (graph, states, failed_at) = blocked_frontier();
    let plan = compose_quiet(&states, &graph);
    // `tests/test_selector.py:406-424`.
    assert_eq!(
        plan.frontier_blocked_until,
        Some(Timestamp::from_micros(failed_at + DAY_US))
    );
    assert!(!plan.course_complete);
    assert!(
        plan.tasks
            .iter()
            .all(|task| task.task_type != TaskType::Lesson)
    );
    assert_eq!(plan_topics(&plan), ids(&["base"]));
    let base = plan.tasks.first().unwrap();
    // `tests/test_selector.py:479-486`: the typed fact, not the prose.
    assert!(base.nearly_due);
    assert!(!base.is_remediation);
    assert!(base.why.contains("nearly-due review"));
}

// --------------------------------------------------------------------------- //
// Open-plan idempotency (test_selector.py:361-380, 488-501)
// --------------------------------------------------------------------------- //

#[test]
fn open_plan_reserves_minus_completed_keeping_ids() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("r0").prereqs(&[("root", 0.3, false)]).build(),
            topic("r1").prereqs(&[("root", 0.3, false)]).build(),
            topic("l0").prereqs(&[("root", 0.3, false)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("root", learned(0.9)),
        ("r0", learned(0.5)),
        ("r1", learned(0.5)),
    ]);
    let quiz = quiz_quiet();
    let (base, plan) = session_s1(&states, &graph, &quiz);
    let served: BTreeMap<String, String> = plan
        .tasks
        .iter()
        .filter_map(|task| task.topic.clone().map(|tid| (tid, task.task_id.clone())))
        .collect();
    // `tests/test_selector.py:370`.
    assert_eq!(
        served.keys().cloned().collect::<BTreeSet<String>>(),
        id_set(&["r0", "r1", "l0"])
    );
    assert_eq!(served.get("r0").map(String::as_str), Some("s1-review-r0"));
    assert_eq!(served.get("l0").map(String::as_str), Some("s1-lesson-l0"));

    let mut updated = states.clone();
    updated.insert("r0".to_owned(), learned(0.9));
    updated.insert("l0".to_owned(), learned(0.9));
    let reserved = compose_with(&updated, &graph, base.with_open_plan(Some(&plan)));
    // `tests/test_selector.py:376-380`.
    assert_eq!(plan_topics(&reserved), ids(&["r1"]));
    assert_eq!(
        reserved.tasks.first().map(|task| task.task_id.as_str()),
        served.get("r1").map(String::as_str)
    );
    assert_eq!(reserved.session, plan.session);
}

#[test]
fn nearly_due_review_survives_the_reserve() {
    let (graph, states, _) = blocked_frontier();
    let quiz = quiz_quiet();
    let (base, plan) = session_s1(&states, &graph, &quiz);
    let original = plan.tasks.first().unwrap().task_id.clone();
    let reserved = compose_with(&states, &graph, base.with_open_plan(Some(&plan)));
    // `tests/test_selector.py:496-501`.
    assert_eq!(plan_topics(&reserved), ids(&["base"]));
    assert_eq!(
        reserved.tasks.first().map(|task| task.task_id.as_str()),
        Some(original.as_str())
    );
    assert!(reserved.tasks.first().unwrap().nearly_due);
}

// --------------------------------------------------------------------------- //
// Remediation first (test_selector.py:552-622)
// --------------------------------------------------------------------------- //

#[test]
fn remediation_is_served_first_as_a_review() {
    let graph = graph_of(
        vec![
            topic("prereq").build(),
            topic("target").prereqs(&[("prereq", 0.9, true)]).build(),
            topic("r0").prereqs(&[("prereq", 0.3, false)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("prereq", learned(0.9)),
        ("target", learned(0.9)),
        ("r0", learned(0.5)),
    ]);
    let pending = vec![remediation_for_quiz_miss("target").unwrap()];
    let plan = compose_with_remediation(&states, &graph, &pending);
    // `tests/test_selector.py:566-573`.
    let first = plan.tasks.first().unwrap();
    assert_eq!(first.topic.as_deref(), Some("target"));
    assert_eq!(first.task_type, TaskType::Review);
    assert!(first.why.contains("remediation (quiz_miss)"));
    assert_eq!(REMEDIATION_QUIZ_MISS, "quiz_miss");
    assert!(first.is_remediation);
    assert!(!first.nearly_due);
}

#[test]
fn remediation_supersedes_the_same_topic_due_review() {
    let graph = graph_of(vec![topic("dup").build(), topic("other").build()], &[]);
    let states = states_of(vec![("dup", learned(0.5)), ("other", learned(0.5))]);
    assert_eq!(
        due_reviews(&states, &graph, &cfg(), T_US, &BTreeSet::new()),
        ids(&["dup", "other"])
    );
    let pending = vec![remediation_for_quiz_miss("dup").unwrap()];
    let plan = compose_with_remediation(&states, &graph, &pending);
    // `tests/test_selector.py:588-600`: `dup` is served ONCE, as the remediation.
    assert_eq!(plan_topics(&plan), ids(&["dup", "other"]));
    let dup = plan.tasks.first().unwrap();
    assert!(dup.is_remediation);
    let other = plan.tasks.get(1).unwrap();
    assert!(other.why.contains("due review"));
    assert!(!other.is_remediation);
    assert!(!other.nearly_due);
}

#[test]
fn unmastered_remediation_target_becomes_a_lesson() {
    let graph = graph_of(
        vec![
            topic("prereq").build(),
            topic("topic").prereqs(&[("prereq", 0.9, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![("prereq", learned(0.9))]);
    let pending = vec![PendingRemediation {
        kind: REMEDIATION_QUIZ_MISS.to_owned(),
        targets: vec![cadus_core::event::Slug::new("topic").unwrap()],
    }];
    let plan = compose_with_remediation(&states, &graph, &pending);
    // `tests/test_selector.py:620-622`.
    let first = plan.tasks.first().unwrap();
    assert_eq!(first.topic.as_deref(), Some("topic"));
    assert_eq!(first.task_type, TaskType::Lesson);
    assert!(first.is_remediation);
}

#[test]
fn repeat_fail_remediation_targets_key_prereqs() {
    let graph = graph_of(
        vec![
            topic("equivalent-fractions").build(),
            topic("adding-fractions")
                .kps(vec![kp("kp1", &["equivalent-fractions"])])
                .build(),
        ],
        &[],
    );
    let rem = remediation_for_repeat_fail("adding-fractions", "kp1", &graph);
    // `tests/test_selector.py:610-613`.
    assert_eq!(rem.kind, "repeat_fail");
    assert_eq!(
        rem.targets
            .iter()
            .map(|slug| slug.as_str().to_owned())
            .collect::<Vec<String>>(),
        ids(&["equivalent-fractions"])
    );
}
