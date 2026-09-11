//! Grading one integrated task: per step, then the final answer (D-F10).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::AnswerContract;
use cadus_core::integrated::{FieldResponse, IntegratedItem, Submission, grade};

/// A rates item with an approximate final answer and an alternate step form.
const RATE_ITEM: &str = r#"
id: integrated-fuel-run
title: Plan the delivery run
course: foundations
topic: measurement-units
component_topics: [measurement-units]
domain: rates_units
scenario: A van covers 180 km on a delivery run and uses 12 L of fuel.
given:
  - label: Distance
    value: 180 km
  - label: Fuel used
    value: 12 L
steps:
  - id: fuel-rate
    ask:
      prompt: How many kilometers does the van cover on one liter?
      answer: "15"
      accept_also: ["180/12"]
      unit: km/L
      hints:
        - A rate divides one quantity by another.
    skills: [unit-rates/kp1]
  - id: cost-share
    ask:
      prompt: The fuel costs 1.80 per liter. What is the cost of the run?
      answer: "21.6"
      contract:
        kind: approx
        decimals: 2
      hints:
        - Multiply the liters by the price of one liter.
    skills: [decimal-operations/kp1]
final:
  ask:
    prompt: What is the fuel cost of one kilometer, to 3 decimals?
    answer: "0.12"
    contract:
      kind: approx
      decimals: 3
    hints:
      - Divide the cost of the run by the distance.
  interpretation: The run costs 0.120 per kilometer, so a 50 km detour costs 6.00.
  skills: [unit-rates/kp1]
"#;

fn item() -> IntegratedItem {
    serde_norway::from_str(RATE_ITEM).expect("the fixture parses")
}

fn response(id: &str, answer: &str) -> FieldResponse {
    FieldResponse {
        id: id.into(),
        answer: answer.into(),
        hints_used: 0,
    }
}

fn complete_submission(final_answer: &str, reasoning: Option<&str>) -> Submission {
    Submission {
        method: None,
        steps: vec![response("fuel-rate", "15"), response("cost-share", "21.60")],
        final_answer: response("final", final_answer),
        reasoning: reasoning.map(str::to_owned),
    }
}

#[test]
fn the_authored_contract_of_every_field_is_read() {
    let item = item();
    assert_eq!(item.steps[0].ask.contract, AnswerContract::Exact);
    assert_eq!(
        item.steps[1].ask.contract,
        AnswerContract::Approx { decimals: 2 }
    );
    assert_eq!(
        item.final_answer.ask.contract,
        AnswerContract::Approx { decimals: 3 }
    );
}

#[test]
fn a_whole_correct_submission_credits_every_skill_once() {
    let item = item();
    let result = grade(&item, &complete_submission("0.120", Some("  ")));
    assert!(result.solved);
    assert!(!result.assisted);
    assert!(!result.ungraded);
    assert_eq!(result.correct_steps, 2);
    assert_eq!(result.total_steps, 2);
    assert_eq!(
        result.skills_credited,
        ["unit-rates/kp1", "decimal-operations/kp1"]
    );
    assert!(result.method.is_none());
    assert!(!result.reasoning_recorded, "blank prose is not a note");
    assert!(result.interpretation.contains("0.120 per kilometer"));
}

#[test]
fn an_alternate_authored_form_of_a_step_is_accepted() {
    let item = item();
    let result = grade(
        &item,
        &Submission {
            method: None,
            steps: vec![response("fuel-rate", "180/12")],
            final_answer: response("final", "0.120"),
            reasoning: None,
        },
    );
    assert!(result.steps[0].correct);
    assert!(!result.steps[1].answered, "the second step was left out");
    assert!(!result.steps[1].correct);
    assert_eq!(result.correct_steps, 1);
    // An unanswered step credits nothing, and its skill is not credited.
    assert_eq!(result.skills_credited, ["unit-rates/kp1"]);
}

#[test]
fn a_wrong_final_answer_keeps_the_credit_of_the_correct_steps() {
    let item = item();
    let result = grade(
        &item,
        &complete_submission("1.2", Some("I divided the distance by the cost.")),
    );
    assert!(!result.solved);
    assert!(!result.final_grade.correct);
    assert_eq!(result.correct_steps, 2);
    assert!(result.reasoning_recorded);
    // The prose is recorded and never graded: no field of the grade reads it.
    assert!(
        result
            .skills_credited
            .contains(&"decimal-operations/kp1".into())
    );
}

#[test]
fn an_opened_hint_marks_the_field_assisted() {
    let item = item();
    let result = grade(
        &item,
        &Submission {
            method: None,
            steps: vec![FieldResponse {
                id: "fuel-rate".into(),
                answer: "15".into(),
                hints_used: 1,
            }],
            final_answer: response("final", "0.120"),
            reasoning: None,
        },
    );
    assert!(result.steps[0].assisted);
    assert!(!result.final_grade.assisted);
    assert!(result.assisted, "the task is assisted when any field is");
}

#[test]
fn an_answer_the_checker_cannot_read_is_ungraded_and_credits_nothing() {
    let item = item();
    let result = grade(
        &item,
        &Submission {
            method: None,
            steps: vec![response("fuel-rate", "about fifteen")],
            final_answer: response("final", "0.120"),
            reasoning: None,
        },
    );
    assert!(result.steps[0].ungraded);
    assert!(!result.steps[0].correct);
    assert!(result.ungraded);
    assert_eq!(
        result.skills_credited,
        ["unit-rates/kp1"],
        "only the final field"
    );
}

#[test]
fn the_method_choice_is_graded_with_the_steps() {
    let mut item = item();
    item.method = Some(
        serde_norway::from_str(
            r#"
prompt: Which method finds the cost of one kilometer?
options:
  - id: cost-over-distance
    label: Divide the cost of the run by the distance.
    correct: true
    why: The unit of the answer is cost per kilometer.
  - id: distance-over-cost
    label: Divide the distance by the cost of the run.
    correct: false
    why: That answers kilometers per unit of cost.
"#,
        )
        .expect("the method parses"),
    );
    let submit = |chosen: Option<&str>| {
        grade(
            &item,
            &Submission {
                method: chosen.map(str::to_owned),
                steps: Vec::new(),
                final_answer: response("final", "0.120"),
                reasoning: None,
            },
        )
    };
    let right = submit(Some("cost-over-distance")).method.expect("a grade");
    assert!(right.correct);
    assert!(right.why.expect("the reason").contains("per kilometer"));
    let wrong = submit(Some("distance-over-cost")).method.expect("a grade");
    assert!(!wrong.correct);
    let none = submit(None).method.expect("a grade");
    assert!(!none.correct);
    assert!(none.chosen.is_none());
}
