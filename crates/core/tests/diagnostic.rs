//! The placement diagnostic (`cadus_core::diagnostic`), ported from
//! `cadus/diagnostic.py` and its 1.0 tests.
//!
//! The four questions this file answers:
//!
//! 1. Is the probe set a valid cover, and does it always hold the extremes?
//! 2. Does one answer move the balances of exactly the topics section 9 names?
//! 3. Does the questioning stop where section 9 says it stops?
//! 4. Does the final tally place, flag, and defer the right topics?
//!
//! Every expected value is a literal.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

mod common;

use std::collections::BTreeSet;

use cadus_core::config::Config;
use cadus_core::curriculum::model::{Exemplar, Topic};
use cadus_core::curriculum::{AnswerKind, Curriculum};
use cadus_core::diagnostic::{
    DiagState, SLOW_WEIGHT_FLOOR, answer_weight, apply_answer, init_session, is_leaf, next_probe,
    placement, probe_set, undetermined_set,
};

use common::{graph, graph_of_units, plain_topic};

/// The four topics of the chain `a -> b -> c -> d`, each with an exemplar.
fn chain_topics() -> Vec<Topic> {
    vec![
        with_exemplar(plain_topic("a", &[])),
        with_exemplar(plain_topic("b", &[("a", 1.0, true)])),
        with_exemplar(plain_topic("c", &[("b", 1.0, true)])),
        with_exemplar(plain_topic("d", &[("c", 1.0, true)])),
    ]
}

/// A chain `a -> b -> c -> d`: `a` is the sole root and `d` the sole leaf.
fn chain() -> Curriculum {
    graph(chain_topics())
}

/// Two leaves under one root, in one module: each is a sibling of the other.
fn two_leaves() -> Curriculum {
    graph(vec![
        with_exemplar(plain_topic("root", &[])),
        with_exemplar(plain_topic("left", &[("root", 1.0, true)])),
        with_exemplar(plain_topic("right", &[("root", 1.0, true)])),
    ])
}

/// The same topic, with a diagnostic exemplar the probe set needs.
fn with_exemplar(mut topic: Topic) -> Topic {
    topic.diagnostic_exemplar = Some(Exemplar {
        problem: format!("probe {}", topic.id.as_str()),
        answer: "7".to_owned(),
        solution_sketch: None,
    });
    topic
}

/// A state over the named topics, all at zero, with the same probe set.
fn state_over(topics: &[&str]) -> DiagState {
    DiagState {
        course: Some("c".to_owned()),
        probe_set: topics.iter().map(|id| (*id).to_owned()).collect(),
        balances: topics.iter().map(|id| ((*id).to_owned(), 0.0)).collect(),
        answered: Vec::new(),
        supplemental: false,
    }
}

// --------------------------------------------------------------------------- //
// The probe set
// --------------------------------------------------------------------------- //

/// Every topic has a probe among its ancestors within the radius, and one among
/// its descendants within the radius.
fn assert_covers(graph: &Curriculum, probes: &BTreeSet<String>, radius: i64) {
    for topic in graph.topics() {
        let id = topic.id.as_str();
        let idx = graph.idx_of(id).unwrap();
        let mut up: BTreeSet<String> = BTreeSet::new();
        up.insert(id.to_owned());
        let mut down = up.clone();
        // The whole chain is shorter than any radius this file uses, so the
        // transitive sets stand in for the bounded walk.
        for other in graph.ancestors(idx) {
            up.insert(graph.id_of(other).to_owned());
        }
        for other in graph.descendants(idx) {
            down.insert(graph.id_of(other).to_owned());
        }
        assert!(
            up.iter().any(|candidate| probes.contains(candidate)),
            "radius {radius}: {id} has no probe above it"
        );
        assert!(
            down.iter().any(|candidate| probes.contains(candidate)),
            "radius {radius}: {id} has no probe below it"
        );
    }
}

#[test]
fn the_probe_set_is_a_valid_cover_and_holds_the_extremes() {
    let graph = chain();
    let probes = probe_set(&graph, Some("c"), 3);
    assert_covers(&graph, &probes, 3);
    // Only a root covers its own ancestor demand, and only a leaf its own
    // descendant demand, so both are forced.
    assert!(probes.contains("a"), "the root is not a probe: {probes:?}");
    assert!(probes.contains("d"), "the leaf is not a probe: {probes:?}");
}

