//! Individual scheduler-configuration sections.

use serde::{Deserialize, Deserializer, Serialize};

use crate::projector::PassRule;

use super::ConfigError;

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
