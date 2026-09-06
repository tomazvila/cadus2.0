//! The two log events of the integrated task (D-F10, D-F9).
//!
//! # Why the serve is an event
//!
//! An integrated task is one problem with many fields, and a learner can leave it
//! half answered. Without a serve event, a task the learner opened and abandoned
//! would look unseen, and the exposure of D-F9 would count the next serve as the
//! first one. [`IntegratedServed`] records the hand-off, so a refresh and an
//! abandon both leave the same trace.
//!
//! # What the attempt event holds
//!
//! [`IntegratedAttempt`] holds the whole submission and the whole verdict: every
//! step answer as the learner wrote it, the contract each field was decided
//! under, the outcome of each field, the assistance, the credited knowledge
//! points, the item digest, and the reasoning prose. The prose is stored with
//! `graded: false` in the field name itself
//! ([`IntegratedAttempt::reasoning_ungraded`]), because no rule reads it and no
//! rule may later start reading it by accident.
//!
//! Only a DECIDED field credits a skill: `skills_credited` holds the knowledge
//! points of the fields the checker decided correct, and an ungraded field
//! credits nothing (D-F2).

use serde::{Deserialize, Serialize};

use super::kind::AttemptOutcome;
use super::scalar::{SchemaVersion, Timestamp};
use crate::answer::AnswerContract;

/// One field of an integrated submission, as it was answered and decided.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegratedField {
    /// The step id, or `final` for the final answer.
    pub id: String,
    /// The answer the learner wrote, kept verbatim.
    pub answer: String,
    /// The policy the field was decided under (D-F1).
    pub contract: AnswerContract,
    /// The verdict: correct, incorrect, or ungraded (D-F2).
    pub outcome: AttemptOutcome,
    /// True when the learner opened a hint of this field first.
    pub assisted: bool,
    /// The knowledge points this field exercises, as `<topic>/<kp>` keys.
    #[serde(default)]
    pub skills: Vec<String>,
}

/// One integrated task handed to a learner (the exposure of D-F9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegratedServed {
    /// When the hand-off happened.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The task the item was served for.
    pub task_id: String,
    /// The authored item.
    pub item_id: String,
    /// The digest of the served statement.
    pub item_digest: String,
    /// The topic the outcome records against.
    pub topic: String,
    /// The knowledge points the item exercises.
    #[serde(default)]
    pub skills: Vec<String>,
}

/// One graded submission of a whole integrated task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegratedAttempt {
    /// Server-measured whole-item time; historical events have no reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secs: Option<super::Secs>,
    /// Whether this item has uninterrupted usable timing evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing_reliable: Option<bool>,
    /// Frozen policy reading, separate from the mathematical verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timing: Option<crate::timing::SpeedReading>,
    /// When the submission was graded.
    pub ts: Timestamp,
    /// The session id.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The idempotency key of the submission. A repeat writes no second row.
    pub attempt_id: String,
    /// The task the submission belongs to.
    pub task_id: String,
    /// The authored item.
    pub item_id: String,
    /// The digest of the item the learner answered.
    pub item_digest: String,
    /// The topic the outcome records against.
    pub topic: String,
    /// The method the learner chose, when the item offers a choice.
    #[serde(default)]
    pub method: Option<String>,
    /// Whether the chosen method solves the scenario.
    #[serde(default)]
    pub method_correct: Option<bool>,
    /// Every intermediate step, in author order.
    pub steps: Vec<IntegratedField>,
    /// The final answer.
    pub final_field: IntegratedField,
    /// The knowledge points the DECIDED correct fields credit (D-F9).
    #[serde(default)]
    pub skills_credited: Vec<String>,
    /// True when the final answer is correct.
    pub solved: bool,
    /// True when the learner opened any hint of the task.
    pub assisted: bool,
    /// The learner's own words about the model they built.
    ///
    /// The field name carries the rule: it is never graded, and no projector
    /// reads it. It is kept so a human reviewer can read what the learner meant.
    #[serde(default)]
    pub reasoning_ungraded: Option<String>,
    /// Frozen server verdict for idempotent response replay; absent in older events.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<Box<crate::integrated::IntegratedGrade>>,
}

/// One hint rung actually revealed by the server, scoped to its served task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegratedHintRevealed {
    /// When the server committed the reveal.
    pub ts: Timestamp,
    /// The session that owns the task.
    #[serde(default)]
    pub session: Option<String>,
    /// The schema version.
    #[serde(default = "SchemaVersion::current")]
    pub v: SchemaVersion,
    /// The task in that session.
    pub task_id: String,
    /// The authored item digest; edited content has separate assistance state.
    pub item_digest: String,
    /// The step id, or `final`.
    pub field: String,
    /// The zero-based rung the server returned.
    pub index: usize,
}
