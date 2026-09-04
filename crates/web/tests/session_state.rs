//! M5 U6 acceptance, the D-S6 document and the pure event scans.
//!
//! Requirements: D-S6, D5, C3. Spec `docs/reference/web-service-1.0-spec.md`
//! sections 4.1 and 4.2, and section 11 row U6.
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal session id, a literal count. Nothing is read back from the
//! code under test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use axum::http::StatusCode;
use cadus_core::event::{
    AnswerKind, Attempt, AttemptProblem, EnrollReason, Enrolled, Event, LessonResult, QuizResult,
    SchemaVersion, Secs, SessionEnd, SessionStart, Slug, TaskServed, TaskType, Timestamp,
    WorkQuality,
};
use cadus_core::pool::{PoolAnswer, Ring, TaskMemory};
use cadus_store::state::{EventRow, append_event, load_web_state, lock_web_state, save_web_state};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, begin_tenant};
use cadus_web::error::ApiError;
use cadus_web::session::{
    QUIZ_HIGH_SCORE, active_study_days, closed_task_ids, current_session, enrollment_stack,
    last_drill_at, learned_at, new_session_id, quiz_high_score_streak, session_xp,
};
use cadus_web::state::{
    ServedProblem, TASK_COMPLETE, TaskProgress, UNKNOWN_PROBLEM, ValidateError, WebState,
};
use serde_json::json;
use sqlx::types::chrono::{DateTime, NaiveDate, Utc};

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
const BASE_US: i64 = 1_767_225_600_000_000;

/// One microsecond day.
const DAY_US: i64 = 86_400_000_000;

/// Wrap an [`EventRow`] around an event at line `seq`.
fn row(seq: i64, event: Event) -> EventRow {
    EventRow { seq, event }
}

/// A `session_start` at `BASE_US + offset`.
fn start(session: &str, offset: i64) -> Event {
    Event::SessionStart(SessionStart {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some(session.to_string()),
        v: SchemaVersion,
    })
}

/// A `session_end` at `BASE_US + offset`.
fn end(session: &str, offset: i64) -> Event {
    Event::SessionEnd(SessionEnd {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some(session.to_string()),
        v: SchemaVersion,
        xp_earned: 0.0,
        minutes: 0.0,
    })
}

/// A passed or failed `lesson_result` worth `xp`.
fn lesson(session: &str, topic: &str, xp: f64, passed: bool, offset: i64) -> Event {
    Event::LessonResult(LessonResult {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some(session.to_string()),
        v: SchemaVersion,
        topic: Slug::new(topic).unwrap(),
        passed,
        failed_at_kp: None,
        xp,
        quality_tier: WorkQuality::NearlyPerfect,
        assisted: false,
    })
}

/// An `enrolled` event with an optional switch reason.
fn enrolled(course: &str, reason: Option<EnrollReason>) -> Event {
    Event::Enrolled(Enrolled {
        ts: Timestamp::from_micros(BASE_US),
        session: None,
        v: SchemaVersion,
        course: Slug::new(course).unwrap(),
        reason,
        return_to: None,
    })
}

/// A served task of a given kind.
fn served(task_id: &str, task_type: TaskType, topic: Option<&str>, offset: i64) -> Event {
    Event::TaskServed(TaskServed {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion,
        task_id: task_id.to_string(),
        task_type,
        topic: topic.map(|id| Slug::new(id).unwrap()),
        kp: None,
        problems: Vec::new(),
        component_topics: Vec::new(),
        seed: None,
    })
}

/// One graded attempt on `addition` at `BASE_US + offset`.
fn graded(attempt_id: &str, offset: i64) -> Event {
    Event::Attempt(Attempt {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion,
        attempt_id: attempt_id.to_string(),
        task_id: "s_2026-01-01a-review-addition".to_string(),
        topic: Slug::new("addition").unwrap(),
        kp: None,
        task_type: TaskType::Review,
        problem: AttemptProblem {
            text: "Compute $8 - 5$.".to_string(),
            expected: "3".to_string(),
        },
        given_answer: "3".to_string(),
        work: None,
        answer_kind: Some(AnswerKind::Numeric),
        correct: true,
        secs: Secs::new(12).unwrap(),
        error_tags: Vec::new(),
        work_quality: WorkQuality::NearlyPerfect,
        grader_note: Some("deterministic".to_string()),
        assisted: false,
    })
}

/// A quiz result with a score.
fn quiz(score: f64, offset: i64) -> Event {
    Event::QuizResult(QuizResult {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion,
        quiz_id: format!("q{offset}"),
        score,
        per_topic: Vec::new(),
        xp: 0.0,
    })
}

// --------------------------------------------------------------------------- //
// The document
// --------------------------------------------------------------------------- //

