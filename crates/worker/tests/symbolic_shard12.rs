//! Current-gate and semantic regression for linear factoring.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{rows, verify_rows};

const KEYS: &[&str] = &[
    "factoring-linear-expressions/kp1",
    "factoring-linear-expressions/kp2",
];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard12-templates.json"];

#[test]
fn factored_families_pass_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}
#[test]
fn recipes_extract_the_actual_gcf_and_include_signed_two_variable_cases() {
    for row in rows(PATHS) {
        let body = &row["body"];
        assert_eq!(body["answer_contract"]["form"], "factored_linear");
        assert_eq!(body["samples"].as_array().unwrap().len(), 16);
    }
    let rows = rows(PATHS);
    let signed = rows.iter().find(|r| r["kp_id"] == KEYS[1]).unwrap();
    assert!(
        signed["body"]["answer_expr"]
            .as_str()
            .unwrap()
            .starts_with("-gcd")
    );
}
