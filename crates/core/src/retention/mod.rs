//! Delayed retention: the probes, their tallies, and the report (D-F11, D-F12).
//!
//! A lesson pass states that the learner answered right ON THE DAY. It states
//! nothing about the next month. This module measures the difference:
//!
//! - [`policy`] holds the versioned numbers: the probe delays, the rate, and the
//!   [`PolicyVersion`] stamp that says whether real delayed outcomes calibrated
//!   them. The shipped answer is `false`.
//! - [`schedule`] decides which knowledge point one session probes, from the
//!   lesson pass instant and the probes already done. The caller hands the clock
//!   over, so a test drives the date.
//! - [`state`] is the fold of the `retention_probe` events, with the provenance
//!   of every answer kept apart.
//! - [`report`] turns the state into the numbers of the report route and the
//!   dashboard card.
//!
//! # What this module does NOT do
//!
//! It never claims a course taught anything. It reports what the probes answered,
//! how many answers used help, how many repeated a seen item, and how many carry
//! no verdict at all. `docs/plans/CALIBRATION.md` states which of the numbers a
//! real delayed sample must calibrate before anyone reads them as an effect.

pub mod policy;
pub mod report;
pub mod schedule;
pub mod state;

pub use policy::{
    DEFAULT_MIN_SAMPLE, DEFAULT_PROBE_DELAYS, POLICY_VERSION, PolicyVersion, RetentionConfig,
};
pub use report::{IntegratedPerformance, PlacementError, RetentionReport, RetentionRow};
pub use schedule::{ProbePlan, due_probe, seen_digests, unseen_item};
pub use state::{DIGEST_WINDOW, SESSION_WINDOW, RetentionState, RetentionTally};
