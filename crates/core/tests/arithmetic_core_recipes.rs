//! `apply_arithmetic_core_recipes.py` raised six explicitly hand-verified
//! `arithmetic-core` knowledge points to four decidable exemplars each — one
//! per-KP recipe, not the coarse operator-only generator, so this is a
//! narrow, exact fixture naming only the reviewed knowledge points (not a
//! broad sweep). This test checks the same readiness facts the other
//! `foundations_heldout_*` tests check, against the real curriculum and the
//! real [`ReadinessIndex`]. `scripts/authoring/test_apply_arithmetic_core_recipes.py`
//! is the semantic table proving each new exemplar obeys its OWN knowledge
//! point's authored constraint (digit counts, nonzero remainder, factors
//! ending in zero); this test proves only that the curriculum-level
//! consequence (practicable, assessable, solutions) follows.
#![allow(clippy::unwrap_used)]
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_keys() -> Vec<String> {
    let text = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/arithmetic_core_recipes_kps.json"),
    )
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

#[test]
fn every_recipe_kp_is_practicable_assessable_and_has_solutions() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let index = ReadinessIndex::build(&curriculum);
    let keys = fixture_keys();
    assert_eq!(keys.len(), 6);
    let mut failures = Vec::new();
    for key in &keys {
        let Some(facts) = index.get(key) else {
            failures.push(format!("{key}: not found in the real curriculum"));
            continue;
        };
        if facts.decidable.len() < 4 {
            failures.push(format!(
                "{key}: only {} decidable exemplars, expected at least 4",
                facts.decidable.len()
            ));
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
        if !facts.solutions {
            failures.push(format!("{key}: solutions blocker still set"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
