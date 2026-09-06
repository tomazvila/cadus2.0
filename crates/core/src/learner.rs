//! The learner model: the rebuildable derived state of the fold (D3, spec section 3).
//!
//! The wire shape is the 1.0 shape byte for byte, because the parity oracle compares
//! the serialized model against 1.0's:
//!
//! - `repNum` and `memoryBase` keep their camelCase spellings. They are load-bearing.
//! - `explicit_only` is dead in 1.0: nothing writes or reads it, and it is always
//!   false. The field stays for shape parity. Never write it.
//! - [`LearnerModel::through_seq`] is new in 2.0 (D4). Serde SKIPS it, so it never
//!   enters the parity blob; the store row `learner_models.through_seq` carries it.
//!
//! [`LearnerModel::parity_blob`] builds the compared bytes: canonical JSON with
//! `built_from_ts` removed, because `built_from_ts` is wall-clock `now` (trap T10).

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha1::{Digest as _, Sha1};
use sha2::Sha256;

use crate::event::{EventError, KpProgress, Slug, Timestamp, TopicStatus};
use crate::retention::state::RetentionState;

/// The number of hex characters of the SHA-1 digest that [`problem_text_hash`] keeps.
const PROBLEM_TEXT_HASH_LEN: usize = 12;

/// The recent-problem window kept on each [`TopicState`] (`LAST_PROBLEMS_WINDOW`).
pub const LAST_PROBLEMS_WINDOW: usize = 20;

/// The count of ungraded attempts the recovery list keeps (D-F2).
pub const UNGRADED_WINDOW: usize = 20;

/// A stable, unsalted short digest of a problem text, for the dedup window (trap T17).
///
/// This is the ONE definition of "the digest that identifies a served problem".
/// Everything that reads or writes [`TopicState::last_problems`] calls this and never
/// re-derives it: a second spelling makes digests that never compare equal, which
/// turns the near-duplicate guard into a silent no-op.
///
/// There is NO whitespace normalization. `" padded "` and `"padded"` hash differently.
#[must_use]
pub fn problem_text_hash(text: &str) -> String {
    let digest = Sha1::digest(text.as_bytes());
    let mut hex = String::with_capacity(PROBLEM_TEXT_HASH_LEN);
    for byte in digest.iter().take(PROBLEM_TEXT_HASH_LEN.div_ceil(2)) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex.truncate(PROBLEM_TEXT_HASH_LEN);
    hex
}

/// The first 16 hex characters of the SHA-256 of `preimage` (trap T16).
///
/// [`crate::config::Config::config_hash`] is the one caller.
#[must_use]
pub(crate) fn short_sha256(preimage: &str) -> String {
    let digest = <Sha256 as sha2::Digest>::digest(preimage.as_bytes());
    let mut hex = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// Per-topic FIRe state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicState {
    /// The scheduling status of the topic.
    #[serde(default)]
    pub status: TopicStatus,
    /// The repetition number that drives the review interval.
    #[serde(rename = "repNum", default)]
    pub rep_num: f64,
    /// The memory strength at `t0`, before decay.
    #[serde(rename = "memoryBase", default)]
    pub memory_base: f64,
    /// When the topic was last practiced. `None` means never.
    #[serde(default)]
    pub t0: Option<Timestamp>,
    /// The current review interval, in days.
    #[serde(default)]
    pub interval_days: f64,
    /// The learner's ability on the topic, from 0.0 to 1.0.
    #[serde(default)]
    pub ability: f64,
    /// How fast the topic accrues repetitions, from 0.33 to 3.0.
    #[serde(default = "default_speed")]
    pub speed: f64,
    /// Whether the placement credit on this topic is conditional.
    #[serde(default)]
    pub conditional: bool,
    /// Dead in 1.0. Nothing writes or reads it. Kept for shape parity only.
    #[serde(default)]
    pub explicit_only: bool,
    /// The recent problem digests, oldest first, at most [`LAST_PROBLEMS_WINDOW`].
    #[serde(default)]
    pub last_problems: Vec<String>,
    /// The progress of each knowledge point of the topic.
    #[serde(default)]
    pub kp_progress: BTreeMap<String, KpProgress>,
    /// The count of ungraded attempts on the topic (D-F2). NEW IN 2.0.
    ///
    /// An ungraded attempt moves no other field of this state. The writer skips a
    /// zero count, so a model with no ungraded attempt keeps the 1.0 shape.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ungraded_attempts: u32,
}

/// Whether a count is zero. It keeps a zero count out of the wire shape.
const fn is_zero(count: &u32) -> bool {
    *count == 0
}

/// The initial `speed` of a topic state.
const fn default_speed() -> f64 {
    1.0
}

impl Default for TopicState {
    fn default() -> Self {
        Self {
            status: TopicStatus::Untouched,
            rep_num: 0.0,
            memory_base: 0.0,
            t0: None,
            interval_days: 0.0,
            ability: 0.0,
            speed: default_speed(),
            conditional: false,
            explicit_only: false,
            last_problems: Vec::new(),
            kp_progress: BTreeMap::new(),
            ungraded_attempts: 0,
        }
    }
}

impl TopicState {
    /// Whether this state still equals a default state.
    ///
    /// `finalize` drops every topic that equals a default state, so the emitted map
    /// holds only topics the fold actually touched (spec section 3).
    #[must_use]
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}

