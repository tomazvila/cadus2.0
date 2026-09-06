//! The scheduler constants and their drift digest (spec section 9, trap T16).
//!
//! [`Config`] mirrors 1.0's `config.yaml`. The default values ARE the 1.0 defaults,
//! and `tests/test_model.py` pins that `config.yaml` equals `Config()`.
//!
//! [`Config::config_hash`] is the drift detector the learner model carries. 1.0
//! computes it as `sha256(cfg.model_dump_json())[:16]`, so the preimage is pydantic's
//! exact JSON: field DECLARATION order, compact separators, `[0.33,3.0]` for the
//! tuple, and `2.0` for the int literal `2` inside a `list[float]`. The field order
//! of every struct here therefore matches 1.0's declaration order, and serde writes
//! fields in declaration order. Do not sort these fields.

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

use crate::learner::short_sha256;
use crate::projector::PassRule;
use crate::retention::policy::{PolicyVersion, RetentionConfig};

/// A configuration value the core refuses.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    /// `lesson.kp_pass` holds no term.
    #[error("`lesson.kp_pass` is empty; it needs one term such as `2consec`")]
    EmptyPassRule,
    /// One term of `lesson.kp_pass` is outside the grammar of [`PassRule`].
    #[error("`lesson.kp_pass` term `{term}` is not `<n>consec` or `<k>of<m>`")]
    PassRuleTerm {
        /// The term the parser refused.
        term: String,
    },
}

/// The FIRe engine constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FireConfig {
    /// The memory level at or below which a review is due.
    pub due_threshold: f64,
    /// The review interval, in days, per whole repetition number.
    pub interval_table: Vec<f64>,
    /// The lowest early-credit factor of a pass.
    pub early_floor: f64,
    /// The largest overdue decay factor of a miss.
    pub decay_cap: f64,
    /// The smallest propagated credit or penalty that still lands.
    pub min_credit: f64,
    /// The edge weight at which a review knocks out another review.
    pub knockout_weight: f64,
    /// The speed below which a topic absorbs no propagated credit.
    pub explicit_speed_threshold: f64,
    /// The lowest and highest speed of a topic.
    pub speed_clamp: (f64, f64),
}

impl Default for FireConfig {
    fn default() -> Self {
        Self {
            due_threshold: 0.5,
            interval_table: vec![2.0, 4.5, 10.0, 21.0, 45.0, 100.0, 220.0, 480.0],
            early_floor: 0.15,
            decay_cap: 3.0,
            min_credit: 0.05,
            knockout_weight: 0.8,
            explicit_speed_threshold: 1.0,
            speed_clamp: (0.33, 3.0),
        }
    }
}

/// The ability-update constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AbilityConfig {
    /// The exponential moving average rate of the ability update.
    pub ewma_alpha: f64,
}

impl Default for AbilityConfig {
    fn default() -> Self {
        Self { ewma_alpha: 0.3 }
    }
}

/// The 1.0 default of `lesson.kp_pass` (spec section 9).
const DEFAULT_KP_PASS: &str = "2consec|3of4";

/// The lesson constants.
///
/// `kp_pass` is private, and [`LessonConfig::new`] is the one constructor, because
/// `pass_rule` is the parse of `kp_pass`: a writable `kp_pass` field leaves the two out
/// of step and the gate then reads a stale rule.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LessonConfig {
    /// The rule that passes a knowledge point.
    kp_pass: String,
    /// The number of misses that fails a lesson.
    pub fail_after: i64,
    /// The days a failed topic waits before a retry.
    pub retry_delay_days: i64,
    /// The parsed form of `kp_pass`, built once at construction (D-F7).
    ///
    /// It never serializes: the config hash preimage holds the 1.0 field set and
    /// nothing more (trap T16).
    #[serde(skip_serializing)]
    pass_rule: PassRule,
}

/// The stored fields of [`LessonConfig`].
///
/// The [`Deserialize`] of [`LessonConfig`] reads this struct and then parses `kp_pass`,
/// so a bad rule string fails the config load with the term named.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LessonFields {
    kp_pass: String,
    fail_after: i64,
    retry_delay_days: i64,
}

impl<'de> Deserialize<'de> for LessonConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let fields = LessonFields::deserialize(deserializer)?;
        Self::new(&fields.kp_pass, fields.fail_after, fields.retry_delay_days)
            .map_err(serde::de::Error::custom)
    }
}

