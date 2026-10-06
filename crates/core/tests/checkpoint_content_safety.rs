//! Regression guards for content changes excluded from the filtered unit checkpoints.
#![allow(clippy::unwrap_used)]

use std::path::Path;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::{KnowledgePoint, Unit};

fn unit() -> Unit {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../curriculum/foundations/07-polynomials-quadratics.yaml");
    serde_norway::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn kp<'a>(unit: &'a Unit, topic_id: &str, kp_id: &str) -> &'a KnowledgePoint {
    unit.topics
        .iter()
        .find(|topic| topic.id.as_str() == topic_id)
        .unwrap()
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == kp_id)
        .unwrap()
}

#[test]
fn standard_form_request_remains_in_the_authored_answer() {
    let unit = unit();
    // "Write x^2 + 5 = 3x in the form ax^2 + bx + c = 0, and identify a, b
    // and c." The item asks for all three coefficients now, not b alone.
    let exemplar = &kp(&unit, "applying-the-quadratic-formula", "kp1").exemplars[0];
    assert!(
        exemplar.problem.contains("in the form $ax^2 + bx + c = 0$"),
        "the prompt must ask for the standard form before the coefficients"
    );
    assert_eq!(exemplar.answer, "a = 1; b = -3; c = 5");
    let contract = exemplar.answer_contract.clone().unwrap();
    assert!(matches!(contract, AnswerContract::Multipart { .. }));
    assert!(matches!(
        check_contract(&exemplar.answer, "1, -3, 5", contract.clone()),
        Outcome::Decided(verdict) if verdict.correct
    ));
    // "1, 3, 5" reads b off the unrearranged equation; "2, -6, 10" is the
    // doubled standard form, not the one the rearrangement gives.
    for wrong in ["1, 3, 5", "2, -6, 10"] {
        assert!(matches!(
            check_contract(&exemplar.answer, wrong, contract.clone()),
            Outcome::Decided(verdict) if !verdict.correct
        ));
    }
}

#[test]
fn monic_prompts_do_not_use_a_scale_invariant_contract() {
    let unit = unit();
    for kp_id in ["kp1", "kp2"] {
        for exemplar in &kp(&unit, "writing-quadratics-from-roots", kp_id).exemplars {
            assert!(
                !matches!(
                    exemplar.answer_contract.as_ref(),
                    Some(AnswerContract::PolynomialRelation)
                ),
                "polynomial_relation accepts scalar multiples and cannot enforce monic form"
            );
        }
    }
}
