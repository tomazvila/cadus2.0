//! New hard-residual drafts run through the current production gate offline.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_worker::authoring::{job::verify_kind, prompt::Kind};
use common::reviewed_templates::{directory_rows, run_rows, spec};
use serde_json::{Value, json};

fn drafts() -> Vec<Value> {
    directory_rows("docs/content-foundations/u08-u09-hard")
}

#[test]
fn exhaustive_pending_families_pass_and_wrong_samples_fail() {
    let rows = drafts();
    let report = run_rows(&rows, "target/hard-residual/regression");
    assert_eq!(report["passed"], rows.len(), "{report}");
    assert_eq!(rows.len(), 5);
    for row in rows {
        assert_eq!(row["status"], "pending");
        let spec = spec(row["kp_id"].as_str().unwrap());
        let mut wrong = row["arguments"].clone();
        wrong["samples"][0]["expected"] = json!("999");
        assert!(verify_kind(Kind::Template, &spec, &wrong, &[]).is_err());
    }
}
