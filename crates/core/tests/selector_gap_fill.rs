//! Selector part 5: the cross-course gap fill, the retry delay and the nearly-due order.
//!
//! The selector of spec section 6, pinned against the ORDER assertions of the
//! 1.0 suite (`tests/test_selector.py`, `tests/test_gap_fill.py`).
//!
//! Every expected value here is a LITERAL taken from the 1.0 test that pins it.
//! Nothing is re-derived from the code under test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use cadus_core::curriculum::Curriculum;
use cadus_core::curriculum::load::{RawCurriculum, RawUnit};
use cadus_core::curriculum::model::{Catalog, Course, Slug, Topic, Unit};
use cadus_core::event::{KpProgress, TopicStatus};
use cadus_core::learner::TopicState;
use cadus_core::selector::{
    blocking_gap_ancestors, gap_course_for, gap_fill_chain_for_stack, in_retry_delay, known_set,
    nearly_due, resolve_gap_fill_stack, serveable_gap_frontier,
};
use common::selector::{cfg, failed_lesson, graph_of, id_set, ids, learned, states_of, topic};
use common::{DAY_US, T_US, days};

// --------------------------------------------------------------------------- //
// Cross-course gap fill (tests/test_gap_fill.py)
// --------------------------------------------------------------------------- //

/// The 1.0 `_topic` of `tests/test_gap_fill.py:53-66`: every prerequisite is a
/// key edge at weight 1.0.
fn gap_topic(id: &str, prereqs: &[&str]) -> Topic {
    let edges: Vec<(&str, f64, bool)> = prereqs.iter().map(|id| (*id, 1.0, true)).collect();
    topic(id).prereqs(&edges).build()
}

/// One catalog course at `order`, with `floor` as its mastery floor.
fn course(id: &str, order: i64, floor: &[&str]) -> Course {
    Course {
        id: Slug::new(id).unwrap(),
        name: id.to_owned(),
        order,
        mastery_floor: floor.iter().map(|id| Slug::new(id).unwrap()).collect(),
        mastery_floor_course: None,
    }
}

/// The 1.0 three-course ladder of `tests/test_gap_fill.py:100-115`.
fn ladder() -> Curriculum {
    let courses = vec![
        course("low", 1, &["low-a"]),
        course("mid", 2, &[]),
        course("top", 3, &[]),
    ];
    let per_course: Vec<(&str, Vec<Topic>)> = vec![
        (
            "low",
            vec![gap_topic("low-a", &[]), gap_topic("low-b", &["low-a"])],
        ),
        (
            "mid",
            vec![
                gap_topic("mid-a", &["low-a"]),
                gap_topic("mid-b", &["low-b"]),
                gap_topic("mid-free", &[]),
            ],
        ),
        ("top", vec![gap_topic("top-a", &["mid-a"])]),
    ];
    catalog_of(courses, per_course)
}

/// `alt` and `mid` share the order 2 under `top`: `mid-a` needs `alt-a`, and
/// `mid` holds a free topic when `with_free` is set.
fn twins(with_free: bool) -> Curriculum {
    let courses = vec![
        course("alt", 2, &[]),
        course("mid", 2, &[]),
        course("top", 3, &[]),
    ];
    let mut mid = vec![gap_topic("mid-a", &["alt-a"])];
    if with_free {
        mid.push(gap_topic("mid-free", &[]));
    }
    let per_course: Vec<(&str, Vec<Topic>)> = vec![
        ("alt", vec![gap_topic("alt-a", &[])]),
        ("mid", mid),
        ("top", vec![gap_topic("top-a", &["mid-a"])]),
    ];
    catalog_of(courses, per_course)
}

/// A curriculum of `courses` with one unit file per course.
fn catalog_of(courses: Vec<Course>, per_course: Vec<(&str, Vec<Topic>)>) -> Curriculum {
    let mut units = Vec::new();
    let mut first_load_index = 0;
    for (course, topics) in per_course {
        let count = topics.len();
        units.push(RawUnit {
            course_id: course.to_owned(),
            file_name: format!("{course}-u.yaml"),
            unit: Unit {
                unit: format!("{course}-u"),
                course: Slug::new(course).unwrap(),
                module: format!("{course}-M"),
                topics,
            },
            first_load_index,
        });
        first_load_index += count;
    }
    Curriculum::build(RawCurriculum {
        catalog: Catalog { courses },
        units,
    })
    .unwrap()
}

/// The 1.0 `_floor(graph, course)` helper: the mastery floor of a course, all in
/// `floor` status.
fn floor_states(graph: &Curriculum, course: &str) -> BTreeMap<String, TopicState> {
    graph
        .mastery_floor(course)
        .unwrap_or_default()
        .into_iter()
        .map(|idx| {
            (
                graph.id_of(idx).to_owned(),
                TopicState {
                    status: TopicStatus::Floor,
                    ..TopicState::default()
                },
            )
        })
        .collect()
}

#[test]
fn gap_course_is_lazy_and_descends_one_level() {
    let graph = ladder();
    let states = floor_states(&graph, "top");
    // `tests/test_gap_fill.py:140`.
    assert_eq!(
        gap_course_for(&states, &graph, &cfg(), T_US, Some("top"), None),
        Some("mid".to_owned())
    );
    // `tests/test_gap_fill.py:147`: every topic mastered, so no gap.
    let complete: BTreeMap<String, TopicState> = graph
        .topics()
        .iter()
        .map(|topic| {
            (
                topic.id.as_str().to_owned(),
                TopicState {
                    status: TopicStatus::Floor,
                    ..TopicState::default()
                },
            )
        })
        .collect();
    assert_eq!(
        gap_course_for(&complete, &graph, &cfg(), T_US, Some("top"), None),
        None
    );
}

