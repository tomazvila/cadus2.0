//! The retention report of D-F11: retained accuracy by delay, assistance
//! dependence, placement error, and integrated-task performance.
//!
//! Every number carries its provenance, and every number that has NO evidence is
//! `None` and never a zero. A rate over fewer than `retention.min_sample`
//! independent probes reports `sufficient: false`, so a reader never reads a rate
//! of two answers as a measurement.
//!
//! The report MEASURES. It states what the log holds and it claims nothing about
//! the effect of the course on the learner.

use std::collections::{BTreeMap, BTreeSet};

use crate::event::Event;
use crate::learner::{LearnerModel, PendingRemediation};
use crate::xp::is_inferred;

use super::policy::{PolicyVersion, RetentionConfig};
use super::state::{RetentionState, RetentionTally};

/// The remediation kind a failed confirmation queues (D-F6).
pub const CONFIRM_FAILED: &str = "confirm_failed";

/// One row of the retention report: the probes of one delay.
#[derive(Debug, Clone, PartialEq)]
pub struct RetentionRow {
    /// The delay, in days, the row reports.
    pub delay_days: u32,
    /// The tally the row summarizes.
    pub tally: RetentionTally,
    /// The independent correct share. `None` means no independent probe.
    pub retained_accuracy: Option<f64>,
    /// The share of the probes that used help. `None` means no probe.
    pub assistance_dependence: Option<f64>,
    /// The mean seconds of the independent probes. `None` means no such probe.
    pub mean_independent_secs: Option<f64>,
    /// Whether the independent sample reaches `retention.min_sample`.
    pub sufficient: bool,
}

impl RetentionRow {
    /// The row of one delay.
    fn of(delay_days: u32, tally: &RetentionTally, min_sample: u32) -> Self {
        Self {
            delay_days,
            tally: tally.clone(),
            retained_accuracy: tally.retained_accuracy(),
            assistance_dependence: tally.assistance_dependence(),
            mean_independent_secs: tally.mean_independent_secs(),
            sufficient: tally.independent >= min_sample,
        }
    }
}

/// The placement error of D-F11: the inferred topics that failed confirmation.
///
/// A placement gives CREDIT and no evidence (D-F6). `failed` counts the topics
/// whose confirmation item came back wrong, and `awaiting` counts the inferred
/// topics that carry no direct answer yet.
///
/// There is no rate here on purpose. A CONFIRMED topic leaves the inferred set,
/// so the model holds no denominator for a placement-error rate, and this report
/// does not invent one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlacementError {
    /// The inferred topics that still owe a confirmation.
    pub awaiting: Vec<String>,
    /// The inferred topics whose confirmation failed.
    pub failed: Vec<String>,
}

impl PlacementError {
    /// The placement error the model states.
    #[must_use]
    pub fn of_model(model: &LearnerModel) -> Self {
        let failed: BTreeSet<String> = model
            .pending_remediation
            .iter()
            .filter(|entry: &&PendingRemediation| entry.kind == CONFIRM_FAILED)
            .flat_map(|entry| entry.targets.iter().map(|id| id.as_str().to_owned()))
            .collect();
        let awaiting: Vec<String> = model
            .topics
            .iter()
            .filter(|(id, state)| is_inferred(state) && !failed.contains(*id))
            .map(|(id, _)| id.clone())
            .collect();
        Self {
            awaiting,
            failed: failed.into_iter().collect(),
        }
    }
}

/// The integrated-task performance of D-F11.
///
/// It tallies whole integrated items handed off or submitted, keyed by session,
/// task and item digest. An item with no submission counts as `open`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntegratedPerformance {
    /// The whole integrated items handed off or submitted.
    pub served: u32,
    /// The served tasks the learner passed.
    pub passed: u32,
    /// The served tasks the learner failed.
    pub failed: u32,
    /// The served tasks that ended without a decision (D-F7).
    pub inconclusive: u32,
    /// The served tasks with no result in the log.
    pub open: u32,
}

impl IntegratedPerformance {
    /// The pass share of the DECIDED tasks. `None` means no decided task.
    #[must_use]
    pub fn pass_rate(&self) -> Option<f64> {
        let decided = self.passed + self.failed;
        (decided > 0).then(|| f64::from(self.passed) / f64::from(decided))
    }

    /// Tally genuine integrated evidence, keeping the first submission per item.
    /// Component-based multi-step tasks have separate review evidence.
    #[must_use]
    pub fn of_events(events: &[Event]) -> Self {
        let mut items = BTreeMap::new();
        for event in events {
            match event {
                Event::IntegratedServed(body) => {
                    items
                        .entry((
                            body.session.as_deref(),
                            body.task_id.as_str(),
                            body.item_digest.as_str(),
                        ))
                        .or_insert(None);
                }
                Event::IntegratedAttempt(body) => {
                    items
                        .entry((
                            body.session.as_deref(),
                            body.task_id.as_str(),
                            body.item_digest.as_str(),
                        ))
                        .or_insert(None)
                        .get_or_insert(body);
                }
                _ => {}
            }
        }
        let mut tally = Self {
            served: u32::try_from(items.len()).unwrap_or(u32::MAX),
            ..Self::default()
        };
        for attempt in items.into_values() {
            match attempt {
                None => tally.open += 1,
                Some(body) if body.final_field.outcome.is_ungraded() => tally.inconclusive += 1,
                Some(body) if body.solved => tally.passed += 1,
                Some(_) => tally.failed += 1,
            }
        }
        tally
    }
}

