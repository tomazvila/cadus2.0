//! Part of `tests/session_state.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

use cadus_core::event::{
    EnrollReason, Event, SchemaVersion, Slug, TaskType, Timestamp, WorkQuality,
};
use cadus_store::state::{append_event, lock_web_state};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, begin_tenant};
use cadus_web::session::{
    QUIZ_HIGH_SCORE, active_study_days, closed_task_ids, current_session, enrollment_stack,
    last_drill_at, learned_at, new_session_id, quiz_high_score_streak, session_xp,
};
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};

// --------------------------------------------------------------------------- //
// The pure event scans
// --------------------------------------------------------------------------- //

/// `current_session` reports the `session_start` with no later `session_end`.
#[test]
fn current_session_reads_the_open_session() {
    assert_eq!(current_session(&[]), None);

    let closed = vec![
        row(1, start("s_2026-01-01a", 0)),
        row(2, end("s_2026-01-01a", 60_000_000)),
    ];
    assert_eq!(current_session(&closed), None);

    let mut open = closed;
    open.push(row(3, start("s_2026-01-01b", 120_000_000)));
    assert_eq!(current_session(&open).as_deref(), Some("s_2026-01-01b"));
}

/// `new_session_id` takes the next unused letter of the day.
#[test]
fn new_session_id_takes_the_next_unused_letter() {
    let today = DateTime::<Utc>::from_timestamp_micros(BASE_US).unwrap();
    assert_eq!(new_session_id(&[], today), "s_2026-01-01a");

    let one = vec![row(1, start("s_2026-01-01a", 0))];
    assert_eq!(new_session_id(&one, today), "s_2026-01-01b");

    let two = vec![
        row(1, start("s_2026-01-01a", 0)),
        row(2, start("s_2026-01-01b", 1)),
    ];
    assert_eq!(new_session_id(&two, today), "s_2026-01-01c");

    // Another day starts at `a` again.
    let tomorrow = DateTime::<Utc>::from_timestamp_micros(BASE_US + DAY_US).unwrap();
    assert_eq!(new_session_id(&two, tomorrow), "s_2026-01-02a");
}

/// `session_xp` sums the results INSIDE one session and rounds to two places.
#[test]
fn session_xp_sums_the_results_inside_one_session() {
    let events = vec![
        row(1, start("s_2026-01-01a", 0)),
        row(2, lesson("s_2026-01-01a", "addition", 12.5, true, 10)),
        row(3, lesson("s_2026-01-01a", "fractions", 0.25, false, 20)),
        row(4, end("s_2026-01-01a", 30)),
        row(5, start("s_2026-01-01b", 40)),
        row(6, lesson("s_2026-01-01b", "addition", 7.0, true, 50)),
    ];

    assert_eq!(session_xp(&events, "s_2026-01-01a"), 12.75);
    assert_eq!(session_xp(&events, "s_2026-01-01b"), 7.0);
    assert_eq!(session_xp(&events, "s_2026-01-01z"), 0.0);
}

/// The enrollment stack: a manual enroll resets, `gap-fill` pushes, and
/// `gap-return` pops one level and never empties the stack.
#[test]
fn the_enrollment_stack_folds_the_enrolled_events() {
    assert_eq!(enrollment_stack(&[]), Vec::<String>::new());

    let manual = vec![row(1, enrolled("proofs", None))];
    assert_eq!(enrollment_stack(&manual), vec!["proofs".to_string()]);

    let pushed = vec![
        row(1, enrolled("proofs", None)),
        row(2, enrolled("foundations", Some(EnrollReason::GapFill))),
    ];
    assert_eq!(
        enrollment_stack(&pushed),
        vec!["proofs".to_string(), "foundations".to_string()]
    );

    let popped = vec![
        row(1, enrolled("proofs", None)),
        row(2, enrolled("foundations", Some(EnrollReason::GapFill))),
        row(3, enrolled("proofs", Some(EnrollReason::GapReturn))),
    ];
    assert_eq!(enrollment_stack(&popped), vec!["proofs".to_string()]);

    // A `gap-return` at the base level pops nothing.
    let over_popped = vec![
        row(1, enrolled("proofs", None)),
        row(2, enrolled("proofs", Some(EnrollReason::GapReturn))),
    ];
    assert_eq!(enrollment_stack(&over_popped), vec!["proofs".to_string()]);

    // A manual enroll after a gap-fill resets the whole stack.
    let reset = vec![
        row(1, enrolled("proofs", None)),
        row(2, enrolled("foundations", Some(EnrollReason::GapFill))),
        row(3, enrolled("category-theory", None)),
    ];
    assert_eq!(
        enrollment_stack(&reset),
        vec!["category-theory".to_string()]
    );
}

