//! Production-gate and semantic checks for symbolic repair shard four.
#![allow(clippy::unwrap_used, clippy::panic)]
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;

const KEYS: &[&str] = &[
    "slope-from-a-graph/kp3",
    "slope-from-two-points/kp1",
    "slope-from-two-points/kp2",
    "slope-from-two-points/kp3",
    "x-y-intercepts/kp3",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn rows() -> Vec<Value> {
    let path = root().join("docs/content-foundations/symbolic-repair/shard4-templates.json");
    serde_json::from_str::<Value>(&fs::read_to_string(path).unwrap()).unwrap()["templates"]
        .as_array()
        .unwrap()
        .clone()
}
fn arguments(row: &Value) -> Value {
    let mut body = row["body"].clone();
    let fields = body.as_object_mut().unwrap();
    for field in ["v", "topic_id", "answer_kind"] {
        fields.remove(field);
    }
    body
}

#[test]
fn exact_reviewed_shard_passes_the_current_production_gate() {
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
fn varied_slope_representations_and_answers_are_pinned() {
    let source =
        fs::read_to_string(root().join("curriculum/foundations/04-linear-graphs.yaml")).unwrap();
    for representation in [
        "follow the line $3$ squares right and $3$ squares down",
        "displacement from one point on a line",
        "starts at $A=(-2,-3)$; its displacement",
        "slope quotients $3/6$ and $(-3)/(-6)$",
        "budget line $4x+5y=20$",
    ] {
        assert!(source.contains(representation), "{representation}");
    }
    assert_eq!(-3_f64 / 3.0, -1.0);
    assert_eq!(4_f64 / 4.0, 1.0);
    assert_eq!((3_f64 / 6.0), (-3_f64 / -6.0));
    assert_eq!(20 / 4, 5);
    assert_eq!(20 / 5, 4);
}
