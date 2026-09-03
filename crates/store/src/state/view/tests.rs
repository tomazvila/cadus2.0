//! The unit tests of the session view: the literal values of the whole-log
//! fold, the forward-fold property, the failure map, and the JSON round trip.

use cadus_core::event::{
    Attempt, AttemptProblem, DiagnosticPlaced, EnrollReason, Enrolled, Event, LessonResult,
    QuizResult, ReviewResult, SchemaVersion, Secs, SessionEnd, SessionStart, Slug, TaskServed,
    TaskType, Timestamp, WorkQuality,
};

use std::collections::BTreeSet;

use sqlx::types::chrono::{DateTime, Utc};

use super::{SESSION_VIEW_VERSION, SessionView};
use crate::state::EventRow;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
const BASE_US: i64 = 1_767_225_600_000_000;

/// One event at line `seq`.
fn row(seq: i64, event: Event) -> EventRow {
    EventRow { seq, event }
}

/// The instant `minutes` minutes after [`BASE_US`].
fn at(minutes: i64) -> Timestamp {
    Timestamp::from_micros(BASE_US + minutes * 60_000_000)
}

/// A slug of a test fixture.
fn slug(id: &str) -> Slug {
    Slug::new(id).expect("the fixture slug is valid")
}

/// A log that touches every branch of [`SessionView::apply`].
fn sample_log() -> Vec<EventRow> {
    vec![
        row(
            1,
            Event::Enrolled(Enrolled {
                ts: at(0),
                session: None,
                v: SchemaVersion,
                course: slug("algebra-1"),
                reason: None,
                return_to: None,
            }),
        ),
        row(
            2,
            Event::SessionStart(SessionStart {
                ts: at(1),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
            }),
        ),
        row(
            3,
            Event::TaskServed(TaskServed {
                ts: at(2),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
                task_id: "s_2026-01-01a-drill-adding-integers".to_string(),
                task_type: TaskType::Drill,
                topic: Some(slug("adding-integers")),
                kp: None,
                problems: Vec::new(),
                component_topics: Vec::new(),
                seed: None,
            }),
        ),
        row(
            4,
            Event::Attempt(Attempt {
                ts: at(3),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
                attempt_id: "s_2026-01-01a-lesson-adding-integers-1".to_string(),
                task_id: "s_2026-01-01a-lesson-adding-integers".to_string(),
                topic: slug("adding-integers"),
                kp: None,
                task_type: TaskType::Lesson,
                problem: AttemptProblem {
                    text: "Compute $1 + 1$.".to_string(),
                    expected: "2".to_string(),
                },
                given_answer: "2".to_string(),
                work: None,
                answer_kind: None,
                correct: true,
                secs: Secs::new(9).expect("nine seconds"),
                error_tags: Vec::new(),
                work_quality: WorkQuality::NearlyPerfect,
                grader_note: None,
                assisted: false,
            }),
        ),
        row(
            5,
            Event::LessonResult(LessonResult {
                ts: at(4),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
                topic: slug("adding-integers"),
                passed: true,
                failed_at_kp: None,
                xp: 5.5,
                quality_tier: WorkQuality::NearlyPerfect,
                assisted: false,
            }),
        ),
        row(
            6,
            Event::QuizResult(QuizResult {
                ts: at(5),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
                quiz_id: "q1".to_string(),
                score: 0.95,
                per_topic: Vec::new(),
                xp: 0.0,
            }),
        ),
        row(
            7,
            Event::DiagnosticPlaced(DiagnosticPlaced {
                ts: at(6),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
                balances: Default::default(),
                conditional: Vec::new(),
                refresh: false,
            }),
        ),
        row(
            8,
            Event::SessionEnd(SessionEnd {
                ts: at(7),
                session: Some("s_2026-01-01a".to_string()),
                v: SchemaVersion,
                xp_earned: 5.5,
                minutes: 7.0,
            }),
        ),
        row(
            9,
            Event::SessionStart(SessionStart {
                ts: at(1441),
                session: Some("s_2026-01-02a".to_string()),
                v: SchemaVersion,
            }),
        ),
        row(
            10,
            Event::Enrolled(Enrolled {
                ts: at(1442),
                session: Some("s_2026-01-02a".to_string()),
                v: SchemaVersion,
                course: slug("pre-algebra"),
                reason: Some(EnrollReason::GapFill),
                return_to: None,
            }),
        ),
        row(
            11,
            Event::ReviewResult(ReviewResult {
                ts: at(1443),
                session: Some("s_2026-01-02a".to_string()),
                v: SchemaVersion,
                topic: slug("adding-integers"),
                passed: true,
                weighted_score: 1.0,
                xp: 2.25,
                quality_tier: WorkQuality::NearlyPerfect,
                assisted: false,
                task_id: Some("s_2026-01-02a-review-adding-integers".to_string()),
            }),
        ),
        row(
            12,
            Event::LessonResult(LessonResult {
                ts: at(1444),
                session: Some("s_2026-01-02a".to_string()),
                v: SchemaVersion,
                topic: slug("adding-integers"),
                passed: false,
                failed_at_kp: Some(slug("kp2")),
                xp: 0.0,
                quality_tier: WorkQuality::NearlyPassable,
                assisted: false,
            }),
        ),
    ]
}

