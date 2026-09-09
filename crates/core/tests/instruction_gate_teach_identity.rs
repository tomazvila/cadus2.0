#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use cadus_core::instruction::gate_teach;
use common::instruction_fixtures::spec_with_instances;

#[test]
fn unrelated_equal_result_is_accepted() {
    let examples = [];
    let served = [("Compute $63 / 7$.", "9")];
    let body = r#"{"concept":"Division finds a missing factor.","worked_example":{"problem":"Compute $54 \\div 6$.","steps":["$54 \\div 6 = 9$."]}}"#;
    gate_teach(body, &spec_with_instances(&examples, &served)).unwrap();
}

#[test]
fn normalized_math_wrapper_identity_is_refused() {
    let examples = [];
    let served = [("Compute $54 / 6$.", "9")];
    let body = r#"{"concept":"Division finds a missing factor.","worked_example":{"problem":"Compute $ 54 \\div {6} $.","steps":["$54 \\div 6 = 9$."]}}"#;
    assert_eq!(
        gate_teach(body, &spec_with_instances(&examples, &served))
            .unwrap_err()
            .code,
        "teach-worked-example"
    );
}

#[test]
fn secondary_served_expression_and_answer_is_refused() {
    let examples = [];
    let served = [("Compute $9^2$.", "81")];
    let body = r#"{"concept":"Squaring multiplies a number by itself.","worked_example":{"problem":"Compute $15^2$.","steps":["The product is 225, the same way $9^2$ is 81."]}}"#;
    assert_eq!(
        gate_teach(body, &spec_with_instances(&examples, &served))
            .unwrap_err()
            .code,
        "teach-answer"
    );
}
