//! Unit03 identity shard ten: retired after the count items were rewritten.
//!
//! The shard counted the values that fail an identity ("How many real values
//! fail to satisfy ...?", key 0). Commit 88a9a73c rewrote the knowledge point
//! as Solve items keyed by an all-real-numbers label and parameter questions,
//! so the count template left the pending set.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{assert_shard_retired, curriculum_source};

const KEYS: &[&str] = &["equations-special-cases/kp2"];
const PATH: &str = "docs/content-foundations/symbolic-repair/shard10-template.json";

#[test]
fn the_identity_count_family_is_retired_with_its_recorded_gate_verdict() {
    assert_shard_retired(PATH, KEYS);
}

#[test]
fn the_rewritten_knowledge_point_solves_identities_and_counts_nothing() {
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    assert!(!source.contains("How many real values fail to satisfy"));
    assert!(source.contains("Solve $2(3x + 4) = 6x + 8$."));
    assert!(source.contains("answer: 'all real numbers'"));
}