/// The view of the whole log holds the literal values of the 1.0 scans.
#[test]
fn the_whole_log_fold_holds_its_literal_values() {
    let view = SessionView::of_log(&sample_log());
    assert_eq!(view.v, SESSION_VIEW_VERSION);
    assert_eq!(view.current_session.as_deref(), Some("s_2026-01-02a"));
    assert_eq!(view.session_start_seq, Some(9));
    assert_eq!(
        view.enrollment_stack,
        vec!["algebra-1".to_string(), "pre-algebra".to_string()]
    );
    assert_eq!(
        view.learned_at.get("adding-integers"),
        Some(&at(4).micros())
    );
    assert_eq!(
        view.last_drill_at.get("adding-integers"),
        Some(&at(2).micros())
    );
    assert!(
        view.closed_task_ids
            .contains("s_2026-01-02a-review-adding-integers")
    );
    // The failed lesson of line 12 folds into the map; the passed lesson of
    // line 5 folds into `learned_at` and never here (V2).
    assert_eq!(view.lesson_failures.len(), 1);
    assert_eq!(
        view.lesson_failures.get("adding-integers"),
        Some(&BTreeSet::from(["kp2".to_string()]))
    );
    assert_eq!(view.study_days().len(), 1);
    assert_eq!(view.quiz_high_score_streak, 1);
    assert!(view.has_diagnostic);
    // The lesson XP falls inside session a and the review XP inside b.
    assert_eq!(view.xp_in_session("s_2026-01-01a"), 5.5);
    assert_eq!(view.xp_in_session("s_2026-01-02a"), 2.25);
    assert_eq!(view.xp_in_session("s_2026-01-09z"), 0.0);
}

/// The forward fold from ANY cut equals the fold of the whole log.
///
/// This is the property [`project_current`] rests on: a request that adds no
/// event reads the cached view, and a request that adds events folds only
/// those events into it (F15, F18).
#[test]
fn the_forward_fold_equals_the_whole_log_fold() {
    let log = sample_log();
    let whole = SessionView::of_log(&log);
    for cut in 0..=log.len() {
        let mut resumed = SessionView::of_log(&log[..cut]);
        resumed.fold(&log[cut..]);
        assert_eq!(
            resumed, whole,
            "the fold resumed at line {cut} left another document"
        );
    }
}

/// One failed lesson, with and without a knowledge point.
fn failed_lesson(topic: &str, kp: Option<&str>) -> Event {
    Event::LessonResult(LessonResult {
        ts: at(8),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion,
        topic: slug(topic),
        passed: false,
        failed_at_kp: kp.map(slug),
        xp: 0.0,
        quality_tier: WorkQuality::NearlyPassable,
        assisted: false,
    })
}

/// The map folds every FAILED `lesson_result` that names a knowledge point,
/// and [`SessionView::already_failed`] answers the repeat test from it (V2).
#[test]
fn the_failure_map_holds_every_failed_knowledge_point() {
    let log = vec![
        row(1, failed_lesson("adding-integers", Some("kp1"))),
        row(2, failed_lesson("adding-integers", Some("kp2"))),
        row(3, failed_lesson("adding-integers", Some("kp1"))),
        row(4, failed_lesson("fractions", None)),
    ];
    let view = SessionView::of_log(&log);

    assert_eq!(view.lesson_failures.len(), 1);
    assert_eq!(
        view.lesson_failures.get("adding-integers"),
        Some(&BTreeSet::from(["kp1".to_string(), "kp2".to_string()]))
    );
    assert!(view.already_failed("adding-integers", Some("kp1")));
    assert!(view.already_failed("adding-integers", Some("kp2")));
    assert!(!view.already_failed("adding-integers", Some("kp3")));
    // A result with no knowledge point folds nothing, so it never repeats.
    assert!(!view.already_failed("adding-integers", None));
    assert!(!view.already_failed("fractions", Some("kp1")));
    assert!(!view.already_failed("subtracting-integers", Some("kp1")));
}

