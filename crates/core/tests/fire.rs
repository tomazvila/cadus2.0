//! The pinned FIRe behaviors of spec section 8 (`tests/test_fire.py`).
//!
//! Every expected number here is a LITERAL from the 1.0 test file or from the
//! spec, never a value re-derived from the code under test. Where 1.0 asserts an
//! exact value, the assertion is exact; where 1.0 uses `pytest.approx`, the
//! assertion is `|a - b| <= 1e-6 * max(1, |b|)`. Every ORDER assertion is exact
//! list equality.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::float_cmp
)]

mod common;

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::{TopicStatus, WorkQuality};
use cadus_core::fire::{
    ASSISTED_CREDIT, AttemptResult, INTERVAL_CAP_DAYS, NEARLY_DUE_THRESHOLD,
    PASS_QUALITY_THRESHOLD, PropagationKind, QUALITY_Q, ReviewState, TEST_PREP_DUE_THRESHOLD,
    ability_update, apply_attempt, decay_for, grade_review, initial_ability, interval_for,
    is_pass_quality, knockout, memory_at, quality_q, raw_delta, review_state, speed_for,
};
use cadus_core::learner::TopicState;
use common::{Learned, T_US, assert_approx, days, graph, learned, p364_graph, plain_topic, topic};

/// The states the 1.0 tests build with a dict comprehension over the graph.
fn states_of(entries: &[(&str, TopicState)]) -> BTreeMap<String, TopicState> {
    entries
        .iter()
        .map(|(id, state)| ((*id).to_owned(), state.clone()))
        .collect()
}

// --------------------------------------------------------------------------- //
// Quality tiers (test_fire.py:131-143)
// --------------------------------------------------------------------------- //

#[test]
fn quality_tier_q_values() {
    assert_eq!(
        QUALITY_Q,
        [
            (WorkQuality::Perfect, 1.0),
            (WorkQuality::NearlyPerfect, 0.85),
            (WorkQuality::Passable, 0.7),
            (WorkQuality::NearlyPassable, 0.4),
            (WorkQuality::Poor, 0.15),
            (WorkQuality::Blowoff, 0.0),
        ]
    );
    assert_eq!(quality_q(WorkQuality::Perfect), 1.0);
    assert!(is_pass_quality(WorkQuality::Passable));
    assert!(!is_pass_quality(WorkQuality::NearlyPassable));
}

#[test]
fn module_constants_hold_the_1_0_values() {
    assert_eq!(PASS_QUALITY_THRESHOLD, 0.7);
    assert_eq!(NEARLY_DUE_THRESHOLD, 0.6);
    assert_eq!(TEST_PREP_DUE_THRESHOLD, 0.7);
    assert_eq!(INTERVAL_CAP_DAYS, 730.0);
    assert_eq!(ASSISTED_CREDIT, 0.5);
}

// --------------------------------------------------------------------------- //
// memory_at (test_fire.py:152-164)
// --------------------------------------------------------------------------- //

#[test]
fn memory_at_halflife() {
    let one_interval = Learned::new(1.0).interval(10.0).t0(T_US - days(10)).state();
    assert_approx(memory_at(&one_interval, T_US), 0.5, "one interval elapsed");
    let two_intervals = Learned::new(1.0).interval(10.0).t0(T_US - days(20)).state();
    assert_approx(memory_at(&two_intervals, T_US), 0.25, "two intervals");
}

#[test]
fn memory_at_untouched_is_undecayed_base() {
    let fresh = TopicState::default();
    assert_eq!(memory_at(&fresh, T_US), 0.0);
    let seeded = TopicState {
        memory_base: 0.9,
        ..TopicState::default()
    };
    assert_eq!(memory_at(&seeded, T_US), 0.9);
}

// --------------------------------------------------------------------------- //
// review_state bands (test_fire.py:170-240)
// --------------------------------------------------------------------------- //