/// The whole retention report (D-F11, D-F12).
#[derive(Debug, Clone, PartialEq)]
pub struct RetentionReport {
    /// The policy version the numbers were produced under.
    pub policy: PolicyVersion,
    /// The configured probe delays, in days.
    pub probe_delays_days: Vec<u32>,
    /// The smallest independent sample a rate needs to count as sufficient.
    pub min_sample: u32,
    /// One row per configured delay, plus every delay the log holds.
    pub rows: Vec<RetentionRow>,
    /// The row over every delay.
    pub total: RetentionRow,
    /// The placement error.
    pub placement: PlacementError,
    /// The integrated-task performance.
    pub integrated: IntegratedPerformance,
}

impl RetentionReport {
    /// Build the report from the model, the policy, and the integrated tally.
    ///
    /// A configured delay with no probe yet still gets a row, so the card prints
    /// the whole schedule and names the delays that carry no evidence.
    #[must_use]
    pub fn build(
        model: &LearnerModel,
        cfg: &RetentionConfig,
        policy: &PolicyVersion,
        integrated: IntegratedPerformance,
    ) -> Self {
        let state: &RetentionState = &model.retention;
        let mut delays: BTreeMap<u32, RetentionTally> = cfg
            .delays()
            .into_iter()
            .map(|days| (days, RetentionTally::default()))
            .collect();
        for (days, tally) in &state.by_delay {
            delays.insert(*days, tally.clone());
        }
        let rows = delays
            .iter()
            .map(|(days, tally)| RetentionRow::of(*days, tally, cfg.min_sample))
            .collect();
        Self {
            policy: policy.clone(),
            probe_delays_days: cfg.delays(),
            min_sample: cfg.min_sample,
            rows,
            total: RetentionRow::of(0, &state.total(), cfg.min_sample),
            placement: PlacementError::of_model(model),
            integrated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{AttemptOutcome, Slug, TopicStatus};
    use crate::learner::TopicState;
    use crate::retention::state::tests::{record, record_pair};

    /// A model with the named retention state.
    fn model_of(state: RetentionState) -> LearnerModel {
        LearnerModel {
            retention: state,
            ..LearnerModel::default()
        }
    }

    #[test]
    fn every_configured_delay_gets_a_row_even_with_no_probe() {
        let report = RetentionReport::build(
            &model_of(RetentionState::default()),
            &RetentionConfig::default(),
            &PolicyVersion::default(),
            IntegratedPerformance::default(),
        );
        let delays: Vec<u32> = report.rows.iter().map(|row| row.delay_days).collect();
        assert_eq!(delays, vec![7, 30, 90]);
        assert!(
            report
                .rows
                .iter()
                .all(|row| row.retained_accuracy.is_none())
        );
        assert!(report.rows.iter().all(|row| !row.sufficient));
        assert_eq!(report.policy.label(), "v1 (uncalibrated)");
    }

    #[test]
    fn a_small_sample_reports_the_rate_and_calls_it_insufficient() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        record(&mut state, &cfg, 7, AttemptOutcome::Correct);
        let report = RetentionReport::build(
            &model_of(state),
            &cfg,
            &PolicyVersion::default(),
            IntegratedPerformance::default(),
        );
        let row = &report.rows[0];
        assert_eq!(row.retained_accuracy, Some(1.0));
        assert!(!row.sufficient, "one probe is no measurement");
        assert_eq!(report.min_sample, 20);
    }

    #[test]
    fn the_total_row_sums_the_delays() {
        let cfg = RetentionConfig::default();
        let mut state = RetentionState::default();
        record_pair(&mut state, &cfg);
        let report = RetentionReport::build(
            &model_of(state),
            &cfg,
            &PolicyVersion::default(),
            IntegratedPerformance::default(),
        );
        assert_eq!(report.total.tally.probes, 2);
        assert_eq!(report.total.retained_accuracy, Some(0.5));
    }

    #[test]
    fn placement_error_names_the_failed_confirmations_and_the_waiting_topics() {
        let inferred = TopicState {
            status: TopicStatus::Placed,
            ability: 0.8,
            ..TopicState::default()
        };
        let model = LearnerModel {
            topics: BTreeMap::from([
                ("t1".to_owned(), inferred.clone()),
                ("t2".to_owned(), inferred),
            ]),
            pending_remediation: vec![PendingRemediation {
                kind: CONFIRM_FAILED.to_owned(),
                targets: vec![Slug::new("t2").expect("a slug")],
            }],
            ..LearnerModel::default()
        };
        let placement = PlacementError::of_model(&model);
        assert_eq!(placement.failed, vec!["t2".to_owned()]);
        assert_eq!(placement.awaiting, vec!["t1".to_owned()]);
    }
}

#[cfg(test)]
#[path = "report_integrated_tests.rs"]
mod integrated_tests;
