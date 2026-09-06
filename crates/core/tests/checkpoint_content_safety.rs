//! Regression guards for content changes excluded from the filtered unit checkpoints.
#![allow(clippy::unwrap_used)]

use std::path::Path;

use cadus_core::answer::AnswerContract;
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
    let exemplar = &kp(&unit, "applying-the-quadratic-formula", "kp1").exemplars[1];
    assert!(
        exemplar.answer.contains("x^2 - 4x + 3 = 0"),
        "the graded answer must preserve the prompt's standard-form request"
    );
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