#[test]
fn a_five_chain_at_radius_one_takes_the_greedy_four() {
    // Each topic raises an ancestor demand and a descendant demand. At radius 1
    // the inner topics `b`, `c`, `d` each cover four demands, and the greedy
    // cover takes `b` first, then `d`, then the two extremes for their own
    // demands. The set is `{a, b, d, e}` and not `{a, c, e}`.
    let mut topics = chain_topics();
    topics.push(with_exemplar(plain_topic("e", &[("d", 1.0, true)])));
    let graph = common::graph(topics);
    let probes = probe_set(&graph, Some("c"), 1);
    let expected: BTreeSet<String> = ["a", "b", "d", "e"]
        .iter()
        .map(|id| (*id).to_owned())
        .collect();
    assert_eq!(probes, expected);
}

#[test]
fn a_smaller_radius_never_needs_fewer_probes() {
    let graph = chain();
    let tight = probe_set(&graph, Some("c"), 1);
    let wide = probe_set(&graph, Some("c"), 3);
    assert_covers(&graph, &tight, 1);
    assert!(
        tight.len() >= wide.len(),
        "radius 1 took {} probes and radius 3 took {}",
        tight.len(),
        wide.len()
    );
}

#[test]
fn a_zero_radius_takes_every_topic_and_an_unknown_course_takes_none() {
    let graph = chain();
    let every: BTreeSet<String> = ["a", "b", "c", "d"].map(str::to_owned).into();
    assert_eq!(probe_set(&graph, Some("c"), 0), every);
    assert_eq!(probe_set(&graph, None, 3), probe_set(&graph, Some("c"), 3));
    assert!(probe_set(&graph, Some("nope"), 3).is_empty());
    // No course means the whole curriculum, with no mastery floor.
    let state = init_session(&graph, &Config::default(), None);
    assert_eq!(state.course, None);
    assert_eq!(state.balances.len(), 4);
}

#[test]
fn init_session_drops_a_topic_with_no_exemplar_from_the_probe_set() {
    let graph = graph(vec![
        with_exemplar(plain_topic("a", &[])),
        plain_topic("b", &[("a", 1.0, true)]),
    ]);
    let state = init_session(&graph, &Config::default(), Some("c"));
    assert_eq!(state.probe_set, vec!["a".to_owned()]);
    // The universe keeps `b`: it is in scope and it takes propagated evidence.
    assert_eq!(
        state.balances.keys().cloned().collect::<Vec<String>>(),
        vec!["a".to_owned(), "b".to_owned()]
    );
    assert_eq!(state.course.as_deref(), Some("c"));
    assert!(state.answered.is_empty());
}

// --------------------------------------------------------------------------- //
// The weight of one answer
// --------------------------------------------------------------------------- //

#[test]
fn a_correct_answer_at_the_expected_time_weighs_one() {
    assert_eq!(answer_weight(true, 30.0, 30.0), 1.0);
    assert_eq!(answer_weight(true, 30.0, 10.0), 1.0);
}

#[test]
fn a_slow_correct_answer_discounts_to_the_floor() {
    assert_eq!(answer_weight(true, 30.0, 60.0), 0.5);
    assert_eq!(answer_weight(true, 30.0, 6000.0), SLOW_WEIGHT_FLOOR);
}

#[test]
fn an_incorrect_answer_always_weighs_one_and_a_missing_time_counts_as_fast() {
    assert_eq!(answer_weight(false, 30.0, 6000.0), 1.0);
    assert_eq!(answer_weight(true, 30.0, 0.0), 1.0);
}

// --------------------------------------------------------------------------- //
// Propagation
// --------------------------------------------------------------------------- //

#[test]
fn a_correct_answer_credits_the_topic_and_every_ancestor() {
    let graph = chain();
    let cfg = Config::default();
    let mut state = state_over(&["a", "b", "c", "d"]);
    apply_answer(&mut state, &graph, "c", true, 1.0, &cfg);
    assert_eq!(state.balances["a"], 1.0);
    assert_eq!(state.balances["b"], 1.0);
    assert_eq!(state.balances["c"], 1.0);
    // A descendant takes nothing from a correct answer.
    assert_eq!(state.balances["d"], 0.0);
    assert_eq!(state.answered, vec!["c".to_owned()]);
}

#[test]
fn an_incorrect_answer_debits_the_topic_and_every_descendant() {
    let graph = chain();
    let cfg = Config::default();
    let mut state = state_over(&["a", "b", "c", "d"]);
    apply_answer(&mut state, &graph, "b", false, 1.0, &cfg);
    assert_eq!(state.balances["a"], 0.0);
    assert_eq!(state.balances["b"], -1.0);
    assert_eq!(state.balances["c"], -1.0);
    assert_eq!(state.balances["d"], -1.0);
}

