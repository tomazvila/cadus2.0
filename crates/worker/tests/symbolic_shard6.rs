//! Current-gate and semantic regression for Unit03 symbolic shard six.
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
    "checking-a-solution/kp2",
    "equations-with-fractions/kp3",
    "evaluating-expressions/kp2",
    "money-geometry-problems/kp1",
    "parts-of-an-expression/kp2",
];
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn rows() -> Vec<Value> {
    let p = root().join("docs/content-foundations/symbolic-repair/shard6-templates.json");
    serde_json::from_str::<Value>(&fs::read_to_string(p).unwrap()).unwrap()["templates"]
        .as_array()
        .unwrap()
        .clone()
}
fn arguments(row: &Value) -> Value {
    let mut body = row["body"].clone();
    let f = body.as_object_mut().unwrap();
    for k in ["v", "topic_id", "answer_kind"] {
        f.remove(k);
    }
    body
}
#[test]
fn exact_repaired_set_passes_current_production_gate() {
    let actual: BTreeSet<_> = rows()
        .into_iter()
        .map(|r| r["kp_id"].as_str().unwrap().to_owned())
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
fn repaired_family_defects_are_pinned() {
    let rows = rows();
    let by_key = |key: &str| rows.iter().find(|r| r["kp_id"] == key).unwrap();
    let checking = &by_key("checking-a-solution/kp2")["body"];
    let positions: BTreeSet<_> = checking["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["params"]["b"].as_i64().unwrap())
        .collect();
    assert_eq!(positions, BTreeSet::from([-1, 0, 1]));
    let evaluating = &by_key("evaluating-expressions/kp2")["body"];
    assert!(
        evaluating["samples"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["params"]["a"].as_i64().unwrap() < 0)
    );
    let parts = &by_key("parts-of-an-expression/kp2")["body"];
    assert_eq!(parts["params"].as_object().unwrap().len(), 1);
    let fractions = &by_key("equations-with-fractions/kp3")["body"];
    assert!(
        fractions["solution_sketch"]
            .as_str()
            .unwrap()
            .contains("Distribute")
    );
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    assert!(source.contains("After a €4 coupon"));
    assert!(source.contains("answer: \"6\""));
}
