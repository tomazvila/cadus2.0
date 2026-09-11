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

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::learner::short_sha256;
use crate::retention::policy::{PolicyVersion, RetentionConfig};

/// A configuration value the core refuses.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConfigError {
    /// `lesson.kp_pass` holds no term.
    #[error("`lesson.kp_pass` is empty; it needs one term such as `2consec`")]
    EmptyPassRule,
    /// One term of `lesson.kp_pass` is outside the grammar of
    /// [`crate::projector::PassRule`].
    #[error("`lesson.kp_pass` term `{term}` is not `<n>consec` or `<k>of<m>`")]
    PassRuleTerm {
        /// The term the parser refused.
        term: String,
    },
}

mod sections;

pub use sections::{
    AbilityConfig, DiagConfig, DrillConfig, FireConfig, LessonConfig, MasteryConfig, QuizConfig,
    ReadinessConfig, ReviewConfig, SelectorConfig, XpConfig, XpTiers,
};

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
