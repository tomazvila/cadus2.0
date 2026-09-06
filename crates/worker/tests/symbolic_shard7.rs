//! Current-gate and semantic regression for Unit03 symbolic shard seven.
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
    "combining-like-terms/kp2",
    "combining-like-terms/kp3",
    "distributive-property/kp1",
    "distributive-property/kp2",
    "distributive-property/kp3",
    "equivalent-expressions/kp2",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn rows() -> Vec<Value> {
    (1..=3)
        .flat_map(|part| {
            let path = root().join(format!(
                "docs/content-foundations/symbolic-repair/shard7-templates-{part}.json"
            ));
            serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap()).unwrap()["templates"]
                .as_array()
                .unwrap()
                .clone()
        })
        .collect()
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
fn repaired_expression_families_avoid_degenerate_and_generic_cases() {
    for row in rows() {
        let body = &row["body"];
        assert_eq!(body["samples"].as_array().unwrap().len(), 16);
        let hint = body["hints"][0].as_str().unwrap();
        assert!(!hint.contains("isolates the unknown"));
    }
    let rows = rows();
    let by_key = |key: &str| rows.iter().find(|row| row["kp_id"] == key).unwrap();
    for key in ["combining-like-terms/kp3", "equivalent-expressions/kp2"] {
        assert!(
            by_key(key)["body"]["samples"]
                .as_array()
                .unwrap()
                .iter()
                .all(|sample| !sample["expected"].as_str().unwrap().starts_with("0x"))
        );
    }
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    assert!(source.contains("A balance changes by $-7x$"));
    assert!(source.contains("A rectangle has height $4$"));
    assert!(source.contains("Correct the sign error"));
    assert!(source.contains("Express its perimeter in the form $ax+b$"));
}
