//! Selector part 3b: the validity of each task kind on a re-serve, and the
//! constraint report of a re-served plan.
//!
//! These pins are 2.0's own reading of `_reserve_open_plan`
//! (`selector.py:1630-1736`): a drill needs its eligibility, a lesson needs the
//! frontier, and the report counts the surviving reviews and lessons.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};

mod common;

use cadus_core::curriculum::Curriculum;
use cadus_core::event::TaskType;
use cadus_core::learner::TopicState;
use cadus_core::selector::{SessionContext, SessionPlan, Task, compose_session};
use common::T_US;
use common::selector::{
    LearnedSpec, cfg, compose_with, graph_of, learned, plan_topics, quiz_quiet, sampler, states_of,
    topic, two_reviews_one_lesson,
};

/// An open plan of session `s1` that holds one task of `kind` on `topic`.
fn open_plan_of(kind: TaskType, topic_id: &str) -> SessionPlan {
    SessionPlan {
        session: "s1".to_owned(),
        tasks: vec![Task {
            task_id: format!("s1-{}-{topic_id}", kind.as_str()),
            task_type: kind,
            topic: Some(topic_id.to_owned()),
            ..Task::default()
        }],
        ..SessionPlan::default()
    }
}

/// Re-serve `open` over `states` with a quiet quiz.
fn reserve(
    states: &BTreeMap<String, TopicState>,
    graph: &Curriculum,
    open: &SessionPlan,
) -> SessionPlan {
    let quiz = quiz_quiet();
    let ctx = SessionContext::default()
        .with_session_id("s1")
        .with_quiz_state(Some(&quiz))
        .with_open_plan(Some(open));
    compose_session(states, graph, &cfg(), T_US, &mut sampler(1), &ctx)
}

#[test]
fn a_drill_survives_the_reserve_only_while_its_topic_is_drill_eligible() {
    let graph = graph_of(vec![topic("fast").drill(true).build()], &[]);
    let open = open_plan_of(TaskType::Drill, "fast");
    // Mastered below the automaticity bar: the drill is owed.
    let eligible = states_of(vec![("fast", LearnedSpec::new(0.9).ability(0.5).build())]);
    assert_eq!(plan_topics(&reserve(&eligible, &graph, &open)), ["fast"]);
    // At the automaticity bar: the drill has no reason to be served.
    let automatic = states_of(vec![("fast", LearnedSpec::new(0.9).ability(0.95).build())]);
    assert!(reserve(&automatic, &graph, &open).tasks.is_empty());
}

#[test]
fn a_lesson_survives_the_reserve_only_while_its_topic_is_on_the_frontier() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("next").prereqs(&[("root", 0.9, true)]).build(),
        ],
        &[],
    );
    let open = open_plan_of(TaskType::Lesson, "next");
    // `root` is mastered, so `next` is a frontier lesson.
    let on_frontier = states_of(vec![("root", learned(0.9))]);
    assert_eq!(plan_topics(&reserve(&on_frontier, &graph, &open)), ["next"]);
    // Nothing is mastered: `next` is unmastered and not delayed, but it is
    // behind `root`, so the lesson is dropped.
    let behind: BTreeMap<String, TopicState> = BTreeMap::new();
    assert!(reserve(&behind, &graph, &open).tasks.is_empty());
}

#[test]
fn the_reserved_plan_reports_its_reviews_and_lessons() {
    let (graph, states) = two_reviews_one_lesson();
    let quiz = quiz_quiet();
    let base = SessionContext::default()
        .with_session_id("s1")
        .with_quiz_state(Some(&quiz));
    let plan = compose_with(&states, &graph, base.clone());
    let reserved = compose_with(&states, &graph, base.with_open_plan(Some(&plan)));
    assert_eq!(plan_topics(&reserved), plan_topics(&plan));
    let report = &reserved.constraints;
    assert_eq!((report.reviews, report.lessons), (2, 1));
    assert_eq!(report.lesson_ratio, 0.3333);
    assert!(report.lesson_ratio_ok && report.throttle_ok);
}

#[test]
fn five_reviews_with_no_lesson_available_pass_the_throttle() {
    let mut topics = vec![topic("root").build()];
    topics.extend(common::selector::fan("r", 5, 0.3));
    let graph = graph_of(topics, &[]);
    // The multi-step cadence is spent, so every due review is served as one.
    let quiz = quiz_quiet();
    let closed = BTreeSet::new();
    let ctx = SessionContext::default()
        .with_quiz_state(Some(&quiz))
        .with_multistep(9, &closed);
    let plan = compose_with(&common::selector::star_states(5), &graph, ctx);
    let report = &plan.constraints;
    assert_eq!((report.reviews, report.lessons), (5, 0));
    assert!(report.throttle_ok && report.lesson_ratio_ok);
    assert_eq!(report.lesson_ratio, 0.0);
}
