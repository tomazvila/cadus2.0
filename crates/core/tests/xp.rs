//! The pinned XP behaviors of spec section 8 (`tests/test_xp.py`).
//!
//! Every expected number here is a LITERAL from the 1.0 test file, from the spec,
//! or from a run of the 1.0 engine, never a value re-derived from the code under
//! test. Where 1.0 asserts an exact value, the assertion is exact; where 1.0 uses
//! `pytest.approx`, the assertion is `|a - b| <= 1e-6 * max(1, |b|)`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::float_cmp
)]
mod common;

use cadus_core::config::Config;
use cadus_core::event::{TaskType, WorkQuality};
use cadus_core::numeric::local_day;
use cadus_core::xp::{
    BLOWOFF_ESCALATION, DEFAULT_XP_PER_TOPIC, RUSH_PENALTY_MULT, RUSH_TIME_FRACTION,
    VELOCITY_WINDOW_DAYS, base_xp, current_streak, daily_totals, is_rushing, quality_multiplier,
    task_xp, topics_per_week, xp_per_day,
};
use common::{assert_approx, noon_us, on, utc};

/// Every work-quality tier, in declaration order.
const ALL_TIERS: [WorkQuality; 6] = [
    WorkQuality::Perfect,
    WorkQuality::NearlyPerfect,
    WorkQuality::Passable,
    WorkQuality::NearlyPassable,
    WorkQuality::Poor,
    WorkQuality::Blowoff,
];

// --------------------------------------------------------------------------- //
// Base task XP (test_xp.py:42-49)
// --------------------------------------------------------------------------- //

#[test]
fn base_xp_values() {
    assert_approx(base_xp(TaskType::Lesson, 2), 7.0, "lesson with 2 KPs");
    assert_approx(base_xp(TaskType::Lesson, 4), 14.0, "lesson with 4 KPs");
    assert_eq!(base_xp(TaskType::Review, 0), 5.0);
    assert_eq!(base_xp(TaskType::Quiz, 0), 15.0);
    assert_eq!(base_xp(TaskType::Drill, 0), 5.0);
    assert_eq!(base_xp(TaskType::Diagnostic, 0), 0.0);
    assert_eq!(base_xp(TaskType::MultiStep, 0), 15.0);
}

#[test]
fn module_constants_hold_the_1_0_values() {
    assert_eq!(BLOWOFF_ESCALATION, 1.5);
    assert_eq!(RUSH_TIME_FRACTION, 0.5);
    assert_eq!(RUSH_PENALTY_MULT, 0.5);
    assert_eq!(VELOCITY_WINDOW_DAYS, 28);
    assert_eq!(DEFAULT_XP_PER_TOPIC, 12.0);
}

// --------------------------------------------------------------------------- //
// Quality multipliers and the blow-off escalation (test_xp.py:57-74)
// --------------------------------------------------------------------------- //

#[test]
fn quality_multipliers_match_config() {
    let cfg = Config::default();
    assert_eq!(quality_multiplier(WorkQuality::Perfect, &cfg, 0), 1.3);
    assert_eq!(quality_multiplier(WorkQuality::NearlyPerfect, &cfg, 0), 1.0);
    assert_eq!(quality_multiplier(WorkQuality::Passable, &cfg, 0), 0.85);
    assert_eq!(
        quality_multiplier(WorkQuality::NearlyPassable, &cfg, 0),
        0.3
    );
    assert_eq!(quality_multiplier(WorkQuality::Poor, &cfg, 0), 0.0);
}

#[test]
fn blowoff_penalty_escalates() {
    let cfg = Config::default();
    let blow = |run: i64| quality_multiplier(WorkQuality::Blowoff, &cfg, run);
    assert_approx(blow(1), -0.5, "first blow-off");
    assert_approx(blow(2), -0.75, "second blow-off");
    assert_approx(blow(3), -1.125, "third blow-off");
    assert_approx(blow(0), -0.5, "an unspecified run is un-escalated");
}

// --------------------------------------------------------------------------- //
// task_xp (test_xp.py:77-109)
// --------------------------------------------------------------------------- //

#[test]
fn task_xp_tiers_and_escalation() {
    let cfg = Config::default();
    assert_approx(
        task_xp(TaskType::Lesson, WorkQuality::Perfect, &cfg, 2, 0, false),
        9.1,
        "perfect 2-KP lesson",
    );
    assert_approx(
        task_xp(TaskType::Review, WorkQuality::Passable, &cfg, 0, 0, false),
        4.25,
        "passable review",
    );
    assert_approx(
        task_xp(TaskType::Review, WorkQuality::Poor, &cfg, 0, 0, false),
        0.0,
        "poor review",
    );
    assert_approx(
        task_xp(TaskType::Review, WorkQuality::Blowoff, &cfg, 0, 2, false),
        -3.75,
        "second consecutive blow-off review",
    );
}

#[test]
fn diagnostics_award_no_xp() {
    let cfg = Config::default();
    for quality in ALL_TIERS {
        assert_eq!(
            task_xp(TaskType::Diagnostic, quality, &cfg, 0, 0, false),
            0.0
        );
    }
}

