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

use std::collections::BTreeMap;

use cadus_core::config::Config;
use cadus_core::event::{TaskType, TopicStatus, WorkQuality};
use cadus_core::learner::TopicState;
use cadus_core::numeric::local_day;
use cadus_core::xp::{
    BLOWOFF_ESCALATION, DEFAULT_XP_PER_TOPIC, RUSH_PENALTY_MULT, RUSH_TIME_FRACTION,
    VELOCITY_WINDOW_DAYS, VelocityInput, base_xp, compute_velocity_state, course_progress,
    current_streak, daily_totals, estimate_eta, is_rushing, quality_multiplier, task_xp,
    topics_per_week, window_start, xp_per_day,
};
use common::{MINI_FRACTIONS, MINI_NUMBERS, assert_approx, mini_curriculum, noon_us, on, utc};

/// Every work-quality tier, in declaration order.
const ALL_TIERS: [WorkQuality; 6] = [
    WorkQuality::Perfect,
    WorkQuality::NearlyPerfect,
    WorkQuality::Passable,
    WorkQuality::NearlyPassable,
    WorkQuality::Poor,
    WorkQuality::Blowoff,
];

/// The 12 topic ids of the mini curriculum, sorted, as the 1.0 test sorts them.
fn mini_ids_sorted() -> Vec<String> {
    let mut ids: Vec<String> = MINI_NUMBERS
        .iter()
        .chain(MINI_FRACTIONS.iter())
        .map(|id| (*id).to_owned())
        .collect();
    ids.sort();
    ids
}

/// The 1.0 states of the progress tests: the first `mastered` sorted topics are
/// `learning`, the rest are `untouched`.
fn mini_states(mastered: usize) -> BTreeMap<String, TopicState> {
    mini_ids_sorted()
        .into_iter()
        .enumerate()
        .map(|(index, id)| {
            let status = if index < mastered {
                TopicStatus::Learning
            } else {
                TopicStatus::Untouched
            };
            (
                id,
                TopicState {
                    status,
                    ..TopicState::default()
                },
            )
        })
        .collect()
}

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

// --------------------------------------------------------------------------- //
// Course progress (test_xp.py:185-200)
// --------------------------------------------------------------------------- //

#[test]
fn course_progress_counts_mastered_topics() {
    let graph = mini_curriculum();
    let states = mini_states(6);
    assert_approx(
        course_progress(&states, &graph, "testcourse"),
        6.0 / 12.0,
        "six of twelve mastered",
    );
}

#[test]
fn progress_ignores_review_only_credit() {
    let graph = mini_curriculum();
    let states: BTreeMap<String, TopicState> = mini_ids_sorted()
        .into_iter()
        .map(|id| {
            (
                id,
                TopicState {
                    status: TopicStatus::Untouched,
                    rep_num: 3.0,
                    ..TopicState::default()
                },
            )
        })
        .collect();
    assert_eq!(course_progress(&states, &graph, "testcourse"), 0.0);
}

// --------------------------------------------------------------------------- //
// ETA (test_xp.py:208-233)
// --------------------------------------------------------------------------- //

#[test]
fn estimate_eta_from_remaining_and_velocity() {
    let graph = mini_curriculum();
    let states = mini_states(6);
    // 6 done, total XP 120 -> 20 XP per topic; 6 remaining -> 120 XP; at 10 XP
    // per day -> 12 days.
    let eta = estimate_eta(&states, &graph, "testcourse", 120.0, 10.0, on(2026, 7, 14));
    assert_eq!(eta, Some(on(2026, 7, 26)));
}

#[test]
fn estimate_eta_none_without_velocity() {
    let graph = mini_curriculum();
    let states = mini_states(0);
    assert_eq!(
        estimate_eta(&states, &graph, "testcourse", 0.0, 0.0, on(2026, 7, 14)),
        None
    );
}

#[test]
fn estimate_eta_uses_default_before_any_completion() {
    let graph = mini_curriculum();
    let states = mini_states(0);
    // 12 topics * 12.0 XP per topic / 12.0 XP per day -> 12 days.
    let eta = estimate_eta(&states, &graph, "testcourse", 0.0, 12.0, on(2026, 7, 14));
    assert_eq!(eta, Some(on(2026, 7, 26)));
}

#[test]
fn estimate_eta_of_a_complete_course_is_today() {
    let graph = mini_curriculum();
    let states = mini_states(12);
    assert_eq!(
        estimate_eta(&states, &graph, "testcourse", 240.0, 10.0, on(2026, 7, 14)),
        Some(on(2026, 7, 14))
    );
}

// --------------------------------------------------------------------------- //
// compute_velocity_state (test_xp.py:236-250)
// --------------------------------------------------------------------------- //

#[test]
fn compute_velocity_state_integration() {
    let graph = mini_curriculum();
    let states = mini_states(6);
    let ids = mini_ids_sorted();
    let xp_entries = [(noon_us(2026, 7, 14), 280.0)];
    let completions: Vec<(i64, String)> = (0..6)
        .map(|index| (noon_us(2026, 7, 12), ids[index].clone()))
        .collect();

    let velocity = compute_velocity_state(&VelocityInput {
        states: &states,
        graph: &graph,
        course_id: Some("testcourse"),
        xp_entries: &xp_entries,
        completions: &completions,
        total_xp: 120.0,
        t_us: noon_us(2026, 7, 14),
        zone: utc(),
        window_days: 28,
    })
    .unwrap();

    assert_approx(velocity.xp_per_day_28d, 10.0, "280 XP over 28 days");
    assert_approx(velocity.course_progress, 0.5, "six of twelve");
    assert_approx(velocity.topics_per_week_28d, 1.5, "six topics over 4 weeks");
    assert!(velocity.eta.is_some());
}

