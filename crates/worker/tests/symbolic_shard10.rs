//! Current-gate and semantic regression for the Unit03 identity family.
#![allow(clippy::unwrap_used, clippy::panic)]
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn row() -> Value {
    let path = root().join("docs/content-foundations/symbolic-repair/shard10-template.json");
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
fn identity_family_passes_current_production_gate() {
    let row = row();
    assert_eq!(row["kp_id"], "equations-special-cases/kp2");
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    let spec = select(&curriculum, &["equations-special-cases/kp2".to_owned()])
        .unwrap()
        .remove(0);
    verify_kind(Kind::Template, &spec, &arguments(&row), &[]).unwrap();
}
#[test]
fn identity_family_counts_counterexamples_without_a_singleton_label() {
    let body = &row()["body"];
    assert_eq!(body["answer_expr"], "0");
    assert_eq!(body["answer_contract"]["kind"], "exact");
    assert_eq!(body["samples"].as_array().unwrap().len(), 16);
    assert!(body["constraints"].as_array().unwrap().len() == 1);
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    assert_eq!(
        source
            .matches("How many real values fail to satisfy")
            .count(),
        3
    );
}
