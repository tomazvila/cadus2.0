//! Production-readiness check for five explicitly reviewed radical recipes.
#![allow(clippy::unwrap_used)]
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn every_radical_recipe_kp_is_practicable_assessable_and_has_solutions() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/radical_core_recipes_kps.json");
    let keys: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    assert_eq!(keys.len(), 5);
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let index = ReadinessIndex::build(&curriculum);
    let mut failures = Vec::new();
    for key in keys {
        let Some(facts) = index.get(&key) else {
            failures.push(format!("{key}: missing"));
            continue;
        };
        if facts.decidable.len() < 4
            || facts.held_out.is_none()
            || facts.practice_exemplars() < 3
            || !facts.solutions
        {
            failures.push(format!(
                "{key}: decidable={}, practice={}, held_out={}, solutions={}",
                facts.decidable.len(),
                facts.practice_exemplars(),
                facts.held_out.is_some(),
                facts.solutions
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
