//! The pinned XP behaviors of spec section 8, part 2: course progress, the
//! ETA, the velocity state, and the bit-exact values (`tests/test_xp.py`).
//!
//! Every expected number here is a LITERAL from the 1.0 test file, from the spec,
//! or from a run of the 1.0 engine, never a value re-derived from the code under
//! test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::float_cmp
)]
mod common;

use cadus_core::event::{TaskType, TopicStatus, WorkQuality};
use cadus_core::learner::TopicState;
use cadus_core::xp::{
    VELOCITY_WINDOW_DAYS, VelocityInput, base_xp, compute_velocity_state, course_progress,
    current_streak, daily_totals, estimate_eta, quality_multiplier, task_xp, topics_per_week,
    window_start, xp_per_day,
};
use common::selector::cfg;
use common::xp_states::{mini_ids_sorted, mini_states};
use common::{assert_approx, mini_curriculum, noon_us, on, utc};

// --------------------------------------------------------------------------- //
// Course progress (test_xp.py:185-200)
// --------------------------------------------------------------------------- //

#[test]
fn course_progress_counts_practiced_topics() {
    let graph = mini_curriculum();
    let states = mini_states(6);
    assert_approx(
        course_progress(&states, &graph, "testcourse", &cfg()),
        6.0 / 12.0,
        "six of twelve practiced",
    );
}

#[test]
fn progress_ignores_review_only_credit() {
    let graph = mini_curriculum();
    let states: std::collections::BTreeMap<String, TopicState> = mini_ids_sorted()
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
    assert_eq!(course_progress(&states, &graph, "testcourse", &cfg()), 0.0);
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
    let eta = estimate_eta(
        &states,
        &graph,
        "testcourse",
        120.0,
        10.0,
        on(2026, 7, 14),
        &cfg(),
    );
    assert_eq!(eta, Some(on(2026, 7, 26)));
}

#[test]
fn estimate_eta_none_without_velocity() {
    let graph = mini_curriculum();
    let states = mini_states(0);
    assert_eq!(
        estimate_eta(
            &states,
            &graph,
            "testcourse",
            0.0,
            0.0,
            on(2026, 7, 14),
            &cfg(),
        ),
        None
    );
}

#[test]
fn estimate_eta_uses_default_before_any_completion() {
    let graph = mini_curriculum();
    let states = mini_states(0);
    // 12 topics * 12.0 XP per topic / 12.0 XP per day -> 12 days.
    let eta = estimate_eta(
        &states,
        &graph,
        "testcourse",
        0.0,
        12.0,
        on(2026, 7, 14),
        &cfg(),
    );
    assert_eq!(eta, Some(on(2026, 7, 26)));
}

#[test]
fn estimate_eta_of_a_complete_course_is_today() {
    let graph = mini_curriculum();
    let states = mini_states(12);
    assert_eq!(
        estimate_eta(
            &states,
            &graph,
            "testcourse",
            240.0,
            10.0,
            on(2026, 7, 14),
            &cfg(),
        ),
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
        cfg: &cfg(),
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
        cfg: &cfg(),
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
    let cfg = cadus_core::config::Config::default();
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
// --------------------------------------------------------------------------- //
// The edges 1.0 raises on
// --------------------------------------------------------------------------- //

/// A streak walk that reaches the first representable date stops there, where
/// 1.0 raises `OverflowError`.
#[test]
fn a_streak_walk_stops_at_the_first_representable_date() {
    let first = chrono::NaiveDate::MIN;
    let mut daily = indexmap::IndexMap::new();
    assert_eq!(
        current_streak(&daily, 10.0, first),
        0,
        "below the goal, no yesterday"
    );
    daily.insert(first, 10.0);
    assert_eq!(
        current_streak(&daily, 10.0, first),
        1,
        "at the goal, no yesterday"
    );
}

/// A course with no topic has progress 0.0, and a quotient with no finite
/// day count has no ETA.
#[test]
fn an_empty_course_and_an_infinite_quotient_give_the_undefined_answers() {
    let graph = mini_curriculum();
    let states = mini_states(0);
    assert_eq!(course_progress(&states, &graph, "nocourse", &cfg()), 0.0);
    assert_eq!(
        estimate_eta(
            &states,
            &graph,
            "testcourse",
            0.0,
            f64::MIN_POSITIVE,
            on(2026, 7, 28),
            &cfg(),
        ),
        None
    );
}

/// An instant outside the range `chrono` represents is an error value on every
/// path, never a panic.
#[test]
fn an_unrepresentable_instant_is_an_error_on_every_path() {
    let far = i64::MAX;
    let bad = [(far, 1.0)];
    let good = [(noon_us(2026, 7, 14), 1.0)];
    let bad_topics = [(far, "addition".to_owned())];
    assert!(daily_totals(&bad, utc()).is_err());
    assert!(window_start(far, utc(), 28).is_err());
    assert!(xp_per_day(&bad, noon_us(2026, 7, 14), utc(), 28).is_err());
    assert!(xp_per_day(&good, far, utc(), 28).is_err());
    assert!(topics_per_week(&bad_topics, noon_us(2026, 7, 14), utc(), 28).is_err());
    assert!(topics_per_week(&[], far, utc(), 28).is_err());

    let graph = mini_curriculum();
    let states = mini_states(0);
    let input = VelocityInput {
        states: &states,
        graph: &graph,
        course_id: Some("testcourse"),
        xp_entries: &good,
        completions: &[],
        total_xp: 1.0,
        t_us: noon_us(2026, 7, 14),
        zone: utc(),
        window_days: 28,
        cfg: &cfg(),
    };
    assert!(compute_velocity_state(&VelocityInput { t_us: far, ..input }).is_err());
    assert!(
        compute_velocity_state(&VelocityInput {
            xp_entries: &bad,
            ..input
        })
        .is_err()
    );
    assert!(
        compute_velocity_state(&VelocityInput {
            completions: &bad_topics,
            ..input
        })
        .is_err()
    );
    assert!(compute_velocity_state(&input).is_ok());
}
