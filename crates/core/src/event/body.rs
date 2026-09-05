//! The task bodies: the served task, the graded attempt, and the three task
//! results, with the nested payloads they carry.

use serde::{Deserialize, Serialize};

use super::{
    AnswerKind, PositiveSecs, SchemaVersion, Secs, Slug, TaskType, Timestamp, WorkQuality,
};

/// A problem stub recorded on `task_served`.
///
/// `text_hash` is inert in 1.0: nothing computes or reads it. The live near-duplicate
/// guard hashes the ATTEMPT's problem text through
/// [`crate::learner::problem_text_hash`]. The field stays on the wire because a
/// removed field would break replay of a historical event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServedProblem {
    /// The id of the served problem instance.
    pub id: String,
    /// The inert digest of the problem text.
    pub text_hash: String,
    /// The time the author expects the problem to take.
    pub expected_time_secs: PositiveSecs,
}

/// The problem body carried on an `attempt` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttemptProblem {
    /// The problem text the learner saw. [`crate::learner::problem_text_hash`] hashes
    /// this text, with no whitespace normalization (trap T17).
    pub text: String,
    /// The expected answer.
    pub expected: String,
}

/// One topic's row inside a `quiz_result`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizTopicResult {
    /// The topic the question tested.
    pub topic: Slug,
    /// Whether the answer was mathematically correct.
    pub correct: bool,
    /// The time the learner took.
    pub secs: Secs,
}

/// The replacement grade fields for one superseded `attempt`.
///
/// `correct` is deliberately absent: a correction restates how well the work was
/// done, never whether the answer was right (C4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegradedAttempt {
    /// The attempt this correction supersedes.
    pub attempt_id: String,
    /// The replacement work-quality tier.
    pub work_quality: WorkQuality,
    /// The replacement error tags. They replace the whole list, in order.
    #[serde(default)]
    pub error_tags: Vec<String>,
    /// The replacement grader note.
    #[serde(default)]
    pub grader_note: Option<String>,
}

/// A task was served. The fold treats it as a no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskServed {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The id of the served task.
    pub task_id: String,
    /// The kind of task.
    pub task_type: TaskType,
    /// The topic of the task, when it has one.
    #[serde(default)]
    pub topic: Option<Slug>,
    /// The knowledge point of a lesson task.
    #[serde(default)]
    pub kp: Option<Slug>,
    /// The served problem stubs.
    #[serde(default)]
    pub problems: Vec<ServedProblem>,
    /// The component topics of a multi-step task.
    #[serde(default)]
    pub component_topics: Vec<Slug>,
    /// The seed the composer used.
    #[serde(default)]
    pub seed: Option<i64>,
}

/// One graded problem attempt. It is the load-bearing event.
///
/// The projector reads only `problem.text` and `correct` from it (spec section 4.1).
/// It never reads `work_quality`, `error_tags`, `secs`, `assisted`, `work`,
/// `answer_kind`, or `kp`. FIRe fires from the result events instead.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempt {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The idempotency key of the attempt. The fold uses it for regrade matching only.
    pub attempt_id: String,
    /// The task the attempt belongs to.
    pub task_id: String,
    /// The topic the attempt practiced.
    pub topic: Slug,
    /// The knowledge point, for a lesson attempt.
    #[serde(default)]
    pub kp: Option<Slug>,
    /// The kind of task.
    pub task_type: TaskType,
    /// The problem the learner saw.
    pub problem: AttemptProblem,
    /// The answer the learner gave.
    pub given_answer: String,
    /// The learner's written work.
    #[serde(default)]
    pub work: Option<String>,
    /// The shape of the answer.
    #[serde(default)]
    pub answer_kind: Option<AnswerKind>,
    /// Whether the answer was mathematically correct (C4).
    pub correct: bool,
    /// The time the learner took.
    pub secs: Secs,
    /// The grader's error tags.
    #[serde(default)]
    pub error_tags: Vec<String>,
    /// The grader's work-quality tier. Partial credit lives here, not in `correct`.
    pub work_quality: WorkQuality,
    /// The grader's note.
    #[serde(default)]
    pub grader_note: Option<String>,
    /// Whether the learner used help.
    #[serde(default)]
    pub assisted: bool,
}

/// A lesson closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LessonResult {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The topic the lesson taught.
    pub topic: Slug,
    /// Whether the lesson passed.
    pub passed: bool,
    /// The knowledge point the lesson stopped at, when it failed.
    #[serde(default)]
    pub failed_at_kp: Option<Slug>,
    /// The XP the lesson priced.
    #[serde(default)]
    pub xp: f64,
    /// The work-quality tier of the closing attempt.
    pub quality_tier: WorkQuality,
    /// Whether any attempt of the task used help.
    #[serde(default)]
    pub assisted: bool,
}

/// A spaced review closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewResult {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The topic the review tested.
    pub topic: Slug,
    /// Whether the review passed.
    pub passed: bool,
    /// The weighted score of the review.
    pub weighted_score: f64,
    /// The XP the review priced.
    #[serde(default)]
    pub xp: f64,
    /// The work-quality tier of the closing attempt.
    pub quality_tier: WorkQuality,
    /// Whether any attempt of the task used help.
    #[serde(default)]
    pub assisted: bool,
    /// The task the review closed. A null value binds through the preceding attempt.
    #[serde(default)]
    pub task_id: Option<String>,
}

/// A quiz closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizResult {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The id of the quiz.
    pub quiz_id: String,
    /// The fraction of questions the learner answered correctly.
    pub score: f64,
    /// One row per question, in the order the quiz asked them.
    #[serde(default)]
    pub per_topic: Vec<QuizTopicResult>,
    /// The XP the quiz priced.
    #[serde(default)]
    pub xp: f64,
}