/// The 1.0 band table, `(label, state, test_prep, expected)`.
fn review_state_cases() -> Vec<(&'static str, TopicState, bool, ReviewState)> {
    let floor_with_memory = TopicState {
        status: TopicStatus::Floor,
        memory_base: 0.1,
        t0: Some(cadus_core::event::Timestamp::from_micros(T_US)),
        interval_days: 10.0,
        ..TopicState::default()
    };
    vec![
        (
            "untouched (memory 0!)",
            TopicState::default(),
            false,
            ReviewState::OffSchedule,
        ),
        (
            "untouched, test prep",
            TopicState::default(),
            true,
            ReviewState::OffSchedule,
        ),
        (
            "frontier",
            TopicState {
                status: TopicStatus::Frontier,
                ..TopicState::default()
            },
            false,
            ReviewState::OffSchedule,
        ),
        (
            "mastery floor",
            TopicState {
                status: TopicStatus::Floor,
                ..TopicState::default()
            },
            false,
            ReviewState::OffSchedule,
        ),
        (
            "floor with decayed memory",
            floor_with_memory,
            false,
            ReviewState::OffSchedule,
        ),
        (
            "placed counts as history",
            TopicState {
                status: TopicStatus::Placed,
                ..TopicState::default()
            },
            false,
            ReviewState::Due,
        ),
        ("m 0.0", learned(0.0), false, ReviewState::Due),
        ("m 0.49", learned(0.49), false, ReviewState::Due),
        (
            "m 0.5 (due boundary)",
            learned(0.5),
            false,
            ReviewState::Due,
        ),
        ("m 0.5001", learned(0.5001), false, ReviewState::NearlyDue),
        ("m 0.55", learned(0.55), false, ReviewState::NearlyDue),
        (
            "m 0.6 (nearly boundary)",
            learned(0.6),
            false,
            ReviewState::NearlyDue,
        ),
        ("m 0.6001", learned(0.6001), false, ReviewState::OnSchedule),
        ("m 0.65", learned(0.65), false, ReviewState::OnSchedule),
        ("m 0.7", learned(0.7), false, ReviewState::OnSchedule),
        ("m 1.0", learned(1.0), false, ReviewState::OnSchedule),
        ("m 0.5, test prep", learned(0.5), true, ReviewState::Due),
        ("m 0.55, test prep", learned(0.55), true, ReviewState::Due),
        ("m 0.6, test prep", learned(0.6), true, ReviewState::Due),
        (
            "m 0.65, test prep (due, NOT nearly)",
            learned(0.65),
            true,
            ReviewState::Due,
        ),
        (
            "m 0.7 (test-prep boundary)",
            learned(0.7),
            true,
            ReviewState::Due,
        ),
        (
            "m 0.7001, test prep",
            learned(0.7001),
            true,
            ReviewState::OnSchedule,
        ),
        (
            "m 1.0, test prep",
            learned(1.0),
            true,
            ReviewState::OnSchedule,
        ),
    ]
}

#[test]
fn review_state_bands() {
    let cfg = Config::default();
    for (label, state, test_prep, expected) in review_state_cases() {
        assert_eq!(
            review_state(&state, T_US, &cfg, test_prep),
            expected,
            "{label}"
        );
    }
}

#[test]
fn review_state_bands_are_disjoint_and_total() {
    let cfg = Config::default();
    let mut seen: Vec<ReviewState> = review_state_cases()
        .into_iter()
        .map(|(_, state, test_prep, _)| review_state(&state, T_US, &cfg, test_prep))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(
        seen,
        vec![
            ReviewState::OffSchedule,
            ReviewState::Due,
            ReviewState::NearlyDue,
            ReviewState::OnSchedule,
        ]
    );
}

#[test]
fn review_state_never_calls_an_untouched_topic_due() {
    let cfg = Config::default();
    let fresh = TopicState::default();
    assert!(memory_at(&fresh, T_US) <= cfg.fire.due_threshold);
    assert_eq!(
        review_state(&fresh, T_US, &cfg, false),
        ReviewState::OffSchedule
    );
}

#[test]
fn review_state_decays_into_the_bands_over_time() {
    let cfg = Config::default();
    let state = Learned::new(1.0).interval(10.0).t0(T_US).state();
    assert_eq!(
        review_state(&state, T_US, &cfg, false),
        ReviewState::OnSchedule
    );
    assert_eq!(
        review_state(&state, T_US + days(7), &cfg, false),
        ReviewState::OnSchedule
    );
    assert_eq!(
        review_state(&state, T_US + days(8), &cfg, false),
        ReviewState::NearlyDue
    );
    assert_eq!(
        review_state(&state, T_US + days(10), &cfg, false),
        ReviewState::Due
    );
}

// --------------------------------------------------------------------------- //
// interval table (test_fire.py:249-257)
// --------------------------------------------------------------------------- //

