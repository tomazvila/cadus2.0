//! The pinned FIRe behaviors of spec section 8 (`tests/test_fire.py`), part 3:
//! the ability model, the review grade, the neighborhood, and the bit-exact and
//! non-finite values of 1.0.
//!
//! Every expected number here is a LITERAL from the 1.0 test file or from the
//! spec, never a value re-derived from the code under test. Where 1.0 asserts an
//! exact value, the assertion is exact; where 1.0 uses `pytest.approx`, the
//! assertion is `|a - b| <= 1e-6 * max(1, |b|)`. Every ORDER assertion is exact
//! list equality.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

mod common;

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::{TopicStatus, WorkQuality};
use cadus_core::fire::{
    AttemptResult, Propagation, PropagationKind, apply_attempt, apply_attempt_checked,
    grade_review, initial_ability, interval_for, knockout, memory_at, raw_delta, speed_for,
};
use cadus_core::learner::TopicState;
use common::fire::{abilities_after, p364_states, pass_two_digit, seed_states, states_of};
use common::{Learned, T_US, assert_approx, days, graph, learned, p364_graph, plain_topic, topic};

// --------------------------------------------------------------------------- //
// The two legs of apply_attempt and their gates
// --------------------------------------------------------------------------- //

/// The p.364 states, all learned at memory 0.5, with the given `min_credit`.
fn p364_run(attempt: &AttemptResult, min_credit: f64) -> Vec<Propagation> {
    let mut cfg = Config::default();
    cfg.fire.min_credit = min_credit;
    let states = p364_states(|| learned(0.5));
    let (_, props) = apply_attempt(&states, attempt, &p364_graph(), &cfg, T_US);
    props
}

#[test]
fn a_passed_attempt_with_a_zero_raw_delta_sends_no_credit() {
    // A blow-off that still passes earns `q = 0`, so the raw delta is 0.0 and
    // the credit leg does not run, even with no minimum credit to drop it.
    let attempt = AttemptResult::new("two-digit-mult", true, WorkQuality::Blowoff);
    assert_eq!(p364_run(&attempt, 0.0), Vec::new());
}

#[test]
fn a_missed_attempt_with_a_zero_raw_delta_sends_no_penalty() {
    // A perfect miss earns `-(1 - 1) = -0.0`, which is not below zero, so the
    // penalty leg does not run, even with no minimum credit to drop it.
    let attempt = AttemptResult::new("one-digit-mult", false, WorkQuality::Perfect);
    assert_eq!(p364_run(&attempt, 0.0), Vec::new());
}

#[test]
fn a_penalty_at_exactly_min_credit_lands() {
    // A blow-off miss on `one-digit-mult` sends `-1.0 x 0.8` up to
    // `two-digit-mult`. The gate drops a penalty STRICTLY UNDER `min_credit`,
    // so 0.8 lands when the minimum is 0.8.
    let attempt = AttemptResult::new("one-digit-mult", false, WorkQuality::Blowoff);
    let props = p364_run(&attempt, 0.8);
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].topic, "two-digit-mult");
    assert_eq!(props[0].raw_delta, -0.8);
    assert_eq!(props[0].kind, PropagationKind::Penalty);
}

// --------------------------------------------------------------------------- //
// initial_ability (test_fire.py:449-466)
// --------------------------------------------------------------------------- //

#[test]
fn initial_ability_is_neighborhood_mean() {
    let cfg = Config::default();
    let graph = p364_graph();
    assert_approx(
        initial_ability("two-digit-mult", &graph, &seed_states(), &cfg),
        0.6,
        "neighborhood mean",
    );
}

#[test]
fn initial_ability_default_when_no_touched_neighbor() {
    let cfg = Config::default();
    let graph = p364_graph();
    let empty: BTreeMap<String, TopicState> = BTreeMap::new();
    assert_eq!(initial_ability("two-digit-mult", &graph, &empty, &cfg), 0.5);

    let untouched = states_of(&[(
        "addition",
        TopicState {
            status: TopicStatus::Untouched,
            ability: 0.9,
            ..TopicState::default()
        },
    )]);
    assert_eq!(
        initial_ability("two-digit-mult", &graph, &untouched, &cfg),
        0.5
    );
}

// --------------------------------------------------------------------------- //
// ability_update (test_fire.py:474-497)
// --------------------------------------------------------------------------- //