// --------------------------------------------------------------------------- //
// The rushing hook (test_xp.py:96-109)
// --------------------------------------------------------------------------- //

#[test]
fn is_rushing_detects_fast_and_wrong() {
    assert!(is_rushing(false, 10, 30));
    assert!(!is_rushing(true, 10, 30));
    assert!(!is_rushing(false, 20, 30));
    assert!(!is_rushing(false, 0, 30));
}

#[test]
fn task_xp_rushing_penalty_applies_to_positive_xp() {
    let cfg = Config::default();
    let full = task_xp(TaskType::Review, WorkQuality::Passable, &cfg, 0, 0, false);
    let rushed = task_xp(TaskType::Review, WorkQuality::Passable, &cfg, 0, 0, true);
    assert_approx(rushed, full * 0.5, "a rushed pass is halved");
    assert_approx(rushed, 2.125, "the 1.0 literal");
    let blown = task_xp(TaskType::Review, WorkQuality::Blowoff, &cfg, 0, 1, true);
    assert_approx(blown, -2.5, "rushing never rewards a negative");
}

// --------------------------------------------------------------------------- //
// Local day boundaries (test_xp.py:117-122)
// --------------------------------------------------------------------------- //

#[test]
fn local_day_uses_timezone() {
    // 2026-07-14T01:00Z. A naive 1.0 timestamp is the SAME instant, read as UTC.
    let ts_us = 1_783_990_800_000_000;
    assert_eq!(
        local_day(ts_us, Some("America/New_York")).unwrap(),
        on(2026, 7, 13)
    );
    assert_eq!(local_day(ts_us, Some("UTC")).unwrap(), on(2026, 7, 14));
    assert_eq!(local_day(ts_us, None).unwrap(), on(2026, 7, 14));
}

// --------------------------------------------------------------------------- //
// Streak (test_xp.py:129-150)
// --------------------------------------------------------------------------- //

#[test]
fn streak_counts_consecutive_goal_days() {
    let entries = [
        (noon_us(2026, 7, 10), 50.0),
        (noon_us(2026, 7, 11), 45.0),
        (noon_us(2026, 7, 12), 60.0),
        (noon_us(2026, 7, 13), 40.0),
        (noon_us(2026, 7, 14), 30.0),
    ];
    let mut daily = daily_totals(&entries, utc()).unwrap();
    assert_eq!(current_streak(&daily, 40.0, on(2026, 7, 14)), 4);
    daily.insert(on(2026, 7, 14), 45.0);
    assert_eq!(current_streak(&daily, 40.0, on(2026, 7, 14)), 5);
}

#[test]
fn streak_breaks_on_missed_day() {
    let entries = [
        (noon_us(2026, 7, 13), 40.0),
        (noon_us(2026, 7, 12), 10.0),
        (noon_us(2026, 7, 11), 90.0),
    ];
    let daily = daily_totals(&entries, utc()).unwrap();
    assert_eq!(current_streak(&daily, 40.0, on(2026, 7, 14)), 1);
}

#[test]
fn daily_totals_accumulates_naively_per_local_day() {
    let entries = [
        (noon_us(2026, 7, 13), 1.5),
        (noon_us(2026, 7, 13), 2.5),
        (noon_us(2026, 7, 14), 7.0),
    ];
    let daily = daily_totals(&entries, utc()).unwrap();
    assert_eq!(daily[&on(2026, 7, 13)], 4.0);
    assert_eq!(daily[&on(2026, 7, 14)], 7.0);
    // The map keeps the 1.0 dict insertion order.
    let order: Vec<_> = daily.keys().copied().collect();
    assert_eq!(order, vec![on(2026, 7, 13), on(2026, 7, 14)]);
}

// --------------------------------------------------------------------------- //
// Velocity (test_xp.py:158-177)
// --------------------------------------------------------------------------- //

#[test]
fn xp_per_day_over_window() {
    let entries = [
        (noon_us(2026, 7, 14), 100.0),
        (noon_us(2026, 7, 1), 180.0),
        (noon_us(2026, 5, 1), 999.0),
    ];
    let rate = xp_per_day(&entries, noon_us(2026, 7, 14), utc(), 28).unwrap();
    assert_approx(rate, (100.0 + 180.0) / 28.0, "trailing 28-day XP rate");
    assert_eq!(rate, 10.0);
}

#[test]
fn topics_per_week_over_window() {
    let completions = [
        (noon_us(2026, 7, 14), "a".to_owned()),
        (noon_us(2026, 7, 10), "b".to_owned()),
        (noon_us(2026, 7, 10), "b".to_owned()),
        (noon_us(2026, 4, 1), "old".to_owned()),
    ];
    let rate = topics_per_week(&completions, noon_us(2026, 7, 14), utc(), 28).unwrap();
    assert_approx(rate, 2.0 / 4.0, "two distinct topics over four weeks");
    assert_eq!(rate, 0.5);
}
