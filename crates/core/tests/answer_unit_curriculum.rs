//! The requested output, including a signed rate, determines a measured answer.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::canonical_dump;
use serde_json::Value;

fn item<'a>(node: &'a Value, question: &str) -> Option<&'a Value> {
    if node.get("problem").and_then(Value::as_str) == Some(question) {
        return Some(node);
    }
    match node {
        Value::Object(fields) => fields.values().find_map(|child| item(child, question)),
        Value::Array(children) => children.iter().find_map(|child| item(child, question)),
        _ => None,
    }
}

#[test]
fn measured_outputs_keep_their_requested_unit_and_dimension() {
    let tree: Value = serde_json::from_str(&canonical_dump(common::events::tree())).unwrap();
    for (question, equivalent, wrong) in [
        (
            "Convert $2$ square metres to square centimetres.",
            "2 m^2",
            "20000 m^2",
        ),
        (
            "Use $F=9C/5+32$ with $C=10$ to calculate the temperature in degrees Fahrenheit.",
            "10 degrees Celsius",
            "50 degrees Celsius",
        ),
        (
            "A tank falls from $80$ L at minute $2$ to $50$ L at minute $5$. Give the signed rate in L/min.",
            "-600 L/hour",
            "-10 L",
        ),
        (
            "A candle is $30$ cm tall at $t = 0$ and $18$ cm tall at $t = 4$ hours. Find the rate of change of its height, in cm per hour.",
            "-1/120000 m/s",
            "-3 cm",
        ),
    ] {
        let found = item(&tree, question).expect("the reviewed question remains in the curriculum");
        let key = found["answer"].as_str().unwrap();
        let contract: AnswerContract =
            serde_json::from_value(found["answer_contract"].clone()).unwrap();
        for correct in [key, equivalent] {
            assert!(
                matches!(check_contract(key, correct, contract.clone()), Outcome::Decided(verdict) if verdict.correct),
                "{question}: {correct}"
            );
        }
        assert!(
            matches!(check_contract(key, wrong, contract), Outcome::Decided(verdict) if !verdict.correct),
            "{question}: {wrong}"
        );
    }
}
