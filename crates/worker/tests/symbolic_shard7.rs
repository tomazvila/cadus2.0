//! Current-gate and semantic regression for Unit03 symbolic shard seven.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};

// The courses rewrite (2026-10-07) retired combining-like-terms/kp2 and kp3 and
// distributive-property/kp1, kp2 and kp3; only equivalent-expressions/kp2 stays.
const KEYS: &[&str] = &["equivalent-expressions/kp2"];
const RETIRED: &[&str] = &[
    "combining-like-terms/kp2",
    "combining-like-terms/kp3",
    "distributive-property/kp1",
    "distributive-property/kp2",
    "distributive-property/kp3",
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
    // The combining-like-terms and distributive-property families were retired by
    // the courses rewrite (2026-10-07); equivalent-expressions/kp2 is the one left.
    let key = "equivalent-expressions/kp2";
    assert!(
        by_key(key)["body"]["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|sample| !sample["expected"].as_str().unwrap().starts_with("0x"))
    );
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(source.contains("A balance changes by $-7x$"));
    // Re-pinned 2026-10-07 to the courses rewrite wording.
    assert!(source.contains("A rectangle is $4$ m high and $2x+3$ m wide."));
    // 2026-10-08, story framing removed: the error item is now a direct question.
    assert!(source.contains("What is the expansion of $-(4-x)$?"));
    assert!(source.contains("Express its perimeter in the form $ax+b$"));
}

#[test]
fn the_retired_expression_families_keep_their_recorded_gate_verdict() {
    let retired: std::collections::BTreeSet<String> = common::retired::report("symbolic-repair")
        .into_iter()
        .filter(|row| PATHS.contains(&row["source"].as_str().unwrap()))
        .map(|row| row["kp_key"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        retired,
        RETIRED.iter().map(|key| (*key).to_owned()).collect()
    );
    common::retired::assert_retired(
        "symbolic-repair",
        common::symbolic::RETIRED_SOURCES,
        &common::symbolic::pending_keys(),
    );
}
