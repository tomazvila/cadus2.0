//! Current-gate and semantic regression for three-way linear classification.
#![allow(clippy::unwrap_used, clippy::panic)]
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn row() -> Value {
    let path = root().join("docs/content-foundations/symbolic-repair/shard11-template.json");
    serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap()).unwrap()["templates"][0]
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
fn three_way_classifier_passes_current_production_gate() {
    let row = row();
    assert_eq!(row["kp_id"], "equations-special-cases/kp3");
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let spec = select(&curriculum, &["equations-special-cases/kp3".to_owned()])
        .unwrap()
        .remove(0);
    verify_kind(Kind::Template, &spec, &arguments(&row), &[]).unwrap();
}
#[test]
fn three_way_classifier_covers_all_outcomes_and_signed_coefficients() {
    let row = row();
    let body = &row["body"];
    let labels: BTreeSet<_> = body["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["expected"].as_str().unwrap())
        .collect();
    assert_eq!(
        labels,
        BTreeSet::from(["all real numbers", "no solution", "one solution"])
    );
    assert!(
        body["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|sample| sample["params"]["a"].as_i64().unwrap() < 0)
    );
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    assert!(source.contains("Exactly one real value works"));
    assert!(source.contains("false comparison"));
}
