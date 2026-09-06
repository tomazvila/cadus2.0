//! The reviewed manifest covers every legacy multi-step statement, including diagnostics.
#![allow(clippy::unwrap_used)]
mod common;

use cadus_core::curriculum::{canonical_dump, sha256_hex};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn all_78_topics_and_542_statements_match_the_reviewed_classification() {
    let dump: Value = serde_json::from_str(&canonical_dump(common::events::tree())).unwrap();
    let mut current = Vec::new();
    for topic in dump["topics"].as_array().unwrap() {
        if topic["course"] != "foundations" || topic["answer_kind"] != "multi-step" {
            continue;
        }
        for kp in topic["knowledge_points"].as_array().unwrap() {
            for (index, item) in kp["exemplars"].as_array().unwrap().iter().enumerate() {
                current.push(json!({"topic_id":topic["id"],"kp_id":kp["id"],"exemplar_index":index,"problem":item["problem"],"answer":item["answer"]}));
            }
        }
        let item = &topic["diagnostic_exemplar"];
        if !item.is_null() {
            current.push(json!({"topic_id":topic["id"],"kp_id":"diagnostic","exemplar_index":0,"problem":item["problem"],"answer":item["answer"]}));
        }
    }
    current.sort_by_key(key);
    assert_eq!(current.len(), 542);
    assert_eq!(
        current
            .iter()
            .map(|r| r["topic_id"].as_str().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        78
    );
    assert_eq!(
        current
            .iter()
            .filter(|r| r["kp_id"] == "diagnostic")
            .count(),
        78
    );
    assert_eq!(
        sha256_hex(serde_json::to_string(&current).unwrap().as_bytes()),
        "432d59e238c00ffe8d83bac6e3e0e9277689c55f21818249577c96d6476ab00d"
    );
    let text = include_str!("../../../docs/reports/legacy-multistep-statements.jsonl");
    assert_eq!(
        sha256_hex(text.as_bytes()),
        "bb7c2a471bc05786a13bc702a8f9a694e722dbbdfad96ab5e36f8c8329b97599"
    );
    let mut counts = BTreeMap::new();
    let mut reviewed = Vec::new();
    for line in text.lines() {
        let mut row: Value = serde_json::from_str(line).unwrap();
        let object = row.as_object_mut().unwrap();
        let category = object
            .remove("classification")
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned();
        *counts.entry(category).or_insert(0) += 1;
        assert!(
            !object
                .remove("review_reason")
                .unwrap()
                .as_str()
                .unwrap()
                .is_empty()
        );
        reviewed.push(row);
    }
    assert_eq!(reviewed, current);
    assert_eq!(
        counts,
        BTreeMap::from([
            ("component_exercise".to_owned(), 342),
            ("coherent_model".to_owned(), 161),
            ("linked_outputs".to_owned(), 35),
            ("context_fragment".to_owned(), 4),
        ])
    );
}

fn key(row: &Value) -> (String, String, u64) {
    (
        row["topic_id"].as_str().unwrap().into(),
        row["kp_id"].as_str().unwrap().into(),
        row["exemplar_index"].as_u64().unwrap(),
    )
}
