//! The tests of the `choices` key of the serve payload.

use super::super::fixture::{graph, task};
use super::tests::served;
use super::*;

use cadus_core::answer::AnswerPart;

/// A Label contract of four steps, one alias for each step.
fn steps() -> AnswerContract {
    AnswerContract::Label {
        options: ["Step 1", "Step 2", "Step 3", "Step 4"]
            .iter()
            .map(|text| vec![(*text).to_string()])
            .collect(),
    }
}

/// The live problem of `addition` with `contract` and the key `Step 3`.
fn served_with(contract: AnswerContract) -> ServedProblem {
    let mut problem = served(Some("addition"));
    problem.expected.answer_contract = Some(contract);
    problem.expected.answer = "Step 3".to_string();
    problem
}

/// The sorted key names of a payload.
fn keys_of(payload: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = payload
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

#[test]
fn a_label_item_carries_the_choices_of_its_problem_id() {
    let problem = served_with(steps());
    let lesson = task(TaskType::Lesson, Some("addition"));
    let payload = serve_payload(&problem, &lesson, &graph(), 45, None);
    let expected = label_choices(Some(&steps()), &problem.problem_id).unwrap();
    assert_eq!(payload["choices"], json!(expected));
    assert_eq!(expected.len(), 4);
    // A second serve of the same problem gives the same order.
    let again = serve_payload(&problem, &lesson, &graph(), 45, None);
    assert_eq!(again["choices"], payload["choices"]);
    assert_eq!(
        keys_of(&payload),
        [
            "choices",
            "countdown",
            "index",
            "kp",
            "problem_id",
            "text",
            "time_budget_secs",
            "total"
        ]
    );
}

#[test]
fn a_quiz_serve_of_a_label_item_carries_the_choices_and_the_clock() {
    let quiz = task(TaskType::Quiz, None);
    let payload = serve_payload(&served_with(steps()), &quiz, &graph(), 45, Some(90));
    assert_eq!(payload["choices"].as_array().unwrap().len(), 4);
    assert_eq!(payload["quiz_elapsed_secs"], 90);
}

#[test]
fn each_other_contract_keeps_the_key_set_of_today() {
    let multipart = AnswerContract::Multipart {
        parts: vec![AnswerPart {
            name: "step".to_string(),
            contract: steps(),
        }],
    };
    let lesson = task(TaskType::Lesson, Some("addition"));
    let problems = [
        served(Some("addition")),
        served_with(AnswerContract::Exact),
        served_with(multipart),
        served_with(AnswerContract::None),
    ];
    for problem in problems {
        let payload = serve_payload(&problem, &lesson, &graph(), 45, None);
        assert_eq!(
            keys_of(&payload),
            [
                "countdown",
                "index",
                "kp",
                "problem_id",
                "text",
                "time_budget_secs",
                "total"
            ],
            "{payload}"
        );
    }
}

#[test]
fn a_feedback_practice_serve_stops_the_countdown() {
    let mut problem = served(Some("addition"));
    problem.rework = Some(json!({"digest": "d1"}));
    let drill = task(TaskType::Drill, Some("addition"));
    let payload = serve_payload(&problem, &drill, &graph(), 45, None);
    assert_eq!(payload["feedback_practice"], true);
    assert_eq!(payload["countdown"], false);
    assert!(payload.get("choices").is_none());
}

#[test]
fn a_self_check_item_shows_its_solution_and_no_choices() {
    let mut problem = served_with(AnswerContract::None);
    problem.solution_sketch = Some("Add the two numbers.".to_string());
    let lesson = task(TaskType::Lesson, Some("addition"));
    let payload = serve_payload(&problem, &lesson, &graph(), 45, None);
    assert_eq!(payload["solution"], "Add the two numbers.");
    assert!(payload.get("choices").is_none());
}
