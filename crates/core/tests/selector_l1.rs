//! Selector part 6: the L1 composition over the full curriculum, the importance, the decay bands and the knockout mass.
//!
//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeSet;

use cadus_core::curriculum::model::Topic;
use cadus_core::selector::{
    SessionContext, compose_session, course_scope, due_reviews, importance, nearly_due,
};
use common::selector::{
    LearnedSpec, bench_states, cfg, compose_with, graph_of, ids, quiz_quiet, real_curriculum,
    sampler, states_of, topic,
};
use common::{T_US, days};

// --------------------------------------------------------------------------- //
// L1 — compose_session on the full curriculum (REQUIREMENTS L1)
// --------------------------------------------------------------------------- //

#[test]
fn compose_session_runs_on_the_full_curriculum() {
    let graph = real_curriculum();
    assert_eq!(graph.topic_count(), 1090);
    let states = bench_states(&graph);
    let course = graph
        .topics()
        .first()
        .map(|topic| {
            graph
                .course_of(graph.idx_of(topic.id.as_str()).unwrap())
                .to_owned()
        })
        .unwrap();
    let quiz = quiz_quiet();
    let plan = compose_with(
        &states,
        &graph,
        SessionContext::default()
            .with_course(Some(&course))
            .with_quiz_state(Some(&quiz)),
    );
    assert!(!plan.tasks.is_empty(), "the plan serves work");
    // Every served task carries a content-stable id.
    for task in &plan.tasks {
        assert!(task.task_id.starts_with("s-"), "id {}", task.task_id);
    }
}

/// L1: composing a session over the full curriculum stays under 5 ms.
///
/// The budget is a RELEASE number, so the test measures only when
/// `CADUS_RELEASE_BENCH` is set. Run it with:
///
/// ```sh
/// CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test selector -- l1_
/// ```
#[test]
fn l1_compose_session_under_5_ms() {
    if std::env::var_os("CADUS_RELEASE_BENCH").is_none() {
        return;
    }
    if cfg!(debug_assertions) {
        panic!("CADUS_RELEASE_BENCH needs a release build: add --release");
    }
    let graph = real_curriculum();
    let states = bench_states(&graph);
    let quiz = quiz_quiet();
    // No course scope: the frontier, the compression, and the lesson ordering
    // all run over the whole 1,090-topic graph, the heaviest shape L1 has.
    let ctx = SessionContext::default().with_quiz_state(Some(&quiz));
    let cfg = cfg();
    // Warm up, then take the best of 20 runs.
    for _ in 0..5 {
        let _ = compose_session(&states, &graph, &cfg, T_US, &mut sampler(1), &ctx);
    }
    let mut best = std::time::Duration::from_secs(1);
    for _ in 0..20 {
        let started = std::time::Instant::now();
        let plan = compose_session(&states, &graph, &cfg, T_US, &mut sampler(1), &ctx);
        let elapsed = started.elapsed();
        assert!(!plan.tasks.is_empty());
        best = best.min(elapsed);
    }
    assert!(
        best < std::time::Duration::from_millis(5),
        "L1: compose_session took {best:?}, budget 5 ms"
    );
    println!("L1 compose_session: {best:?} (budget 5 ms)");
}

// --------------------------------------------------------------------------- //
// Importance, budgets, and decay
// --------------------------------------------------------------------------- //

#[test]
fn importance_adds_the_core_bonus_and_the_dependent_count() {
    let graph = graph_of(
        vec![
            topic("leaf-core").core(true).build(),
            topic("leaf-plain").core(false).build(),
            topic("parent")
                .core(false)
                .prereqs(&[("leaf-plain", 0.5, false)])
                .build(),
        ],
        &[],
    );
    let scope = course_scope(&graph, None);
    let empty: BTreeSet<String> = BTreeSet::new();
    // `selector.py:605-614`: mass 0 + dependents 0 + core bonus 0.5.
    assert!((importance("leaf-core", &graph, &empty, &scope) - 0.5).abs() < 1e-12);
    // Not core, but one in-course dependent: 0 + 1 + 0.
    assert!((importance("leaf-plain", &graph, &empty, &scope) - 1.0).abs() < 1e-12);
    // Not core, no dependent, no review target.
    assert!((importance("parent", &graph, &empty, &scope) - 0.0).abs() < 1e-12);
}

#[test]
fn decay_moves_a_topic_through_the_review_bands() {
    let graph = graph_of(vec![topic("a").build()], &[]);
    // Memory base 1.0, interval 10 days: memory halves every 10 days.
    let at = |days_ago: i64| {
        states_of(vec![(
            "a",
            LearnedSpec::new(1.0).t0(T_US - days(days_ago)).build(),
        )])
    };
    let cfg = cfg();
    let empty = BTreeSet::new();
    // 0 days: memory 1.0, on schedule.
    assert_eq!(
        due_reviews(&at(0), &graph, &cfg, T_US, &empty),
        Vec::<String>::new()
    );
    assert_eq!(nearly_due(&at(0), &graph, &cfg, T_US), Vec::<String>::new());
    // 8 days: memory 0.574, nearly due.
    assert_eq!(nearly_due(&at(8), &graph, &cfg, T_US), ids(&["a"]));
    // 10 days: memory 0.5, due.
    assert_eq!(
        due_reviews(&at(10), &graph, &cfg, T_US, &empty),
        ids(&["a"])
    );
}

#[test]
fn the_lesson_knockout_mass_is_a_compensated_sum() {
    // Trap T1 at `selector.py:602`: the knockout mass is a CPython `sum()` over the
    // review targets. Ten targets at weight 0.1 each total exactly 1.0 in CPython
    // 3.12 and later, where a naive left-to-right add gives 0.9999999999999999.
    // The topic is not core and has no dependent, so the importance IS the mass:
    // the live 1.0 `importance` on this graph prints 1.0 (finding #8).
    let leaves: Vec<String> = (0..10).map(|index| format!("leaf-{index}")).collect();
    let edges: Vec<(&str, f64, bool)> = leaves
        .iter()
        .map(|id| (id.as_str(), 0.1_f64, false))
        .collect();
    let mut topics: Vec<Topic> = leaves.iter().map(|id| topic(id).build()).collect();
    topics.push(topic("lesson-topic").core(false).prereqs(&edges).build());
    let graph = graph_of(topics, &[]);

    let targets: BTreeSet<String> = leaves.iter().cloned().collect();
    let scope = course_scope(&graph, None);
    let mass = importance("lesson-topic", &graph, &targets, &scope);

    // The comparison is EXACT: the naive total differs from 1.0 by one unit in the
    // last place, which is below `f64::EPSILON`.
    assert_eq!(
        mass.to_bits(),
        1.0_f64.to_bits(),
        "the knockout mass is {mass:?}, so the total is not compensated"
    );
}