/// The document round-trips through the `session_view` column shape.
#[test]
fn the_view_round_trips_through_json() {
    let view = SessionView::of_log(&sample_log());
    let doc = serde_json::to_value(&view).expect("the view serializes");
    let back: SessionView = serde_json::from_value(doc).expect("the view reads back");
    assert_eq!(back, view);
}

/// The branches the sample log leaves out: a session start and end with no
/// id, a gap return that pops the stack and one that keeps a lone base, a
/// served task that is not a drill, a review with no task id, and a quiz
/// under the high score that resets the streak.
#[test]
fn the_other_branches_of_the_fold_fold_nothing_or_reset() {
    let mut view = SessionView::default();
    view.apply(
        1,
        &Event::SessionStart(SessionStart {
            ts: at(0),
            session: None,
            v: SchemaVersion,
        }),
    );
    assert_eq!(view.current_session, None);
    assert!(view.session_ids.is_empty());
    view.apply(
        2,
        &Event::SessionEnd(SessionEnd {
            ts: at(1),
            session: None,
            v: SchemaVersion,
            xp_earned: 0.0,
            minutes: 1.0,
        }),
    );
    assert!(view.open_sessions.is_empty());

    let enroll = |reason| {
        Event::Enrolled(Enrolled {
            ts: at(2),
            session: None,
            v: SchemaVersion,
            course: slug("algebra-1"),
            reason,
            return_to: None,
        })
    };
    view.apply(3, &enroll(Some(EnrollReason::GapReturn)));
    assert!(view.enrollment_stack.is_empty());
    view.apply(4, &enroll(None));
    view.apply(5, &enroll(Some(EnrollReason::GapFill)));
    view.apply(6, &enroll(Some(EnrollReason::GapReturn)));
    assert_eq!(view.enrollment_stack, vec!["algebra-1".to_string()]);
    view.apply(7, &enroll(Some(EnrollReason::GapReturn)));
    assert_eq!(view.enrollment_stack, vec!["algebra-1".to_string()]);

    view.apply(
        8,
        &Event::TaskServed(TaskServed {
            ts: at(3),
            session: None,
            v: SchemaVersion,
            task_id: "lesson".to_string(),
            task_type: TaskType::Lesson,
            topic: Some(slug("adding-integers")),
            kp: None,
            problems: Vec::new(),
            component_topics: Vec::new(),
            seed: None,
        }),
    );
    assert!(view.last_drill_at.is_empty());

    view.apply(
        9,
        &Event::ReviewResult(ReviewResult {
            ts: at(4),
            session: None,
            v: SchemaVersion,
            topic: slug("adding-integers"),
            passed: true,
            weighted_score: 1.0,
            xp: 1.0,
            quality_tier: WorkQuality::NearlyPerfect,
            assisted: false,
            task_id: None,
        }),
    );
    assert!(view.closed_task_ids.is_empty());

    let quiz = |score| {
        Event::QuizResult(QuizResult {
            ts: at(5),
            session: None,
            v: SchemaVersion,
            quiz_id: "q".to_string(),
            score,
            per_topic: Vec::new(),
            xp: 0.0,
        })
    };
    view.apply(10, &quiz(0.95));
    view.apply(11, &quiz(0.95));
    assert_eq!(view.quiz_high_score_streak, 2);
    view.apply(12, &quiz(0.5));
    assert_eq!(view.quiz_high_score_streak, 0);
}

/// The next session id of a day takes the first unused letter, and a day
/// that used all 26 gives `z` again.
#[test]
fn the_next_session_id_takes_the_first_unused_letter() {
    let today = DateTime::<Utc>::from_timestamp(1_767_225_600, 0).expect("the instant");
    let mut view = SessionView::default();
    assert_eq!(view.new_session_id(today), "s_2026-01-01a");
    view.session_ids.insert("s_2026-01-01a".to_string());
    view.session_ids.insert("s_2026-01-01b".to_string());
    assert_eq!(view.new_session_id(today), "s_2026-01-01c");
    for letter in "abcdefghijklmnopqrstuvwxyz".chars() {
        view.session_ids.insert(format!("s_2026-01-01{letter}"));
    }
    assert_eq!(view.new_session_id(today), "s_2026-01-01z");
}
