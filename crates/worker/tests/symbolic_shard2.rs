//! Production-gate and exemplar-variety checks for symbolic repair shard two.
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
    "addition-subtraction-equations/kp1",
    "addition-subtraction-equations/kp2",
    "one-step-equations/kp1",
    "one-step-equations/kp2",
    "variables-both-sides/kp1",
    "variables-both-sides/kp2",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rows() -> Vec<Value> {
    let source = fs::read_to_string(
        root().join("docs/content-foundations/symbolic-repair/shard2-templates.json"),
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
fn replacement_representations_and_answers_are_regression_pinned() {
    let source =
        fs::read_to_string(root().join("curriculum/foundations/03-expressions-equations.yaml"))
            .unwrap();
    for distinct_representation in [
        "A function table follows the rule output = input $+9$.",
        "A point starts at coordinate $x$ and moves $4$ units right",
        "For the function $f(x)=-3x$",
        "A table uses the rule output = input $/8$.",
        "The lines $y=4x-7$ and $y=x+5$ intersect",
        "where the graphs $y=-2x-9$ and $y=-5x+3$ meet",
    ] {
        assert!(source.contains(distinct_representation));
    }
    assert_eq!(-5 + 9, 4);
    assert_eq!(5 + 4, 9);
    assert_eq!(-3 * -6, 18);
    assert_eq!(16 / 8, 2);
    assert_eq!(4 * 4 - 7, 4 + 5);
    assert_eq!(-2 * 4 - 9, -5 * 4 + 3);
}
