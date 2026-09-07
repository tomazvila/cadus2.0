//! Production-readiness check for five explicitly reviewed radical recipes.
#![allow(clippy::unwrap_used)]

mod common;
use std::path::Path;

#[test]
fn every_radical_recipe_kp_is_practicable_assessable_and_has_solutions() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/radical_core_recipes_kps.json");
    let keys: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    assert_eq!(keys.len(), 5);
    common::readiness::assert_kps_ready(keys);
}
