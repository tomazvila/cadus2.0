//! Current-gate and semantic regression for Unit03 no-solution shard nine.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};

const KEYS: &[&str] = &[
    "absolute-value-equations/kp3",
    "equations-special-cases/kp1",
];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard9-templates.json"];

#[test]
fn exact_repaired_set_passes_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}
#[test]
fn no_solution_families_count_zero_and_vary_the_equations() {
    for row in rows(PATHS) {
        let body = &row["body"];
        assert_eq!(body["answer_expr"], "0");
        assert_eq!(body["answer_contract"]["kind"], "exact");
        assert!(body["params"].as_object().unwrap().len() >= 2);
        assert!(body["samples"].as_array().unwrap().len() >= 16);
    }
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(source.contains("answer: \"0\""));
    assert!(source.contains("How many real solutions"));
    assert!(!source.contains("answer: \"none\""));
}
