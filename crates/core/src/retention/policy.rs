//! The versioned 2.0 policy set and the retention-probe constants (D-F11, D-F12).
//!
//! Every adaptive number of 2.0 lives under one version. [`PolicyVersion`] names
//! that version and states whether real delayed outcomes calibrated the numbers.
//! The default answer is `false`: the shipped numbers are engineering defaults,
//! and `docs/plans/CALIBRATION.md` gives the protocol that changes the answer.

use serde::{Deserialize, Serialize};

/// The version of the 2.0 policy set. A change of any versioned number needs a bump.
pub const POLICY_VERSION: u32 = 1;

/// The delays, in days, of the retention probes of D-F11.
pub const DEFAULT_PROBE_DELAYS: [u32; 3] = [7, 30, 90];

/// The smallest sample a reported retention rate needs before the report calls it
/// sufficient. It is a reporting threshold and no scheduling constant.
pub const DEFAULT_MIN_SAMPLE: u32 = 20;

/// The version stamp of the adaptive policy set (D-F12).
///
/// The 1.0 `config_hash` detects drift of the 1.0 SCHEDULER CONSTANTS. This stamp
/// covers the 2.0 policies on top of them: the probe delays, the pass rule, the
/// readiness rule, and the mastery rule. It stays OUT of the 1.0 hash preimage, so
/// a policy bump never invalidates a stored projection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyVersion {
    /// The version number of the policy set.
    pub version: u32,
    /// Whether real delayed outcomes calibrated the numbers of this version.
    ///
    /// `false` means the numbers are engineering defaults. Software tests and
    /// simulated learners are engineering evidence and never set this to `true`.
    pub calibrated: bool,
}

impl Default for PolicyVersion {
    fn default() -> Self {
        Self {
            version: POLICY_VERSION,
            calibrated: false,
        }
    }
}

impl PolicyVersion {
    /// The one-line label of the version, for a report and a dashboard card.
    #[must_use]
    pub fn label(&self) -> String {
        let state = if self.calibrated {
            "calibrated"
        } else {
            "uncalibrated"
        };
        format!("v{} ({state})", self.version)
    }
}

/// The delayed-retention constants (D-F11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionConfig {
    /// Whether the selector schedules delayed probes.
    pub enabled: bool,
    /// The delays, in days, between the lesson pass and the probe.
    pub probe_delays_days: Vec<u32>,
    /// The largest number of probes one session carries.
    pub max_per_session: u32,
    /// The smallest sample a reported rate needs before it counts as sufficient.
    pub min_sample: u32,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            probe_delays_days: DEFAULT_PROBE_DELAYS.to_vec(),
            max_per_session: 1,
            min_sample: DEFAULT_MIN_SAMPLE,
        }
    }
}

impl RetentionConfig {
    /// The configured delays, sorted and deduplicated, with a zero delay dropped.
    #[must_use]
    pub fn delays(&self) -> Vec<u32> {
        let mut delays: Vec<u32> = self
            .probe_delays_days
            .iter()
            .copied()
            .filter(|days| *days > 0)
            .collect();
        delays.sort_unstable();
        delays.dedup();
        delays
    }

    /// The configured delay one probe reports under.
    ///
    /// A probe carries the delay it was SCHEDULED for, so the value is normally a
    /// configured one. A probe of an earlier policy version falls into the largest
    /// configured delay at or below it, and a probe below the smallest configured
    /// delay keeps its own value as its bucket.
    #[must_use]
    pub fn bucket_of(&self, delay_days: u32) -> u32 {
        self.delays()
            .into_iter()
            .rfind(|days| *days <= delay_days)
            .unwrap_or(delay_days)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_delays_are_7_30_90() {
        let cfg = RetentionConfig::default();
        assert_eq!(cfg.delays(), vec![7, 30, 90]);
        assert_eq!(cfg.max_per_session, 1);
        assert!(cfg.enabled);
    }

    #[test]
    fn delays_sort_dedup_and_drop_zero() {
        let cfg = RetentionConfig {
            probe_delays_days: vec![30, 0, 7, 30],
            ..RetentionConfig::default()
        };
        assert_eq!(cfg.delays(), vec![7, 30]);
    }

    #[test]
    fn bucket_takes_the_largest_delay_at_or_below() {
        let cfg = RetentionConfig::default();
        assert_eq!(cfg.bucket_of(7), 7);
        assert_eq!(cfg.bucket_of(29), 7);
        assert_eq!(cfg.bucket_of(30), 30);
        assert_eq!(cfg.bucket_of(120), 90);
    }

    #[test]
    fn a_delay_below_the_smallest_bucket_keeps_itself() {
        let cfg = RetentionConfig::default();
        assert_eq!(cfg.bucket_of(3), 3);
    }

    #[test]
    fn the_default_policy_version_is_uncalibrated() {
        let version = PolicyVersion::default();
        assert_eq!(version.version, POLICY_VERSION);
        assert!(!version.calibrated);
        assert_eq!(version.label(), "v1 (uncalibrated)");
    }
}
