//! The "How to type answers" panel lists examples in `web/src/lib/notation-examples.json`.
//! Each row must be true: the typed spelling and the LaTeX spelling both grade correct
//! against the key under the contract the row names (`exact` when it names none).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use serde_json::Value;

fn examples() -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/src/lib/notation-examples.json"
    );
    let text = std::fs::read_to_string(path).expect("the examples file exists");
    serde_json::from_str(&text).expect("the examples file is JSON")
}

fn correct(key: &str, learner: &str, contract: &AnswerContract) -> bool {
    matches!(
        check_contract(key, learner, contract.clone()),
        Outcome::Decided(verdict) if verdict.correct
    )
}

#[test]
fn every_example_row_grades_correct_in_both_spellings() {
    let doc = examples();
    let mut checked = 0;
    for section in doc["sections"].as_array().unwrap() {
        for row in section["rows"].as_array().unwrap() {
            let typed = row["type"].as_str().unwrap();
            let latex = row["latex"].as_str().unwrap();
            let key = row.get("key").map_or(typed, |k| k.as_str().unwrap());
            let contract = row.get("contract").map_or_else(
                || AnswerContract::Exact,
                |c| serde_json::from_value(c.clone()).expect("the row contract is valid"),
            );
            assert!(
                correct(key, typed, &contract),
                "typed {typed:?} against {key:?}"
            );
            assert!(
                correct(key, latex, &contract),
                "latex {latex:?} against {key:?}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 60, "only {checked} rows");
}

#[test]
fn every_row_has_a_want_a_typed_spelling_and_a_latex_spelling() {
    for section in examples()["sections"].as_array().unwrap() {
        assert!(section["name"].as_str().is_some_and(|n| !n.is_empty()));
        for row in section["rows"].as_array().unwrap() {
            for field in ["want", "type", "latex"] {
                assert!(
                    row[field].as_str().is_some_and(|s| !s.is_empty()),
                    "{row} lacks {field}"
                );
            }
        }
    }
}
