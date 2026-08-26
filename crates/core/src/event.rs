//! The 1.0 event log on the wire (C2, spec section 2).
//!
//! Every state change of a learner is an event, and the events are the only input of
//! the fold. The shapes here are the 1.0 shapes byte for byte, because 2.0 must read
//! and write the same log:
//!
//! - [`Event`] is a tagged union on the `type` key, with 16 members.
//! - Every member carries the envelope `ts`, `session`, and `v`.
//! - Every member forbids an unknown key, as 1.0's `extra="forbid"` does.
//! - `v` is `1`. 1.0 holds an empty shim table, so any other version is an error.
//! - [`Timestamp`] parses RFC 3339, reads a naive value as UTC (trap T8), and writes
//!   `...Z` with second precision when the microsecond part is zero.
//!
//! [`Event::from_json`] is the reader and [`Event::to_canonical_json`] is the writer.
//! The canonical form sorts every key, uses compact separators, and leaves non-ASCII
//! text unescaped, which is the form the fixtures hold.

use std::fmt;

use chrono::{DateTime, NaiveDateTime, Utc};
use indexmap::IndexMap;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::numeric;

/// The one schema version this build reads. 1.0 holds an empty shim table.
pub const SCHEMA_VERSION: i64 = 1;

/// An event that does not parse, or does not satisfy the 1.0 schema.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EventError {
    /// The JSON text is malformed, holds an unknown key, or misses a required key.
    #[error("event JSON is invalid: {0}")]
    Json(String),
    /// The timestamp text is not an RFC 3339 or naive date-time.
    #[error("timestamp `{0}` is not an RFC 3339 date-time")]
    InvalidTimestamp(String),
    /// The canonical form could not be built.
    #[error("event does not serialize: {0}")]
    Serialize(String),
}

// --------------------------------------------------------------------------- //
// Envelope scalars
// --------------------------------------------------------------------------- //

/// The `v` key of the envelope. It reads and writes the value `1` and nothing else.
///
/// 1.0 reads `v`, then applies `SHIMS[v]` while `v` is below `SCHEMA_VERSION`. The
/// table is empty and `SCHEMA_VERSION` is `1`, so every other version raises there
/// and is an error value here.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SchemaVersion;

impl SchemaVersion {
    /// The numeric value on the wire.
    #[must_use]
    pub const fn get(self) -> i64 {
        SCHEMA_VERSION
    }
}

impl Serialize for SchemaVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i64(SCHEMA_VERSION)
    }
}

impl<'de> Deserialize<'de> for SchemaVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;
        if value == SCHEMA_VERSION {
            Ok(Self)
        } else {
            Err(de::Error::custom(format!(
                "unsupported event schema version v{value}; this build reads v{SCHEMA_VERSION} only"
            )))
        }
    }
}

/// A UTC instant, held as microseconds since the Unix epoch (trap T8).
///
/// The wire form is RFC 3339. A value with no offset is read as UTC, exactly as
/// 1.0's `as_utc` reads it. The value is written back with a `Z` suffix: second
/// precision when the microsecond part is zero, and six fractional digits when it is
/// not, which is the form 1.0 writes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(i64);

impl Timestamp {
    /// Build an instant from microseconds since the Unix epoch.
    #[must_use]
    pub const fn from_micros(micros: i64) -> Self {
        Self(micros)
    }

    /// The instant as microseconds since the Unix epoch.
    #[must_use]
    pub const fn micros(self) -> i64 {
        self.0
    }

    /// Parse an RFC 3339 date-time, or a naive date-time that is read as UTC.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::InvalidTimestamp`] when no accepted form matches.
    pub fn parse(text: &str) -> Result<Self, EventError> {
        if let Ok(offset) = DateTime::parse_from_rfc3339(text) {
            return Ok(Self(offset.with_timezone(&Utc).timestamp_micros()));
        }
        for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
            if let Ok(naive) = NaiveDateTime::parse_from_str(text, format) {
                return Ok(Self(numeric::from_naive_utc(naive)));
            }
        }
        Err(EventError::InvalidTimestamp(text.to_owned()))
    }

    /// The wire form: RFC 3339 with a `Z` suffix.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::InvalidTimestamp`] when the instant is outside the range
    /// `chrono` represents.
    pub fn to_wire_string(self) -> Result<String, EventError> {
        let stamp = numeric::to_datetime(self.0)
            .map_err(|_| EventError::InvalidTimestamp(self.0.to_string()))?;
        let format = if self.0.rem_euclid(1_000_000) == 0 {
            "%Y-%m-%dT%H:%M:%SZ"
        } else {
            "%Y-%m-%dT%H:%M:%S%.6fZ"
        };
        Ok(stamp.format(format).to_string())
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_wire_string() {
            Ok(text) => f.write_str(&text),
            Err(_) => write!(f, "{}us", self.0),
        }
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let text = self
            .to_wire_string()
            .map_err(|error| serde::ser::Error::custom(error.to_string()))?;
        serializer.serialize_str(&text)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(|error| de::Error::custom(error.to_string()))
    }
}