/// `learned_at` keeps the FIRST pass of each topic, and a failed lesson is not a
/// pass.
#[test]
fn learned_at_keeps_the_first_pass() {
    let events = vec![
        row(1, lesson("s_2026-01-01a", "fractions", 1.0, false, 0)),
        row(2, lesson("s_2026-01-01a", "addition", 1.0, true, 10)),
        row(3, lesson("s_2026-01-01a", "addition", 1.0, true, 20)),
    ];
    let map = learned_at(&events);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("addition"), Some(&(BASE_US + 10)));
    assert_eq!(map.get("fractions"), None);
}

/// `last_drill_at` keeps the LAST served drill of each topic and ignores every
/// other task kind.
#[test]
fn last_drill_at_keeps_the_last_served_drill() {
    let events = vec![
        row(1, served("t-r", TaskType::Review, Some("addition"), 0)),
        row(2, served("t-d1", TaskType::Drill, Some("addition"), 10)),
        row(3, served("t-d2", TaskType::Drill, Some("addition"), 20)),
        row(4, served("t-q", TaskType::Quiz, None, 30)),
    ];
    let map = last_drill_at(&events);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get("addition"), Some(&(BASE_US + 20)));
}

/// `active_study_days` counts distinct dates that carry an ATTEMPT, so a day
/// with no attempt never advances the F12 quiz cadence.
#[test]
fn active_study_days_counts_distinct_attempt_dates() {
    // No attempt at all, only lessons: the F12 cadence stands still.
    let none = vec![row(1, lesson("s_2026-01-01a", "addition", 1.0, true, 0))];
    assert_eq!(active_study_days(&none).len(), 0);

    // Two attempts on 2026-01-01 and one on 2026-01-03. The 2nd of January
    // carries a lesson only, so it is not an active study day.
    let events = vec![
        row(1, graded("a1", 0)),
        row(2, graded("a2", 3_600_000_000)),
        row(3, lesson("s_2026-01-01a", "addition", 1.0, true, DAY_US)),
        row(4, graded("a3", 2 * DAY_US)),
    ];
    let days = active_study_days(&events);
    assert_eq!(
        days,
        vec![
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 3).unwrap(),
        ]
    );
}

/// `quiz_high_score_streak` counts the TRAILING run at or above 0.9.
#[test]
fn quiz_high_score_streak_counts_the_trailing_run() {
    assert_eq!(QUIZ_HIGH_SCORE, 0.9);
    assert_eq!(quiz_high_score_streak(&[]), 0);

    let two = vec![
        row(1, quiz(0.5, 0)),
        row(2, quiz(0.9, 10)),
        row(3, quiz(1.0, 20)),
    ];
    assert_eq!(quiz_high_score_streak(&two), 2);

    let broken = vec![row(1, quiz(1.0, 0)), row(2, quiz(0.89, 10))];
    assert_eq!(quiz_high_score_streak(&broken), 0);
}

/// `closed_task_ids` reads the task id off a `review_result`, which is how a
/// multi-step task closes (F8).
#[test]
fn closed_task_ids_reads_the_review_results() {
    let events = vec![
        row(1, start("s_2026-01-01a", 0)),
        row(
            2,
            Event::ReviewResult(cadus_core::event::ReviewResult {
                ts: Timestamp::from_micros(BASE_US),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
                topic: Slug::new("addition").unwrap(),
                passed: true,
                weighted_score: 1.0,
                xp: 6.0,
                quality_tier: WorkQuality::NearlyPerfect,
                assisted: false,
                task_id: Some("s_2026-01-01a-multi-step".to_string()),
            }),
        ),
    ];
    let closed = closed_task_ids(&events);
    assert_eq!(closed.len(), 1);
    assert!(closed.contains("s_2026-01-01a-multi-step"));
}

/// The scans read the LOG, so a dense `seq` written by the store feeds them
/// unchanged (C2, D4).
#[tokio::test]
async fn the_scans_read_the_log_the_store_wrote() {
    TestDb::with(|db| async move {
        let user = db.seed_user("scan@example.com").await;
        let handle = Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut tx, user).await.unwrap();
        append_event(&mut tx, user, &enrolled("c", None), None)
            .await
            .unwrap();
        append_event(&mut tx, user, &start("s_2026-01-01a", 0), None)
            .await
            .unwrap();
        append_event(
            &mut tx,
            user,
            &lesson("s_2026-01-01a", "addition", 9.5, true, 10),
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        let events = cadus_store::state::load_events(&mut tx, user)
            .await
            .unwrap();
        tx.rollback().await.unwrap();

        assert_eq!(events.len(), 3);
        assert_eq!(current_session(&events).as_deref(), Some("s_2026-01-01a"));
        assert_eq!(enrollment_stack(&events), vec!["c".to_string()]);
        assert_eq!(session_xp(&events, "s_2026-01-01a"), 9.5);
        assert_eq!(learned_at(&events).get("addition"), Some(&(BASE_US + 10)));
    })
    .await;
}
