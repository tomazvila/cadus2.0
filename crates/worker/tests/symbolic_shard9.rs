//! Unit03 no-solution shard nine: retired after the count items were rewritten.
//!
//! The shard counted the solutions of a no-solution equation ("How many real
//! solutions ...?", key 0). Commit 88a9a73c rewrote both knowledge points as
//! Solve items keyed by a no-solution label, so the count templates left the
//! pending set (`docs/reports/symbolic-repair-retired-pending-templates.json`).
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{assert_shard_retired, curriculum_source};

const KEYS: &[&str] = &[
    "absolute-value-equations/kp3",
    "equations-special-cases/kp1",
];
const PATH: &str = "docs/content-foundations/symbolic-repair/shard9-templates.json";

#[test]
fn the_count_family_is_retired_with_its_recorded_gate_verdict() {
    assert_shard_retired(PATH, KEYS);
}

#[test]
fn the_rewritten_knowledge_points_ask_solve_items_and_no_count() {
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(!source.contains("How many real solutions"));
    assert!(source.contains("Solve $|x - 5| = -3$."));
    assert!(source.contains("Solve $3x + 5 = 3x - 2$."));
    assert!(source.contains("answer: 'no solution'"));
    assert!(!source.contains("answer: \"none\""));
}