/// A non-empty curriculum id: a topic, a course, or a knowledge point.
///
/// 1.0 declares these as
/// `Annotated[str, StringConstraints(min_length=1, strip_whitespace=True)]`
/// (`model.py:23`), so pydantic REMOVES the outer whitespace and then applies the
/// length rule. The port does the same: the value keeps its trimmed form, and a
/// value with nothing left after the trim is an error. Without the trim a padded
/// topic id folds onto a phantom topic and the real one keeps no credit.
///
/// [`crate::curriculum::model::Slug`] is the same 1.0 type on the curriculum side
/// and carries the same rule. The two stay separate types because they report
/// through different error enums.
///
/// The trim is Rust `str::trim`, which removes the Unicode `White_Space` set.
/// Python `str.strip()` removes that set AND the four separators `U+001C` to
/// `U+001F`, so an id padded with one of those four keeps it here. The same rule
/// holds on the curriculum side, and no authored id carries such a character.
///
/// The type orders by byte, which is the code-point order Python's `sorted()` gives
/// for a `str` (trap T18).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Slug(String);

impl Slug {
    /// Build a slug from text. The outer whitespace goes away.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when nothing is left after the trim, which
    /// covers the empty string and a whitespace-only id.
    pub fn new(text: impl Into<String>) -> Result<Self, EventError> {
        let text = text.into();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(EventError::Json("a curriculum id must not be empty".into()));
        }
        Ok(Self(trimmed.to_owned()))
    }

    /// The id as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Take the id as an owned `String`.
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Slug {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Slug {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(|error| de::Error::custom(error.to_string()))
    }
}

/// A duration in whole seconds that is zero or more (`secs` in 1.0, `ge=0`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Secs(i64);

impl Secs {
    /// Build a duration.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when `value` is negative.
    pub fn new(value: i64) -> Result<Self, EventError> {
        if value < 0 {
            return Err(EventError::Json(format!("secs {value} must be 0 or more")));
        }
        Ok(Self(value))
    }

    /// The duration in seconds.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Secs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;
        Self::new(value).map_err(|error| de::Error::custom(error.to_string()))
    }
}

/// A duration in whole seconds that is more than zero (`expected_time_secs`, `gt=0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct PositiveSecs(i64);

impl PositiveSecs {
    /// Build a duration.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when `value` is zero or negative.
    pub fn new(value: i64) -> Result<Self, EventError> {
        if value <= 0 {
            return Err(EventError::Json(format!(
                "expected_time_secs {value} must be more than 0"
            )));
        }
        Ok(Self(value))
    }

    /// The duration in seconds.
    #[must_use]
    pub const fn get(self) -> i64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for PositiveSecs {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = i64::deserialize(deserializer)?;
        Self::new(value).map_err(|error| de::Error::custom(error.to_string()))
    }
}

/// A diagnostic answer weight in the closed range 0.0 to 1.0.
#[derive(Debug, Clone, Copy, Default, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Weight(f64);

impl Weight {
    /// Build a weight.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] when `value` is outside 0.0 to 1.0, `NaN`
    /// included.
    pub fn new(value: f64) -> Result<Self, EventError> {
        if !(0.0..=1.0).contains(&value) {
            return Err(EventError::Json(format!(
                "weight {value} must be between 0.0 and 1.0"
            )));
        }
        Ok(Self(value))
    }

    /// The weight.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for Weight {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(|error| de::Error::custom(error.to_string()))
    }
}

// --------------------------------------------------------------------------- //
// Enumerations (model.py:37-89)
// --------------------------------------------------------------------------- //

