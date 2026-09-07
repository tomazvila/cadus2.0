//! Production-readiness check: every knowledge point of
//! `curriculum/foundations/08-functions-exponentials.yaml` carries at least
//! four decidable exemplars, a held-out family member, and a solution
//! sketch on every practice exemplar (`crates/core/src/readiness/facts.rs`).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

#[test]
fn every_functions_exponentials_kp_is_practicable_assessable_and_has_solutions() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/functions_exponentials_recipes_kps.json");
    let keys: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    assert_eq!(keys.len(), 92, "one entry per knowledge point of the unit");
    common::readiness::assert_kps_ready(&keys);
}

#[test]
fn every_functions_exponentials_topic_is_covered_by_the_fixture() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/functions_exponentials_recipes_kps.json");
    let keys: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    let (curriculum, _) = load_curriculum(&common::paths::curriculum_root()).unwrap();
    let index = ReadinessIndex::build(&curriculum);
    let unit: serde_json::Value = serde_norway::from_str(
        &std::fs::read_to_string(
            common::paths::curriculum_root().join("foundations/08-functions-exponentials.yaml"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut actual = std::collections::BTreeSet::new();
    for topic in unit["topics"].as_array().unwrap() {
        let topic_id = topic["id"].as_str().unwrap();
        assert!(curriculum.idx_of(topic_id).is_some());
        assert_eq!(index.course_of(topic_id), "foundations");
        for kp in topic["knowledge_points"].as_array().unwrap() {
            actual.insert(format!("{topic_id}/{}", kp["id"].as_str().unwrap()));
        }
    }
    assert_eq!(actual, keys.into_iter().collect());
}

#[test]
fn logarithm_conversion_labels_grade_aliases_and_reject_the_competing_equation() {
    use cadus_core::answer::{AnswerContract, Outcome, check_contract};
    let (curriculum, _) = load_curriculum(&common::paths::curriculum_root()).unwrap();
    let topic = curriculum
        .topic(curriculum.idx_of("logarithm-basics").unwrap())
        .unwrap();
    let mut checked = 0;
    for kp in topic.knowledge_points.iter().take(2) {
        for exemplar in &kp.exemplars {
            let contract = exemplar.answer_contract.clone().unwrap();
            let AnswerContract::Label { options } = &contract else {
                unreachable!("conversion equations require the reviewed choice policy")
            };
            assert_eq!(options.len(), 2);
            for aliases in options {
                let should_pass = aliases.contains(&exemplar.answer);
                for alias in aliases {
                    let outcome = check_contract(&exemplar.answer, alias, contract.clone());
                    assert!(
                        matches!(outcome, Outcome::Decided(v) if v.correct == should_pass),
                        "{}: {alias}",
                        exemplar.problem
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked >= 32);
}
