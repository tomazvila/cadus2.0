//! The reviewed manifest covers every legacy multi-step statement, including diagnostics.
#![allow(clippy::unwrap_used)]
mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::{canonical_dump, sha256_hex};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn historical_classification_and_all_live_statements_have_a_closed_boundary() {
    let dump: Value = serde_json::from_str(&canonical_dump(common::events::tree())).unwrap();
    let mut current = Vec::new();
    for topic in dump["topics"].as_array().unwrap() {
        if topic["course"] != "foundations" || topic["answer_kind"] != "multi-step" {
            continue;
        }
        for kp in topic["knowledge_points"].as_array().unwrap() {
            for (index, item) in kp["exemplars"].as_array().unwrap().iter().enumerate() {
                current.push(json!({"topic_id":topic["id"],"kp_id":kp["id"],"exemplar_index":index,"problem":item["problem"],"answer":item["answer"],"answer_contract":item["answer_contract"]}));
            }
        }
        let item = &topic["diagnostic_exemplar"];
        if !item.is_null() {
            current.push(json!({"topic_id":topic["id"],"kp_id":"diagnostic","exemplar_index":0,"problem":item["problem"],"answer":item["answer"],"answer_contract":item["answer_contract"]}));
        }
    }
    current.sort_by_key(key);
    // 1025 live statements; the 2026-09-23..10-05 Foundations rewrites added 38
    // multi-step items, and the reviewed unit-contract pass updated existing keys.
    // The 2026-10-06 exponents-radicals and polynomials-quadratics rewrites add
    // 78 more multi-step items, all with a contract (1103).
    // 2026-10-07, courses rewrite: 1718 (was 1103).
    assert_eq!(current.len(), 1718);
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
        // 2026-10-07, courses rewrite (95 thousands separators, three reworded items, mixed-number forms): hash re-pinned.
        "0fa8bc2c9a0b93b15732e63ff01cf48371f78fe362daf843c50aae473c50ad7b"
    );
    let current_keys: BTreeSet<_> = current.iter().map(key).collect();
    assert_eq!(current_keys.len(), current.len());
    let mut contracts = BTreeMap::new();
    for row in &current {
        let Some(contract) = row["answer_contract"].as_object() else {
            *contracts.entry("uncontracted".to_owned()).or_insert(0) += 1;
            continue;
        };
        let kind = contract["kind"].as_str().unwrap().to_owned();
        *contracts.entry(kind).or_insert(0) += 1;
        let contract: AnswerContract =
            serde_json::from_value(row["answer_contract"].clone()).unwrap();
        contract
            .validate_expected(row["answer"].as_str().unwrap())
            .unwrap();
    }
    // 2026-10-07, courses rewrite: the tallies below are the measurement of the
    // rewritten tree (1,718 live statements).
    assert_eq!(
        contracts,
        BTreeMap::from([
            ("approx".to_owned(), 28),
            ("ascending_chain".to_owned(), 5),
            ("coordinates".to_owned(), 123),
            ("exact".to_owned(), 664),
            ("function".to_owned(), 12),
            ("inequality_union".to_owned(), 97),
            ("label".to_owned(), 98),
            ("list".to_owned(), 40),
            ("multipart".to_owned(), 32),
            ("polynomial_relation".to_owned(), 97),
            ("property".to_owned(), 4),
            ("reduced_ratio".to_owned(), 13),
            ("relation_setup".to_owned(), 9),
            ("required_assignment".to_owned(), 5),
            ("required_form".to_owned(), 10),
            ("required_inequality_notation".to_owned(), 23),
            ("required_simplest_radical".to_owned(), 7),
            ("required_single_power".to_owned(), 1),
            ("uncontracted".to_owned(), 44),
            ("unit".to_owned(), 406),
        ])
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
    reviewed.sort_by_key(key);
    assert_eq!(reviewed.len(), 542);
    assert_eq!(
        sha256_hex(serde_json::to_string(&reviewed).unwrap().as_bytes()),
        "432d59e238c00ffe8d83bac6e3e0e9277689c55f21818249577c96d6476ab00d"
    );
    let reviewed_keys: BTreeSet<_> = reviewed.iter().map(key).collect();
    assert_eq!(reviewed_keys.len(), reviewed.len());
    assert!(reviewed_keys.is_subset(&current_keys));
    assert_eq!(current_keys.difference(&reviewed_keys).count(), 1176); // 2026-10-07, courses rewrite (was 561)
    assert!(
        current
            .iter()
            .filter(|row| !reviewed_keys.contains(&key(row)))
            .all(|row| !row["answer_contract"].is_null())
    );
    assert_eq!(
        current
            .iter()
            .filter(|row| row["answer_contract"].is_null())
            .filter(|row| reviewed_keys.contains(&key(row)))
            .count(),
        44 // 2026-10-07, courses rewrite (was 45)
    );
    assert_eq!(
        reviewed
            .iter()
            .filter(|row| {
                let live = current.iter().find(|live| key(live) == key(row)).unwrap();
                live["problem"] == row["problem"] && live["answer"] == row["answer"]
            })
            .count(),
        104 // 2026-10-07, courses rewrite (was 184); 104 after the later thousands-separator pass
    );
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
