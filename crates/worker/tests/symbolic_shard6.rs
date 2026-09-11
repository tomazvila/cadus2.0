//! Current-gate and semantic regression for Unit03 symbolic shard six.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};
use std::collections::BTreeSet;

const KEYS: &[&str] = &[
    "checking-a-solution/kp2",
    "equations-with-fractions/kp3",
    "evaluating-expressions/kp2",
    "money-geometry-problems/kp1",
    "parts-of-an-expression/kp2",
];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard6-templates.json"];

#[test]
fn exact_repaired_set_passes_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}
#[test]
fn repaired_family_defects_are_pinned() {
    let rows = rows(PATHS);
    let by_key = |key: &str| rows.iter().find(|r| r["kp_id"] == key).unwrap();
    let checking = &by_key("checking-a-solution/kp2")["body"];
    let positions: BTreeSet<_> = checking["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["params"]["b"].as_i64().unwrap())
        .collect();
    assert_eq!(positions, BTreeSet::from([-1, 0, 1]));
    let evaluating = &by_key("evaluating-expressions/kp2")["body"];
    assert!(
        evaluating["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["params"]["a"].as_i64().unwrap() < 0)
    );
    let parts = &by_key("parts-of-an-expression/kp2")["body"];
    assert_eq!(parts["params"].as_object().unwrap().len(), 1);
    let fractions = &by_key("equations-with-fractions/kp3")["body"];
    assert!(
        fractions["solution_sketch"]
            .as_str()
            .unwrap()
            .contains("Distribute")
    );
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(source.contains("After a €4 coupon"));
    assert!(source.contains("answer: \"6\""));
}