#[test]
fn a_course_of_the_same_order_is_never_a_gap_course() {
    let none: BTreeMap<String, TopicState> = BTreeMap::new();
    // `mid` is blocked by `alt-a` alone, and `alt` is not a LOWER course.
    assert_eq!(
        gap_course_for(&none, &twins(false), &cfg(), T_US, Some("mid"), None),
        None
    );
    // With a free lesson in `mid` the descent from `top` stops at `mid`: the
    // deeper course must also be lower than the tip.
    assert_eq!(
        resolve_gap_fill_stack(&none, &twins(true), &cfg(), T_US, Some("top")),
        ids(&["top", "mid"])
    );
}

#[test]
fn chain_restricts_to_blocking_topics_only() {
    let graph = ladder();
    let states = floor_states(&graph, "top");
    // `tests/test_gap_fill.py:165-166`: `mid-b` blocks nothing `top` needs.
    let chain = gap_fill_chain_for_stack(&states, &graph, &ids(&["top", "mid"]), None)
        .expect("the stack is switched down");
    assert_eq!(chain.to_id_set(&graph), id_set(&["mid-a"]));
    // `tests/test_gap_fill.py:158`.
    assert!(gap_fill_chain_for_stack(&states, &graph, &ids(&["top"]), None).is_none());
    // `tests/test_gap_fill.py:174-176`.
    let wide = gap_fill_chain_for_stack(&states, &graph, &ids(&["top", "mid", "low"]), None)
        .expect("the stack is switched down");
    assert!(wide.contains_id(&graph, "low-a"));
    assert!(
        blocking_gap_ancestors(&states, &graph, Some("top"), None).contains_id(&graph, "low-a")
    );
}

#[test]
fn descent_reaches_the_course_holding_the_real_blocker() {
    let graph = ladder();
    let states = floor_states(&graph, "top");
    // `tests/test_gap_fill.py:202`.
    assert_eq!(
        resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top")),
        ids(&["top", "mid", "low"])
    );
    // `tests/test_gap_fill.py:211`.
    let stack = resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top"));
    assert_eq!(
        serveable_gap_frontier(&states, &graph, &stack, None).to_id_set(&graph),
        id_set(&["low-a"])
    );
}

#[test]
fn descent_stops_once_the_tip_can_serve() {
    let graph = ladder();
    let mut states = floor_states(&graph, "top");
    states.insert(
        "low-a".to_owned(),
        TopicState {
            status: TopicStatus::Floor,
            ..TopicState::default()
        },
    );
    // `tests/test_gap_fill.py:220-221`.
    let stack = resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top"));
    assert_eq!(stack, ids(&["top", "mid"]));
    assert_eq!(
        serveable_gap_frontier(&states, &graph, &stack, None).to_id_set(&graph),
        id_set(&["mid-a"])
    );
}

#[test]
fn stack_is_just_the_base_course_when_not_blocked() {
    let graph = ladder();
    let states = floor_states(&graph, "low");
    // `tests/test_gap_fill.py:227`.
    assert_eq!(
        resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("low")),
        ids(&["low"])
    );
}

#[test]
fn gap_fill_terminates_and_clears_the_whole_course() {
    let graph = ladder();
    let mut states = floor_states(&graph, "top");
    for _ in 0..20 {
        let stack = resolve_gap_fill_stack(&states, &graph, &cfg(), T_US, Some("top"));
        let serveable = serveable_gap_frontier(&states, &graph, &stack, None).to_id_set(&graph);
        if serveable.is_empty() {
            break;
        }
        for tid in serveable {
            states.insert(
                tid,
                TopicState {
                    status: TopicStatus::Floor,
                    ..TopicState::default()
                },
            );
        }
    }
    let mastered = known_set(&states, &graph);
    // `tests/test_gap_fill.py:243-248`.
    assert!(mastered.contains_id(&graph, "top-a"));
    assert!(!mastered.contains_id(&graph, "mid-b"));
    assert!(!mastered.contains_id(&graph, "mid-free"));
}

// --------------------------------------------------------------------------- //
// Retry delay and nearly-due ordering
// --------------------------------------------------------------------------- //

#[test]
fn in_retry_delay_needs_a_failed_kp_and_a_stamp() {
    let cfg = cfg();
    let failed_at = T_US - days(1) / 2;
    assert!(in_retry_delay(&failed_lesson(failed_at), &cfg, T_US));
    assert!(!in_retry_delay(
        &failed_lesson(failed_at),
        &cfg,
        failed_at + DAY_US
    ));
    // A stamp with no failed knowledge point never blocks.
    assert!(!in_retry_delay(&learned(0.5), &cfg, T_US));
    // A failed knowledge point with no stamp never blocks.
    let mut kp_progress = BTreeMap::new();
    kp_progress.insert("kp1".to_owned(), KpProgress::FailedTwice);
    let unstamped = TopicState {
        kp_progress,
        ..TopicState::default()
    };
    assert!(!in_retry_delay(&unstamped, &cfg, T_US));
}

#[test]
fn nearly_due_is_ordered_soonest_due_first() {
    let graph = graph_of(
        vec![topic("a").build(), topic("b").build(), topic("c").build()],
        &[],
    );
    // Memory 0.51 is nearer its due date than 0.59, so it comes first.
    let states = states_of(vec![
        ("a", learned(0.59)),
        ("b", learned(0.51)),
        ("c", learned(0.9)),
    ]);
    assert_eq!(nearly_due(&states, &graph, &cfg(), T_US), ids(&["b", "a"]));
}
