//! Shared readers and production-gate assertions for symbolic repair shards.

use std::collections::BTreeSet;

use super::{json_rows, repo_root};
use cadus_core::curriculum::load_curriculum;
use cadus_worker::authoring::{cli::select, job::verify_kind, prompt::Kind};
use serde_json::Value;

pub fn rows(paths: &[&str]) -> Vec<Value> {
    json_rows(paths, Some("templates"))
}

pub fn arguments(row: &Value) -> Value {
    let mut body = row["body"].clone();
    let fields = body.as_object_mut().unwrap();
    for field in ["v", "topic_id", "answer_kind"] {
        fields.remove(field);
    }
    body
}

pub fn verify_rows(rows: &[Value], keys: &[&str]) {
    let actual: BTreeSet<_> = rows
        .iter()
        .map(|row| row["kp_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(actual, keys.iter().map(|key| (*key).to_owned()).collect());
    let (curriculum, findings) = load_curriculum(&repo_root().join("curriculum")).unwrap();
    assert!(findings.is_empty());
    for row in rows {
        let key = row["kp_id"].as_str().unwrap();
        let spec = select(&curriculum, &[key.to_owned()]).unwrap().remove(0);
        verify_kind(Kind::Template, &spec, &arguments(row), &[]).unwrap();
    }
}

pub fn curriculum_source(relative: &str) -> String {
    std::fs::read_to_string(repo_root().join(relative)).unwrap()
}

pub fn sample_labels(row: &Value) -> BTreeSet<&str> {
    row["body"]["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sample| sample["expected"].as_str().unwrap())
        .collect()
}
