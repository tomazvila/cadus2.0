//! `apply_foundations_held_out_exemplars.py` cleared the `practicable` and
//! `assessable` readiness blockers (D-F5) of every knowledge point it raised
//! to four decidable exemplars — checked against the real curriculum and the
//! real [`ReadinessIndex`], not against this lane's own Python classifier.
//!
//! `tests/fixtures/heldout_practicable_kps.json` is the exact serving-key
//! list of knowledge points that reached four or more exemplars, generated
//! once and committed so this test needs no Python at run time. Regenerate
//! it after any further curriculum edit to this family.
#![allow(clippy::unwrap_used)]
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_keys() -> Vec<String> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/heldout_practicable_kps.json"),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn every_raised_kp_in_the_fixture_is_now_practicable_and_assessable_from_the_curriculum_alone() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let index = ReadinessIndex::build(&curriculum);
    let keys = fixture_keys();
    assert!(
        keys.len() > 20,
        "the fixture should name dozens of knowledge points"
    );
    let mut failures = Vec::new();
    for key in &keys {
        let Some(facts) = index.get(key) else {
            failures.push(format!("{key}: not found in the real curriculum"));
            continue;
        };
        // Practicable and assessable both take the curriculum alone (no
        // approved template needed) once a knowledge point holds four or
        // more decidable exemplars: three stay in practice, and the last is
        // held out for assessment.
        if facts.decidable.len() < 4 {
            failures.push(format!(
                "{key}: only {} decidable exemplars, expected at least 4",
                facts.decidable.len()
            ));
            continue;
        }
        if facts.held_out.is_none() {
            failures.push(format!("{key}: no exemplar is held out"));
        }
        if facts.practice_exemplars() < 3 {
            failures.push(format!(
                "{key}: only {} practice exemplars, expected at least 3",
                facts.practice_exemplars()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
