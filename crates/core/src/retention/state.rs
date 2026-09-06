//! The derived retention state: what the delayed probes answered (D-F11).
//!
//! The fold builds this state from `retention_probe` events and from NOTHING else.
//! No log written before 2.0 carries that event type, so the state of every 1.0 log
//! is empty and a cached model still equals a full replay. That is the whole reason
//! [`crate::projector::PROJECTOR_VERSION`] stands at 4 for this unit; the argument
//! is the same one D-F6 makes for the `task_served` confirmation marker.
//!
//! Every tally keeps its PROVENANCE apart. A correct answer that used help, or that
//! repeated an item the learner already saw, is not evidence of independent recall,
//! so it never enters `independent`. An ungraded probe (D-F2) enters no accuracy
//! numerator and no accuracy denominator.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::event::{AttemptOutcome, Exposure, RetentionProbe};

use super::policy::RetentionConfig;

/// The number of probe sessions the state remembers.
///
/// The rate rule reads it to hold one probe per session. A window is enough: the
/// rule only asks about the session the selector composes now.
pub const SESSION_WINDOW: usize = 16;

/// The number of probed item digests the state remembers.
///
/// The unseen rule reads it, so a probe never serves an item an earlier probe used.
pub const DIGEST_WINDOW: usize = 64;

/// The tally of the probes of one delay, with the provenance kept apart.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionTally {
    /// Every probe that arrived, whatever its provenance.
    #[serde(default)]
    pub probes: u32,
    /// The probes with no deterministic verdict (D-F2).
    #[serde(default)]
    pub ungraded: u32,
    /// The graded probes the learner answered correctly, whatever the provenance.
    #[serde(default)]
    pub correct: u32,
    /// The probes that used help.
    #[serde(default)]
    pub assisted: u32,
    /// The probes on an item the learner already saw.
    #[serde(default)]
    pub repeated: u32,
    /// The probes that name no exposure, so first exposure is not established.
    #[serde(default)]
    pub unknown_exposure: u32,
    /// The graded, unassisted, first-exposure probes: the independent evidence.
    #[serde(default)]
    pub independent: u32,
    /// The independent probes the learner answered correctly.
    #[serde(default)]
    pub independent_correct: u32,
    /// The seconds the independent probes took, summed.
    #[serde(default)]
    pub independent_secs: i64,
}

impl RetentionTally {
    /// Retained accuracy: the independent correct share.
    ///
    /// `None` means no independent probe arrived. The report prints "no evidence"
    /// for it and never a zero.
    #[must_use]
    pub fn retained_accuracy(&self) -> Option<f64> {
        (self.independent > 0)
            .then(|| f64::from(self.independent_correct) / f64::from(self.independent))
    }

    /// Assistance dependence: the share of the probes that used help.
    ///
    /// `None` means no probe arrived.
    #[must_use]
    pub fn assistance_dependence(&self) -> Option<f64> {
        (self.probes > 0).then(|| f64::from(self.assisted) / f64::from(self.probes))
    }

    /// The mean seconds of the independent probes. `None` means no such probe.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "a mean of a small count; the report prints one decimal"
    )]
    pub fn mean_independent_secs(&self) -> Option<f64> {
        (self.independent > 0).then(|| self.independent_secs as f64 / f64::from(self.independent))
    }

    /// Record one probe under this delay.
    fn record(&mut self, probe: &RetentionProbe) {
        self.probes = self.probes.saturating_add(1);
        if probe.assisted {
            self.assisted = self.assisted.saturating_add(1);
        }
        match probe.exposure {
            Some(Exposure::Repeat) => self.repeated = self.repeated.saturating_add(1),
            None => self.unknown_exposure = self.unknown_exposure.saturating_add(1),
            Some(Exposure::First) => {}
        }
        let graded = match probe.outcome {
            AttemptOutcome::Ungraded { .. } => {
                self.ungraded = self.ungraded.saturating_add(1);
                false
            }
            AttemptOutcome::Correct => {
                self.correct = self.correct.saturating_add(1);
                true
            }
            AttemptOutcome::Incorrect => true,
        };
        let first = probe.exposure == Some(Exposure::First);
        if graded && first && !probe.assisted {
            self.independent = self.independent.saturating_add(1);
            self.independent_secs = self.independent_secs.saturating_add(probe.secs.get());
            if probe.outcome == AttemptOutcome::Correct {
                self.independent_correct = self.independent_correct.saturating_add(1);
            }
        }
    }
}

