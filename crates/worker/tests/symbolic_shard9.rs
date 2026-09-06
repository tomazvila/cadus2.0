//! Current-gate and semantic regression for Unit03 no-solution shard nine.
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
    "absolute-value-equations/kp3",
    "equations-special-cases/kp1",
];
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn rows() -> Vec<Value> {
    let path = root().join("docs/content-foundations/symbolic-repair/shard9-templates.json");
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
fn no_solution_families_count_zero_and_vary_the_equations() {
    for row in rows() {
        let body = &row["body"];
        assert_eq!(body["answer_expr"], "0");
        assert_eq!(body["answer_contract"]["kind"], "exact");
        assert!(body["params"].as_object().unwrap().len() >= 2);
        assert!(body["samples"].as_array().unwrap().len() >= 16);
    }
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    assert!(source.contains("answer: \"0\""));
    assert!(source.contains("How many real solutions"));
    assert!(!source.contains("answer: \"none\""));
}
