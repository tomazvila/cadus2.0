//! Selector part 4: the drills, the multi-step integration task and the drill boundaries.
//!
//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use cadus_core::event::TaskType;
use cadus_core::selector::{
    DRILL_INTERVAL_DAYS, DRILL_MASTERY_ABILITY, SessionContext, multistep_components,
    multistep_is_due, schedule_drills,
};
use common::selector::{
    LearnedSpec, compose_with, fan, graph_of, ids, quiz_quiet, star_states, states_of, topic,
};
use common::{T_US, days};

// --------------------------------------------------------------------------- //
// Drills (test_selector.py:629-640)
// --------------------------------------------------------------------------- //

#[test]
fn schedule_drills_respects_mastery_and_cadence() {
    let graph = graph_of(
        vec![
            topic("d").drill(true).build(),
            topic("nd").drill(false).build(),
        ],
        &[],
    );
    let states = states_of(vec![
        ("d", LearnedSpec::new(0.9).ability(0.8).build()),
        ("nd", LearnedSpec::new(0.9).ability(0.8).build()),
    ]);
    // `tests/test_selector.py:635`.
    assert_eq!(schedule_drills(&states, &graph, T_US, None), ids(&["d"]));
    // `tests/test_selector.py:637`.
    let at_bar = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.96).build())]);
    assert_eq!(
        schedule_drills(&at_bar, &graph, T_US, None),
        Vec::<String>::new()
    );
    // `tests/test_selector.py:640`.
    let mut recent: BTreeMap<String, i64> = BTreeMap::new();
    recent.insert("d".to_owned(), T_US - days(1));
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&recent)),
        Vec::<String>::new()
    );
}

// --------------------------------------------------------------------------- //
// The multi-step integration task (selector.py:1044-1105)
// --------------------------------------------------------------------------- //

#[test]
fn multistep_cadence_is_consumable() {
    // `selector.py:1044-1052`: fires while `n_closed < n // 4`.
    assert!(!multistep_is_due(0, 0));
    assert!(!multistep_is_due(3, 0));
    assert!(multistep_is_due(4, 0));
    assert!(!multistep_is_due(4, 1));
    assert!(multistep_is_due(8, 1));
    assert!(!multistep_is_due(8, 2));
}

#[test]
fn multistep_components_are_dependency_ordered_and_capped() {
    let graph = graph_of(
        vec![
            topic("a").build(),
            topic("b").prereqs(&[("a", 0.5, false)]).build(),
            topic("c").prereqs(&[("b", 0.5, false)]).build(),
            topic("d").prereqs(&[("c", 0.5, false)]).build(),
            topic("e").prereqs(&[("d", 0.5, false)]).build(),
        ],
        &[],
    );
    // Roots lead, and the list is capped at 4 parts.
    assert_eq!(
        multistep_components(&ids(&["e", "d", "c", "b", "a"]), &graph),
        ids(&["a", "b", "c", "d"])
    );
}

#[test]
fn multistep_task_absorbs_its_component_reviews() {
    let mut topics = vec![topic("root").build()];
    topics.extend(fan("r", 6, 0.3));
    let graph = graph_of(topics, &[]);
    let states = star_states(6);
    let quiz = quiz_quiet();
    let plan = compose_with(
        &states,
        &graph,
        SessionContext::default()
            .with_session_id("s1")
            .with_quiz_state(Some(&quiz)),
    );
    let multistep = plan
        .tasks
        .iter()
        .find(|task| task.task_type == TaskType::MultiStep)
        .expect("the cadence fires with 7 reviewable topics");
    // The id spells the wire form of the task type (`_assign_ids`).
    assert_eq!(multistep.task_id, "s1-multi-step");
    assert_eq!(multistep.component_topics.len(), 4);
    assert!(multistep.topic.is_none());
    // A component is never also served as a standalone review.
    for component in &multistep.component_topics {
        assert!(
            !plan.tasks.iter().any(|task| {
                task.task_type == TaskType::Review && task.topic.as_ref() == Some(component)
            }),
            "component {component} is served twice"
        );
    }
}

// --------------------------------------------------------------------------- //
// The selector boundaries, read AT the threshold
// --------------------------------------------------------------------------- //
//
// Every expected value below came from the live 1.0 selector on the same input,
// through `scripts/oracle/dump_selector_boundaries_1_0.py`. M3 review round 1,
// findings #8, #9, and #10.

/// 3.49 days in microseconds: one step INSIDE the drill cadence window.
const DRILL_GAP_INSIDE_US: i64 = 301_536_000_000;

/// 3.5 days in microseconds: the drill cadence window itself.
const DRILL_GAP_AT_WINDOW_US: i64 = 302_400_000_000;

/// 3.51 days in microseconds: one step OUTSIDE the drill cadence window.
const DRILL_GAP_OUTSIDE_US: i64 = 303_264_000_000;

#[test]
fn schedule_drills_brackets_the_automaticity_bar() {
    // `selector.py:113` holds `DRILL_MASTERY_ABILITY = 0.95` and `selector.py:866`
    // drops a topic whose `ability >= DRILL_MASTERY_ABILITY`. 1.0 on this graph:
    // ability 0.949 -> ['d'], 0.95 -> [], 0.951 -> [] (finding #9).
    assert!((DRILL_MASTERY_ABILITY - 0.95).abs() < f64::EPSILON);
    let graph = graph_of(vec![topic("d").drill(true).build()], &[]);

    let below = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.949).build())]);
    assert_eq!(schedule_drills(&below, &graph, T_US, None), ids(&["d"]));

    let at_bar = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.95).build())]);
    assert_eq!(
        schedule_drills(&at_bar, &graph, T_US, None),
        Vec::<String>::new()
    );

    let above = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.951).build())]);
    assert_eq!(
        schedule_drills(&above, &graph, T_US, None),
        Vec::<String>::new()
    );
}

#[test]
fn schedule_drills_brackets_the_cadence_window() {
    // `selector.py:116` holds `DRILL_INTERVAL_DAYS = 3.5` and `selector.py:868`
    // skips a topic while `t - last_drill_at < timedelta(days=3.5)`. 1.0 on this
    // graph: a gap of 3.49 days -> [], 3.5 days -> ['d'], 3.51 days -> ['d'].
    // The gap of exactly 3.5 days is the one the strict `<` decides (finding #9).
    assert!((DRILL_INTERVAL_DAYS - 3.5).abs() < f64::EPSILON);
    let graph = graph_of(vec![topic("d").drill(true).build()], &[]);
    let states = states_of(vec![("d", LearnedSpec::new(0.9).ability(0.8).build())]);

    let mut inside: BTreeMap<String, i64> = BTreeMap::new();
    inside.insert("d".to_owned(), T_US - DRILL_GAP_INSIDE_US);
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&inside)),
        Vec::<String>::new()
    );

    let mut at_window: BTreeMap<String, i64> = BTreeMap::new();
    at_window.insert("d".to_owned(), T_US - DRILL_GAP_AT_WINDOW_US);
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&at_window)),
        ids(&["d"])
    );

    let mut outside: BTreeMap<String, i64> = BTreeMap::new();
    outside.insert("d".to_owned(), T_US - DRILL_GAP_OUTSIDE_US);
    assert_eq!(
        schedule_drills(&states, &graph, T_US, Some(&outside)),
        ids(&["d"])
    );
}