#[test]
fn ability_update_ewma_and_downward_on_correct() {
    let deltas = abilities_after("two-digit-mult", true);
    assert_approx(deltas["two-digit-mult"], 0.15, "explicit EWMA step");
    assert_approx(deltas["one-digit-mult"], 0.12, "downward at W 0.8");
    assert_approx(deltas["addition"], 0.09, "downward at W 0.6");

    // Trap T6: the attempted topic is inserted first, then the neighbors in
    // sorted-id order, and the caller applies the map in that order.
    let order: Vec<&str> = deltas.keys().map(String::as_str).collect();
    assert_eq!(order, vec!["two-digit-mult", "addition", "one-digit-mult"]);
}

#[test]
fn ability_update_incorrect_propagates_up() {
    let deltas = abilities_after("addition", false);
    assert_approx(deltas["addition"], -0.15, "explicit EWMA step");
    assert_approx(deltas["two-digit-mult"], -0.09, "upward at W 0.6");
    assert!(!deltas.contains_key("one-digit-mult"));

    let order: Vec<&str> = deltas.keys().map(String::as_str).collect();
    assert_eq!(order, vec!["addition", "two-digit-mult"]);
}

// --------------------------------------------------------------------------- //
// knockout (test_fire.py:505-512)
// --------------------------------------------------------------------------- //

#[test]
fn knockout_predicate() {
    let cfg = Config::default();
    let graph = p364_graph();
    assert!(knockout("two-digit-mult", "one-digit-mult", &graph, &cfg));
    assert!(!knockout("two-digit-mult", "addition", &graph, &cfg));
    assert!(knockout("addition", "addition", &graph, &cfg));
}

// --------------------------------------------------------------------------- //
// grade_review (test_fire.py:520-549)
// --------------------------------------------------------------------------- //

#[test]
fn grade_review_canonical_patterns() {
    let cfg = Config::default();
    let (passed, score) = grade_review(&[false, true, false, true, true], &cfg);
    assert!(passed);
    assert_approx(score, 11.0 / 15.0, "improving trajectory score");

    let (passed2, score2) = grade_review(&[true, true, false, true, false], &cfg);
    assert!(!passed2);
    assert_approx(score2, 7.0 / 15.0, "deteriorating trajectory score");
}

#[test]
fn grade_review_final_question_gate() {
    let cfg = Config::default();
    let (passed, score) = grade_review(&[true, true, true, true, false], &cfg);
    assert_approx(score, 10.0 / 15.0, "score above the pass line");
    assert!(!passed);
}

#[test]
fn grade_review_edge_cases() {
    let cfg = Config::default();
    let (passed, score) = grade_review(&[true], &cfg);
    assert!(passed);
    assert_approx(score, 1.0, "single correct question");

    let (passed, score) = grade_review(&[false], &cfg);
    assert!(!passed);
    assert_approx(score, 0.0, "single wrong question");

    let (passed, score) = grade_review(&[true, true, true, true], &cfg);
    assert!(passed);
    assert_approx(score, 1.0, "all correct");

    let (passed, score) = grade_review(&[false, false, false, false], &cfg);
    assert!(!passed);
    assert_approx(score, 0.0, "all wrong");

    assert_eq!(grade_review(&[], &cfg), (false, 0.0));
}

// --------------------------------------------------------------------------- //
// The neighborhood a key knowledge-point prerequisite adds (graph.py:408-431)
// --------------------------------------------------------------------------- //

#[test]
fn neighborhood_reaches_a_key_knowledge_point_prerequisite() {
    let cfg = Config::default();
    let mut owner = topic("owner", &[], 0.3, &[]);
    owner.knowledge_points = vec![common::knowledge_point("kp1", &["far"])];
    // `far` sits in another module, so only the knowledge-point key edge reaches it.
    let graph = common::graph_of_units(
        &[("M", vec![owner]), ("N", vec![plain_topic("far", &[])])],
        "c",
    );
    let states = states_of(&[("far", Learned::new(0.5).ability(0.8).state())]);
    assert_approx(
        initial_ability("owner", &graph, &states, &cfg),
        0.8,
        "the one touched neighbor",
    );
}

// --------------------------------------------------------------------------- //
// Bit-exact 1.0 values (a run of the 1.0 engine, not a re-derivation)
// --------------------------------------------------------------------------- //

