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

/// The symbolic-repair files whose rows the 2026-10-05 retirement removed.
pub const RETIRED_SOURCES: &[&str] = &[
    "docs/content-foundations/symbolic-repair/shard9-templates.json",
    "docs/content-foundations/symbolic-repair/shard10-template.json",
    "docs/content-foundations/symbolic-repair/shard11-template.json",
    "docs/content-foundations/symbolic-repair/templates.json",
    "docs/content-foundations/symbolic-repair/shard7-templates-1.json",
    "docs/content-foundations/symbolic-repair/shard7-templates-2.json",
    "docs/content-foundations/symbolic-repair/shard7-templates-3.json",
    "docs/content-foundations/symbolic-repair/shard8-templates.json",
];

/// Every knowledge point that still has a pending symbolic-repair template.
pub fn pending_keys() -> BTreeSet<String> {
    let directory = repo_root().join("docs/content-foundations/symbolic-repair");
    let mut keys = BTreeSet::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        let value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for row in value["templates"].as_array().unwrap() {
            keys.insert(row["kp_id"].as_str().unwrap().to_owned());
        }
    }
    keys
}

/// The shard at `path` holds no pending row, its retired rows are exactly
/// `keys`, and every retired symbolic-repair row keeps its recorded verdict.
pub fn assert_shard_retired(path: &str, keys: &[&str]) {
    assert!(rows(&[path]).is_empty(), "{path} still holds pending rows");
    let retired: BTreeSet<_> = super::retired::report("symbolic-repair")
        .into_iter()
        .filter(|row| row["source"] == path)
        .map(|row| row["kp_key"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        retired,
        keys.iter()
            .map(|key| (*key).to_owned())
            .collect::<BTreeSet<_>>()
    );
    super::retired::assert_retired("symbolic-repair", RETIRED_SOURCES, &pending_keys());
}
