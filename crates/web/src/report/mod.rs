//! The retention report of D-F11: `GET /api/report/retention`.
//!
//! The route is a PURE READ. It appends no event and it writes no row.
//!
//! The payload holds four blocks:
//!
//! 1. `policy` — the version stamp of D-F12, its digest, and the probe delays the
//!    numbers came from. A reader always sees which policy produced a row.
//! 2. `retention` — one row per configured delay: retained accuracy, assistance
//!    dependence, and the provenance counts behind them.
//! 3. `placement` — the inferred topics that failed confirmation, and the ones
//!    that still owe one (D-F6).
//! 4. `integrated` — the multi-step tasks the log holds and how they closed.
//!
//! Every rate that has no evidence is `null`, never `0`. A rate over fewer than
//! `retention.min_sample` independent probes carries `"sufficient": false`, so a
//! reader never takes two answers for a measurement.
//!
//! # Why the route reads the whole log
//!
//! The retention rows come off the CACHED model, so they cost one projection. The
//! integrated-task tally reads `task_served` and `review_result`, and the fold
//! keeps no such index, so the block needs the log. `GET /api/export` reads the
//! whole log for the same reason, and section 8 of `docs/reference/l1-budget.md`
//! gives that route no p95 either. The report is an operator-rate read.

use cadus_core::retention::report::{
    IntegratedPerformance, PlacementError, RetentionReport, RetentionRow,
};
use cadus_store::state::{load_events, project_current};
use serde_json::{Value, json};

use crate::session::{Ready, Reply, reply_read};

/// The JSON of one retention row.
fn row_json(row: &RetentionRow) -> Value {
    let tally = &row.tally;
    json!({
        "delay_days": row.delay_days,
        "probes": tally.probes,
        "retained_accuracy": row.retained_accuracy,
        "assistance_dependence": row.assistance_dependence,
        "mean_independent_secs": row.mean_independent_secs,
        "sufficient": row.sufficient,
        "provenance": {
            "independent": tally.independent,
            "independent_correct": tally.independent_correct,
            "correct": tally.correct,
            "assisted": tally.assisted,
            "repeated": tally.repeated,
            "unknown_exposure": tally.unknown_exposure,
            "ungraded": tally.ungraded,
        },
    })
}

/// The JSON of the placement error.
fn placement_json(placement: &PlacementError) -> Value {
    json!({
        "failed_confirmation": placement.failed,
        "awaiting_confirmation": placement.awaiting,
    })
}

/// The JSON of the integrated-task performance.
fn integrated_json(integrated: &IntegratedPerformance) -> Value {
    json!({
        "served": integrated.served,
        "passed": integrated.passed,
        "failed": integrated.failed,
        "inconclusive": integrated.inconclusive,
        "open": integrated.open,
        "pass_rate": integrated.pass_rate(),
    })
}

/// The whole report as JSON.
#[must_use]
pub fn report_json(report: &RetentionReport, policy_digest: &str) -> Value {
    json!({
        "policy": {
            "version": report.policy.version,
            "label": report.policy.label(),
            "calibrated": report.policy.calibrated,
            "digest": policy_digest,
            "probe_delays_days": report.probe_delays_days,
            "min_sample": report.min_sample,
        },
        "retention": {
            "by_delay": report.rows.iter().map(row_json).collect::<Vec<Value>>(),
            "total": row_json(&report.total),
        },
        "placement": placement_json(&report.placement),
        "integrated": integrated_json(&report.integrated),
    })
}

/// `GET /api/report/retention` (D-F11).
///
/// # Errors
///
/// Returns the store envelope when the projection or the log read fails.
pub async fn retention(req: Ready) -> Reply {
    let input = req.input();
    let mut tx = req.begin().await?;
    let projection = req
        .store(project_current(&mut tx, req.user_id, &input))
        .await?;
    let rows = req.store(load_events(&mut tx, req.user_id)).await?;
    let events: Vec<_> = rows.into_iter().map(|row| row.event).collect();
    let cfg = &req.content.cfg;
    let report = RetentionReport::build(
        &projection.model,
        &cfg.retention,
        &cfg.policy_version,
        IntegratedPerformance::of_events(&events),
    );
    reply_read(tx, report_json(&report, &cfg.policy_digest())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadus_core::config::Config;
    use cadus_core::event::{
        AttemptOutcome, Exposure, RetentionProbe, SchemaVersion, Secs, Slug, Timestamp,
    };
    use cadus_core::learner::LearnerModel;
    use cadus_core::retention::RetentionState;

    #[test]
    fn an_empty_report_states_null_rates_and_the_uncalibrated_policy() {
        let cfg = Config::default();
        let report = RetentionReport::build(
            &LearnerModel::default(),
            &cfg.retention,
            &cfg.policy_version,
            IntegratedPerformance::default(),
        );
        let body = report_json(&report, &cfg.policy_digest());
        assert_eq!(body["policy"]["label"], "v1 (uncalibrated)");
        assert_eq!(body["policy"]["calibrated"], false);
        assert_eq!(body["policy"]["probe_delays_days"], json!([7, 30, 90]));
        let rows = body["retention"]["by_delay"].as_array().expect("rows");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["delay_days"], 7);
        assert_eq!(rows[0]["retained_accuracy"], Value::Null);
        assert_eq!(rows[0]["sufficient"], false);
        assert_eq!(body["integrated"]["pass_rate"], Value::Null);
    }

    /// One 7-day probe of `k1` with the named provenance.
    fn probe(assisted: bool, exposure: Exposure) -> RetentionProbe {
        RetentionProbe {
            ts: Timestamp::from_micros(0),
            session: Some("s1".to_owned()),
            v: SchemaVersion::current(),
            kp: Slug::new("k1").expect("a slug"),
            topic: Slug::new("t1").expect("a slug"),
            delay_days: 7,
            item_digest: None,
            outcome: AttemptOutcome::Correct,
            assisted,
            exposure: Some(exposure),
            secs: Secs::new(20).expect("in range"),
        }
    }

    #[test]
    fn the_payload_carries_the_provenance_of_every_row() {
        let cfg = Config::default();
        let mut state = RetentionState::default();
        for (assisted, exposure) in [
            (false, Exposure::First),
            (true, Exposure::First),
            (false, Exposure::Repeat),
        ] {
            state.apply(&probe(assisted, exposure), &cfg.retention);
        }
        let model = LearnerModel {
            retention: state,
            ..LearnerModel::default()
        };
        let report = RetentionReport::build(
            &model,
            &cfg.retention,
            &cfg.policy_version,
            IntegratedPerformance::default(),
        );
        let body = report_json(&report, &cfg.policy_digest());
        let row = &body["retention"]["by_delay"][0];
        assert_eq!(row["probes"], 3);
        assert_eq!(row["provenance"]["independent"], 1);
        assert_eq!(row["provenance"]["assisted"], 1);
        assert_eq!(row["provenance"]["repeated"], 1);
        assert_eq!(row["retained_accuracy"], 1.0);
        assert!(
            (row["assistance_dependence"].as_f64().expect("a rate") - 1.0 / 3.0).abs() < 1e-9,
            "one of the three probes used help"
        );
    }
}
