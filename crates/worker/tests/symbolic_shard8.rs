//! Current-gate and semantic regression for Unit03 symbolic shard eight.
#![allow(clippy::unwrap_used, clippy::panic)]
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const KEYS: &[&str] = &[
    "combining-like-terms/kp1",
    "equivalent-expressions/kp1",
    "writing-expressions-from-patterns/kp1",
];
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn rows() -> Vec<Value> {
    let path = root().join("docs/content-foundations/symbolic-repair/shard8-templates.json");
    serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap()).unwrap()["templates"]
        .as_array()
        .unwrap()
        .clone()
}
fn arguments(row: &Value) -> Value {
    let mut body = row["body"].clone();
    let fields = body.as_object_mut().unwrap();
    for key in ["v", "topic_id", "answer_kind"] {
        fields.remove(key);
    }
    body
}
#[test]
fn exact_repaired_set_passes_current_production_gate() {
    let actual: BTreeSet<_> = rows()
        .into_iter()
        .map(|row| row["kp_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, KEYS.iter().map(|key| (*key).to_owned()).collect());
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        verify_kind(Kind::Template, &spec, &arguments(&row), &[]).unwrap();
    }
}
#[test]
fn repaired_families_pin_identifiability_variety_and_two_labels() {
    let rows = rows();
    let equivalent = rows
        .iter()
        .find(|row| row["kp_id"] == "equivalent-expressions/kp1")
        .unwrap();
    let labels: BTreeSet<_> = equivalent["body"]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["expected"].as_str().unwrap())
        .collect();
    assert_eq!(labels, BTreeSet::from(["no", "yes"]));
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    assert!(source.contains("table follows a linear rule"));
    assert!(source.contains("learner claims $4y+6y=10y^2$"));
    assert!(source.contains("One counterexample disproves equivalence"));
    assert!(!source.contains("Complete the simplification $x+x+x+x$"));
}
