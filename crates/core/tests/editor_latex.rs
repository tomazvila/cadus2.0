//! The keypad of the answer editor (`web/src/lib/keypad.ts`) inserts LaTeX. Each row of
//! `web/test/fixtures/editor-latex.json` holds the LaTeX of one key with its slots filled by
//! sample tokens, and the answer key it must equal. The checker reads that LaTeX, so a key
//! the checker cannot read must leave the keypad.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use serde_json::Value;

fn rows() -> Vec<Value> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/test/fixtures/editor-latex.json"
    );
    let text = std::fs::read_to_string(path).expect("the fixture exists");
    let doc: Value = serde_json::from_str(&text).expect("the fixture is JSON");
    doc["rows"].as_array().expect("rows is a list").clone()
}

fn contract(row: &Value) -> AnswerContract {
    serde_json::from_value(row["contract"].clone()).expect("the row contract is valid")
}

fn correct(key: &str, learner: &str, contract: &AnswerContract) -> bool {
    matches!(
        check_contract(key, learner, contract.clone()),
        Outcome::Decided(verdict) if verdict.correct
    )
}

#[test]
fn every_fixture_row_grades_correct() {
    let rows = rows();
    let mut wrong = Vec::new();
    for row in &rows {
        let latex = row["latex"].as_str().unwrap();
        let answer = row["answer"].as_str().unwrap();
        if !correct(answer, latex, &contract(row)) {
            wrong.push(format!(
                "key {}: latex {latex:?} against {answer:?}",
                row["key"]
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "rows that grade wrong:\n{}",
        wrong.join("\n")
    );
    assert!(rows.len() >= 10, "only {} rows", rows.len());
}

#[test]
fn the_fixture_has_a_row_for_each_structural_key() {
    let keys: Vec<String> = rows()
        .iter()
        .map(|r| r["key"].as_str().unwrap().to_owned())
        .collect();
    for structural in [
        "\\frac{#@}{#0}",
        "\\sqrt{#0}",
        "#@^{#?}",
        "#@\\frac{#?}{#?}",
        "\\sqrt[#?]{#0}",
    ] {
        assert!(
            keys.iter().any(|k| k == structural),
            "no row for {structural}"
        );
    }
}

#[test]
fn a_wrong_value_in_the_same_shape_grades_wrong() {
    let exact = AnswerContract::Exact;
    assert!(!correct("3/4", "\\frac{3}{5}", &exact));
    assert!(!correct("sqrt(2)", "\\sqrt{3}", &exact));
}
