//! Current-gate and semantic regression for the Unit03 identity family.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};

const KEYS: &[&str] = &["equations-special-cases/kp2"];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard10-template.json"];

#[test]
fn identity_family_passes_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}
#[test]
fn identity_family_counts_counterexamples_without_a_singleton_label() {
    let body = &rows(PATHS).remove(0)["body"];
    assert_eq!(body["answer_expr"], "0");
    assert_eq!(body["answer_contract"]["kind"], "exact");
    assert_eq!(body["samples"].as_array().unwrap().len(), 16);
    assert!(body["constraints"].as_array().unwrap().len() == 1);
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert_eq!(
        source
            .matches("How many real values fail to satisfy")
            .count(),
        3
    );
}