#[test]
fn interval_table_interpolation_and_cap() {
    let cfg = Config::default();
    assert_eq!(interval_for(0.0, &cfg), 2.0);
    assert_eq!(interval_for(1.0, &cfg), 4.5);
    assert_approx(interval_for(3.5, &cfg), 33.0, "repNum 3.5");
    assert_eq!(interval_for(7.0, &cfg), 480.0);
    assert_eq!(interval_for(20.0, &cfg), 480.0);
    assert_eq!(interval_for(-4.0, &cfg), 2.0);
}

// --------------------------------------------------------------------------- //
// speed (test_fire.py:267-272)
// --------------------------------------------------------------------------- //

#[test]
fn speed_formula_and_clamp() {
    let cfg = Config::default();
    assert_approx(speed_for(0.5, 0.0, &cfg), 2.0, "ability 0.5 difficulty 0.0");
    assert_approx(speed_for(0.3, 0.3, &cfg), 1.0, "ability equals difficulty");
    assert_eq!(speed_for(-5.0, 1.0, &cfg), 0.33);
    assert_eq!(speed_for(50.0, 0.0, &cfg), 3.0);
}

// --------------------------------------------------------------------------- //
// raw_delta (test_fire.py:282-293)
// --------------------------------------------------------------------------- //

#[test]
fn raw_delta_pass_early_factor() {
    let cfg = Config::default();
    assert_approx(raw_delta(1.0, 0.5, true, &cfg, false), 1.0, "fully due");
    assert_approx(
        raw_delta(1.0, 0.7, true, &cfg, false),
        0.6,
        "partially early",
    );
    assert_approx(
        raw_delta(1.0, 1.0, true, &cfg, false),
        0.15,
        "much too early, floored",
    );
}

#[test]
fn raw_delta_fail_no_discount() {
    let cfg = Config::default();
    assert_approx(raw_delta(0.15, 0.5, false, &cfg, false), -0.85, "poor, due");
    assert_approx(
        raw_delta(0.15, 1.0, false, &cfg, false),
        -0.85,
        "poor, fresh",
    );
    assert_approx(raw_delta(0.0, 0.5, false, &cfg, false), -1.0, "blowoff");
}

// --------------------------------------------------------------------------- //
// decay (test_fire.py:304-313)
// --------------------------------------------------------------------------- //

#[test]
fn decay_grows_with_overdueness_and_caps() {
    let cfg = Config::default();
    let on_time = Learned::new(1.0).interval(10.0).t0(T_US - days(10)).state();
    assert_approx(decay_for(&on_time, T_US, &cfg), 1.0, "on time");
    let twice = Learned::new(1.0).interval(10.0).t0(T_US - days(20)).state();
    assert_approx(decay_for(&twice, T_US, &cfg), 2.0, "two intervals");
    let way_over = Learned::new(1.0).interval(10.0).t0(T_US - days(50)).state();
    assert_eq!(decay_for(&way_over, T_US, &cfg), 3.0);
    let early = Learned::new(1.0).interval(10.0).t0(T_US - days(3)).state();
    assert_eq!(decay_for(&early, T_US, &cfg), 1.0);
}

// --------------------------------------------------------------------------- //
// Golden scenario (a): p.364 credit (test_fire.py:321-341)
// --------------------------------------------------------------------------- //

