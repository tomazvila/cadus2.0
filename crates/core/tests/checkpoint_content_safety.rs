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
    // "When x^2 + 5 = 3x is written as ax^2 + bx + c = 0 with a = 1, what is b?"
    let exemplar = &kp(&unit, "applying-the-quadratic-formula", "kp1").exemplars[0];
    assert!(
        exemplar.problem.contains("written as $ax^2 + bx + c = 0$")
            && exemplar.problem.contains("$a = 1$"),
        "the prompt must bind standard form and its scale to the graded coefficient"
    );
    assert_eq!(exemplar.answer, "-3");
    assert_eq!(exemplar.answer_contract, Some(AnswerContract::Exact));
    // "3" reads b off the unrearranged equation; "-6" is the coefficient of
    // the doubled standard form, which the fixed scale a = 1 excludes.
    for wrong in ["3", "-6"] {
        assert!(matches!(
            check_contract(&exemplar.answer, wrong, AnswerContract::Exact),
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
