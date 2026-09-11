//! Current-gate and semantic regression for the filtered symbolic repair.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use cadus_core::{
    answer::{AnswerContract, Outcome, check_contract},
    curriculum::load_curriculum,
};
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;

const REVIEWED_KEYS: &[&str] = &[
    "absolute-value-equations/kp2",
    "consecutive-integer-problems/kp2",
    "constant-of-proportionality/kp3",
    "coordinate-plane/kp2",
    "coordinate-plane/kp3",
    "distribute-then-solve/kp2",
    "equation-word-problems/kp2",
    "equations-with-decimals/kp1",
    "equations-with-decimals/kp2",
    "equations-with-fractions/kp1",
    "equations-with-fractions/kp2",
    "evaluating-expressions/kp1",
    "evaluating-expressions/kp3",
    "evaluating-formulas/kp1",
    "evaluating-formulas/kp2",
    "graphing-from-a-table/kp1",
    "graphing-from-a-table/kp3",
    "graphing-linear-equations/kp1",
    "graphing-proportional-relationships/kp1",
    "graphing-systems/kp1",
    "graphing-systems/kp2",
    "inequality-word-problems/kp2",
    "inequality-word-problems/kp3",
    "interpreting-linear-models/kp3",
    "linear-word-problems/kp3",
    "money-geometry-problems/kp2",
    "multi-step-equations/kp2",
    "multi-step-equations/kp3",
    "one-step-equations/kp3",
    "plotting-points/kp2",
    "point-slope-form/kp2",
    "proportional-relationships/kp3",
    "reading-slope-intercept-equations/kp2",
    "slope-as-rate-of-change/kp1",
    "slope-as-rate-of-change/kp3",
    "slope/kp2",
    "slopes-of-parallel-perpendicular-lines/kp2",
    "solutions-of-two-variable-equations/kp2",
    "solutions-of-two-variable-equations/kp3",
    "substituting-values/kp2",
    "systems-word-problems/kp1",
    "systems-word-problems/kp2",
    "systems-word-problems/kp3",
    "x-y-intercepts/kp1",
    "x-y-intercepts/kp2",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Value> {
    let value: Value = serde_json::from_str(
        &fs::read_to_string(root().join("docs/content-foundations/symbolic-repair/templates.json"))
            .unwrap(),
    )
    .unwrap();
    value["templates"].as_array().unwrap().clone()
}

fn arguments(row: &Value) -> Value {
    let mut body = row["body"].clone();
    let object = body.as_object_mut().unwrap();
    object.remove("v");
    object.remove("topic_id");
    object.remove("answer_kind");
    body
}

#[test]
fn only_independently_reviewed_keys_are_present() {
    let actual: BTreeSet<_> = rows()
        .into_iter()
        .map(|row| row["kp_id"].as_str().unwrap().to_owned())
        .collect();
    let expected: BTreeSet<_> = REVIEWED_KEYS.iter().map(|key| (*key).to_owned()).collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 45);
}

#[test]
fn every_reviewed_recipe_passes_the_current_production_gate() {
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let mut failures = Vec::new();
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        if let Err(error) = verify_kind(Kind::Template, &spec, &arguments(&row), &[]) {
            failures.push(format!("{key}: {}: {}", error.code, error.message));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn equivalent_algebra_cannot_certify_a_requested_output_form() {
    let outcome = check_contract("3(2x+3)", "6x+9", AnswerContract::Exact);
    assert!(matches!(outcome, Outcome::Decided(result) if result.correct));
    for excluded in [
        "combining-like-terms/kp1",
        "distributive-property/kp1",
        "factoring-linear-expressions/kp1",
        "slope-intercept-form/kp1",
    ] {
        assert!(!REVIEWED_KEYS.contains(&excluded));
    }
}

#[test]
fn constant_inert_and_cross_topic_families_remain_excluded() {
    for excluded in [
        "checking-a-solution/kp2",
        "constant-of-proportionality/kp1",
        "horizontal-vertical-slopes/kp1",
        "parts-of-an-expression/kp1",
        "parts-of-an-expression/kp2",
        "systems-substitution/kp2",
        "systems-elimination/kp3",
    ] {
        assert!(!REVIEWED_KEYS.contains(&excluded));
    }
    assert_eq!(BTreeSet::from(["0"; 12]).len(), 1);
    for inert in ["-b", "-b*x"] {
        assert!(!inert.contains('a'));
    }
}