#[test]
fn golden_p364_pass_credits_both_encompassed_topics() {
    let cfg = Config::default();
    let graph = p364_graph();
    let states = states_of(&[
        ("addition", learned(0.5)),
        ("one-digit-mult", learned(0.5)),
        ("two-digit-mult", learned(0.5)),
    ]);

    let (next, props) = apply_attempt(
        &states,
        &AttemptResult::new("two-digit-mult", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );

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
    let states = states_of(&[
        ("addition", Learned::new(1.0).rep(3.0).state()),
        ("one-digit-mult", Learned::new(1.0).rep(3.0).state()),
        ("two-digit-mult", Learned::new(1.0).rep(3.0).state()),
    ]);

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
    let cfg = Config::default();
    let graph = graph(vec![plain_topic("solo", &[])]);
    let attempt = AttemptResult::new("solo", false, WorkQuality::Poor);

    let on_time = Learned::new(1.0)
        .rep(5.0)
        .interval(10.0)
        .t0(T_US - days(10))
        .state();
    let (next_on, _) = apply_attempt(
        &states_of(&[("solo", on_time)]),
        &attempt,
        &graph,
        &cfg,
        T_US,
    );

    let overdue = Learned::new(1.0)
        .rep(5.0)
        .interval(10.0)
        .t0(T_US - days(30))
        .state();
    let (next_over, _) = apply_attempt(
        &states_of(&[("solo", overdue)]),
        &attempt,
        &graph,
        &cfg,
        T_US,
    );

    assert_approx(next_on["solo"].rep_num, 4.15, "on-time repNum");
    assert_approx(next_over["solo"].rep_num, 2.45, "overdue repNum");
    let step_on = 5.0 - next_on["solo"].rep_num;
    let step_over = 5.0 - next_over["solo"].rep_num;
    assert!(step_over > step_on);
    assert_approx(step_over, 3.0 * step_on, "overdue step is three times");
}

// --------------------------------------------------------------------------- //
// Golden scenario (c): early-credit floor (test_fire.py:400-418)
// --------------------------------------------------------------------------- //

#[test]
fn golden_too_early_review_floor_credit() {
    let cfg = Config::default();
    let graph = graph(vec![plain_topic("solo", &[])]);
    let attempt = AttemptResult::new("solo", true, WorkQuality::Perfect);

    let too_early = Learned::new(1.0).rep(3.0).state();
    let (next_early, _) = apply_attempt(
        &states_of(&[("solo", too_early)]),
        &attempt,
        &graph,
        &cfg,
        T_US,
    );
    assert_approx(next_early["solo"].memory_base, 1.15, "too-early memoryBase");
    assert_approx(next_early["solo"].rep_num, 3.15, "too-early repNum");

    let on_time = Learned::new(0.5).rep(3.0).state();
    let (next_on, _) = apply_attempt(
        &states_of(&[("solo", on_time)]),
        &attempt,
        &graph,
        &cfg,
        T_US,
    );
    assert_approx(next_on["solo"].memory_base, 1.5, "on-time memoryBase");
    assert_approx(next_on["solo"].rep_num, 4.0, "on-time repNum");

    assert_approx(
        next_early["solo"].rep_num - 3.0,
        0.15 * (next_on["solo"].rep_num - 3.0),
        "the floor fraction of the on-time advance",
    );
}

// --------------------------------------------------------------------------- //
// Golden scenario (d): speed (test_fire.py:426-441)
// --------------------------------------------------------------------------- //

#[test]
fn golden_speed_two_advances_twice_as_fast() {
    let cfg = Config::default();
    let graph = graph(vec![plain_topic("fast", &[]), plain_topic("slow", &[])]);

    let fast = Learned::new(0.5).rep(1.0).speed(2.0).state();
    let (next_fast, _) = apply_attempt(
        &states_of(&[("fast", fast)]),
        &AttemptResult::new("fast", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );
    let slow = Learned::new(0.5).rep(1.0).speed(1.0).state();
    let (next_slow, _) = apply_attempt(
        &states_of(&[("slow", slow)]),
        &AttemptResult::new("slow", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );

    let delta_fast = next_fast["fast"].rep_num - 1.0;
    let delta_slow = next_slow["slow"].rep_num - 1.0;
    assert_approx(delta_fast, 2.0, "speed 2 advance");
    assert_approx(delta_slow, 1.0, "speed 1 advance");
    assert_approx(delta_fast, 2.0 * delta_slow, "twice as fast");
}

// --------------------------------------------------------------------------- //
// initial_ability (test_fire.py:449-466)
// --------------------------------------------------------------------------- //

#[test]
fn initial_ability_is_neighborhood_mean() {
    let cfg = Config::default();
    let graph = p364_graph();
    let states = states_of(&[
        ("addition", Learned::new(0.5).ability(0.4).state()),
        ("one-digit-mult", Learned::new(0.5).ability(0.8).state()),
    ]);
    assert_approx(
        initial_ability("two-digit-mult", &graph, &states, &cfg),
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
    let cfg = Config::default();
    let graph = p364_graph();
    let states = states_of(&[
        ("addition", Learned::new(0.5).ability(0.5).state()),
        ("one-digit-mult", Learned::new(0.5).ability(0.5).state()),
        ("two-digit-mult", Learned::new(0.5).ability(0.5).state()),
    ]);

    let deltas = ability_update(&states, "two-digit-mult", true, &graph, &cfg);
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
    let cfg = Config::default();
    let graph = p364_graph();
    let states = states_of(&[
        ("addition", Learned::new(0.5).ability(0.5).state()),
        ("one-digit-mult", Learned::new(0.5).ability(0.5).state()),
        ("two-digit-mult", Learned::new(0.5).ability(0.5).state()),
    ]);

    let deltas = ability_update(&states, "addition", false, &graph, &cfg);
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
// Gates (test_fire.py:557-589)
// --------------------------------------------------------------------------- //

#[test]
fn forced_explicit_topic_absorbs_no_credit() {
    let cfg = Config::default();
    let graph = graph(vec![
        plain_topic("addition", &[]),
        plain_topic("two-digit-mult", &[("addition", 0.6, false)]),
    ]);
    let states = states_of(&[
        ("addition", Learned::new(0.5).speed(0.5).state()),
        ("two-digit-mult", Learned::new(0.5).speed(1.0).state()),
    ]);

    let (next, props) = apply_attempt(
        &states,
        &AttemptResult::new("two-digit-mult", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );
    assert_eq!(next["addition"], states["addition"]);
    assert!(props.iter().all(|p| p.topic != "addition"));
}

#[test]
fn min_credit_drops_tiny_propagation() {
    let cfg = Config::default();
    let graph = graph(vec![
        plain_topic("addition", &[]),
        plain_topic("two-digit-mult", &[("addition", 0.04, false)]),
    ]);
    let states = states_of(&[("addition", learned(0.5)), ("two-digit-mult", learned(0.5))]);

    let (next, props) = apply_attempt(
        &states,
        &AttemptResult::new("two-digit-mult", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );
    assert_eq!(next["addition"], states["addition"]);
    assert!(props.is_empty());
}

#[test]
fn apply_attempt_does_not_mutate_inputs() {
    let cfg = Config::default();
    let graph = p364_graph();
    let states = states_of(&[
        ("addition", learned(0.5)),
        ("one-digit-mult", learned(0.5)),
        ("two-digit-mult", learned(0.5)),
    ]);
    let snapshot = states.clone();
    let _ = apply_attempt(
        &states,
        &AttemptResult::new("two-digit-mult", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );
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
    let cfg = Config::default();
    // `ghost` is named by a prerequisite edge but is not a topic of the tree.
    let graph = graph(vec![plain_topic(
        "two-digit-mult",
        &[("ghost", 0.6, false)],
    )]);
    let states = states_of(&[("two-digit-mult", learned(0.5))]);

    let (next, props) = apply_attempt(
        &states,
        &AttemptResult::new("two-digit-mult", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );
    // A default recipient has speed 1.0 and memory 0.0, so its own credit is
    // raw_delta(1.0, 0.0, pass) = 1.0, scaled by W 0.6.
    let order: Vec<&str> = props.iter().map(|p| p.topic.as_str()).collect();
    assert_eq!(order, vec!["ghost"]);
    assert_approx(props[0].raw_delta, 0.6, "credit to the dangling id");
    assert_approx(next["ghost"].memory_base, 0.6, "dangling memoryBase");
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
    let states = states_of(&[
        ("addition", learned(0.5)),
        ("one-digit-mult", learned(0.5)),
        ("two-digit-mult", learned(0.5)),
    ]);
    let (next, _) = apply_attempt(
        &states,
        &AttemptResult::new("two-digit-mult", true, WorkQuality::Perfect),
        &graph,
        &cfg,
        T_US,
    );
    assert_eq!(next["two-digit-mult"].memory_base, 1.5);
    assert_eq!(next["two-digit-mult"].rep_num, 4.0);
    assert_eq!(next["two-digit-mult"].interval_days, 45.0);
    assert_eq!(next["one-digit-mult"].memory_base, 1.3);
    assert_eq!(next["one-digit-mult"].rep_num, 3.8);
    assert_eq!(next["addition"].memory_base, 1.1);
    assert_eq!(next["addition"].rep_num, 3.6);

    let failing = states_of(&[
        ("addition", Learned::new(1.0).rep(3.0).state()),
        ("one-digit-mult", Learned::new(1.0).rep(3.0).state()),
        ("two-digit-mult", Learned::new(1.0).rep(3.0).state()),
    ]);
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

    let seed_states = states_of(&[
        ("addition", Learned::new(0.5).ability(0.4).state()),
        ("one-digit-mult", Learned::new(0.5).ability(0.8).state()),
    ]);
    assert_eq!(
        initial_ability("two-digit-mult", &graph, &seed_states, &cfg),
        0.600_000_000_000_000_1
    );

    let ability_states = states_of(&[
        ("addition", Learned::new(0.5).ability(0.5).state()),
        ("one-digit-mult", Learned::new(0.5).ability(0.5).state()),
        ("two-digit-mult", Learned::new(0.5).ability(0.5).state()),
    ]);
    let deltas = ability_update(&ability_states, "two-digit-mult", true, &graph, &cfg);
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
