//! The pinned FIRe behaviors of spec section 8 (`tests/test_fire.py`), part 1:
//! the grade table, the memory decay, the review bands, the interval table,
//! the speed, the raw delta and the overdue decay.
//!
//! Every expected number here is a LITERAL from the 1.0 test file or from the
//! spec, never a value re-derived from the code under test. Where 1.0 asserts an
//! exact value, the assertion is exact; where 1.0 uses `pytest.approx`, the
//! assertion is `|a - b| <= 1e-6 * max(1, |b|)`. Every ORDER assertion is exact
//! list equality.

#![allow(clippy::unwrap_used, clippy::float_cmp)]

mod common;

use cadus_core::config::Config;
use cadus_core::event::{TopicStatus, WorkQuality};
use cadus_core::fire::{
    ASSISTED_CREDIT, INTERVAL_CAP_DAYS, NEARLY_DUE_THRESHOLD, PASS_QUALITY_THRESHOLD, QUALITY_Q,
    ReviewState, TEST_PREP_DUE_THRESHOLD, decay_for, interval_for, is_pass_quality, memory_at,
    py_max, py_min, quality_q, raw_delta, review_state, speed_for,
};
use cadus_core::learner::TopicState;
use common::{Learned, T_US, assert_approx, days, learned};

// --------------------------------------------------------------------------- //
// Quality tiers (test_fire.py:131-143)
// --------------------------------------------------------------------------- //

#[test]
fn quality_tier_q_values() {
    let tiers: [WorkQuality; 6] = [
        WorkQuality::Perfect,
        WorkQuality::NearlyPerfect,
        WorkQuality::Passable,
        WorkQuality::NearlyPassable,
        WorkQuality::Poor,
        WorkQuality::Blowoff,
    ];
    assert_eq!(QUALITY_Q.map(|(tier, _)| tier), tiers);
    assert_eq!(QUALITY_Q.map(|(_, q)| q), [1.0, 0.85, 0.7, 0.4, 0.15, 0.0]);
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

#[test]
fn interval_table_interpolates_between_two_unequal_entries() {
    // Between the entries 4.5 and 10.0 a half step is 7.25, and the position is
    // the fraction of the step, not the whole rep number.
    let cfg = Config::default();
    assert_eq!(interval_for(1.5, &cfg), 7.25);
}

// --------------------------------------------------------------------------- //
// The Python min and max of two floats
// --------------------------------------------------------------------------- //

#[test]
fn py_max_keeps_the_first_of_two_equal_zeros() {
    // CPython returns `a` unless `b > a`, and `-0.0 > 0.0` is false.
    assert_eq!(py_max(0.0, -0.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(py_max(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
}

#[test]
fn py_min_keeps_the_first_of_two_equal_zeros() {
    // CPython returns `a` unless `b < a`, and `0.0 < -0.0` is false.
    assert_eq!(py_min(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
    assert_eq!(py_min(0.0, -0.0).to_bits(), 0.0f64.to_bits());
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
// Review 1 finding #1: the memory decay calls the platform `pow` (trap T21)
// --------------------------------------------------------------------------- //

/// The exponent of the state below, as the port computes it. CPython prints the
/// same value for `((T - t0).total_seconds()) / 86400.0 / 1.0`.
const T21_EXPONENT: f64 = 0.034_594_907_407_407_41;

#[test]
fn memory_at_keeps_the_cpython_pow_value() {
    // 2989 seconds before T, over a one-day interval. CPython 3.13 on this box:
    //   0.5 ** 0.03459490740740741 == 0.9763058580317109
    //   math.exp2(-0.03459490740740741) == 0.976305858031711
    // The two differ by one unit in the last place. An optimized build rewrites a
    // LITERAL base into the `exp2` call, so `fire::pow_half` hides the base behind
    // `std::hint::black_box` and the fold keeps the CPython value in BOTH profiles.
    let state = Learned::new(1.0)
        .interval(1.0)
        .t0(T_US - 2_989_000_000)
        .state();
    assert_eq!(memory_at(&state, T_US), 0.976_305_858_031_710_9);
    assert_eq!(T21_EXPONENT, 0.034_594_907_407_407_41);
}
