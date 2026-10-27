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
    // 2026-10-12, Foundations merge (D-C1): 1669 (was 1718).
    assert_eq!(current.len(), 1669);
    assert_eq!(
        current
            .iter()
            .map(|r| r["topic_id"].as_str().unwrap())
            .collect::<BTreeSet<_>>()
            .len(),
        72 // 2026-10-12, Foundations merge (D-C1): was 78
    );
    assert_eq!(
        current
            .iter()
            .filter(|r| r["kp_id"] == "diagnostic")
            .count(),
        72 // 2026-10-12, Foundations merge (D-C1): was 78
    );
    assert_eq!(
        sha256_hex(serde_json::to_string(&current).unwrap().as_bytes()),
        // 2026-10-07, courses rewrite (95 thousands separators, three reworded items, mixed-number forms): hash re-pinned.
        // 2026-10-08, story framing removed: hash re-pinned (was 0fa8bc22...ad7b).
        // 2026-10-12, Foundations merge (D-C1): hash re-pinned (was c1ea8818...013c).
        // 2026-10-27, placement probes: the 39 Foundations multi-step probes gained a contract; hash re-pinned (was 91526de1...c71f).
        "7af156655f1c9cfa30802bf5adae467755a81133a0697dd30ef1c8216d75ef1d"
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
            // 2026-10-12, Foundations merge (D-C1): the tallies below pin the merged
            // tree (1,669 live statements). The `ascending_chain` kind (5) left with
            // the dropped twin steps.
            // 2026-10-27, placement probes: the 39 uncontracted rows gained a contract (4 coordinates, 28 exact, 1 inequality_union, 6 unit); no row is uncontracted.
            ("approx".to_owned(), 26),
            ("coordinates".to_owned(), 123),
            ("exact".to_owned(), 676),
            ("function".to_owned(), 12),
            ("inequality_union".to_owned(), 98),
            ("label".to_owned(), 91),
            ("list".to_owned(), 38),
            ("multipart".to_owned(), 30),
            ("polynomial_relation".to_owned(), 97),
            ("property".to_owned(), 3),
            ("reduced_ratio".to_owned(), 13),
            ("relation_setup".to_owned(), 9),
            ("required_assignment".to_owned(), 5),
            ("required_form".to_owned(), 10),
            ("required_inequality_notation".to_owned(), 23),
            ("required_simplest_radical".to_owned(), 7),
            ("required_single_power".to_owned(), 1),
            ("unit".to_owned(), 407),
        ])
    );

    let text = include_str!("../../../docs/reports/legacy-multistep-statements.jsonl");
    assert_eq!(
        sha256_hex(text.as_bytes()),
        // 2026-10-12, Foundations merge (D-C1): the manifest was re-keyed; hash re-pinned (was bb7c2a47...7599).
        "b08c731e8ddbb43aa8a8b3b8f8d70f4a569f4fbfdbaa0d0c607aa7c78a964d5c"
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
    assert_eq!(reviewed.len(), 520); // 2026-10-12, Foundations merge (D-C1): was 542
    assert_eq!(
        sha256_hex(serde_json::to_string(&reviewed).unwrap().as_bytes()),
        // 2026-10-12, Foundations merge (D-C1): hash re-pinned (was 432d59e2...b00d).
        "e9f5502dad3ef7c6e19d02435e8167d51404f2a8b1a5333bf3edf4a3f16c74c7"
    );
    let reviewed_keys: BTreeSet<_> = reviewed.iter().map(key).collect();
    assert_eq!(reviewed_keys.len(), reviewed.len());
    assert!(reviewed_keys.is_subset(&current_keys));
    assert_eq!(current_keys.difference(&reviewed_keys).count(), 1149); // 2026-10-12, Foundations merge (D-C1): was 1176; 2026-10-07, courses rewrite (was 561)
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
        0 // 2026-10-27, placement probes: was 39; 2026-10-12, Foundations merge (D-C1): was 44
    );
    assert_eq!(
        reviewed
            .iter()
            .filter(|row| {
                let live = current.iter().find(|live| key(live) == key(row)).unwrap();
                live["problem"] == row["problem"] && live["answer"] == row["answer"]
            })
            .count(),
        61 // 2026-10-27, placement probes: 34 reviewed diagnostic rows changed problem or answer text (was 95); 2026-10-12, Foundations merge (D-C1): was 104
    );
    assert_eq!(
        counts,
        BTreeMap::from([
            // 2026-10-12, Foundations merge (D-C1): the category tallies pin the re-keyed manifest.
            ("component_exercise".to_owned(), 325),
            ("coherent_model".to_owned(), 159),
            ("linked_outputs".to_owned(), 32),
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