impl LessonConfig {
    /// Build the lesson constants and parse `kp_pass` once.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when `kp_pass` is outside the grammar of [`PassRule`].
    pub fn new(kp_pass: &str, fail_after: i64, retry_delay_days: i64) -> Result<Self, ConfigError> {
        Ok(Self {
            kp_pass: kp_pass.to_owned(),
            fail_after,
            retry_delay_days,
            pass_rule: PassRule::parse(kp_pass)?,
        })
    }

    /// The rule text, as `config.yaml` spells it.
    #[must_use]
    pub fn kp_pass(&self) -> &str {
        &self.kp_pass
    }

    /// The parsed rule the knowledge-point gate reads.
    #[must_use]
    pub const fn pass_rule(&self) -> &PassRule {
        &self.pass_rule
    }
}

impl Default for LessonConfig {
    fn default() -> Self {
        Self {
            kp_pass: DEFAULT_KP_PASS.to_owned(),
            fail_after: 5,
            retry_delay_days: 1,
            pass_rule: PassRule::default(),
        }
    }
}

/// The review constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewConfig {
    /// The number of questions in a review.
    pub questions: i64,
    /// The weighted score that passes a review.
    pub pass_weighted: f64,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self {
            questions: 4,
            pass_weighted: 0.65,
        }
    }
}

/// The session-composition constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectorConfig {
    /// The smallest share of a session that lessons take.
    pub lesson_ratio_min: f64,
    /// The reviews served before a lesson is forced.
    pub max_reviews_per_lesson: i64,
}

impl Default for SelectorConfig {
    fn default() -> Self {
        Self {
            lesson_ratio_min: 0.25,
            max_reviews_per_lesson: 3,
        }
    }
}

/// The quiz cadence constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuizConfig {
    /// The days between quizzes.
    pub cadence_days: i64,
    /// The XP between quizzes.
    pub cadence_xp: i64,
    /// The number of questions in a quiz.
    pub questions: i64,
    /// The score below which a retake becomes pending.
    pub retake_below: f64,
}

impl Default for QuizConfig {
    fn default() -> Self {
        Self {
            cadence_days: 7,
            cadence_xp: 200,
            questions: 8,
            retake_below: 0.8,
        }
    }
}

/// The placement diagnostic constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagConfig {
    /// The largest number of diagnostic questions.
    pub max_questions: i64,
    /// The graph radius a diagnostic answer covers.
    pub coverage_radius: i64,
    /// The credit a sibling topic gets from an answer.
    pub sibling_credit: f64,
    /// The largest conditional balance.
    pub conditional_max: f64,
}

impl Default for DiagConfig {
    fn default() -> Self {
        Self {
            max_questions: 40,
            coverage_radius: 3,
            sibling_credit: 0.5,
            conditional_max: 1.0,
        }
    }
}

/// The XP multiplier of each work-quality tier.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XpTiers {
    /// The multiplier of `perfect`.
    pub perfect: f64,
    /// The multiplier of `nearly_perfect`.
    pub nearly_perfect: f64,
    /// The multiplier of `passable`.
    pub passable: f64,
    /// The multiplier of `nearly_passable`.
    pub nearly_passable: f64,
    /// The multiplier of `poor`.
    pub poor: f64,
    /// The multiplier of `blowoff`. It is negative on purpose.
    pub blowoff: f64,
}

impl Default for XpTiers {
    fn default() -> Self {
        Self {
            perfect: 1.3,
            nearly_perfect: 1.0,
            passable: 0.85,
            nearly_passable: 0.3,
            poor: 0.0,
            blowoff: -0.5,
        }
    }
}

/// The XP constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct XpConfig {
    /// The daily XP goal that carries the streak.
    pub daily_goal: i64,
    /// The multiplier of each work-quality tier.
    pub tiers: XpTiers,
}

impl Default for XpConfig {
    fn default() -> Self {
        Self {
            daily_goal: 40,
            tiers: XpTiers::default(),
        }
    }
}

/// The speed-drill constants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DrillConfig {
    /// The number of questions in a drill.
    pub questions: i64,
    /// The seconds a drill question targets.
    pub target_secs: i64,
}

impl Default for DrillConfig {
    fn default() -> Self {
        Self {
            questions: 20,
            target_secs: 6,
        }
    }
}

