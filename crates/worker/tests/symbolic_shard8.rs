//! Current-gate and semantic regression for Unit03 symbolic shard eight.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, sample_labels, verify_rows};

// combining-like-terms/kp1 was retired on 2026-10-07 (courses rewrite).
const KEYS: &[&str] = &[
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
    // Re-pinned 2026-10-07 to the courses rewrite wording for the error item.
    assert!(source.contains("table follows a linear rule"));
    // 2026-10-08, story framing removed: the error item is now a direct question.
    assert!(source.contains("What is the simplified expression for $4y+6y$?"));
    // The reviewed "no" item of the yes/no family (rewritten in c8a84e32).
    assert!(source.contains("Are $4(x + 3)$ and $4x + 3$ equivalent?"));
    assert!(!source.contains("Complete the simplification $x+x+x+x$"));
}

#[test]
fn the_retired_like_terms_family_keeps_its_recorded_gate_verdict() {
    let retired: Vec<String> = common::retired::report("symbolic-repair")
        .into_iter()
        .filter(|row| PATHS.contains(&row["source"].as_str().unwrap()))
        .map(|row| row["kp_key"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(retired, ["combining-like-terms/kp1"]);
    common::retired::assert_retired(
        "symbolic-repair",
        common::symbolic::RETIRED_SOURCES,
        &common::symbolic::pending_keys(),
    );
}
