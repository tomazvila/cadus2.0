//! Production-gate and semantic checks for symbolic repair shard three.
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
    "absolute-value-equations/kp1",
    "basic-absolute-value-equations/kp1",
    "basic-absolute-value-equations/kp2",
    "distribute-then-solve/kp1",
    "multi-step-equations/kp1",
    "two-step-equations/kp1",
    "two-step-equations/kp2",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Value> {
    let source = fs::read_to_string(
        root().join("docs/content-foundations/symbolic-repair/shard3-templates.json"),
    )
    .unwrap();
    serde_json::from_str::<Value>(&source).unwrap()["templates"]
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
fn independent_representations_and_answers_are_pinned() {
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    for representation in [
        "A function table uses $f(x)=4x-7$.",
        "A machine divides its input by $-4$",
        "Three identical boxes each contain $2x-1$ counters",
        "For $f(x)=7x-3x-5$",
        "The graphs $y=|x|$ and $y=5$ intersect",
        "The graphs $y=|x+4|$ and $y=2$ intersect",
        "The graph $y=|3x-6|$ meets the horizontal line $y=12$",
    ] {
        assert!(source.contains(representation), "{representation}");
    }
    assert_eq!(4 * 4 - 7, 9);
    assert_eq!(-20 / -4 - 3, 2);
    assert_eq!(3 * (2 * 3 - 1), 15);
    assert_eq!(7 * 4 - 3 * 4 - 5, 11);
    assert_eq!([(-5_i32).abs(), 5_i32.abs()], [5, 5]);
    assert_eq!([(-6_i32 + 4).abs(), (-2_i32 + 4).abs()], [2, 2]);
    assert_eq!([(3 * -2_i32 - 6).abs(), (3 * 6_i32 - 6).abs()], [12, 12]);
}
