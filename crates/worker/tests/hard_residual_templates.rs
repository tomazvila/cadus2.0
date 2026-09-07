//! Reviewed hard-residual artifacts retain their current production-gate verdicts.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_worker::authoring::{job::verify_kind, prompt::Kind};
use common::reviewed_templates::{
    assert_report_with_authored_collisions, assert_template19_replacements, directory_rows,
    run_rows, spec,
};
use serde_json::{Value, json};

fn drafts() -> Vec<Value> {
    directory_rows("docs/content-foundations/u08-u09-hard")
}

#[test]
fn archived_families_keep_exact_current_verdicts_and_wrong_samples_fail() {
    let rows = drafts();
    let report = run_rows(&rows, "target/hard-residual/regression");
    assert_eq!(rows.len(), 5);
    assert_report_with_authored_collisions(
        &report,
        rows.len(),
        Some(36),
        &["logarithm-basics/kp1", "logarithm-basics/kp2"],
    );
    assert_template19_replacements(&rows, &["logarithm-basics/kp1", "logarithm-basics/kp2"]);
    for row in rows {
        assert_eq!(row["status"], "pending");
        let spec = spec(row["kp_id"].as_str().unwrap());
        let mut wrong = row["arguments"].clone();
        wrong["samples"][0]["expected"] = json!("999");
        assert!(verify_kind(Kind::Template, &spec, &wrong, &[]).is_err());
    }
}
