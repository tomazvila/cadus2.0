//! Three-way linear classification shard eleven: retired after the rewrite.
//!
//! The shard keyed every equation by one of three labels, "one solution" among
//! them. Commits c8a84e32 and 88a9a73c key a one-solution item by the solution
//! itself, so the classifier template left the pending set.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{assert_shard_retired, curriculum_source};

const KEYS: &[&str] = &["equations-special-cases/kp3"];
const PATH: &str = "docs/content-foundations/symbolic-repair/shard11-template.json";

#[test]
fn the_three_way_classifier_is_retired_with_its_recorded_gate_verdict() {
    assert_shard_retired(PATH, KEYS);
}

#[test]
fn the_rewritten_knowledge_point_keys_one_solution_by_its_value() {
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(source.contains("Solve $7x + 6 = 4x + 6$."));
    assert!(source.contains("Solve $4(2x + 3) = 8x + 15$."));
    assert!(source.contains("Solve $9(x + 2) = 9x + 18$."));
    assert!(!source.contains("\"one solution\""));
}
