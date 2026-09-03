//! The event fixtures of the state tests: a two-topic curriculum and the
//! events of one session on it.

use cadus_core::config::Config;
use cadus_core::curriculum::{Catalog, Course, Curriculum, RawCurriculum, RawUnit, Topic, Unit};
use cadus_core::event::{
    AnswerKind, Attempt, AttemptProblem, Event, Regraded, RegradedAttempt, ReviewResult,
    SchemaVersion, Secs, SessionEnd, SessionStart, Slug, TaskType, Timestamp, WorkQuality,
};
use cadus_core::projector::ProjectionInput;

/// The Unix microsecond instant of 2026-01-01T00:00:00Z.
pub const BASE_US: i64 = 1_767_225_600_000_000;

/// The session id of every fixture event.
pub const SESSION: &str = "s_2026-01-01a";

/// The curriculum and the config one fold reads.
pub struct Fixture {
    pub graph: Curriculum,
    pub cfg: Config,
}

impl Fixture {
    /// The two-topic micro-curriculum with the default config.
    pub fn micro() -> Self {
        Self {
            graph: graph(),
            cfg: Config::default(),
        }
    }

    /// `graph` with the default config.
    pub fn of(graph: Curriculum) -> Self {
        Self {
            graph,
            cfg: Config::default(),
        }
    }

    /// The projection input at `BASE_US`, with the 1.0 defaults.
    pub fn input(&self) -> ProjectionInput<'_> {
        ProjectionInput::new(&self.graph, &self.cfg, Timestamp::from_micros(BASE_US))
    }
}

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

/// One graded correct attempt of `topic` on the review task `task_id`, at
/// `ts` inside `session`, with the statement `text` and its `answer`.
///
/// Every other field is the deterministic grade of a correct numeric answer.
pub fn attempt_row(
    ts: Timestamp,
    session: &str,
    task_id: &str,
    topic: &str,
    attempt_id: &str,
    text: String,
    answer: String,
) -> Event {
    Event::Attempt(Attempt {
        ts,
        session: Some(session.to_string()),
        v: SchemaVersion,
        attempt_id: attempt_id.to_string(),
        task_id: task_id.to_string(),
        topic: Slug::new(topic).expect("the topic slug"),
        kp: None,
        task_type: TaskType::Review,
        problem: AttemptProblem {
            text,
            expected: answer.clone(),
        },
        given_answer: answer,
        work: None,
        answer_kind: Some(AnswerKind::Numeric),
        correct: true,
        secs: Secs::new(12).expect("twelve seconds"),
        error_tags: Vec::new(),
        work_quality: WorkQuality::NearlyPerfect,
        grader_note: Some("deterministic".to_string()),
        assisted: false,
    })
}

/// One graded attempt on `addition`.
pub fn attempt(attempt_id: &str) -> Event {
    attempt_row(
        Timestamp::from_micros(BASE_US),
        SESSION,
        "s_2026-01-01a-review-addition",
        "addition",
        attempt_id,
        "Compute $8 - 5$.".to_string(),
        "3".to_string(),
    )
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
