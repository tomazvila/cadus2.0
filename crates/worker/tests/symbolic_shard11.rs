//! Current-gate and semantic regression for three-way linear classification.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, sample_labels, verify_rows};

const KEYS: &[&str] = &["equations-special-cases/kp3"];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard11-template.json"];

#[test]
fn three_way_classifier_passes_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}
#[test]
fn three_way_classifier_covers_all_outcomes_and_signed_coefficients() {
    let row = rows(PATHS).remove(0);
    let body = &row["body"];
    let labels = sample_labels(&row);
    assert_eq!(
        labels.into_iter().collect::<Vec<_>>(),
        ["all real numbers", "no solution", "one solution"]
    );
    assert!(
        body["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|sample| sample["params"]["a"].as_i64().unwrap() < 0)
    );
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(source.contains("Exactly one real value works"));
    assert!(source.contains("false comparison"));
}