/// The kind of task an event reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TaskType {
    /// A lesson on one topic.
    #[serde(rename = "lesson")]
    Lesson,
    /// A spaced review of one topic.
    #[serde(rename = "review")]
    Review,
    /// A quiz over several topics.
    #[serde(rename = "quiz")]
    Quiz,
    /// A speed drill on one topic.
    #[serde(rename = "drill")]
    Drill,
    /// A placement diagnostic question.
    #[serde(rename = "diagnostic")]
    Diagnostic,
    /// A multi-step task over several component topics.
    #[serde(rename = "multi-step")]
    MultiStep,
}

impl TaskType {
    /// The 1.0 wire spelling of the variant (`TaskType`, `model.py:49-61`).
    ///
    /// The selector builds a task id out of it, so the text is load-bearing:
    /// a multi-step task id reads `{session}-multi-step`, with the hyphen.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lesson => "lesson",
            Self::Review => "review",
            Self::Quiz => "quiz",
            Self::Drill => "drill",
            Self::Diagnostic => "diagnostic",
            Self::MultiStep => "multi-step",
        }
    }
}

/// The grader's work-quality tier. It also spells `quality_tier` on a result event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum WorkQuality {
    /// Complete and correct work.
    #[serde(rename = "perfect")]
    Perfect,
    /// Correct work with a cosmetic flaw.
    #[serde(rename = "nearly_perfect")]
    NearlyPerfect,
    /// Work that passes.
    #[serde(rename = "passable")]
    Passable,
    /// Work just below the pass line.
    #[serde(rename = "nearly_passable")]
    NearlyPassable,
    /// Work well below the pass line.
    #[serde(rename = "poor")]
    Poor,
    /// No real attempt.
    #[serde(rename = "blowoff")]
    Blowoff,
}

/// The scheduling status of one topic.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub enum TopicStatus {
    /// Never seen.
    #[default]
    #[serde(rename = "untouched")]
    Untouched,
    /// Ready to learn.
    #[serde(rename = "frontier")]
    Frontier,
    /// Learned and on the review schedule.
    #[serde(rename = "learning")]
    Learning,
    /// Placed by the diagnostic.
    #[serde(rename = "placed")]
    Placed,
    /// Below the mastery floor of the enrolled course.
    #[serde(rename = "floor")]
    Floor,
}

/// The progress of one knowledge point inside a topic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum KpProgress {
    /// The knowledge point passed.
    #[serde(rename = "passed")]
    Passed,
    /// The knowledge point failed once.
    #[serde(rename = "failed_once")]
    FailedOnce,
    /// The knowledge point failed twice.
    #[serde(rename = "failed_twice")]
    FailedTwice,
}

/// The shape of the answer a learner gave.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnswerKind {
    /// A single number.
    #[serde(rename = "numeric")]
    Numeric,
    /// An algebraic expression.
    #[serde(rename = "expression")]
    Expression,
    /// A worked sequence of steps.
    #[serde(rename = "multi-step")]
    MultiStep,
    /// A proof.
    #[serde(rename = "proof")]
    Proof,
}

/// Why a course enrollment happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EnrollReason {
    /// The learner dropped to a prerequisite course to fill a gap.
    #[serde(rename = "gap-fill")]
    GapFill,
    /// The learner came back from a gap-fill course.
    #[serde(rename = "gap-return")]
    GapReturn,
}

// --------------------------------------------------------------------------- //
// Nested payload bodies
// --------------------------------------------------------------------------- //

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

// --------------------------------------------------------------------------- //
// The 16 event bodies
// --------------------------------------------------------------------------- //

/// A session opened. The fold treats it as a no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionStart {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
}

/// A session closed. The fold treats it as a no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionEnd {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The XP the session earned. The fold ignores it; `xp_events` come from results.
    #[serde(default)]
    pub xp_earned: f64,
    /// How long the session lasted.
    #[serde(default)]
    pub minutes: f64,
}

/// The learner enrolled in a course.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enrolled {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The course the learner entered.
    pub course: Slug,
    /// Why the enrollment happened.
    #[serde(default)]
    pub reason: Option<EnrollReason>,
    /// The course to come back to after a gap-fill.
    #[serde(default)]
    pub return_to: Option<Slug>,
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
    #[serde(default)]
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
    #[serde(default)]
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
    #[serde(default)]
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
    #[serde(default)]
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
    #[serde(default)]
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

/// Remediation became pending for a set of topics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemediationTriggered {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The kind of remediation.
    pub kind: String,
    /// The topic whose miss triggered the remediation.
    pub source_topic: Slug,
    /// The topics to remediate.
    #[serde(default)]
    pub targets: Vec<Slug>,
}

