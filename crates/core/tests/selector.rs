//! Selector part 1: the due reviews, the compression, the throttle and the review mix.
//!
//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use cadus_core::event::{TaskType, TopicStatus};
use cadus_core::learner::TopicState;
use cadus_core::selector::{compress, course_scope, due_reviews, order_lessons, review_mix};
use common::selector::{
    LearnedSpec, Rng, cfg, compose_quiet, failed_lesson, fan, graph_of, id_set, ids, kp, learned,
    plan_kinds, pool_of, random_states, real_curriculum, star_states, states_of, topic,
};
use common::{T_US, days};

// --------------------------------------------------------------------------- //
// due_reviews (test_selector.py:135-157)
// --------------------------------------------------------------------------- //

#[test]
fn due_reviews_restricts_to_review_history() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("a").prereqs(&[("root", 1.0, true)]).build(),
            topic("b").prereqs(&[("root", 1.0, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        (
            "root",
            LearnedSpec::new(0.4).status(TopicStatus::Floor).build(),
        ),
        ("a", LearnedSpec::new(0.4).build()),
        (
            "b",
            TopicState {
                status: TopicStatus::Untouched,
                rep_num: 3.0,
                ..TopicState::default()
            },
        ),
    ]);
    // `tests/test_selector.py:151`.
    assert_eq!(
        due_reviews(&states, &graph, &cfg(), T_US, &BTreeSet::new()),
        ids(&["a"])
    );
}