/// The mastery-claim constants (D-F6). NEW IN 2.0.
///
/// The field is absent from the 1.0 `config.yaml`, so [`Config::hash_preimage`]
/// skips it and the drift digest of trap T16 keeps its 1.0 value. The fold
/// never reads this section: it gates the SELECTOR and the progress display
/// only, and a replay of the same log always gives the same model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MasteryConfig {
    /// Whether the course owes each inferred topic one confirmation item.
    ///
    /// `true` is the D-F6 rule: course completion and the course-progress
    /// percentage count practiced topics, and the selector serves confirmation
    /// items. `false` restores the 1.0 rule, and the 1.0 parity fixtures run
    /// with it.
    pub confirm_inferred: bool,
    /// The largest number of confirmation items one session serves.
    pub max_per_session: usize,
}

impl Default for MasteryConfig {
    fn default() -> Self {
        Self {
            confirm_inferred: true,
            max_per_session: 2,
        }
    }
}

/// The readiness gate of D-F5.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessConfig {
    /// Whether the selector serves a lesson only when its knowledge point is
    /// teachable, practicable and assessable, and a review or a quiz only when
    /// its topic is practicable.
    ///
    /// The default is `true`. A parity test that composes over a tree with no
    /// authored content sets it to `false`.
    pub enforce: bool,
}

impl Default for ReadinessConfig {
    fn default() -> Self {
        Self { enforce: true }
    }
}

/// The error tags a grader may assign, in 1.0 order.
///
/// `blank_answer` is SERVER-assigned and never model-assigned: the server stamps it
/// on a blank submission, so the grader is never invited to claim an answer was blank.
#[must_use]
pub fn default_error_tags() -> Vec<String> {
    [
        "sign-error",
        "arithmetic-slip",
        "algebra-slip",
        "wrong-method",
        "formula-recall",
        "misread-problem",
        "incomplete",
        "notation",
        "units",
        "timing-unreliable",
        "blowoff",
        "blank_answer",
    ]
    .iter()
    .map(|tag| (*tag).to_owned())
    .collect()
}

/// The scheduler constants, mirroring 1.0's `config.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The FIRe engine constants.
    pub fire: FireConfig,
    /// The ability-update constants.
    pub ability: AbilityConfig,
    /// The lesson constants.
    pub lesson: LessonConfig,
    /// The review constants.
    pub review: ReviewConfig,
    /// The session-composition constants.
    pub selector: SelectorConfig,
    /// The quiz cadence constants.
    pub quiz: QuizConfig,
    /// The placement diagnostic constants.
    pub diag: DiagConfig,
    /// The XP constants.
    pub xp: XpConfig,
    /// The speed-drill constants.
    pub drill: DrillConfig,
    /// The readiness gate of D-F5.
    ///
    /// The field is NOT serialized, so it stays out of the
    /// [`Config::hash_preimage`] and the drift digest keeps the 1.0 value
    /// (trap T16). The reason is the contract of that digest: it detects drift
    /// of the 1.0 SCHEDULER CONSTANTS, and every stored `learner_models` row
    /// and every parity fixture carries `797575e985c12149` for the defaults.
    /// The readiness gate is a serve-eligibility policy of 2.0 and no
    /// scheduling constant, so a change to it must not invalidate a projection.
    /// D-F12 adds `policy_version`, which is where a 2.0 policy is versioned.
    #[serde(default, skip_serializing)]
    pub readiness: ReadinessConfig,
    /// The error tags a grader may assign.
    pub error_tags: Vec<String>,
    /// The IANA time zone of the day boundary. `None` means the profile's zone, and
    /// a profile with no zone means UTC (trap T9).
    pub timezone: Option<String>,
    /// The mastery-claim constants (D-F6). It stays OUT of the hash preimage.
    #[serde(default, skip_serializing)]
    pub mastery: MasteryConfig,
    /// The delayed-retention constants (D-F11). It stays OUT of the hash preimage.
    #[serde(default, skip_serializing)]
    pub retention: RetentionConfig,
    /// The version stamp of the 2.0 policy set (D-F12).
    ///
    /// It stays OUT of the hash preimage for the reason `readiness` states: the
    /// 1.0 digest detects drift of the 1.0 SCHEDULER CONSTANTS, and a stored
    /// projection must survive a 2.0 policy bump. [`Config::policy_digest`] is
    /// the digest of the 2.0 policies.
    #[serde(default, skip_serializing)]
    pub policy_version: PolicyVersion,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fire: FireConfig::default(),
            ability: AbilityConfig::default(),
            lesson: LessonConfig::default(),
            review: ReviewConfig::default(),
            selector: SelectorConfig::default(),
            quiz: QuizConfig::default(),
            diag: DiagConfig::default(),
            xp: XpConfig::default(),
            drill: DrillConfig::default(),
            readiness: ReadinessConfig::default(),
            error_tags: default_error_tags(),
            timezone: None,
            mastery: MasteryConfig::default(),
            retention: RetentionConfig::default(),
            policy_version: PolicyVersion::default(),
        }
    }
}

