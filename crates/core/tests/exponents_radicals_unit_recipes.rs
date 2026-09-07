//! Production-readiness check for every knowledge point of the
//! `06-exponents-radicals.yaml` content unit: P2.3/P2.4 exemplar-count and
//! solution-sketch closure. Preserves and does not replace
//! `radical_core_recipes.rs`, which pins the five earlier-reviewed KPs.
#![allow(clippy::unwrap_used)]

mod common;
use std::path::Path;

use cadus_core::curriculum::load_curriculum;

#[test]
fn every_exponents_radicals_kp_is_practicable_assessable_and_has_solutions() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/exponents_radicals_unit_kps.json");
    let keys: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    assert_eq!(keys.len(), 78, "the unit holds 26 topics of 3 KPs each");
    common::readiness::assert_kps_ready(&keys);
}

#[test]
fn every_exponents_radicals_topic_has_a_decidable_diagnostic() {
    let (curriculum, findings) = load_curriculum(&common::paths::curriculum_root()).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let mut failures = Vec::new();
    for topic in curriculum.topics() {
        if topic.id.as_str().is_empty() {
            continue;
        }
        let unit_topic = [
            "exponent-product-rule",
            "exponent-quotient-rule",
            "exponent-product-quotient-rules",
            "power-of-a-power-rule",
            "power-rule-exponents",
            "zero-exponent-rule",
            "negative-zero-exponents",
            "simplifying-negative-exponent-expressions",
            "scientific-notation-conversion",
            "scientific-notation",
            "scientific-notation-addition-subtraction",
            "perfect-square-roots",
            "square-roots",
            "estimating-square-roots",
            "cube-roots",
            "simplifying-radicals",
            "simplifying-radicals-variables",
            "adding-subtracting-radicals",
            "radical-operations",
            "dividing-radicals",
            "rationalizing-denominators",
            "radical-exponent-conversion",
            "rational-exponents",
            "pythagorean-theorem",
            "pythagorean-converse",
            "radical-equations-basic",
        ]
        .contains(&topic.id.as_str());
        if !unit_topic {
            continue;
        }
        let Some(diagnostic) = &topic.diagnostic_exemplar else {
            failures.push(format!("{}: no diagnostic_exemplar", topic.id.as_str()));
            continue;
        };
        if diagnostic.canonical_answer().is_err() {
            failures.push(format!(
                "{}: diagnostic answer {:?} is undecidable",
                topic.id.as_str(),
                diagnostic.answer
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
