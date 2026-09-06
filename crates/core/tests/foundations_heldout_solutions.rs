//! `apply_foundations_solution_sketches.py` cleared the `solutions` blocker
//! (D-F5) of every pure-numeric compute knowledge point it addressed —
//! checked against the real curriculum and the real [`ReadinessIndex`], not
//! against this lane's own Python classifier.
//!
//! `tests/fixtures/heldout_solutions_kps.json` is the exact serving-key list
//! `scripts/authoring/foundations_drafts.pure_numeric_exemplars` names as
//! qualifying, generated once and committed so this test needs no Python at
//! run time. Regenerate it after any further curriculum edit to this family.
#![allow(clippy::unwrap_used)]
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_keys() -> Vec<String> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/heldout_solutions_kps.json"),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn every_pure_numeric_compute_kp_in_the_fixture_now_satisfies_the_solutions_condition() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let index = ReadinessIndex::build(&curriculum);
    let keys = fixture_keys();
    assert!(
        keys.len() > 50,
        "the fixture should name dozens of knowledge points"
    );
    let mut failures = Vec::new();
    for key in &keys {
        match index.get(key) {
            Some(facts) if facts.solutions => {}
            Some(_) => failures.push(format!("{key}: solutions is still false")),
            None => failures.push(format!("{key}: not found in the real curriculum")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
