//! Current-gate and semantic regression for Unit03 symbolic shard seven.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};

const KEYS: &[&str] = &[
    "combining-like-terms/kp2",
    "combining-like-terms/kp3",
    "distributive-property/kp1",
    "distributive-property/kp2",
    "distributive-property/kp3",
    "equivalent-expressions/kp2",
];
const PATHS: &[&str] = &[
    "docs/content-foundations/symbolic-repair/shard7-templates-1.json",
    "docs/content-foundations/symbolic-repair/shard7-templates-2.json",
    "docs/content-foundations/symbolic-repair/shard7-templates-3.json",
];

#[test]
fn exact_repaired_set_passes_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}

#[test]
fn repaired_expression_families_avoid_degenerate_and_generic_cases() {
    for row in rows(PATHS) {
        let body = &row["body"];
        assert_eq!(body["samples"].as_array().unwrap().len(), 16);
        let hint = body["hints"][0].as_str().unwrap();
        assert!(!hint.contains("isolates the unknown"));
    }
    let rows = rows(PATHS);
    let by_key = |key: &str| rows.iter().find(|row| row["kp_id"] == key).unwrap();
    for key in ["combining-like-terms/kp3", "equivalent-expressions/kp2"] {
        assert!(
            by_key(key)["body"]["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|sample| !sample["expected"].as_str().unwrap().starts_with("0x"))
        );
    }
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(source.contains("A balance changes by $-7x$"));
    assert!(source.contains("A rectangle has height $4$"));
    assert!(source.contains("Correct the sign error"));
    assert!(source.contains("Express its perimeter in the form $ax+b$"));
}