/// The delayed-probe state the fold derives (D-F11). NEW IN 2.0.
///
/// The writer skips an empty state, so a model with no probe keeps the 1.0 shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionState {
    /// The tallies, keyed by the configured delay in days.
    #[serde(default)]
    pub by_delay: BTreeMap<u32, RetentionTally>,
    /// The delays already probed, per knowledge point, sorted.
    #[serde(default)]
    pub done: BTreeMap<String, Vec<u32>>,
    /// The ids of the sessions that carried a probe, oldest first, at most
    /// [`SESSION_WINDOW`].
    #[serde(default)]
    pub sessions: Vec<String>,
    /// The probed item digests, oldest first, at most [`DIGEST_WINDOW`].
    #[serde(default)]
    pub digests: Vec<String>,
}

impl RetentionState {
    /// Whether nothing was probed. The writer skips such a state.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_delay.is_empty()
            && self.done.is_empty()
            && self.sessions.is_empty()
            && self.digests.is_empty()
    }

    /// Fold one `retention_probe` event.
    ///
    /// `cfg` decides the bucket the probe reports under. The step is deterministic
    /// and order-independent for the tallies; the two windows keep event order.
    pub fn apply(&mut self, probe: &RetentionProbe, cfg: &RetentionConfig) {
        let bucket = cfg.bucket_of(probe.delay_days);
        self.by_delay.entry(bucket).or_default().record(probe);

        let done = self.done.entry(probe.kp.as_str().to_owned()).or_default();
        if let Err(at) = done.binary_search(&probe.delay_days) {
            done.insert(at, probe.delay_days);
        }
        if let Some(session) = probe.session.as_ref() {
            push_window(&mut self.sessions, session.clone(), SESSION_WINDOW);
        }
        if let Some(digest) = probe.item_digest.as_ref() {
            push_window(&mut self.digests, digest.clone(), DIGEST_WINDOW);
        }
    }

    /// The number of probes the state holds for `session`.
    #[must_use]
    pub fn probes_in_session(&self, session: &str) -> usize {
        self.sessions.iter().filter(|id| *id == session).count()
    }

    /// Whether `kp` already carried a probe at `delay_days`.
    #[must_use]
    pub fn is_done(&self, kp: &str, delay_days: u32) -> bool {
        self.done
            .get(kp)
            .is_some_and(|done| done.binary_search(&delay_days).is_ok())
    }

    /// Every probe the state counted, over every delay.
    #[must_use]
    pub fn total(&self) -> RetentionTally {
        let mut total = RetentionTally::default();
        for tally in self.by_delay.values() {
            total.probes = total.probes.saturating_add(tally.probes);
            total.ungraded = total.ungraded.saturating_add(tally.ungraded);
            total.correct = total.correct.saturating_add(tally.correct);
            total.assisted = total.assisted.saturating_add(tally.assisted);
            total.repeated = total.repeated.saturating_add(tally.repeated);
            total.unknown_exposure = total.unknown_exposure.saturating_add(tally.unknown_exposure);
            total.independent = total.independent.saturating_add(tally.independent);
            total.independent_correct = total
                .independent_correct
                .saturating_add(tally.independent_correct);
            total.independent_secs = total.independent_secs.saturating_add(tally.independent_secs);
        }
        total
    }
}

