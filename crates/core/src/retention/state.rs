//! The derived retention state: what the delayed probes answered (D-F11).
//!
//! The fold builds this state from `retention_probe` events and from NOTHING else.
//! No log written before 2.0 carries that event type, so the state of every 1.0 log
//! is empty and a cached model still equals a full replay. That is the whole reason
//! [`crate::projector::PROJECTOR_VERSION`] first moved to 5 for retention; the argument
//! is the same one D-F6 makes for the `task_served` confirmation marker.
//!
//! Every tally keeps its PROVENANCE apart. A correct answer that used help, or that
//! repeated an item the learner already saw, is not evidence of independent recall,
//! so it never enters `independent`. An ungraded probe (D-F2) enters no accuracy
//! numerator and no accuracy denominator.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::event::{AttemptOutcome, Exposure, RetentionProbe};

use super::policy::RetentionConfig;

/// The number of probe sessions the state remembers.
///
/// The rate rule reads it to hold one probe per session. A window is enough: the
/// rule only asks about the session the selector composes now.
pub const SESSION_WINDOW: usize = 16;

/// The key of one knowledge point inside the `done` map.
///
/// A knowledge point id is unique inside its topic and NOT across the tree: the
/// checked-in Foundations tree spells `kp1` under many topics. The key therefore
/// carries the topic, or one probe of `kp1` would silently close another topic's
/// `kp1`.
#[must_use]
pub fn kp_key(topic: &str, kp: &str) -> String {
    format!("{topic}/{kp}")
}

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
    /// The delays already probed, keyed by [`kp_key`], sorted.
    #[serde(default)]
    pub done: BTreeMap<String, Vec<u32>>,
    /// The ids of the sessions that carried a probe, oldest first, at most
    /// [`SESSION_WINDOW`].
    #[serde(default)]
    pub sessions: Vec<String>,
    /// The probed item digests, oldest first, at most [`DIGEST_WINDOW`].
    #[serde(default)]
    pub digests: Vec<String>,
    /// Every problem digest the learner met, for LIFE (D-F11).
    ///
    /// `TopicState::last_problems` is a bounded recent window, and a window cannot
    /// answer "did this learner ever see this item". The unseen rule of the probe
    /// needs the lifetime answer, so the fold keeps this set: one
    /// [`crate::learner::problem_text_hash`] per answered problem, per served
    /// problem stub, and per probed item.
    ///
    /// Serde SKIPS it. It is a LIGHT INDEX the fold rebuilds from the whole stream
    /// on every projection, exactly like the XP tally, so it never enters the wire
    /// shape, the parity blob, or a stored row, and [`RetentionState::is_empty`]
    /// ignores it. A model of a 1.0 log therefore keeps its bytes.
    #[serde(skip)]
    pub exposed: BTreeSet<String>,
}

impl RetentionState {
    /// Whether nothing was probed. The writer skips such a state.
    ///
    /// It reads the PERSISTED fields only. `exposed` is a light index of every
    /// stream, probe or not, and a state that holds nothing but exposure must
    /// still serialize away.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_delay.is_empty()
            && self.done.is_empty()
            && self.sessions.is_empty()
            && self.digests.is_empty()
    }

    /// Record one problem digest the learner met.
    pub fn expose(&mut self, digest: &str) {
        if !self.exposed.contains(digest) {
            self.exposed.insert(digest.to_owned());
        }
    }

    /// Record one probe the selector SERVED, before any answer arrives.
    ///
    /// The rate rule of `retention.max_per_session` counts serves, so a refresh, an
    /// abandoned probe, and a second tab never buy the session another one. The
    /// serve of one task id is idempotent, so this runs once per probe.
    pub fn serve(&mut self, session: Option<&str>) {
        if let Some(session) = session {
            push_window(&mut self.sessions, session.to_owned(), SESSION_WINDOW);
        }
    }

    /// Whether the learner ever met `digest`.
    #[must_use]
    pub fn is_exposed(&self, digest: &str) -> bool {
        self.exposed.contains(digest)
    }

    /// Fold one `retention_probe` event.
    ///
    /// `cfg` decides the bucket the probe reports under. The step is deterministic
    /// and order-independent for the tallies; the two windows keep event order.
    pub fn apply(&mut self, probe: &RetentionProbe, cfg: &RetentionConfig) {
        let bucket = cfg.bucket_of(probe.delay_days);
        self.by_delay.entry(bucket).or_default().record(probe);

        let key = kp_key(probe.topic.as_str(), probe.kp.as_str());
        let done = self.done.entry(key).or_default();
        if let Err(at) = done.binary_search(&probe.delay_days) {
            done.insert(at, probe.delay_days);
        }
        // The SERVE of the probe already counted the session (`serve`). A probe
        // that arrives without one — an import, or a client that skipped the serve
        // marker — counts here instead, and a session counts at most once.
        if let Some(session) = probe.session.as_ref()
            && self.probes_in_session(session) == 0
        {
            push_window(&mut self.sessions, session.clone(), SESSION_WINDOW);
        }
        if let Some(digest) = probe.item_digest.as_ref() {
            push_window(&mut self.digests, digest.clone(), DIGEST_WINDOW);
            self.expose(digest);
        }
    }

    /// The number of probes the state holds for `session`.
    #[must_use]
    pub fn probes_in_session(&self, session: &str) -> usize {
        self.sessions.iter().filter(|id| *id == session).count()
    }

    /// Whether the knowledge point `kp` of `topic` already carried a probe at
    /// `delay_days`.
    #[must_use]
    pub fn is_done(&self, topic: &str, kp: &str, delay_days: u32) -> bool {
        self.done
            .get(&kp_key(topic, kp))
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
            total.unknown_exposure = total
                .unknown_exposure
                .saturating_add(tally.unknown_exposure);
            total.independent = total.independent.saturating_add(tally.independent);
            total.independent_correct = total
                .independent_correct
                .saturating_add(tally.independent_correct);
            total.independent_secs = total
                .independent_secs
                .saturating_add(tally.independent_secs);
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
#[path = "state/tests.rs"]
pub(crate) mod tests;
