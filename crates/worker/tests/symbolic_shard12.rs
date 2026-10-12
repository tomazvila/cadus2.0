//! Current-gate and semantic regression for linear factoring.
//!
//! The Foundations merge (2026-10-12) dropped both knowledge points of this
//! shard as twins of factoring-gcf/kp1 and kp2. Their recipes left the pending
//! set and are in `docs/reports/symbolic-repair-retired-pending-templates.json`,
//! keyed by the twin steps.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::assert_shard_retired;

const KEYS: &[&str] = &["factoring-gcf/kp1", "factoring-gcf/kp2"];
const PATH: &str = "docs/content-foundations/symbolic-repair/shard12-templates.json";

#[test]
fn factored_families_are_retired_with_their_recorded_gate_verdict() {
    assert_shard_retired(PATH, KEYS);
}

#[test]
fn recipes_extract_the_actual_gcf_and_include_signed_two_variable_cases() {
    let rows: Vec<_> = common::retired::report("symbolic-repair")
        .into_iter()
        .filter(|row| row["source"] == PATH)
        .collect();
    assert_eq!(rows.len(), 2);
    for row in &rows {
        let body = &row["body"]["body"];
        assert_eq!(body["answer_contract"]["form"], "factored_linear");
        assert_eq!(body["samples"].as_array().unwrap().len(), 16);
    }
    let signed = rows.iter().find(|r| r["kp_key"] == KEYS[1]).unwrap();
    assert!(
        signed["body"]["body"]["answer_expr"]
            .as_str()
            .unwrap()
            .starts_with("-gcd")
    );
}