#[test]
fn a_leaf_answer_carries_its_sign_to_its_sibling_leaves() {
    let graph = two_leaves();
    let cfg = Config::default();
    assert!(is_leaf(&graph, graph.idx_of("left").unwrap()));
    assert!(!is_leaf(&graph, graph.idx_of("root").unwrap()));

    let mut correct = state_over(&["root", "left", "right"]);
    apply_answer(&mut correct, &graph, "left", true, 1.0, &cfg);
    assert_eq!(correct.balances["left"], 1.0);
    assert_eq!(correct.balances["root"], 1.0);
    assert_eq!(correct.balances["right"], cfg.diag.sibling_credit);

    let mut missed = state_over(&["root", "left", "right"]);
    apply_answer(&mut missed, &graph, "left", false, 1.0, &cfg);
    assert_eq!(missed.balances["left"], -1.0);
    assert_eq!(missed.balances["right"], -cfg.diag.sibling_credit);
}

#[test]
fn a_sibling_leaf_takes_the_credit_share_of_the_weight() {
    // The lateral signal is `sibling_credit` TIMES the weight: a half-weight
    // answer moves the sibling by a quarter, not by the full credit.
    let graph = two_leaves();
    let cfg = Config::default();
    let mut state = state_over(&["root", "left", "right"]);
    apply_answer(&mut state, &graph, "left", true, 0.5, &cfg);
    assert_eq!(state.balances["left"], 0.5);
    assert_eq!(state.balances["right"], 0.25);
}

#[test]
fn a_topic_outside_the_universe_takes_no_credit() {
    let graph = chain();
    let cfg = Config::default();
    let mut state = state_over(&["c", "d"]);
    apply_answer(&mut state, &graph, "c", true, 1.0, &cfg);
    assert_eq!(state.balances.len(), 2);
    assert!(!state.balances.contains_key("a"));
}

#[test]
fn an_answer_on_an_unknown_topic_changes_nothing() {
    let graph = chain();
    let cfg = Config::default();
    let mut state = state_over(&["a", "b", "c", "d"]);
    apply_answer(&mut state, &graph, "ghost", true, 1.0, &cfg);
    assert!(state.answered.is_empty());
    assert!(state.balances.values().all(|balance| *balance == 0.0));
}

#[test]
fn one_topic_joins_the_answered_list_once() {
    let graph = chain();
    let cfg = Config::default();
    let mut state = state_over(&["a", "b", "c", "d"]);
    apply_answer(&mut state, &graph, "c", true, 0.4, &cfg);
    apply_answer(&mut state, &graph, "c", true, 0.4, &cfg);
    assert_eq!(state.answered, vec!["c".to_owned()]);
    assert_eq!(state.balances["c"], 0.8);
}

// --------------------------------------------------------------------------- //
// The next question
// --------------------------------------------------------------------------- //

#[test]
fn the_next_probe_settles_the_most_and_a_tie_takes_the_lowest_id() {
    let graph = chain();
    let cfg = Config::default();
    let state = state_over(&["a", "b", "c", "d"]);
    // In a chain every topic reaches the other three, up or down, so all four
    // score 3 and the tie takes the lowest id.
    assert_eq!(next_probe(&state, &graph, &cfg).as_deref(), Some("a"));

    // Give `a` a second dependent, and `a` alone settles four topics while `b`
    // settles three. The winner is then the score and not the id.
    let mut topics = chain_topics();
    topics.push(with_exemplar(plain_topic("e", &[("a", 1.0, true)])));
    let wide = common::graph(topics);
    let wide_state = state_over(&["a", "b", "c", "d", "e"]);
    assert_eq!(next_probe(&wide_state, &wide, &cfg).as_deref(), Some("a"));
}

#[test]
fn a_later_probe_with_a_higher_score_beats_the_first_one() {
    // The root `z` sorts last but settles two topics, while each leaf settles
    // one. The score decides, not the position in the probe set.
    let graph = graph(vec![
        with_exemplar(plain_topic("z", &[])),
        with_exemplar(plain_topic("a", &[("z", 1.0, true)])),
        with_exemplar(plain_topic("b", &[("z", 1.0, true)])),
    ]);
    let cfg = Config::default();
    let state = state_over(&["a", "b", "z"]);
    assert_eq!(next_probe(&state, &graph, &cfg).as_deref(), Some("z"));
}

#[test]
fn a_tie_takes_the_lowest_id_whatever_the_order_of_the_probe_set() {
    // A stored document lists the probes in any order. In a chain all four
    // score 3, and the lowest id wins even when it comes last.
    let graph = chain();
    let cfg = Config::default();
    let state = state_over(&["d", "c", "b", "a"]);
    assert_eq!(next_probe(&state, &graph, &cfg).as_deref(), Some("a"));
}

