//! Current-gate and semantic regression for linear factoring.
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
    "factoring-linear-expressions/kp1",
    "factoring-linear-expressions/kp2",
];
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn rows() -> Vec<Value> {
    let p = root().join("docs/content-foundations/symbolic-repair/shard12-templates.json");
    serde_json::from_str::<Value>(&fs::read_to_string(p).unwrap()).unwrap()["templates"]
        .as_array()
        .unwrap()
        .clone()
}
fn arguments(row: &Value) -> Value {
    let mut b = row["body"].clone();
    let f = b.as_object_mut().unwrap();
    for k in ["v", "topic_id", "answer_kind"] {
        f.remove(k);
    }
    b
}
#[test]
fn factored_families_pass_current_production_gate() {
    let actual: BTreeSet<_> = rows()
        .into_iter()
        .map(|r| r["kp_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, KEYS.iter().map(|k| (*k).to_owned()).collect());
    let (curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    for row in rows() {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        verify_kind(Kind::Template, &spec, &arguments(&row), &[]).unwrap();
    }
}
#[test]
fn recipes_extract_the_actual_gcf_and_include_signed_two_variable_cases() {
    for row in rows() {
        let body = &row["body"];
        assert_eq!(body["answer_contract"]["form"], "factored_linear");
        assert_eq!(body["samples"].as_array().unwrap().len(), 16);
    }
    let rows = rows();
    let signed = rows.iter().find(|r| r["kp_id"] == KEYS[1]).unwrap();
    assert!(
        signed["body"]["answer_expr"]
            .as_str()
            .unwrap()
            .starts_with("-gcd")
    );
}
