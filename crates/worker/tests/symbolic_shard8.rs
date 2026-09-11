//! Current-gate and semantic regression for Unit03 symbolic shard eight.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, sample_labels, verify_rows};

const KEYS: &[&str] = &[
    "combining-like-terms/kp1",
    "equivalent-expressions/kp1",
    "writing-expressions-from-patterns/kp1",
];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard8-templates.json"];

#[test]
fn exact_repaired_set_passes_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}
#[test]
fn repaired_families_pin_identifiability_variety_and_two_labels() {
    let rows = rows(PATHS);
    let equivalent = rows
        .iter()
        .find(|row| row["kp_id"] == "equivalent-expressions/kp1")
        .unwrap();
    let labels = sample_labels(equivalent);
    assert_eq!(labels.into_iter().collect::<Vec<_>>(), ["no", "yes"]);
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(source.contains("table follows a linear rule"));
    assert!(source.contains("learner claims $4y+6y=10y^2$"));
    assert!(source.contains("One counterexample disproves equivalence"));
    assert!(!source.contains("Complete the simplification $x+x+x+x$"));
}