/// One answer of a placement diagnostic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticAnswer {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The topic the question tested.
    pub topic: Slug,
    /// Whether the answer was correct.
    pub correct: bool,
    /// The time the learner took.
    pub secs: Secs,
    /// How much the answer counts, from 0.0 to 1.0.
    pub weight: Weight,
}

/// A placement diagnostic closed and placed the learner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticPlaced {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The placement balance per topic.
    ///
    /// The map keeps INSERTION ORDER, because 1.0 iterates `balances.items()` in dict
    /// order and the refresh path is sensitive to it (trap T6).
    #[serde(default)]
    pub balances: IndexMap<String, f64>,
    /// The topics placed on conditional credit.
    #[serde(default)]
    pub conditional: Vec<Slug>,
    /// Whether this placement refreshes an earlier one.
    #[serde(default)]
    pub refresh: bool,
}

/// The named topics went back to an untouched state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileReset {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The topics to reset. An empty list is a no-op.
    #[serde(default)]
    pub topics: Vec<Slug>,
}

/// An append-only correction of grades already in the log (C2).
///
/// The `events` table holds no UPDATE grant, so a grade recorded in error is never
/// edited. It is superseded: this event names the task it corrects and carries the
/// replacement values, and `apply_regrades` folds them onto their targets BEFORE the
/// projector sees the stream. The original event stays in the log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regraded {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The task the correction applies to.
    pub task_id: String,
    /// The topic of the corrected task.
    pub topic: Slug,
    /// The per-attempt corrections.
    #[serde(default)]
    pub attempts: Vec<RegradedAttempt>,
    /// The replacement quality tier of the task's result event.
    #[serde(default)]
    pub quality_tier: Option<WorkQuality>,
    /// The replacement XP of the task's result event.
    #[serde(default)]
    pub xp: Option<f64>,
    /// Why the correction happened. It is mandatory so the log says why.
    pub reason: String,
}

/// An Anki card was exported. The fold treats it as a no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnkiCardCreated {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// The topic the card covers.
    pub topic: Slug,
    /// The Anki deck the card went to.
    pub deck: String,
    /// The Anki note id.
    pub note_id: i64,
    /// The digest of the card front.
    pub front_hash: String,
}

/// The scheduler config changed. The fold treats it as a no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigChanged {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// What changed.
    pub summary: String,
    /// The git reference of the change.
    #[serde(default)]
    pub git_ref: Option<String>,
}

/// The curriculum changed. The fold treats it as a no-op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurriculumChanged {
    /// When the event happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default)]
    pub v: SchemaVersion,
    /// What changed.
    pub summary: String,
    /// The git reference of the change.
    #[serde(default)]
    pub git_ref: Option<String>,
}

// --------------------------------------------------------------------------- //
// The union
// --------------------------------------------------------------------------- //

/// One event of the log. The `type` key is the discriminator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    /// A session opened.
    #[serde(rename = "session_start")]
    SessionStart(SessionStart),
    /// A session closed.
    #[serde(rename = "session_end")]
    SessionEnd(SessionEnd),
    /// The learner enrolled in a course.
    #[serde(rename = "enrolled")]
    Enrolled(Enrolled),
    /// A task was served.
    #[serde(rename = "task_served")]
    TaskServed(TaskServed),
    /// One graded problem attempt.
    #[serde(rename = "attempt")]
    Attempt(Attempt),
    /// A lesson closed.
    #[serde(rename = "lesson_result")]
    LessonResult(LessonResult),
    /// A spaced review closed.
    #[serde(rename = "review_result")]
    ReviewResult(ReviewResult),
    /// A quiz closed.
    #[serde(rename = "quiz_result")]
    QuizResult(QuizResult),
    /// Remediation became pending.
    #[serde(rename = "remediation_triggered")]
    RemediationTriggered(RemediationTriggered),
    /// One diagnostic answer.
    #[serde(rename = "diagnostic_answer")]
    DiagnosticAnswer(DiagnosticAnswer),
    /// A diagnostic placed the learner.
    #[serde(rename = "diagnostic_placed")]
    DiagnosticPlaced(DiagnosticPlaced),
    /// Topics went back to an untouched state.
    #[serde(rename = "profile_reset")]
    ProfileReset(ProfileReset),
    /// A correction of grades already in the log.
    #[serde(rename = "regraded")]
    Regraded(Regraded),
    /// An Anki card was exported.
    #[serde(rename = "anki_card_created")]
    AnkiCardCreated(AnkiCardCreated),
    /// The scheduler config changed.
    #[serde(rename = "config_changed")]
    ConfigChanged(ConfigChanged),
    /// The curriculum changed.
    #[serde(rename = "curriculum_changed")]
    CurriculumChanged(CurriculumChanged),
}