/// Append `value` to a bounded window, oldest first.
fn push_window(window: &mut Vec<String>, value: String, limit: usize) {
    window.push(value);
    if window.len() > limit {
        let excess = window.len() - limit;
        window.drain(..excess);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::event::{SchemaVersion, Secs, Slug, Timestamp};

    /// One probe of `kp` at `delay_days` with the named provenance.
    pub(crate) fn probe(
        kp: &str,
        delay_days: u32,
        outcome: AttemptOutcome,
        assisted: bool,
        exposure: Option<Exposure>,
    ) -> RetentionProbe {
        RetentionProbe {
            ts: Timestamp::from_micros(0),
            session: Some("s1".to_owned()),
            v: SchemaVersion::current(),
            kp: Slug::new(kp).expect("a slug"),
            topic: Slug::new("t1").expect("a slug"),
            delay_days,
            item_digest: Some(format!("{kp}-{delay_days}")),
            outcome,
            assisted,
            exposure,
            secs: Secs::new(12).expect("in range"),
        }
    }

    #[test]
    fn an_empty_state_reports_empty() {
        assert!(RetentionState::default().is_empty());
    }

    #[test]
    fn an_independent_correct_probe_counts_once_everywhere() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        state.apply(
            &probe("kp1", 7, AttemptOutcome::Correct, false, Some(Exposure::First)),
            &cfg,
        );
        let tally = &state.by_delay[&7];
        assert_eq!(tally.probes, 1);
        assert_eq!(tally.correct, 1);
        assert_eq!(tally.independent, 1);
        assert_eq!(tally.independent_correct, 1);
        assert_eq!(tally.retained_accuracy(), Some(1.0));
        assert_eq!(tally.assistance_dependence(), Some(0.0));
        assert_eq!(tally.mean_independent_secs(), Some(12.0));
        assert!(!state.is_empty());
        assert!(state.is_done("kp1", 7));
    }

    #[test]
    fn an_assisted_correct_probe_is_no_independent_evidence() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        state.apply(
            &probe("kp1", 7, AttemptOutcome::Correct, true, Some(Exposure::First)),
            &cfg,
        );
        let tally = &state.by_delay[&7];
        assert_eq!(tally.correct, 1);
        assert_eq!(tally.assisted, 1);
        assert_eq!(tally.independent, 0);
        assert_eq!(tally.retained_accuracy(), None);
        assert_eq!(tally.assistance_dependence(), Some(1.0));
    }

    #[test]
    fn a_repeated_item_is_no_independent_evidence() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        state.apply(
            &probe(
                "kp1",
                7,
                AttemptOutcome::Correct,
                false,
                Some(Exposure::Repeat),
            ),
            &cfg,
        );
        let tally = &state.by_delay[&7];
        assert_eq!(tally.repeated, 1);
        assert_eq!(tally.independent, 0);
        assert_eq!(tally.retained_accuracy(), None);
    }

    #[test]
    fn an_unknown_exposure_probe_is_no_independent_evidence() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        state.apply(&probe("kp1", 7, AttemptOutcome::Correct, false, None), &cfg);
        let tally = &state.by_delay[&7];
        assert_eq!(tally.unknown_exposure, 1);
        assert_eq!(tally.independent, 0);
    }

    #[test]
    fn an_ungraded_probe_leaves_the_accuracy_alone() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        let ungraded = AttemptOutcome::Ungraded {
            reason: "model-unavailable".to_owned(),
        };
        state.apply(
            &probe("kp1", 7, ungraded, false, Some(Exposure::First)),
            &cfg,
        );
        state.apply(
            &probe("kp2", 7, AttemptOutcome::Correct, false, Some(Exposure::First)),
            &cfg,
        );
        let tally = &state.by_delay[&7];
        assert_eq!(tally.probes, 2);
        assert_eq!(tally.ungraded, 1);
        assert_eq!(tally.independent, 1);
        assert_eq!(tally.retained_accuracy(), Some(1.0));
    }

    #[test]
    fn an_off_bucket_delay_reports_under_the_configured_one() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        state.apply(
            &probe(
                "kp1",
                45,
                AttemptOutcome::Incorrect,
                false,
                Some(Exposure::First),
            ),
            &cfg,
        );
        assert_eq!(state.by_delay[&30].probes, 1);
        assert!(state.is_done("kp1", 45));
    }

    #[test]
    fn the_session_and_digest_windows_stay_bounded() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        for index in 0..(DIGEST_WINDOW + 5) {
            let mut event = probe("kp1", 7, AttemptOutcome::Correct, false, None);
            event.session = Some(format!("s{index}"));
            event.item_digest = Some(format!("d{index}"));
            state.apply(&event, &cfg);
        }
        assert_eq!(state.sessions.len(), SESSION_WINDOW);
        assert_eq!(state.digests.len(), DIGEST_WINDOW);
        assert_eq!(state.digests.last().unwrap(), "d68");
    }

    #[test]
    fn the_rate_rule_counts_the_probes_of_one_session() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        state.apply(&probe("kp1", 7, AttemptOutcome::Correct, false, None), &cfg);
        assert_eq!(state.probes_in_session("s1"), 1);
        assert_eq!(state.probes_in_session("s2"), 0);
    }

    #[test]
    fn the_total_sums_every_delay() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        state.apply(
            &probe("kp1", 7, AttemptOutcome::Correct, false, Some(Exposure::First)),
            &cfg,
        );
        state.apply(
            &probe(
                "kp1",
                30,
                AttemptOutcome::Incorrect,
                false,
                Some(Exposure::First),
            ),
            &cfg,
        );
        let total = state.total();
        assert_eq!(total.probes, 2);
        assert_eq!(total.independent, 2);
        assert_eq!(total.independent_correct, 1);
        assert_eq!(total.retained_accuracy(), Some(0.5));
    }
}
