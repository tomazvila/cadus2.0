#![allow(clippy::unwrap_used)]
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows(path: &str) -> Vec<Value> {
    serde_json::from_str(&std::fs::read_to_string(root().join(path)).unwrap()).unwrap()
}

fn semantic_keys() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "factoring-gcf/kp2",
        "difference-of-squares/kp2",
        "difference-of-squares/kp3",
        "perfect-square-trinomials/kp3",
        "sum-difference-of-cubes/kp1",
        "sum-difference-of-cubes/kp2",
        "sum-difference-of-cubes/kp3",
        "quadratics-in-form/kp2",
        "choosing-factoring-strategy/kp2",
        "choosing-factoring-strategy/kp3",
        "quadratic-formula/kp3",
    ])
}

#[test]
fn unsafe_representation_and_constant_families_are_fail_closed() {
    let pending = rows("docs/content-foundations/unit07/templates.json");
    let pending_keys: BTreeSet<_> = pending
        .iter()
        .map(|row| row["kp_id"].as_str().unwrap())
        .collect();
    assert!(semantic_keys().is_subset(&pending_keys));

    let blockers = rows("docs/reports/unit07-schema-blockers.json");
    let blocked: BTreeSet<_> = blockers
        .iter()
        .filter(|row| row["code"] == "semantic-family")
        .map(|row| row["kp_id"].as_str().unwrap())
        .collect();
    assert!(blocked.is_empty());
}

#[test]
fn negative_controls_preserve_the_rejected_legacy_samples() {
    let blockers = rows("scripts/authoring/unit07/legacy_semantic_controls.json");
    let by_key: BTreeMap<_, _> = blockers
        .iter()
        .filter(|row| row["code"] == "semantic-family")
        .map(|row| (row["kp_id"].as_str().unwrap(), row))
        .collect();
    let expected = |key: &str| {
        by_key[key]["candidate"]["arguments"]["samples"][0]["expected"]
            .as_str()
            .unwrap()
    };
    assert!(!expected("factoring-gcf/kp2").starts_with('-'));
    assert_eq!(expected("difference-of-squares/kp2"), "4*x**4 - 1");
    assert_eq!(expected("difference-of-squares/kp3"), "3*x**2 - 12");
    assert_eq!(expected("perfect-square-trinomials/kp3"), "4*(x + 1)**2");
    assert_eq!(expected("sum-difference-of-cubes/kp1"), "x**3 - 216");
    assert_eq!(expected("sum-difference-of-cubes/kp2"), "x**3 + 216");
    assert_eq!(expected("sum-difference-of-cubes/kp3"), "8*x**3 + 1");
    assert_eq!(expected("quadratics-in-form/kp2"), "x**4 - 10*x**2 + 9");
    assert_eq!(
        expected("choosing-factoring-strategy/kp2"),
        "2*x*(x**2 - 9)"
    );
    assert_eq!(expected("choosing-factoring-strategy/kp3"), "x**4 - 256");

    let answers: BTreeSet<_> = by_key["quadratic-formula/kp3"]["candidate"]["arguments"]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["expected"].as_str().unwrap())
        .collect();
    assert_eq!(answers, BTreeSet::from(["0"]));
}

#[test]
fn unsupported_three_part_multipart_remains_blocked() {
    let blockers = rows("docs/reports/unit07-schema-blockers.json");
    let row = blockers
        .iter()
        .find(|row| row["kp_id"] == "applying-the-quadratic-formula/kp1")
        .unwrap();
    assert_eq!(row["code"], "grammar");
    assert!(row["message"].as_str().unwrap().contains("wrong count"));
}
