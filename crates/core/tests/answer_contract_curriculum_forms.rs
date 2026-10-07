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

/// The evaluated number of a single-power key such as `3^3`.
fn evaluated(key: &str) -> String {
    let (base, exponent) = key.split_once('^').unwrap();
    let base: u128 = base.trim().parse().unwrap();
    let exponent: u32 = exponent.trim().parse().unwrap();
    base.pow(exponent).to_string()
}

/// Every single-power item of the three same-base KPs refuses its evaluated
/// number; the in-context items of those KPs ask for a count and take `exact`.
#[test]
fn same_base_power_tasks_reject_evaluated_numbers() {
    for topic in [
        "exponent-product-rule",
        "exponent-quotient-rule",
        "power-of-a-power-rule",
    ] {
        let mut strict = 0;
        for item in exemplars(topic, "kp2") {
            let expected = item["answer"].as_str().unwrap();
            let contract: AnswerContract =
                serde_json::from_value(item["answer_contract"].clone()).unwrap();
            if contract != AnswerContract::RequiredSinglePower {
                continue;
            }
            strict += 1;
            let number = evaluated(expected);
            assert!(accepts(&number, expected, AnswerContract::Exact));
            assert!(accepts(expected, expected, contract.clone()));
            assert!(!accepts(&number, expected, contract), "{topic}: {expected}");
        }
        assert!(strict >= 4, "{topic}: only {strict} single-power items");
    }
}

#[test]
fn conversion_tasks_reject_equivalent_answers_in_the_input_notation() {
    // Item index and the unchanged form of the input, per knowledge point.
    // Re-pinned 2026-10-07 after the courses rewrite: the rewritten kp2 holds
    // nine items, and the word problems and label items carry no notation to
    // repeat, so the table names the items that convert or solve to a notation.
    for (kp, unchanged_form) in [
        (
            "kp2",
            vec![(0, "x > 2"), (1, "x <= -1"), (3, "[4, ∞)"), (6, "(-∞, 3]")],
        ),
        (
            "kp3",
            vec![
                (0, "x < -2 or x > 4"),
                (1, "x <= -2 or x > 3"),
                (3, "(-∞, 1) ∪ [5, ∞)"),
                (4, "x <= -5 or x > 4"),
                (5, "x <= -1 or x >= 3"),
                (6, "x < -3 or x >= 3"),
            ],
        ),
    ] {
        let items = exemplars("interval-notation", kp);
        for (index, given) in unchanged_form {
            let item = &items[index];
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
