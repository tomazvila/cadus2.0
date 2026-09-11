//! Validates the seven deterministic pending-template recipes of
//! `scripts/authoring/polynomial_quadratic_templates.py` against the REAL
//! answer grammar and the REAL curriculum — not just the Python generator's
//! own self-checks. Each instance must: decide under its declared policy;
//! differ from every already-authored exemplar of its KP; and, for a
//! sample, reject a wrong answer (so a template is not merely parseable,
//! but actually gradable).
#![allow(clippy::unwrap_used, clippy::panic)]
use std::path::Path;

use cadus_core::answer::{AnswerContract, Outcome, Verdict, canonical_form, check_contract};
use cadus_core::curriculum::load_curriculum;
use serde::Deserialize;
use std::collections::BTreeMap;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[derive(Debug, Deserialize)]
struct Instance {
    problem: String,
    answer: String,
    contract: Option<String>,
}

fn fixture() -> BTreeMap<String, Vec<Instance>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/polynomial_quadratic_templates.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn decide(instance: &Instance) -> Outcome {
    match instance.contract.as_deref() {
        Some("exact") => check_contract(&instance.answer, &instance.answer, AnswerContract::Exact),
        None => match canonical_form(&instance.answer) {
            Ok(_) => Outcome::Decided(Verdict {
                correct: true,
                notation: false,
            }),
            Err(reason) => Outcome::Undecidable(reason),
        },
        Some(other) => panic!("unhandled fixture contract kind: {other}"),
    }
}

#[test]
fn the_fixture_covers_seven_kps_with_five_instances_each() {
    let fixture = fixture();
    assert_eq!(fixture.len(), 7);
    for (kp_key, instances) in &fixture {
        assert_eq!(instances.len(), 5, "{kp_key}");
    }
}

#[test]
fn every_template_instance_is_grammar_decidable() {
    let fixture = fixture();
    let mut failures = Vec::new();
    for (kp_key, instances) in &fixture {
        for instance in instances {
            if !matches!(
                decide(instance),
                Outcome::Decided(Verdict { correct: true, .. })
            ) {
                failures.push(format!(
                    "{kp_key}: {} -> {}",
                    instance.problem, instance.answer
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn no_template_instance_repeats_another_instance_of_its_own_kp() {
    let fixture = fixture();
    for (kp_key, instances) in &fixture {
        let mut seen = std::collections::BTreeSet::new();
        for instance in instances {
            assert!(
                seen.insert(instance.problem.trim()),
                "{kp_key}: repeated template problem: {}",
                instance.problem
            );
        }
    }
}

#[test]
fn no_template_instance_repeats_an_already_authored_exemplar() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let fixture = fixture();
    let mut failures = Vec::new();
    for topic in curriculum.topics() {
        for kp in &topic.knowledge_points {
            let key = format!("{}/{}", topic.id.as_str(), kp.id.as_str());
            let Some(instances) = fixture.get(&key) else {
                continue;
            };
            for exemplar in &kp.exemplars {
                for instance in instances {
                    if instance.problem.trim() == exemplar.problem.trim() {
                        failures.push(format!(
                            "{key}: template repeats an authored problem: {}",
                            instance.problem
                        ));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_sample_of_template_instances_reject_a_wrong_answer() {
    let fixture = fixture();
    // completing-the-square/kp2, seed 0: "x^2 + 2x - 8 = 0" -> "x = 2 or x = -4".
    let instance = &fixture["completing-the-square/kp2"][0];
    assert_eq!(instance.answer, "x = 2 or x = -4");
    let wrong = check_contract("x = 2 or x = -4", "x = -2 or x = 4", AnswerContract::Exact);
    assert!(matches!(
        wrong,
        Outcome::Decided(Verdict { correct: false, .. })
    ));

    // discriminant/kp1, seed 2: "3x^2 - 4x + 2" -> discriminant -8.
    let instance = &fixture["discriminant/kp1"][2];
    assert_eq!(instance.answer, "-8");
    let wrong = check_contract("-8", "8", AnswerContract::Exact);
    assert!(matches!(
        wrong,
        Outcome::Decided(Verdict { correct: false, .. })
    ));
}
