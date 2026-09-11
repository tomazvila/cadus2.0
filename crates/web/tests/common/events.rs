//! The fixtures of `tests/session_state.rs` and its parts.

use axum::http::StatusCode;
use cadus_core::event::{
    AnswerKind, Attempt, AttemptOutcome, AttemptProblem, EnrollReason, Enrolled, Event,
    LessonResult, QuizResult, SchemaVersion, Secs, SessionEnd, SessionStart, Slug, TaskServed,
    TaskType, Timestamp, WorkQuality,
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

use super::*;

/// One microsecond day.
pub const DAY_US: i64 = 86_400_000_000;

/// Wrap an [`EventRow`] around an event at line `seq`.
pub fn row(seq: i64, event: Event) -> EventRow {
    EventRow { seq, event }
}

/// A `session_start` at `BASE_US + offset`.
pub fn start(session: &str, offset: i64) -> Event {
    Event::SessionStart(SessionStart {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some(session.to_string()),
        v: SchemaVersion::current(),
    })
}

/// A `session_end` at `BASE_US + offset`.
pub fn end(session: &str, offset: i64) -> Event {
    Event::SessionEnd(SessionEnd {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some(session.to_string()),
        v: SchemaVersion::current(),
        xp_earned: 0.0,
        minutes: 0.0,
    })
}

/// A passed or failed `lesson_result` worth `xp`.
pub fn lesson(session: &str, topic: &str, xp: f64, passed: bool, offset: i64) -> Event {
    Event::LessonResult(LessonResult {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some(session.to_string()),
        v: SchemaVersion::current(),
        topic: Slug::new(topic).unwrap(),
        passed,
        failed_at_kp: None,
        xp,
        quality_tier: WorkQuality::NearlyPerfect,
        assisted: false,
    })
}

/// An `enrolled` event with an optional switch reason.
pub fn enrolled(course: &str, reason: Option<EnrollReason>) -> Event {
    Event::Enrolled(Enrolled {
        ts: Timestamp::from_micros(BASE_US),
        session: None,
        v: SchemaVersion::current(),
        course: Slug::new(course).unwrap(),
        reason,
        return_to: None,
    })
}

/// A served task of a given kind.
pub fn served(task_id: &str, task_type: TaskType, topic: Option<&str>, offset: i64) -> Event {
    Event::TaskServed(TaskServed {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion::current(),
        task_id: task_id.to_string(),
        task_type,
        topic: topic.map(|id| Slug::new(id).unwrap()),
        kp: None,
        problems: Vec::new(),
        component_topics: Vec::new(),
        seed: None,
        probe_delay_days: None,
        confirm: false,
    })
}

/// One graded attempt on `addition` at `BASE_US + offset`.
pub fn graded(attempt_id: &str, offset: i64) -> Event {
    Event::Attempt(Attempt {
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion::current(),
        attempt_id: attempt_id.to_string(),
        task_id: "s_2026-01-01a-review-addition".to_string(),
        topic: Slug::new("addition").unwrap(),
        kp: None,
        task_type: TaskType::Review,
        problem: AttemptProblem {
            answer_contract: None,
            text: "Compute $8 - 5$.".to_string(),
            expected: "3".to_string(),
        },
        given_answer: "3".to_string(),
        work: None,
        answer_kind: Some(AnswerKind::Numeric),
        correct: true,
        outcome: AttemptOutcome::Correct,
        item_digest: None,
        item_source: None,
        exposure: None,
        timing_reliable: None,
        timing: None,
        skills: Vec::new(),
        independent_after_feedback: false,
        feedback_practice: false,
        secs: Secs::new(12).unwrap(),
        error_tags: Vec::new(),
        work_quality: WorkQuality::NearlyPerfect,
        grader_note: Some("deterministic".to_string()),
        assisted: false,
    })
}

/// A quiz result with a score.
pub fn quiz(score: f64, offset: i64) -> Event {
    Event::QuizResult(QuizResult {
        inconclusive: false,
        ts: Timestamp::from_micros(BASE_US + offset),
        session: Some("s_2026-01-01a".to_string()),
        v: SchemaVersion::current(),
        quiz_id: format!("q{offset}"),
        score,
        per_topic: Vec::new(),
        xp: 0.0,
    })
}

/// The live review problem `Compute $8 - 5$.` of `task_id`, dealt at the start
/// of 2026.
pub fn review_problem(task_id: &str) -> ServedProblem {
    ServedProblem {
        timing_interrupted: false,
        problem_id: "p1".to_string(),
        task_id: task_id.to_string(),
        topic: Some("addition".to_string()),
        serve_topic: Some("addition".to_string()),
        kp: None,
        answer_kind: Some("numeric".to_string()),
        text: "Compute $8 - 5$.".to_string(),
        expected: PoolAnswer {
            answer_contract: None,
            v: 1,
            answer: "3".to_string(),
        },
        solution_sketch: None,
        started_at: 1_767_225_600.0,
        hints_given: Vec::new(),
        index: 0,
        rework: None,
        handoff: None,
    }
}
