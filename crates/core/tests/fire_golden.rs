//! The pinned FIRe behaviors of spec section 8 (`tests/test_fire.py`), part 2:
//! the four golden attempt scenarios and the propagation gates.
//!
//! Every expected number here is a LITERAL from the 1.0 test file or from the
//! spec, never a value re-derived from the code under test. Where 1.0 asserts an
//! exact value, the assertion is exact; where 1.0 uses `pytest.approx`, the
//! assertion is `|a - b| <= 1e-6 * max(1, |b|)`. Every ORDER assertion is exact
//! list equality.

#![allow(clippy::float_cmp)]

mod common;

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::WorkQuality;
use cadus_core::fire::{AttemptResult, PropagationKind, apply_attempt, raw_delta};
use common::fire::{p364_states, pair_graph, pass_two_digit, solo_miss, solo_pass, states_of};
use common::{Learned, T_US, assert_approx, days, graph, learned, p364_graph, plain_topic};

// --------------------------------------------------------------------------- //
// Golden scenario (a): p.364 credit (test_fire.py:321-341)
// --------------------------------------------------------------------------- //

#[test]
fn golden_p364_pass_credits_both_encompassed_topics() {
    let graph = p364_graph();
    let states = p364_states(|| learned(0.5));
    let (next, props) = pass_two_digit(&states, &graph);

    assert_approx(
        next["two-digit-mult"].memory_base,
        1.5,
        "explicit memoryBase",
    );
    assert_approx(
        next["one-digit-mult"].memory_base,
        1.3,
        "one-digit-mult memoryBase",
    );
    assert_approx(next["addition"].memory_base, 1.1, "addition memoryBase");

    // 1.0 iterates `sorted(reach_weights(...).items())`, so the report is in
    // sorted-id order (trap T5, trap T18).
    let order: Vec<&str> = props.iter().map(|p| p.topic.as_str()).collect();
    assert_eq!(order, vec!["addition", "one-digit-mult"]);
    let by_topic: BTreeMap<&str, &_> = props.iter().map(|p| (p.topic.as_str(), p)).collect();
    assert_approx(by_topic["one-digit-mult"].raw_delta, 0.8, "credit W 0.8");
    assert_eq!(by_topic["one-digit-mult"].kind, PropagationKind::Credit);
    assert_eq!(by_topic["one-digit-mult"].kind.as_str(), "credit");
    assert_approx(by_topic["addition"].raw_delta, 0.6, "credit W 0.6");
}

// --------------------------------------------------------------------------- //
// Golden scenario (a'): p.364 penalty (test_fire.py:344-367)
// --------------------------------------------------------------------------- //

#[test]
fn golden_p364_fail_encompassed_penalizes_encompassing() {
    let cfg = Config::default();
    let graph = p364_graph();
    let states = p364_states(|| Learned::new(1.0).rep(3.0).state());

    let (next, props) = apply_attempt(
        &states,
        &AttemptResult::new("addition", false, WorkQuality::Poor),
        &graph,
        &cfg,
        T_US,
    );

    assert_approx(next["addition"].rep_num, 2.15, "explicit repNum");
    assert_approx(next["two-digit-mult"].rep_num, 2.49, "dependent repNum");
    assert_approx(
        next["two-digit-mult"].memory_base,
        0.49,
        "dependent memoryBase",
    );
    assert_approx(next["one-digit-mult"].rep_num, 3.0, "unrelated repNum");

    let order: Vec<&str> = props.iter().map(|p| p.topic.as_str()).collect();
    assert_eq!(order, vec!["two-digit-mult"]);
    assert_eq!(props[0].kind, PropagationKind::Penalty);
    assert_eq!(props[0].kind.as_str(), "penalty");
}

// --------------------------------------------------------------------------- //
// Golden scenario (b): overdue failure (test_fire.py:375-392)
// --------------------------------------------------------------------------- //

#[test]
fn golden_overdue_failure_larger_backward_step() {
    let on_time = Learned::new(1.0)
        .rep(5.0)
        .interval(10.0)
        .t0(T_US - days(10))
        .state();
    let next_on = solo_miss(on_time);

    let overdue = Learned::new(1.0)
        .rep(5.0)
        .interval(10.0)
        .t0(T_US - days(30))
        .state();
    let next_over = solo_miss(overdue);

    assert_approx(next_on.rep_num, 4.15, "on-time repNum");
    assert_approx(next_over.rep_num, 2.45, "overdue repNum");
    let step_on = 5.0 - next_on.rep_num;
    let step_over = 5.0 - next_over.rep_num;
    assert!(step_over > step_on);
    assert_approx(step_over, 3.0 * step_on, "overdue step is three times");
}