impl Config {
    /// The exact preimage 1.0 hashes: `cfg.model_dump_json()`.
    ///
    /// Compact separators, field declaration order, and no sorted keys. The float
    /// spellings matter: `interval_table` writes `2.0` for the int literal `2`,
    /// because pydantic coerces it into a `list[float]`, and `speed_clamp` writes
    /// `[0.33,3.0]` for the tuple.
    ///
    /// # Errors
    ///
    /// Returns the `serde_json` error when the config does not serialize.
    pub fn hash_preimage(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// The drift digest of the config: the first 16 hex characters of the SHA-256 of
    /// [`Config::hash_preimage`] (trap T16).
    ///
    /// A default config hashes to `797575e985c12149`.
    ///
    /// # Errors
    ///
    /// Returns the `serde_json` error when the config does not serialize.
    pub fn config_hash(&self) -> Result<String, serde_json::Error> {
        self.hash_preimage().map(|preimage| short_sha256(&preimage))
    }

    /// The preimage of the 2.0 policy digest (D-F12).
    ///
    /// It names every VERSIONED number the 1.0 hash preimage leaves out, plus the
    /// two 1.0 numbers D-F12 versions: the review interval table and the ability
    /// weight. The text is compact JSON in a fixed key order.
    #[must_use]
    pub fn policy_preimage(&self) -> String {
        let delays: Vec<String> = self.retention.delays().iter().map(u32::to_string).collect();
        let intervals: Vec<String> = self
            .fire
            .interval_table
            .iter()
            .map(|days| format!("{days:?}"))
            .collect();
        format!(
            "{{\"version\":{},\"kp_pass\":\"{}\",\"intervals\":[{}],\"ewma_alpha\":{:?},\
             \"due_threshold\":{:?},\"probe_delays\":[{}],\"probes_per_session\":{},\
             \"readiness\":{},\"confirm_inferred\":{}}}",
            self.policy_version.version,
            self.lesson.kp_pass(),
            intervals.join(","),
            self.ability.ewma_alpha,
            self.fire.due_threshold,
            delays.join(","),
            self.retention.max_per_session,
            self.readiness.enforce,
            self.mastery.confirm_inferred,
        )
    }

    /// The drift digest of the 2.0 policy set: the first 16 hex characters of the
    /// SHA-256 of [`Config::policy_preimage`] (D-F12).
    ///
    /// A report prints it beside the policy version, so a reader always knows which
    /// numbers produced a retention row.
    #[must_use]
    pub fn policy_digest(&self) -> String {
        short_sha256(&self.policy_preimage())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_1_0_config_hash_survives_the_2_0_policy_fields() {
        let cfg = Config::default();
        assert_eq!(cfg.config_hash().expect("a hash"), "797575e985c12149");
        assert!(!cfg.hash_preimage().expect("a preimage").contains("policy"));
        assert!(
            !cfg.hash_preimage()
                .expect("a preimage")
                .contains("probe_delays_days")
        );
    }

    #[test]
    fn a_probe_delay_change_moves_the_policy_digest_and_not_the_config_hash() {
        let base = Config::default();
        let mut changed = Config::default();
        changed.retention.probe_delays_days = vec![7, 30];
        assert_eq!(
            base.config_hash().expect("a hash"),
            changed.config_hash().expect("a hash")
        );
        assert_ne!(base.policy_digest(), changed.policy_digest());
    }

    #[test]
    fn the_policy_version_bump_moves_the_policy_digest() {
        let base = Config::default();
        let mut changed = Config::default();
        changed.policy_version.version += 1;
        assert_ne!(base.policy_digest(), changed.policy_digest());
    }

    #[test]
    fn the_policy_preimage_names_every_versioned_number() {
        let preimage = Config::default().policy_preimage();
        for key in [
            "version",
            "kp_pass",
            "intervals",
            "ewma_alpha",
            "due_threshold",
            "probe_delays",
            "probes_per_session",
            "readiness",
            "confirm_inferred",
        ] {
            assert!(preimage.contains(key), "`{key}` is missing from {preimage}");
        }
    }
}