/// The last bit of each value below comes from a run of 1.0 `cadus.fire` on the
/// same inputs. An `approx` assertion passes on a value that drifts in the last
/// bit; the fold digest of U3 does not, so these assertions are exact.
#[test]
fn fire_values_are_bit_exact_with_1_0() {
    let cfg = Config::default();
    assert_eq!(
        raw_delta(1.0, 0.7, true, &cfg, false),
        0.600_000_000_000_000_1
    );
    assert_eq!(raw_delta(1.0, 1.0, true, &cfg, false), 0.15);
    assert_eq!(raw_delta(0.15, 0.5, false, &cfg, false), -0.85);
    assert_eq!(interval_for(3.5, &cfg), 33.0);
    assert_eq!(speed_for(0.5, 0.0, &cfg), 2.0);
    assert_eq!(speed_for(0.3, 0.3, &cfg), 1.0);

    let graph = p364_graph();
    let (next, _) = pass_two_digit(&p364_states(|| learned(0.5)), &graph);
    assert_eq!(next["two-digit-mult"].memory_base, 1.5);
    assert_eq!(next["two-digit-mult"].rep_num, 4.0);
    assert_eq!(next["two-digit-mult"].interval_days, 45.0);
    assert_eq!(next["one-digit-mult"].memory_base, 1.3);
    assert_eq!(next["one-digit-mult"].rep_num, 3.8);
    assert_eq!(next["addition"].memory_base, 1.1);
    assert_eq!(next["addition"].rep_num, 3.6);

    let failing = p364_states(|| Learned::new(1.0).rep(3.0).state());
    let (after_miss, _) = apply_attempt(
        &failing,
        &AttemptResult::new("addition", false, WorkQuality::Poor),
        &graph,
        &cfg,
        T_US,
    );
    assert_eq!(after_miss["addition"].rep_num, 2.15);
    assert_eq!(after_miss["addition"].memory_base, 0.150_000_000_000_000_02);
    assert_eq!(after_miss["two-digit-mult"].rep_num, 2.49);
    assert_eq!(after_miss["two-digit-mult"].memory_base, 0.49);
    assert_eq!(
        after_miss["two-digit-mult"].interval_days,
        15.390_000_000_000_002
    );

    assert_eq!(
        initial_ability("two-digit-mult", &graph, &seed_states(), &cfg),
        0.600_000_000_000_000_1
    );

    let deltas = abilities_after("two-digit-mult", true);
    assert_eq!(deltas["two-digit-mult"], 0.15);
    assert_eq!(deltas["addition"], 0.09);
    assert_eq!(deltas["one-digit-mult"], 0.12);

    assert_eq!(
        grade_review(&[false, true, false, true, true], &cfg),
        (true, 0.733_333_333_333_333_3)
    );
    assert_eq!(
        grade_review(&[true, true, false, true, false], &cfg),
        (false, 0.466_666_666_666_666_7)
    );
    assert_eq!(
        grade_review(&[true, true, true, true, false], &cfg),
        (false, 0.666_666_666_666_666_6)
    );
}

// --------------------------------------------------------------------------- //
// Review 1 finding #11: a decay outside the finite range stops the fold
// --------------------------------------------------------------------------- //

/// A state whose `t0` is 12418 days AFTER the read instant, over a 4.5-day
/// interval. The exponent is `-2759.5555555555557`, and CPython raises
/// `OverflowError: (34, 'Numerical result out of range')` at `cadus/fire.py:179`.
fn overflow_state() -> TopicState {
    Learned::new(1.0)
        .interval(4.5)
        .t0(T_US + days(12418))
        .state()
}

#[test]
fn memory_at_leaves_the_finite_range_where_cpython_raises() {
    assert!(!memory_at(&overflow_state(), T_US).is_finite());
    // The 1.0 fold builds no model there, so the port must not fold it either.
    assert!(memory_at(&learned(1.0), T_US).is_finite());
}

#[test]
fn apply_attempt_checked_reports_the_non_finite_topic() {
    let cfg = Config::default();
    let tree = graph(vec![plain_topic("a", &[])]);
    let states = states_of(&[("a", overflow_state())]);
    let attempt = AttemptResult::new("a", true, WorkQuality::Perfect);

    let error = apply_attempt_checked(&states, &attempt, &tree, &cfg, T_US).unwrap_err();
    assert_eq!(error.topic, "a");
    assert_eq!(
        error.to_string(),
        "the decay of topic `a` is not a finite number"
    );

    // The unchecked form keeps its old result. Only the fold stops.
    let (new_states, props) = apply_attempt(&states, &attempt, &tree, &cfg, T_US);
    assert!(props.is_empty());
    assert!(!new_states.get("a").unwrap().memory_base.is_finite());

    // A finite state reports nothing.
    let good = states_of(&[("a", learned(1.0))]);
    let (ok_states, _props) =
        apply_attempt_checked(&good, &attempt, &tree, &cfg, T_US).expect("a finite state folds");
    assert!(ok_states.get("a").unwrap().memory_base.is_finite());
}