/// The XP totals of the learner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XpState {
    /// The XP of the whole log, rounded to an integer.
    #[serde(default)]
    pub total: i64,
    /// The XP of the reference local day, rounded to an integer.
    #[serde(default)]
    pub today: i64,
    /// The daily XP goal, from the profile.
    #[serde(default = "default_xp_goal")]
    pub goal: i64,
    /// The consecutive days that met the goal, counting back from the reference day.
    #[serde(default)]
    pub streak_days: i64,
}

/// The initial `goal` of the XP state.
const fn default_xp_goal() -> i64 {
    40
}

impl Default for XpState {
    fn default() -> Self {
        Self {
            total: 0,
            today: 0,
            goal: default_xp_goal(),
            streak_days: 0,
        }
    }
}

/// The quiz cadence of the learner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizState {
    /// The local date of the last quiz.
    #[serde(default)]
    pub last_at: Option<NaiveDate>,
    /// The XP earned since the last quiz, rounded to an integer.
    #[serde(default)]
    pub xp_since: i64,
    /// Whether the last quiz scored below the retake threshold.
    #[serde(default)]
    pub retake_pending: bool,
}

/// The pace of the learner over the trailing 28 local days.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VelocityState {
    /// The mean XP per day over the window, rounded to 4 decimal places.
    #[serde(default)]
    pub xp_per_day_28d: f64,
    /// The mean topics per week over the window, rounded to 4 decimal places.
    #[serde(default)]
    pub topics_per_week_28d: f64,
    /// The fraction of the enrolled course that is mastered, rounded to 4 places.
    #[serde(default)]
    pub course_progress: f64,
    /// The projected completion date of the enrolled course.
    #[serde(default)]
    pub eta: Option<NaiveDate>,
}

/// One pending remediation, in trigger order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingRemediation {
    /// The kind of remediation.
    pub kind: String,
    /// The topics that still need remediation.
    #[serde(default)]
    pub targets: Vec<Slug>,
}

/// One ungraded attempt of the recovery path (D-F2). NEW IN 2.0.
///
/// The admin list reads it, and a `regraded` event closes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UngradedAttempt {
    /// The `attempt_id` of the ungraded attempt.
    pub attempt_id: String,
    /// The topic the attempt practiced.
    pub topic: String,
    /// Why the attempt has no verdict.
    pub reason: String,
}

/// The rebuildable derived state of one learner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearnerModel {
    /// The wall-clock `now` of the build. It is NOT a cursor, and it is excluded from
    /// every parity comparison (trap T10).
    #[serde(default)]
    pub built_from_ts: Option<Timestamp>,
    /// The touched topics, keyed by topic id and ordered by it.
    #[serde(default)]
    pub topics: BTreeMap<String, TopicState>,
    /// The XP totals.
    #[serde(default)]
    pub xp: XpState,
    /// The quiz cadence.
    #[serde(default)]
    pub quiz: QuizState,
    /// The pace over the trailing window.
    #[serde(default)]
    pub velocity: VelocityState,
    /// The pending remediations, in trigger order.
    #[serde(default)]
    pub pending_remediation: Vec<PendingRemediation>,
    /// The last [`UNGRADED_WINDOW`] ungraded attempts, oldest first (D-F2). NEW IN 2.0.
    ///
    /// The writer skips an empty list, so a model with no ungraded attempt keeps the
    /// 1.0 shape.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ungraded: Vec<UngradedAttempt>,
    /// What the delayed retention probes answered (D-F11). NEW IN 2.0.
    ///
    /// The fold builds it from `retention_probe` events and from nothing else, and
    /// the writer skips an empty state, so every model of a log without a probe
    /// keeps its 1.0 shape and its parity bytes.
    #[serde(default, skip_serializing_if = "RetentionState::is_empty")]
    pub retention: RetentionState,
    /// The `config_hash` the model was built with. It detects config drift.
    #[serde(default)]
    pub config_hash: Option<String>,
    /// The `PROJECTOR_VERSION` the model was built with. It detects a fold change.
    #[serde(default)]
    pub projector_version: Option<i64>,
    /// The event `seq` this model was folded through (D4). NEW IN 2.0.
    ///
    /// Serde skips it, so it enters neither the wire form nor the parity blob. The
    /// store column `learner_models.through_seq` carries the value.
    #[serde(skip)]
    pub through_seq: Option<i64>,
}

impl LearnerModel {
    /// Read a learner model from JSON text.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Json`] for malformed text, an unknown key, or an
    /// out-of-range value.
    pub fn from_json(text: &str) -> Result<Self, EventError> {
        serde_json::from_str(text).map_err(|error| EventError::Json(error.to_string()))
    }

    /// The bytes the parity oracle compares (spec section 9).
    ///
    /// This is [`crate::projector::canonical_blob`] and nothing else. The blob has ONE
    /// definition, the way `problem_text_hash` has one: a second spelling of the float
    /// text or the key order gives bytes that never compare equal, and the digest then
    /// diverges silently.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Serialize`] when a timestamp is outside the
    /// representable range.
    pub fn parity_blob(&self) -> Result<String, EventError> {
        crate::projector::canonical_blob(self)
            .map_err(|error| EventError::Serialize(error.to_string()))
    }

    /// The SHA-256 of [`LearnerModel::parity_blob`], as lowercase hex.
    ///
    /// # Errors
    ///
    /// Returns [`EventError::Serialize`] when the blob cannot be built.
    pub fn parity_digest(&self) -> Result<String, EventError> {
        let blob = self.parity_blob()?;
        let digest = <Sha256 as sha2::Digest>::digest(blob.as_bytes());
        let mut hex = String::with_capacity(64);
        for byte in digest {
            hex.push_str(&format!("{byte:02x}"));
        }
        Ok(hex)
    }
}
