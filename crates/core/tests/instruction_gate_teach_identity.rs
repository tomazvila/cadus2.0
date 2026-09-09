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

#[test]
fn decimal_values_are_distinct_problem_identities() {
    let examples = [];
    let served = [("Compute $3.5$.", "3.5")];
    let body = r#"{"concept":"Read a decimal.","worked_example":{"problem":"Compute $35$.","steps":["The value is 35."]}}"#;
    gate_teach(body, &spec_with_instances(&examples, &served)).unwrap();
}

#[test]
fn grouping_changes_problem_identity() {
    let examples = [];
    let served = [("Compute $(1+2)*3$.", "9")];
    let body = r#"{"concept":"Use order of operations.","worked_example":{"problem":"Compute $1+2*3$.","steps":["Multiply first: 2*3=6, then add 1 to get 7."]}}"#;
    gate_teach(body, &spec_with_instances(&examples, &served)).unwrap();
}

#[test]
fn variable_case_changes_problem_identity() {
    let examples = [];
    let served = [("Simplify $A+1$.", "A+1")];
    let body = r#"{"concept":"Variables are case-sensitive.","worked_example":{"problem":"Simplify $a+1$.","steps":["The expression is a+1."]}}"#;
    gate_teach(body, &spec_with_instances(&examples, &served)).unwrap();
}

#[test]
fn wrappers_and_operator_aliases_preserve_problem_identity() {
    let examples = [];
    let served = [("Compute \\(54 \\div 6\\).", "9")];
    let body = r#"{"concept":"Division finds a missing factor.","worked_example":{"problem":"Compute $54 \\div 6$.","steps":["54 / 6 = 9."]}}"#;
    assert_eq!(
        gate_teach(body, &spec_with_instances(&examples, &served))
            .unwrap_err()
            .code,
        "teach-worked-example"
    );
}

#[test]
fn expression_boundary_does_not_match_a_larger_expression() {
    let examples = [];
    let served = [("Compute $2+3$.", "5")];
    let body = r#"{"concept":"Addition combines quantities.","worked_example":{"problem":"Compute $12+3-10$.","steps":["$12+3-10 = 5$."]}}"#;
    gate_teach(body, &spec_with_instances(&examples, &served)).unwrap();
}
