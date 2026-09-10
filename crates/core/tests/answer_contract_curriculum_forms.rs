//! Requested forms remain observable in the authored curriculum.
#![allow(clippy::unwrap_used)]
mod common;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::canonical_dump;
use serde_json::Value;

fn exemplars(topic_id: &str, kp_id: &str) -> Vec<Value> {
    let tree: Value = serde_json::from_str(&canonical_dump(common::events::tree())).unwrap();
    let topic = tree["topics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|topic| topic["id"] == topic_id)
        .unwrap();
    let kp = topic["knowledge_points"]
        .as_array()
        .unwrap()
        .iter()
        .find(|kp| kp["id"] == kp_id)
        .unwrap();
    kp["exemplars"].as_array().unwrap().clone()
}

fn accepts(given: &str, expected: &str, contract: AnswerContract) -> bool {
    matches!(check_contract(given, expected, contract), Outcome::Decided(v) if v.correct)
}

#[test]
fn same_base_power_tasks_reject_evaluated_numbers() {
    for (topic, evaluated) in [
        ("exponent-product-rule", ["128", "3125", "729", "117649"]),
        ("exponent-quotient-rule", ["27", "7", "625", "8"]),
        (
            "power-of-a-power-rule",
            ["64", "1000000", "6561", "1953125"],
        ),
    ] {
        let items = exemplars(topic, "kp2");
        assert_eq!(items.len(), evaluated.len());
        for (item, number) in items.iter().zip(evaluated) {
            let expected = item["answer"].as_str().unwrap();
            let contract: AnswerContract =
                serde_json::from_value(item["answer_contract"].clone()).unwrap();
            assert_eq!(contract, AnswerContract::RequiredSinglePower);
            assert!(accepts(number, expected, AnswerContract::Exact));
            assert!(accepts(expected, expected, contract.clone()));
            assert!(!accepts(number, expected, contract));
        }
    }
}

#[test]
fn conversion_tasks_reject_equivalent_answers_in_the_input_notation() {
    for (kp, unchanged_form) in [
        ("kp2", ["x > 2", "x <= -1", "[4, ∞)", "(-∞, 3]"]),
        (
            "kp3",
            [
                "x < -2 or x > 4",
                "x <= -2 or x > 3",
                "(-∞, 1) ∪ [5, ∞)",
                "x <= -5 or x > 4",
            ],
        ),
    ] {
        let items = exemplars("interval-notation", kp);
        assert_eq!(items.len(), unchanged_form.len());
        for (item, given) in items.iter().zip(unchanged_form) {
            let expected = item["answer"].as_str().unwrap();
            let contract: AnswerContract =
                serde_json::from_value(item["answer_contract"].clone()).unwrap();
            assert_eq!(contract, AnswerContract::RequiredInequalityNotation);
            assert!(accepts(given, expected, AnswerContract::InequalityUnion));
            assert!(accepts(expected, expected, contract.clone()));
            assert!(!accepts(given, expected, contract));
        }
    }
}
