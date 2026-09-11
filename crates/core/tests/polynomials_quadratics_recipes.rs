//! Production-readiness check for every knowledge point of
//! `07-polynomials-quadratics.yaml`: P2.4 requires 4 decidable exemplars, a
//! held-out assessment item, and a full practice-set of solution sketches.
#![allow(clippy::unwrap_used)]
use std::path::Path;

use cadus_core::curriculum::load_curriculum;
use cadus_core::readiness::ReadinessIndex;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn kp_keys() -> Vec<String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/polynomials_quadratics_kps.json");
    serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap()
}

#[test]
fn the_fixture_names_every_knowledge_point_of_the_unit() {
    assert_eq!(kp_keys().len(), 102);
}

#[test]
fn every_polynomials_quadratics_kp_is_practicable_assessable_and_has_solutions() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let index = ReadinessIndex::build(&curriculum);
    let mut failures = Vec::new();
    for key in kp_keys() {
        let Some(facts) = index.get(&key) else {
            failures.push(format!("{key}: missing from the curriculum"));
            continue;
        };
        if facts.decidable.len() < 4
            || facts.held_out.is_none()
            || facts.practice_exemplars() < 3
            || !facts.solutions
        {
            failures.push(format!(
                "{key}: decidable={}, held_out={:?}, practice={}, solutions={}",
                facts.decidable.len(),
                facts.held_out,
                facts.practice_exemplars(),
                facts.solutions
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn no_two_exemplars_of_one_knowledge_point_repeat_a_problem_statement() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let topic_ids: std::collections::BTreeSet<String> = kp_keys()
        .into_iter()
        .map(|key| key.split('/').next().unwrap().to_owned())
        .collect();
    let mut failures = Vec::new();
    for topic in curriculum.topics() {
        if !topic_ids.contains(topic.id.as_str()) {
            continue;
        }
        for kp in &topic.knowledge_points {
            let mut seen = std::collections::BTreeSet::new();
            for exemplar in &kp.exemplars {
                if !seen.insert(exemplar.problem.trim()) {
                    failures.push(format!(
                        "{}/{}: repeated problem statement: {}",
                        topic.id.as_str(),
                        kp.id.as_str(),
                        exemplar.problem
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
