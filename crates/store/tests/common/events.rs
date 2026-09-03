//! The event fixtures of the state tests: a two-topic curriculum and the
//! events of one session on it.

use cadus_core::curriculum::{Catalog, Course, Curriculum, RawCurriculum, RawUnit, Topic, Unit};
use cadus_core::event::{
    AnswerKind, Attempt, AttemptProblem, Event, Regraded, RegradedAttempt, ReviewResult,
    SchemaVersion, Secs, SessionEnd, SessionStart, Slug, TaskType, Timestamp, WorkQuality,
};

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
pub const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id of every fixture event.
pub const SESSION: &str = "s_2026-01-01a";

/// One micro-curriculum: one course `c`, one module `M`, two topics.
pub fn graph() -> Curriculum {
    let topics: Vec<Topic> = ["addition", "fractions"]
        .iter()
        .map(|id| Topic {
            id: cadus_core::curriculum::Slug::new(*id).unwrap(),
            name: (*id).to_string(),
            core: true,
            difficulty: 0.3,
            drill: false,
            answer_kind: cadus_core::curriculum::AnswerKind::Numeric,
            expected_time_secs: 30,
            prerequisites: Vec::new(),
            encompassings_extra: Vec::new(),
            knowledge_points: Vec::new(),
            diagnostic_exemplar: None,
            anki_seeds: Vec::new(),
        })
        .collect();
    Curriculum::build(RawCurriculum {
        catalog: Catalog {
            courses: vec![Course {
                id: cadus_core::curriculum::Slug::new("c").unwrap(),
                name: "c".to_string(),
                order: 0,
                mastery_floor: Vec::new(),
                mastery_floor_course: None,
            }],
        },
        units: vec![RawUnit {
            course_id: "c".to_string(),
            file_name: "00-M.yaml".to_string(),
            unit: Unit {
                unit: "M".to_string(),
                course: cadus_core::curriculum::Slug::new("c").unwrap(),
                module: "M".to_string(),
                topics,
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

/// A `session_start` at `BASE_US`.
pub fn start(session: &str) -> Event {
    Event::SessionStart(SessionStart {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(session.to_string()),
        v: SchemaVersion,
    })
}

/// A `session_end` at `BASE_US`.
pub fn end(session: &str) -> Event {
    Event::SessionEnd(SessionEnd {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(session.to_string()),
        v: SchemaVersion,
        xp_earned: 0.0,
        minutes: 0.0,
    })
}

/// One graded attempt on `addition`.
pub fn attempt(attempt_id: &str) -> Event {
    Event::Attempt(Attempt {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(SESSION.to_string()),
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

/// One correction of the attempt above.
pub fn regraded(attempt_id: &str) -> Event {
    Event::Regraded(Regraded {
        ts: Timestamp::from_micros(BASE_US),
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
        task_id: "s_2026-01-01a-review-addition".to_string(),
        topic: Slug::new("addition").unwrap(),
        attempts: vec![RegradedAttempt {
            attempt_id: attempt_id.to_string(),
            work_quality: WorkQuality::Poor,
            error_tags: vec!["arithmetic-slip".to_string()],
            grader_note: Some("operator repair".to_string()),
        }],
        quality_tier: None,
        xp: None,
        reason: "an operator repair".to_string(),
    })
}

/// One passed review of `addition` at `ts`, worth `xp`.
pub fn review(ts: Timestamp, xp: f64) -> Event {
    Event::ReviewResult(ReviewResult {
        ts,
        session: Some(SESSION.to_string()),
        v: SchemaVersion,
        topic: Slug::new("addition").unwrap(),
        passed: true,
        weighted_score: 1.0,
        xp,
        quality_tier: WorkQuality::Perfect,
        assisted: false,
        task_id: Some("s_2026-01-01a-review-addition".to_string()),
    })
}