/// The two D5 windows carry the M4 capacities: 20 per topic, 12 per task.
#[test]
fn the_state_document_carries_the_m4_ring_and_the_task_memory() {
    assert_eq!(Ring::capacity(), 20);
    assert_eq!(TaskMemory::capacity(), 12);

    let mut state = WebState::for_session("s_2026-01-01a");
    for index in 0..25 {
        state.record_served(
            "addition",
            "s_2026-01-01a-review-addition",
            &format!("h{index}"),
        );
    }
    assert_eq!(state.ring("addition").len(), 20);
    assert_eq!(state.memory("s_2026-01-01a-review-addition").len(), 12);
    // The oldest entries fell out of each window and the newest one stands.
    assert!(!state.ring("addition").contains("h4"));
    assert!(state.ring("addition").contains("h5"));
    assert!(
        !state
            .memory("s_2026-01-01a-review-addition")
            .contains("h12")
    );
    assert!(
        state
            .memory("s_2026-01-01a-review-addition")
            .contains("h13")
    );
    assert!(state.ring("addition").contains("h24"));

    // An unknown topic and an unknown task both give empty windows.
    assert_eq!(state.ring("fractions").len(), 0);
    assert_eq!(state.memory("no-such-task").len(), 0);
}

/// The document round-trips through the `web_states.doc` column, and a session
/// drift resets it (`state.py:142-166`).
#[test]
fn a_session_drift_resets_the_document() {
    let mut state = WebState::for_session("s_2026-01-01a");
    state.active_secs = 42.0;
    state.record_served("addition", "s_2026-01-01a-review-addition", "h1");

    let doc = state.to_doc();
    let read = WebState::from_doc(&doc).unwrap();
    assert_eq!(read, state);

    // The same session id changes nothing.
    let mut same = read.clone();
    assert!(!same.bind("s_2026-01-01a"));
    assert_eq!(same.active_secs, 42.0);
    assert_eq!(same.ring("addition").len(), 1);

    // A different session id resets every field.
    let mut drifted = read;
    assert!(drifted.bind("s_2026-01-02a"));
    assert_eq!(drifted.session.as_deref(), Some("s_2026-01-02a"));
    assert_eq!(drifted.active_secs, 0.0);
    assert_eq!(drifted.ring("addition").len(), 0);
    assert_eq!(drifted.served.len(), 0);
    assert_eq!(drifted.tasks.len(), 0);
}

/// `plan_progress` is a pure LOOKUP. Trap W3: it must never install a row for a
/// task that the plan merely lists.
#[test]
fn plan_progress_never_installs_a_row() {
    let mut state = WebState::for_session("s_2026-01-01a");
    state.tasks.insert(
        "s_2026-01-01a-review-addition".to_string(),
        TaskProgress {
            task_id: "s_2026-01-01a-review-addition".to_string(),
            task_type: "review".to_string(),
            total: 3,
            served: 3,
            answered: 2,
            done: true,
            current_kp: None,
        },
    );

    assert_eq!(
        state.plan_progress("s_2026-01-01a-review-addition"),
        (2, true)
    );
    assert_eq!(
        state.plan_progress("s_2026-01-01a-lesson-fractions"),
        (0, false)
    );
    // The lookup of an absent task left the map at its one entry.
    assert_eq!(state.tasks.len(), 1);
}

/// The validate rule of section 4.2, in its two refusals and their literals.
#[test]
fn validate_gives_409_task_complete_and_404_unknown_problem() {
    let mut state = WebState::for_session("s_2026-01-01a");
    state.served.insert(
        "s_2026-01-01a-review-addition".to_string(),
        ServedProblem {
            problem_id: "p1".to_string(),
            task_id: "s_2026-01-01a-review-addition".to_string(),
            topic: Some("addition".to_string()),
            serve_topic: Some("addition".to_string()),
            kp: None,
            answer_kind: Some("numeric".to_string()),
            text: "Compute $8 - 5$.".to_string(),
            expected: PoolAnswer {
                v: 1,
                answer: "3".to_string(),
            },
            solution_sketch: None,
            started_at: 1_767_225_600.0,
            hints_given: Vec::new(),
            index: 0,
            rework: None,
        },
    );

    // The live problem passes.
    let live = state
        .validate("s_2026-01-01a-review-addition", "p1")
        .unwrap();
    assert_eq!(live.problem_id, "p1");

    // A superseded problem id is unknown.
    assert_eq!(
        state.validate("s_2026-01-01a-review-addition", "p0"),
        Err(ValidateError::UnknownProblem)
    );
    // A task that serves nothing is unknown too.
    assert_eq!(
        state.validate("s_2026-01-01a-lesson-fractions", "p1"),
        Err(ValidateError::UnknownProblem)
    );

    // A closed task refuses BEFORE the problem id is compared.
    state.tasks.insert(
        "s_2026-01-01a-review-addition".to_string(),
        TaskProgress {
            done: true,
            ..TaskProgress::default()
        },
    );
    assert_eq!(
        state.validate("s_2026-01-01a-review-addition", "p1"),
        Err(ValidateError::TaskComplete)
    );

    let complete = ApiError::from(ValidateError::TaskComplete);
    assert_eq!(complete.status, StatusCode::CONFLICT);
    assert_eq!(complete.code, "task_complete");
    assert_eq!(TASK_COMPLETE, "task_complete");

    let unknown = ApiError::from(ValidateError::UnknownProblem);
    assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    assert_eq!(unknown.code, "unknown_problem");
    assert_eq!(UNKNOWN_PROBLEM, "unknown_problem");
}