#[test]
fn due_reviews_excludes_not_due() {
    let graph = graph_of(vec![topic("a").build()], &[]);
    // `tests/test_selector.py:156-157`.
    assert_eq!(
        due_reviews(
            &states_of(vec![("a", learned(0.7))]),
            &graph,
            &cfg(),
            T_US,
            &BTreeSet::new()
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        due_reviews(
            &states_of(vec![("a", learned(0.5))]),
            &graph,
            &cfg(),
            T_US,
            &BTreeSet::new()
        ),
        ids(&["a"])
    );
}

// --------------------------------------------------------------------------- //
// compress (test_selector.py:165-190)
// --------------------------------------------------------------------------- //

#[test]
fn compress_free_knockout_by_frontier_lesson() {
    let graph = graph_of(
        vec![
            topic("child").build(),
            topic("parent").prereqs(&[("child", 0.9, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![("child", learned(0.5))]);
    let comp = compress(&ids(&["child"]), &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:171-172`.
    assert_eq!(comp.surviving, Vec::<String>::new());
    assert_eq!(comp.knockouts.get("parent"), Some(&ids(&["child"])));
}

#[test]
fn compress_due_topic_dominoes_another() {
    let graph = graph_of(
        vec![
            topic("add").build(),
            topic("sub").prereqs(&[("add", 0.8, true)]).build(),
        ],
        &[],
    );
    let states = states_of(vec![("add", learned(0.5)), ("sub", learned(0.5))]);
    let comp = compress(&ids(&["add", "sub"]), &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:181-182`.
    assert_eq!(comp.surviving, ids(&["sub"]));
    assert_eq!(comp.knockouts.get("sub"), Some(&ids(&["add"])));
}

#[test]
fn compress_independent_reviews_all_survive() {
    let mut topics = vec![topic("root").build()];
    topics.extend(fan("r", 4, 0.3));
    let graph = graph_of(topics, &[]);
    let states = star_states(4);
    let due = ids(&["r0", "r1", "r2", "r3"]);
    let comp = compress(&due, &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:189-190`.
    assert_eq!(comp.surviving, due);
    assert_eq!(comp.knockouts, BTreeMap::new());
}

#[test]
fn blocked_lesson_does_not_absorb_a_due_review() {
    let graph = graph_of(
        vec![
            topic("child").build(),
            topic("lesson").prereqs(&[("child", 0.9, true)]).build(),
        ],
        &[],
    );
    let failed_at = T_US - days(1) / 2;
    let states = states_of(vec![
        ("child", learned(0.5)),
        ("lesson", failed_lesson(failed_at)),
    ]);
    let comp = compress(&ids(&["child"]), &states, &graph, &cfg(), T_US, None);
    // `tests/test_selector.py:441`.
    assert_eq!(comp.surviving, ids(&["child"]));

    let plan = compose_quiet(&states, &graph);
    assert!(plan.tasks.iter().any(|task| {
        task.topic.as_deref() == Some("child") && task.task_type == TaskType::Review
    }));
}

// --------------------------------------------------------------------------- //
// The compression property (test_selector.py:200-222)
// --------------------------------------------------------------------------- //

#[test]
fn property_knockout_leaves_no_due_topic_uncovered() {
    let graph = real_curriculum();
    let cfg = cfg();
    let pool = pool_of(&graph);
    let mut rng = Rng::new(20_260_826);
    for _ in 0..200 {
        let states = random_states(&pool, &mut rng);
        let due: BTreeSet<String> = due_reviews(&states, &graph, &cfg, T_US, &BTreeSet::new())
            .into_iter()
            .collect();
        let comp = compress(
            &due.iter().cloned().collect::<Vec<String>>(),
            &states,
            &graph,
            &cfg,
            T_US,
            None,
        );
        let surviving: BTreeSet<String> = comp.surviving.iter().cloned().collect();
        let knocked: BTreeSet<String> = comp
            .knockouts
            .values()
            .flat_map(|list| list.iter().cloned())
            .collect();
        let union: BTreeSet<String> = surviving.union(&knocked).cloned().collect();
        assert_eq!(union, due, "surviving union knocked must equal due");
        assert!(
            surviving.is_disjoint(&knocked),
            "surviving and knocked must be disjoint"
        );
        assert!(surviving.is_subset(&due), "surviving must be due");
        assert!(knocked.is_subset(&due), "knocked must be due");
    }
}

// --------------------------------------------------------------------------- //
// Throttle, importance, review mix (test_selector.py:242-286)
// --------------------------------------------------------------------------- //

#[test]
fn backlog_yields_at_least_one_lesson_per_three_reviews() {
    let mut topics = vec![topic("root").build()];
    topics.extend(fan("r", 9, 0.3));
    topics.extend(fan("l", 5, 0.3));
    let graph = graph_of(topics, &[]);
    let states = star_states(9);
    let cfg = cfg();
    let plan = compose_quiet(&states, &graph);
    let kinds: Vec<&str> = plan_kinds(&plan)
        .into_iter()
        .filter(|kind| *kind == "review" || *kind == "lesson")
        .collect();
    let mut run = 0_i64;
    let mut maxrun = 0_i64;
    for kind in &kinds {
        run = if *kind == "review" { run + 1 } else { 0 };
        maxrun = maxrun.max(run);
    }
    // `tests/test_selector.py:253-256`.
    assert!(maxrun <= cfg.selector.max_reviews_per_lesson);
    assert!(plan.constraints.throttle_ok);
    assert!(plan.constraints.lesson_ratio_ok);
    assert!(kinds.iter().filter(|kind| **kind == "lesson").count() >= 3);
}

#[test]
fn order_lessons_prefers_knockout_mass() {
    let graph = graph_of(
        vec![
            topic("root").build(),
            topic("child").prereqs(&[("root", 0.3, false)]).build(),
            topic("la").prereqs(&[("child", 0.9, true)]).build(),
            topic("lb").prereqs(&[("root", 0.9, true)]).build(),
        ],
        &[],
    );
    let scope = course_scope(&graph, None);
    let ordered = order_lessons(&ids(&["lb", "la"]), &graph, &id_set(&["child"]), &scope);
    // `tests/test_selector.py:275`.
    assert_eq!(ordered.first().map(String::as_str), Some("la"));
}

#[test]
fn review_mix_spans_kps_and_component_skills() {
    let graph = graph_of(
        vec![
            topic("multiplication").build(),
            topic("equivalent-fractions").build(),
            topic("adding-fractions")
                .prereqs(&[("equivalent-fractions", 0.8, true)])
                .kps(vec![kp("kp1", &["multiplication"]), kp("kp2", &[])])
                .build(),
        ],
        &[],
    );
    let mix = review_mix(&graph, "adding-fractions");
    // `tests/test_selector.py:286`.
    assert_eq!(mix.get(..2), Some(ids(&["kp1", "kp2"]).as_slice()));
    assert!(mix.contains(&"component:equivalent-fractions".to_owned()));
    assert!(mix.contains(&"component:multiplication".to_owned()));
}