#[test]
fn compute_velocity_state_without_a_course_has_no_progress_and_no_eta() {
    let graph = mini_curriculum();
    let states = mini_states(6);
    let xp_entries = [(noon_us(2026, 7, 14), 280.0)];

    let velocity = compute_velocity_state(&VelocityInput {
        states: &states,
        graph: &graph,
        course_id: None,
        xp_entries: &xp_entries,
        completions: &[],
        total_xp: 120.0,
        t_us: noon_us(2026, 7, 14),
        zone: utc(),
        window_days: 28,
    })
    .unwrap();

    assert_eq!(velocity.xp_per_day_28d, 10.0);
    assert_eq!(velocity.course_progress, 0.0);
    assert_eq!(velocity.topics_per_week_28d, 0.0);
    assert_eq!(velocity.eta, None);
}

// --------------------------------------------------------------------------- //
// Bit-exact 1.0 values (a run of the 1.0 engine, not a re-derivation)
// --------------------------------------------------------------------------- //

#[test]
fn xp_values_are_bit_exact_with_1_0() {
    let cfg = Config::default();
    assert_eq!(base_xp(TaskType::Lesson, 2), 7.0);
    assert_eq!(base_xp(TaskType::Lesson, 4), 14.0);
    assert_eq!(quality_multiplier(WorkQuality::Blowoff, &cfg, 1), -0.5);
    assert_eq!(quality_multiplier(WorkQuality::Blowoff, &cfg, 2), -0.75);
    assert_eq!(quality_multiplier(WorkQuality::Blowoff, &cfg, 3), -1.125);
    assert_eq!(
        task_xp(TaskType::Lesson, WorkQuality::Perfect, &cfg, 2, 0, false),
        9.1
    );
    assert_eq!(
        task_xp(TaskType::Review, WorkQuality::Passable, &cfg, 0, 0, false),
        4.25
    );
    assert_eq!(
        task_xp(TaskType::Review, WorkQuality::Blowoff, &cfg, 0, 2, false),
        -3.75
    );
    assert_eq!(
        task_xp(TaskType::Review, WorkQuality::Passable, &cfg, 0, 0, true),
        2.125
    );
    assert_eq!(
        task_xp(TaskType::Review, WorkQuality::Blowoff, &cfg, 0, 1, true),
        -2.5
    );
}

// --------------------------------------------------------------------------- //
// The XP boundaries, read AT the threshold
// --------------------------------------------------------------------------- //
//
// The 1.0 answers below came from the live 1.0 engine:
//
//   .venv/bin/python -c "from cadus.xp import current_streak, xp_per_day, \
//       _window_start; ..."
//
// M3 review round 1, findings #14 and #15. The whole-fold form of each boundary,
// with a committed 1.0 digest, is in `crates/core/tests/projector.rs` on
// `tests/fixtures/events/boundary/`.

#[test]
fn xp_per_day_counts_the_day_the_window_starts_on() {
    // `xp.py:192-194` puts `window_days` days INCLUDING the reference day in the
    // window, and `xp.py:207` tests `local_day(ts) >= start`. With a reference day
    // of 2026-07-14 and a 28-day window, 1.0 gives a start of 2026-06-17, so the
    // entry ON that day is inside the window and the entry one day earlier is not.
    // The live 1.0 `xp_per_day` on these two entries prints 1.0; a `> start` port
    // prints 0.0 (finding #14).
    assert_eq!(
        window_start(noon_us(2026, 7, 14), utc(), VELOCITY_WINDOW_DAYS).unwrap(),
        on(2026, 6, 17)
    );
    let entries = [(noon_us(2026, 6, 17), 28.0), (noon_us(2026, 6, 16), 999.0)];
    let rate = xp_per_day(&entries, noon_us(2026, 7, 14), utc(), VELOCITY_WINDOW_DAYS).unwrap();
    assert_eq!(rate, 1.0);
}

#[test]
fn a_reference_day_exactly_at_the_goal_starts_the_streak_today() {
    // `xp.py:178` is `if daily.get(today, 0.0) < goal`, so a reference day EQUAL to
    // the goal is not "in progress": the count starts at the reference day itself.
    // The live 1.0 `current_streak` gives 2 here, and 1 when the reference day is
    // one XP below the goal (finding #15).
    let entries = [(noon_us(2026, 7, 13), 40.0), (noon_us(2026, 7, 14), 40.0)];
    let mut daily = daily_totals(&entries, utc()).unwrap();
    assert_eq!(daily[&on(2026, 7, 14)], 40.0);
    assert_eq!(current_streak(&daily, 40.0, on(2026, 7, 14)), 2);

    daily.insert(on(2026, 7, 14), 39.0);
    assert_eq!(current_streak(&daily, 40.0, on(2026, 7, 14)), 1);
}