// --------------------------------------------------------------------------- //
// Golden scenario (c): early-credit floor (test_fire.py:400-418)
// --------------------------------------------------------------------------- //

#[test]
fn golden_too_early_review_floor_credit() {
    let next_early = solo_pass("solo", Learned::new(1.0).rep(3.0).state());
    assert_approx(next_early.memory_base, 1.15, "too-early memoryBase");
    assert_approx(next_early.rep_num, 3.15, "too-early repNum");

    let next_on = solo_pass("solo", Learned::new(0.5).rep(3.0).state());
    assert_approx(next_on.memory_base, 1.5, "on-time memoryBase");
    assert_approx(next_on.rep_num, 4.0, "on-time repNum");

    assert_approx(
        next_early.rep_num - 3.0,
        0.15 * (next_on.rep_num - 3.0),
        "the floor fraction of the on-time advance",
    );
}

// --------------------------------------------------------------------------- //
// Golden scenario (d): speed (test_fire.py:426-441)
// --------------------------------------------------------------------------- //

#[test]
fn golden_speed_two_advances_twice_as_fast() {
    let next_fast = solo_pass("fast", Learned::new(0.5).rep(1.0).speed(2.0).state());
    let next_slow = solo_pass("slow", Learned::new(0.5).rep(1.0).speed(1.0).state());

    let delta_fast = next_fast.rep_num - 1.0;
    let delta_slow = next_slow.rep_num - 1.0;
    assert_approx(delta_fast, 2.0, "speed 2 advance");
    assert_approx(delta_slow, 1.0, "speed 1 advance");
    assert_approx(delta_fast, 2.0 * delta_slow, "twice as fast");
}

// --------------------------------------------------------------------------- //
// Gates (test_fire.py:557-589)
// --------------------------------------------------------------------------- //

#[test]
fn forced_explicit_topic_absorbs_no_credit() {
    let graph = pair_graph(0.6);
    let states = states_of(&[
        ("addition", Learned::new(0.5).speed(0.5).state()),
        ("two-digit-mult", Learned::new(0.5).speed(1.0).state()),
    ]);
    let (next, props) = pass_two_digit(&states, &graph);
    assert_eq!(next["addition"], states["addition"]);
    assert!(props.iter().all(|p| p.topic != "addition"));
}

#[test]
fn min_credit_drops_tiny_propagation() {
    let graph = pair_graph(0.04);
    let states = states_of(&[("addition", learned(0.5)), ("two-digit-mult", learned(0.5))]);
    let (next, props) = pass_two_digit(&states, &graph);
    assert_eq!(next["addition"], states["addition"]);
    assert!(props.is_empty());
}

#[test]
fn apply_attempt_does_not_mutate_inputs() {
    let graph = p364_graph();
    let states = p364_states(|| learned(0.5));
    let snapshot = states.clone();
    let _ = pass_two_digit(&states, &graph);
    assert_eq!(states, snapshot);
}

// --------------------------------------------------------------------------- //
// The assisted discount (DD-3, fire.py:89 and :498)
// --------------------------------------------------------------------------- //

#[test]
fn assisted_pass_halves_the_credit_and_an_assisted_miss_keeps_its_penalty() {
    let cfg = Config::default();
    // A pass at memory 0.5 earns raw 1.0 clean; assisted halves it to 0.5.
    assert_approx(
        raw_delta(1.0, 0.5, true, &cfg, true),
        0.5,
        "assisted pass credit",
    );
    // A miss keeps the full negative delta: -(1 - 0.15).
    assert_approx(
        raw_delta(0.15, 0.5, false, &cfg, true),
        -0.85,
        "assisted miss penalty",
    );
}

// --------------------------------------------------------------------------- //
// Trap T15: a dangling prerequisite id still receives implicit credit
// --------------------------------------------------------------------------- //

#[test]
fn dangling_prerequisite_id_enters_the_encompassing_map_and_gets_state() {
    // `ghost` is named by a prerequisite edge but is not a topic of the tree.
    let graph = graph(vec![plain_topic(
        "two-digit-mult",
        &[("ghost", 0.6, false)],
    )]);
    let states = states_of(&[("two-digit-mult", learned(0.5))]);
    let (next, props) = pass_two_digit(&states, &graph);
    // A default recipient has speed 1.0 and memory 0.0, so its own credit is
    // raw_delta(1.0, 0.0, pass) = 1.0, scaled by W 0.6.
    let order: Vec<&str> = props.iter().map(|p| p.topic.as_str()).collect();
    assert_eq!(order, vec!["ghost"]);
    assert_approx(props[0].raw_delta, 0.6, "credit to the dangling id");
    assert_approx(next["ghost"].memory_base, 0.6, "dangling memoryBase");
}
