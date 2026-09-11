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

mod common;
use std::path::Path;

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
    let keys = fixture_keys();
    assert_eq!(keys.len(), 6);
    common::readiness::assert_kps_ready(&keys);
}