#[test]
fn a_probe_outside_the_curriculum_is_never_asked() {
    let graph = chain();
    let cfg = Config::default();
    let state = state_over(&["ghost", "d"]);
    assert_eq!(next_probe(&state, &graph, &cfg).as_deref(), Some("d"));
}

#[test]
fn a_determined_topic_is_never_asked() {
    let graph = chain();
    let cfg = Config::default();
    let mut state = state_over(&["a", "b", "c", "d"]);
    // One correct answer on `c` determines `a`, `b` and `c` at once.
    apply_answer(&mut state, &graph, "c", true, 1.0, &cfg);
    assert_eq!(undetermined_set(&state), BTreeSet::from(["d".to_owned()]));
    assert_eq!(next_probe(&state, &graph, &cfg).as_deref(), Some("d"));
    apply_answer(&mut state, &graph, "d", true, 1.0, &cfg);
    assert_eq!(next_probe(&state, &graph, &cfg), None);
}

#[test]
fn the_question_cap_ends_the_diagnostic() {
    let graph = chain();
    let mut cfg = Config::default();
    cfg.diag.max_questions = 1;
    let mut state = state_over(&["a", "b", "c", "d"]);
    apply_answer(&mut state, &graph, "a", true, 0.5, &cfg);
    assert_eq!(next_probe(&state, &graph, &cfg), None);
}

// --------------------------------------------------------------------------- //
// Placement
// --------------------------------------------------------------------------- //

#[test]
fn a_positive_balance_places_and_a_thin_one_is_conditional() {
    let cfg = Config::default();
    let mut state = state_over(&["fat", "thin", "missed", "untouched"]);
    state.balances.insert("fat".to_owned(), 2.5);
    state.balances.insert("thin".to_owned(), 0.5);
    state.balances.insert("missed".to_owned(), -1.0);
    state.answered = vec!["fat".to_owned(), "thin".to_owned(), "missed".to_owned()];

    let result = placement(&state, &cfg);
    assert_eq!(
        result.placed.keys().cloned().collect::<Vec<String>>(),
        vec!["fat".to_owned(), "thin".to_owned()]
    );
    assert_eq!(result.placed["fat"], 2.5);
    assert_eq!(result.conditional, vec!["thin".to_owned()]);
    // A zero balance nobody asked about waits for a supplemental diagnostic.
    assert_eq!(result.supplemental_candidates, vec!["untouched".to_owned()]);
}

#[test]
fn a_zero_balance_that_was_asked_places_nothing_and_defers_nothing() {
    let cfg = Config::default();
    let mut state = state_over(&["asked"]);
    state.answered = vec!["asked".to_owned()];
    let result = placement(&state, &cfg);
    assert!(result.placed.is_empty());
    assert!(result.conditional.is_empty());
    assert!(result.supplemental_candidates.is_empty());
}

#[test]
fn the_event_balances_keep_six_decimals() {
    let cfg = Config::default();
    let mut state = state_over(&["t"]);
    state.balances.insert("t".to_owned(), 1.0 / 3.0);
    let result = placement(&state, &cfg);
    assert_eq!(result.event_balances()["t"], 0.333333);
}

// --------------------------------------------------------------------------- //
// The document
// --------------------------------------------------------------------------- //

#[test]
fn the_document_round_trips_and_ignores_a_key_it_does_not_name() {
    let graph = chain();
    let state = init_session(&graph, &Config::default(), Some("c"));
    let doc = serde_json::to_value(&state).unwrap();
    let back: DiagState = serde_json::from_value(doc).unwrap();
    assert_eq!(back, state);

    // An older row carried `universe`. It reads back with no migration.
    let older = serde_json::json!({
        "course": "c",
        "probe_set": ["a"],
        "balances": { "a": 0.0 },
        "answered": [],
        "universe": ["a"],
    });
    let read: DiagState = serde_json::from_value(older).unwrap();
    assert_eq!(read.probe_set, vec!["a".to_owned()]);
}

#[test]
fn the_scope_of_a_course_holds_its_foundations() {
    // Two modules of one course: the probe set spans both.
    let graph = graph_of_units(
        &[
            ("Basics", vec![with_exemplar(plain_topic("base", &[]))]),
            (
                "Later",
                vec![with_exemplar(plain_topic("later", &[("base", 1.0, true)]))],
            ),
        ],
        "c",
    );
    let state = init_session(&graph, &Config::default(), Some("c"));
    assert_eq!(
        state.balances.keys().cloned().collect::<Vec<String>>(),
        vec!["base".to_owned(), "later".to_owned()]
    );
    assert!(matches!(
        graph
            .topic(graph.idx_of("base").unwrap())
            .unwrap()
            .answer_kind,
        AnswerKind::Numeric
    ));
}
