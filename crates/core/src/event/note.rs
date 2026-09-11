//! The session, enrollment, remediation, diagnostic, reset, correction and
//! change-note bodies.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::{
    EnrollReason, RegradedAttempt, SchemaVersion, Secs, Slug, Timestamp, Weight, WorkQuality,
};

/// A change note: the envelope, a summary, and an optional git reference. The
/// fold treats it as a no-op.
macro_rules! change_note {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            /// When the event happened.
            pub ts: Timestamp,
            /// The session id.
            #[serde(default)]
            pub session: Option<String>,
            /// The schema version.
            #[serde(default = "SchemaVersion::current")]
            pub v: SchemaVersion,
            /// What changed.
            pub summary: String,
            /// The git reference of the change.
            #[serde(default)]
            pub git_ref: Option<String>,
        }
    };
}

change_note! {
    /// The scheduler config changed. The fold treats it as a no-op.
    ConfigChanged
}

change_note! {
    /// The curriculum changed. The fold treats it as a no-op.
    CurriculumChanged
}

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
    #[serde(default = "SchemaVersion::current")]
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
    #[serde(default = "SchemaVersion::current")]
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
    #[serde(default = "SchemaVersion::current")]
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
    #[serde(default = "SchemaVersion::current")]
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
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The topic the question tested.
    pub topic: Slug,
    /// Whether the answer was correct.
    pub correct: bool,
    /// The time the learner took.
    pub secs: Secs,
    /// How much the answer counts, from 0.0 to 1.0.
    pub weight: Weight,
    /// The explicit third outcome. Legacy rows derive a verdict from `correct`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<super::AttemptOutcome>,
    /// The served identity, for an audit of an unmarked response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem_id: Option<String>,
    /// The original response, preserved in the private event log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted: Option<String>,
    /// The private problem and its captured policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<super::AttemptProblem>,
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
    #[serde(default = "SchemaVersion::current")]
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
    #[serde(default = "SchemaVersion::current")]
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
    #[serde(default = "SchemaVersion::current")]
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
    #[serde(default = "SchemaVersion::current")]
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