/// Apply `body` to the inner payload of every [`Event`] member.
macro_rules! for_each_event {
    ($event:expr, $inner:ident => $body:expr) => {
        match $event {
            Event::SessionStart($inner) => $body,
            Event::SessionEnd($inner) => $body,
            Event::Enrolled($inner) => $body,
            Event::TaskServed($inner) => $body,
            Event::Attempt($inner) => $body,
            Event::LessonResult($inner) => $body,
            Event::ReviewResult($inner) => $body,
            Event::QuizResult($inner) => $body,
            Event::RemediationTriggered($inner) => $body,
            Event::DiagnosticAnswer($inner) => $body,
            Event::DiagnosticPlaced($inner) => $body,
            Event::ProfileReset($inner) => $body,
            Event::Regraded($inner) => $body,
            Event::AnkiCardCreated($inner) => $body,
            Event::ConfigChanged($inner) => $body,
            Event::CurriculumChanged($inner) => $body,
        }
    };
}

impl Event {
    /// The 16 `type` values, in the order this module declares them.
    pub const TYPE_NAMES: [&'static str; 16] = [
        "session_start",
        "session_end",
        "enrolled",
        "task_served",
        "attempt",
        "lesson_result",
        "review_result",
        "quiz_result",
        "remediation_triggered",
        "diagnostic_answer",
        "diagnostic_placed",
        "profile_reset",
        "regraded",
        "anki_card_created",
        "config_changed",
        "curriculum_changed",
    ];

    /// Read one event from JSON text.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] for malformed text, an unknown `type`, an unknown
    /// key, a missing required key, an out-of-range value, or a `v` other than 1.
    pub fn from_json(text: &str) -> Result<Self, EventError> {
        serde_json::from_str(text).map_err(|error| EventError::Json(error.to_string()))
    }

    /// Write the event as canonical JSON: sorted keys, compact separators, non-ASCII
    /// text left unescaped, no trailing newline.
    ///
    /// This is the form the 1.0 oracle writes with
    /// `json.dumps(..., sort_keys=True, separators=(",", ":"), ensure_ascii=False)`,
    /// and the form the fixture streams hold.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Serialize`] when a timestamp is outside the
    /// representable range.
    pub fn to_canonical_json(&self) -> Result<String, EventError> {
        let value =
            serde_json::to_value(self).map_err(|error| EventError::Serialize(error.to_string()))?;
        serde_json::to_string(&value).map_err(|error| EventError::Serialize(error.to_string()))
    }

    /// The `type` value of this event.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::SessionStart(_) => "session_start",
            Self::SessionEnd(_) => "session_end",
            Self::Enrolled(_) => "enrolled",
            Self::TaskServed(_) => "task_served",
            Self::Attempt(_) => "attempt",
            Self::LessonResult(_) => "lesson_result",
            Self::ReviewResult(_) => "review_result",
            Self::QuizResult(_) => "quiz_result",
            Self::RemediationTriggered(_) => "remediation_triggered",
            Self::DiagnosticAnswer(_) => "diagnostic_answer",
            Self::DiagnosticPlaced(_) => "diagnostic_placed",
            Self::ProfileReset(_) => "profile_reset",
            Self::Regraded(_) => "regraded",
            Self::AnkiCardCreated(_) => "anki_card_created",
            Self::ConfigChanged(_) => "config_changed",
            Self::CurriculumChanged(_) => "curriculum_changed",
        }
    }

    /// The `ts` of this event.
    #[must_use]
    pub fn ts(&self) -> Timestamp {
        for_each_event!(self, inner => inner.ts)
    }

    /// The `session` of this event.
    #[must_use]
    pub fn session(&self) -> Option<&str> {
        for_each_event!(self, inner => inner.session.as_deref())
    }

    /// The `v` of this event. It is always 1.
    #[must_use]
    pub fn v(&self) -> SchemaVersion {
        for_each_event!(self, inner => inner.v)
    }
}
