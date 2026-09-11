//! Unit08 drafts pass the production gate, exhaust the domain, and stay independent.
#![allow(clippy::unwrap_used)]
mod common;

use cadus_worker::authoring::{job::verify_kind, prompt::Kind};
use common::reviewed_templates::{directory_rows, spec};
use serde_json::json;

fn drafts() -> Vec<serde_json::Value> {
    directory_rows("docs/content-foundations/functions-exponentials/templates")
}

crate::reviewed_template_tests!(
    "docs/content-foundations/functions-exponentials/templates",
    78,
    "target/unit08/regression",
    936,
    "evaluating-functions/kp1"
);

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
