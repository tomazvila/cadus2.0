//! The 1.0 event log on the wire (C2, spec section 2).
//!
//! Every state change of a learner is an event, and the events are the only input of
//! the fold. The shapes here are the 1.0 shapes byte for byte, because 2.0 must read
//! and write the same log:
//!
//! - [`Event`] is a tagged union on the `type` key, with 17 members.
//! - Every member carries the envelope `ts`, `session`, and `v`.
//! - Every member forbids an unknown key, as 1.0's `extra="forbid"` does.
//! - `v` is `1` or `2`. 2.0 holds ONE shim: a v1 `attempt` row reads as v2 through
//!   [`Attempt::normalize`], which derives `outcome` from `correct`. An original row
//!   is never rewritten (C2), so the reader writes back the `v` it read.
//! - [`Timestamp`] parses RFC 3339, reads a naive value as UTC (trap T8), and writes
//!   `...Z` with second precision when the microsecond part is zero.
//!
//! [`Event::from_json`] is the reader and [`Event::to_canonical_json`] is the writer.
//! The canonical form sorts every key, uses compact separators, and leaves non-ASCII
//! text unescaped, which is the form the fixtures hold.

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod body;
mod kind;
mod note;
mod scalar;

pub use body::{
    Attempt, AttemptProblem, LessonResult, QuizResult, QuizTopicResult, RegradedAttempt,
    RetentionProbe, ReviewResult, ServedProblem, TaskServed,
};
pub use kind::{
    AnswerKind, AttemptOutcome, EnrollReason, Exposure, ItemSource, KpProgress, TaskType,
    TopicStatus, WorkQuality,
};
pub use note::{
    AnkiCardCreated, ConfigChanged, CurriculumChanged, DiagnosticAnswer, DiagnosticPlaced,
    Enrolled, ProfileReset, Regraded, RemediationTriggered, SessionEnd, SessionStart,
};
pub use scalar::{PositiveSecs, SchemaVersion, Secs, Slug, Timestamp, Weight};

/// The schema version this build writes (D-F2, D-F9).
pub const SCHEMA_VERSION: i64 = 2;

/// The first schema version. A row at this version reads through the shim.
pub const SCHEMA_VERSION_V1: i64 = 1;

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
    /// One delayed retention probe (D-F11).
    #[serde(rename = "retention_probe")]
    RetentionProbe(RetentionProbe),
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
            Event::RetentionProbe($inner) => $body,
        }
    };
}

impl Event {
    /// The 17 `type` values, in the order this module declares them.
    pub const TYPE_NAMES: [&'static str; 17] = [
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
        "retention_probe",
    ];

    /// Read one event from JSON text.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] for malformed text, an unknown `type`, an unknown
    /// key, a missing required key, an out-of-range value, or a `v` other than 1 or 2.
    pub fn from_json(text: &str) -> Result<Self, EventError> {
        let mut event: Self =
            serde_json::from_str(text).map_err(|error| EventError::Json(error.to_string()))?;
        event.normalize();
        Ok(event)
    }

    /// Apply the v1 shim to a body that serde built (D-F2).
    ///
    /// [`Event::from_json`] calls it, and so does every other reader that builds an
    /// event through serde — the store decodes the `jsonb` payload straight into this
    /// type. The step is idempotent for attempts and diagnostic answers.
    pub fn normalize(&mut self) {
        match self {
            Self::Attempt(body) => body.normalize(),
            Self::DiagnosticAnswer(body) => {
                if let Some(outcome) = &body.outcome {
                    body.correct = *outcome == AttemptOutcome::Correct;
                    if outcome.is_ungraded() {
                        body.weight = Weight::default();
                    }
                }
            }
            _ => {}
        }
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
        let value = serde_json::to_value(self).map_err(serialize_error)?;
        serde_json::to_string(&value).map_err(serialize_error)
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
            Self::RetentionProbe(_) => "retention_probe",
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

    /// The `v` of this event. It is 1 on a row this build did not write.
    #[must_use]
    pub fn v(&self) -> SchemaVersion {
        for_each_event!(self, inner => inner.v)
    }
}

/// The error of a canonical form that does not build.
fn serialize_error(error: serde_json::Error) -> EventError {
    EventError::Serialize(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The smallest valid body of each of the 17 types, in declaration order.
    const MINIMAL: [&str; 17] = [
        r#"{"type":"session_start","ts":"2026-03-02T09:00:00Z","session":"s"}"#,
        r#"{"type":"session_end","ts":"2026-03-02T09:00:00Z","session":"s"}"#,
        r#"{"type":"enrolled","ts":"2026-03-02T09:00:00Z","session":"s","course":"c"}"#,
        r#"{"type":"task_served","ts":"2026-03-02T09:00:00Z","session":"s","task_id":"t","task_type":"lesson"}"#,
        r#"{"type":"attempt","ts":"2026-03-02T09:00:00Z","session":"s","attempt_id":"a","task_id":"t","topic":"x","task_type":"lesson","problem":{"text":"p","expected":"1"},"given_answer":"1","correct":true,"secs":3,"work_quality":"perfect"}"#,
        r#"{"type":"lesson_result","ts":"2026-03-02T09:00:00Z","session":"s","topic":"x","passed":true,"quality_tier":"perfect"}"#,
        r#"{"type":"review_result","ts":"2026-03-02T09:00:00Z","session":"s","topic":"x","passed":true,"weighted_score":1.0,"quality_tier":"perfect"}"#,
        r#"{"type":"quiz_result","ts":"2026-03-02T09:00:00Z","session":"s","quiz_id":"q","score":1.0}"#,
        r#"{"type":"remediation_triggered","ts":"2026-03-02T09:00:00Z","session":"s","kind":"k","source_topic":"x"}"#,
        r#"{"type":"diagnostic_answer","ts":"2026-03-02T09:00:00Z","session":"s","topic":"x","correct":true,"secs":3,"weight":1.0}"#,
        r#"{"type":"diagnostic_placed","ts":"2026-03-02T09:00:00Z","session":"s"}"#,
        r#"{"type":"profile_reset","ts":"2026-03-02T09:00:00Z","session":"s"}"#,
        r#"{"type":"regraded","ts":"2026-03-02T09:00:00Z","session":"s","task_id":"t","topic":"x","reason":"r"}"#,
        r#"{"type":"anki_card_created","ts":"2026-03-02T09:00:00Z","session":"s","topic":"x","deck":"d","note_id":1,"front_hash":"h"}"#,
        r#"{"type":"config_changed","ts":"2026-03-02T09:00:00Z","session":"s","summary":"m"}"#,
        r#"{"type":"curriculum_changed","ts":"2026-03-02T09:00:00Z","session":"s","summary":"m"}"#,
        r#"{"type":"retention_probe","ts":"2026-03-02T09:00:00Z","session":"s","kp":"kp1","topic":"x","delay_days":7,"outcome":"correct","secs":9}"#,
    ];

    #[test]
    fn every_type_reads_names_itself_and_writes_back() {
        for (text, name) in MINIMAL.iter().zip(Event::TYPE_NAMES) {
            let event = Event::from_json(text).expect("the minimal body reads");
            assert_eq!(event.type_name(), name);
            assert_eq!(event.session(), Some("s"));
            assert_eq!(event.v(), SchemaVersion::current());
            assert_eq!(event.ts().micros(), 1_772_442_000_000_000);
            let written = event.to_canonical_json().expect("the body writes");
            assert_eq!(Event::from_json(&written).expect("the form reads"), event);
        }
        assert!(Event::from_json("{").is_err());
    }
}