// --------------------------------------------------------------------------- //
// The two-tab race (section 11, row U6, acceptance 1)
// --------------------------------------------------------------------------- //

/// A second concurrent tab that answers a closed task gets `409 task_complete`.
///
/// The shape is section 4.2: both tabs snapshot, the first one commits the close
/// under the advisory lock, and the second one RE-VALIDATES against the row it
/// re-reads, not against the snapshot it opened with.
#[tokio::test]
async fn a_second_tab_answering_a_closed_task_gets_409_task_complete() {
    TestDb::with(|db| async move {
        let user = db.seed_user("tabs@example.com").await;
        let handle = Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);
        let task = "s_2026-01-01a-review-addition";

        let mut open = WebState::for_session("s_2026-01-01a");
        open.served.insert(
            task.to_string(),
            ServedProblem {
                problem_id: "p1".to_string(),
                task_id: task.to_string(),
                topic: Some("addition".to_string()),
                serve_topic: Some("addition".to_string()),
                kp: None,
                answer_kind: Some("numeric".to_string()),
                text: "Compute $8 - 5$.".to_string(),
                expected: PoolAnswer {
                    v: 1,
                    answer: "3".to_string(),
                },
                solution_sketch: None,
                started_at: 1_767_225_600.0,
                hints_given: Vec::new(),
                index: 0,
                rework: None,
            },
        );
        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        save_web_state(&mut tx, user, &open.to_doc()).await.unwrap();
        tx.commit().await.unwrap();

        // Tab B snapshots the OPEN task. Its snapshot says the answer is legal.
        let mut b = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut b, user).await.unwrap();
        let b_snapshot =
            WebState::from_doc(&load_web_state(&mut b, user).await.unwrap().unwrap()).unwrap();
        assert!(b_snapshot.validate(task, "p1").is_ok());
        // Tab B lets go of the lock while it does its work.
        b.rollback().await.unwrap();

        // Tab A closes the task and commits.
        let mut a = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut a, user).await.unwrap();
        let mut a_state =
            WebState::from_doc(&load_web_state(&mut a, user).await.unwrap().unwrap()).unwrap();
        a_state.tasks.insert(
            task.to_string(),
            TaskProgress {
                task_id: task.to_string(),
                task_type: "review".to_string(),
                total: 1,
                served: 1,
                answered: 1,
                done: true,
                current_kp: None,
            },
        );
        save_web_state(&mut a, user, &a_state.to_doc())
            .await
            .unwrap();
        a.commit().await.unwrap();

        // Tab B re-opens, re-reads, and re-validates. The verdict is the literal
        // 409 task_complete, not the stale "ok" of its own snapshot.
        let mut b = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut b, user).await.unwrap();
        let b_fresh =
            WebState::from_doc(&load_web_state(&mut b, user).await.unwrap().unwrap()).unwrap();
        let refusal = b_fresh.validate(task, "p1").unwrap_err();
        b.rollback().await.unwrap();

        assert_eq!(refusal, ValidateError::TaskComplete);
        let answer = ApiError::from(refusal);
        assert_eq!(answer.status.as_u16(), 409);
        assert_eq!(answer.code, "task_complete");
    })
    .await;
}

/// The D-S6 document written by the production writer reads back through the
/// production reader, out of the real column (C3, D-S6).
#[tokio::test]
async fn the_document_round_trips_through_the_web_states_column() {
    TestDb::with(|db| async move {
        let user = db.seed_user("doc@example.com").await;
        let handle = Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS);

        let mut state = WebState::for_session("s_2026-01-01a");
        state.active_secs = 137.5;
        state.record_served("addition", "s_2026-01-01a-review-addition", "abc123");
        state.pending_diagnoses.insert(
            "s_2026-01-01a-review-addition-1".to_string(),
            "3f1b7d2e-0000-4000-8000-000000000001".to_string(),
        );

        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        lock_web_state(&mut tx, user).await.unwrap();
        save_web_state(&mut tx, user, &state.to_doc())
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(handle.pool(), user).await.unwrap();
        let read =
            WebState::from_doc(&load_web_state(&mut tx, user).await.unwrap().unwrap()).unwrap();
        tx.rollback().await.unwrap();

        assert_eq!(read.session.as_deref(), Some("s_2026-01-01a"));
        assert_eq!(read.active_secs, 137.5);
        assert_eq!(read.ring("addition").hashes(), &["abc123".to_string()]);
        assert_eq!(
            read.memory("s_2026-01-01a-review-addition").hashes(),
            &["abc123".to_string()]
        );
        assert_eq!(
            read.pending_diagnoses
                .get("s_2026-01-01a-review-addition-1")
                .map(String::as_str),
            Some("3f1b7d2e-0000-4000-8000-000000000001")
        );
    })
    .await;
}

/// A stored document with a key this build does not know is a failure, not a
/// silent drop: `deny_unknown_fields` is the guard.
#[test]
fn an_unknown_key_in_the_document_is_refused() {
    let doc = json!({"session": "s_2026-01-01a", "served_texts": ["an old field"]});
    assert!(WebState::from_doc(&doc).is_err());
}

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
