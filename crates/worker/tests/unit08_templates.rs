//! Unit08 drafts pass the production gate, exhaust the domain, and stay independent.
#![allow(clippy::unwrap_used)]
mod common;

use cadus_worker::authoring::{job::verify_kind, prompt::Kind};
use common::reviewed_templates::{
    assert_report_with_instance_overrides, assert_standard_negative, directory_rows, run_rows, spec,
};
use serde_json::json;

fn drafts() -> Vec<serde_json::Value> {
    directory_rows("docs/content-foundations/functions-exponentials/templates")
}

#[test]
fn all_pending_templates_exhaust_the_real_gate_and_avoid_authored_and_sibling_problems() {
    let rows = drafts();
    assert_eq!(rows.len(), 78);
    let report = run_rows(&rows, "target/unit08/regression");
    // Reviewed domains: 75 ordinary twelve-case rows, one 24-case rate row,
    // eleven fresh same-base equations, and three eligible natural-exponential cases.
    assert_report_with_instance_overrides(
        &report,
        78,
        938,
        &[
            ("percent-growth-decay-factors/kp3", 24),
            ("exponential-equations-same-base/kp3", 11),
            ("natural-exponential-function/kp2", 3),
        ],
    );
    for row in report["rows"].as_array().unwrap() {
        let key = row["kp_id"].as_str().unwrap();
        let finite = row["evidence"]["finite_cases"].as_array().unwrap();
        let expected: std::collections::BTreeSet<(String, String)> = match key {
            "exponential-equations-same-base/kp3" => [4, 5, 10]
                .into_iter()
                .flat_map(|base| {
                    (3..=6)
                        .filter(move |power| base != 5 || *power != 6)
                        .map(move |power| {
                            (
                                format!("base-exponent-{base}-{power}"),
                                "practice_fresh".to_owned(),
                            )
                        })
                })
                .collect(),
            "natural-exponential-function/kp2" => [
                ("exponent-1".to_owned(), "practice_fresh".to_owned()),
                ("exponent-2".to_owned(), "taught_rehearsal".to_owned()),
                ("exponent-3".to_owned(), "practice_fresh".to_owned()),
            ]
            .into_iter()
            .collect(),
            _ => std::collections::BTreeSet::new(),
        };
        let actual: std::collections::BTreeSet<_> = finite
            .iter()
            .map(|case| {
                (
                    case["case_id"].as_str().unwrap().to_owned(),
                    case["role"].as_str().unwrap().to_owned(),
                )
            })
            .collect();
        assert_eq!(actual, expected, "{key}: exact reviewed finite roles");
        assert_eq!(finite.len(), expected.len(), "{key}: duplicate finite case");
        if expected.is_empty() {
            assert!(
                row["evidence"]["finite_policy_fingerprint"].is_null(),
                "{key}"
            );
        } else {
            let current = spec(key);
            let policy = current.finite.as_ref().unwrap();
            policy.validate(key).unwrap();
            assert_eq!(
                row["evidence"]["finite_policy_fingerprint"], policy.fingerprint,
                "{key}"
            );
        }
    }
}

#[test]
fn wrong_samples_small_spaces_and_wrong_contracts_are_rejected() {
    assert_standard_negative(drafts(), "evaluating-functions/kp1");
}

#[test]
fn label_templates_reject_wrong_samples_and_hidden_answer_bindings() {
    let row = drafts()
        .into_iter()
        .find(|r| r["kp_id"] == "exponential-functions/kp1")
        .unwrap();
    let spec = spec("exponential-functions/kp1");
    let mut wrong = row["arguments"].clone();
    wrong["samples"][0]["expected"] = json!("decay");
    assert_eq!(
        verify_kind(Kind::Template, &spec, &wrong, &[])
            .unwrap_err()
            .code,
        "sample-agreement"
    );
    let mut hidden = row["arguments"].clone();
    hidden["statement"] = json!("Does $f(x)=3\\cdot({a})^x$ model growth or decay?");
    assert_eq!(
        verify_kind(Kind::Template, &spec, &hidden, &[])
            .unwrap_err()
            .code,
        "hidden-parameter"
    );
}

#[test]
fn inequality_templates_reject_wrong_samples_and_unsafe_variables() {
    let row = drafts()
        .into_iter()
        .find(|r| r["kp_id"] == "domain-range/kp2")
        .unwrap();
    let spec = spec("domain-range/kp2");
    let mut wrong = row["arguments"].clone();
    wrong["samples"][0]["expected"] = json!("x >= 1");
    assert_eq!(
        verify_kind(Kind::Template, &spec, &wrong, &[])
            .unwrap_err()
            .code,
        "sample-agreement"
    );
    let mut unsafe_variable = row["arguments"].clone();
    unsafe_variable["params"]["x"]["values"] = json!(["x or y"]);
    for sample in unsafe_variable["samples"].as_array_mut().unwrap() {
        sample["params"]["x"] = json!("x or y");
    }
    assert!(verify_kind(Kind::Template, &spec, &unsafe_variable, &[]).is_err());
}
