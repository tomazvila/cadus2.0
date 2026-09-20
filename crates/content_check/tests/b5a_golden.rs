//! Lane B5a: the new keys of the four golden rows of the freeze pack, and the
//! `function` kind that lane B4b adds.
#![allow(clippy::unwrap_used)]

use serde_json::{Value, json};

#[path = "b5a_support.rs"]
mod support;

/// The golden rows of the freeze pack. `FLOW_SPEC_GOLDEN` names a different place.
const GOLDEN: &str = "/home/deploy/.cache/cadus2_scripts/flow/spec/golden";

// ---- the golden rows ----

/// Each new key of the four golden rows, not `function`, not `p3`.
fn golden_keys() -> Vec<(String, Value, String)> {
    let dir = std::env::var("FLOW_SPEC_GOLDEN").unwrap_or_else(|_| GOLDEN.to_owned());
    let mut keys = Vec::new();
    for name in [
        "calc-chain-rule",
        "linalg-inverse",
        "proof-induction",
        "stats-ci",
    ] {
        let path = format!("{dir}/{name}.row.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("skip {path}: the freeze pack is not on this machine");
            continue;
        };
        let row: Value = serde_json::from_str(&text).unwrap();
        for item in row["items"].as_array().unwrap() {
            let contract = &item["answer_contract"];
            if item["answer"].is_string() && item["p3"] != true && contract["kind"] != "function" {
                let id = format!("{name}:{}", item["id"].as_str().unwrap());
                keys.push((
                    id,
                    contract.clone(),
                    item["answer"].as_str().unwrap().to_owned(),
                ));
            }
        }
    }
    keys
}

/// The same keys, as a copy, so that the test also runs where the pack is absent.
fn golden_copy() -> Vec<(String, Value, String)> {
    let matrix = json!({"kind": "matrix", "rows": 2, "cols": 2});
    let pair = json!({"kind": "coordinates", "arity": 2});
    let exact = json!({"kind": "exact"});
    let steps = json!({"kind": "label",
        "options": [["Step 1"], ["Step 2"], ["Step 3"], ["Step 4"]]});
    let forms = json!({"kind": "label", "options": [["$4(4^k - 1) + 3$"],
        ["$4(4^k - 1) + 4$"], ["$4(4^k - 1) - 3$"], ["$(4^k - 1) + 3$"]]});
    [
        ("calc-chain-rule:n1", &exact, "3"),
        ("linalg-inverse:n1", &matrix, "[2,-7;-1,4]"),
        ("linalg-inverse:n2", &matrix, "[-1,3;1,-2]"),
        ("linalg-inverse:n3", &matrix, "[-2,1;3/2,-1/2]"),
        ("linalg-inverse:n4", &matrix, "[3,5;1,2]"),
        ("proof-induction:n1", &exact, "n^2+n+1"),
        ("proof-induction:n2", &exact, "n+1"),
        ("proof-induction:n3", &exact, "5"),
        ("proof-induction:n4", &exact, "3"),
        ("proof-induction:n5", &steps, "Step 3"),
        ("proof-induction:n6", &forms, "$4(4^k - 1) + 3$"),
        ("stats-ci:n1", &pair, "(496, 504)"),
        ("stats-ci:n2", &pair, "(114, 126)"),
        ("stats-ci:n3", &exact, "10"),
        ("stats-ci:n4", &exact, "2"),
    ]
    .map(|(id, contract, key)| (id.to_owned(), contract.clone(), key.to_owned()))
    .to_vec()
}

#[test]
fn golden_copy_is_equal_to_the_freeze_pack() {
    let keys = golden_keys();
    if !keys.is_empty() {
        assert_eq!(keys, golden_copy());
    }
}

#[test]
fn each_golden_key_grades_correct_and_its_mutants_pass() {
    let keys = golden_copy();
    let grade_lines: Vec<String> = keys
        .iter()
        .map(|(id, contract, key)| {
            json!({"id": id, "contract": contract, "expected": key, "learner": key}).to_string()
        })
        .collect();
    let result = support::run(&[
        "grade",
        "--batch",
        &support::batch_file("b5a_golden_grade.jsonl", &grade_lines),
    ]);
    assert_eq!(result.exit, 0);
    let docs = result.lines();
    assert_eq!(docs.len(), keys.len());
    for (doc, (id, _, _)) in docs.iter().zip(&keys) {
        assert_eq!(doc["id"], id.as_str());
        assert_eq!(doc["verdict"], "correct", "{doc}");
    }
    let result = support::run(&[
        "mutants",
        "--batch",
        &support::batch_file("b5a_golden_mutants.jsonl", &grade_lines),
    ]);
    assert_eq!(result.exit, 0);
    for (doc, (id, _, _)) in result.lines().iter().zip(&keys) {
        assert_eq!(doc["id"], id.as_str());
        assert_eq!(doc["pass"], true, "{doc}");
    }
}

// ---- the `function` kind ----

#[test]
fn function_contract_is_exit_2_until_lane_b4b_merges() {
    // FLOW: remove this test after B4b; the test below takes its place.
    let contract = r#"{"kind":"function","vars":["x"]}"#;
    if support::grade(contract, "x^2", "x^2").exit == 0 {
        return;
    }
    support::assert_error(&support::grade(contract, "x^2", "x^2"), 2);
    support::assert_error(&support::mutants(contract, "x^2"), 2);
}

/// The `function` keys of the golden rows, with the pack v7 rules of lane B4b.
#[test]
fn function_keys_of_the_golden_rows_pass_after_b4b() {
    for key in ["x/sqrt(x^2+9)", "-3/(3x+1)^2"] {
        let doc = support::mutants(r#"{"kind":"function","vars":["x"]}"#, key).doc();
        assert_eq!(doc["key_verdict"], "correct", "{doc}");
        assert_eq!(support::mutant_learners(&doc), [format!("2*({key}) + x")]);
        assert_eq!(doc["mutants"][0]["rule"], "function-2x");
        assert_eq!(doc["pass"], true);
    }
    let contract = r#"{"kind":"function","vars":["x"],"up_to_constant":true}"#;
    let doc = support::mutants(contract, "y = x^2/2 + C").doc();
    assert_eq!(support::mutant_learners(&doc), ["2*(x^2/2) + x"]);
    assert_eq!(doc["pass"], true);
}
