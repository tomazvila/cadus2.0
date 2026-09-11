//! Conservative annotation candidates; every new policy requires content review.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;

use cadus_core::answer::{AnswerContract, Ast, Canon, canonical_form, normalize, parse};
use cadus_core::curriculum::load_raw_curriculum;
use serde_json::{Value, json};

fn candidate(problem: &str, answer: &str) -> (Option<AnswerContract>, &'static str) {
    let prompt = problem.to_lowercase();
    if prompt
        .split(|ch: char| !ch.is_ascii_alphabetic())
        .any(|word| {
            matches!(
                word,
                "round"
                    | "rounded"
                    | "rounding"
                    | "nearest"
                    | "approx"
                    | "approximate"
                    | "approximately"
                    | "estimate"
                    | "estimated"
                    | "significant"
            )
        })
        || prompt.contains("decimal place")
        || answer.contains('≈')
    {
        return (None, "review_authored_precision");
    }
    let Ok(value) = canonical_form(answer) else {
        return (None, "review_vocabulary_or_grammar");
    };
    if let Ok(Ast::Quantity { unit, .. }) = parse(&normalize(answer).source)
        && let Canon::Quantity { quantity, .. } = value
    {
        return (
            Some(AnswerContract::Unit {
                quantity,
                unit: unit.into(),
            }),
            "review_unit_policy",
        );
    }
    if answer.contains(" R") || answer.contains("remainder") {
        return (
            Some(AnswerContract::QuotientRemainder { divisor: None }),
            "review_divisor",
        );
    }
    if matches!(value, Canon::Set(_)) {
        return (Some(AnswerContract::Set), "review_set_policy");
    }
    (
        Some(AnswerContract::Exact),
        "review_exactness_and_required_form",
    )
}

fn identity(row: &Value) -> (&str, &str, u64) {
    (
        row["topic_id"].as_str().unwrap(),
        row["kp_id"].as_str().unwrap(),
        row["exemplar_index"].as_u64().unwrap(),
    )
}

#[test]
fn current_inventory_covers_every_exemplar_and_preserves_the_review_snapshot() {
    let (raw, findings) = load_raw_curriculum(&common::paths::curriculum_root()).unwrap();
    assert!(findings.is_empty());
    let historical: Vec<Value> =
        include_str!("../../../docs/reports/foundations-contract-candidates.jsonl")
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(historical.len(), 1695);
    assert_eq!(
        historical
            .iter()
            .filter(|row| !row["existing_contract"].is_null())
            .count(),
        322
    );
    assert!(
        historical
            .iter()
            .all(|row| row["automatic_approval"] == false)
    );
    let historical_keys: BTreeSet<_> = historical.iter().map(identity).collect();
    assert_eq!(historical_keys.len(), historical.len());

    let mut rows: Vec<Value> = Vec::new();
    for entry in raw
        .topics()
        .filter(|entry| entry.course_dir == "foundations")
    {
        for kp in &entry.topic.knowledge_points {
            for (index, exemplar) in kp.exemplars.iter().enumerate() {
                let (proposed, reason) = candidate(&exemplar.problem, &exemplar.answer);
                rows.push(json!({
                    "file": format!("curriculum/foundations/{}", entry.file_name),
                    "topic_id":entry.topic.id, "kp_id":kp.id, "exemplar_index":index,
                    "problem":exemplar.problem, "answer":exemplar.answer,
                    "existing_contract":exemplar.answer_contract,
                    "candidate_contract":proposed, "review_reason":reason,
                    "automatic_approval":false,
                }));
            }
        }
    }
    assert_eq!(rows.len(), 3169);
    assert_eq!(
        rows.iter()
            .filter(|row| !row["existing_contract"].is_null())
            .count(),
        1814 // Curriculum repairs 02 added twelve reviewed single-power contracts.
    );
    assert!(rows.iter().all(|row| row["automatic_approval"] == false));
    let current_keys: BTreeSet<_> = rows.iter().map(identity).collect();
    assert_eq!(current_keys.len(), rows.len());
    assert!(historical_keys.is_subset(&current_keys));
    assert!(
        rows.iter()
            .any(|row| row["review_reason"] == "review_authored_precision")
    );
    if let Ok(path) = std::env::var("CADUS_CONTRACT_INVENTORY_DUMP") {
        let body = rows
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        std::fs::write(path, body).unwrap();
    }
}

#[test]
fn approximation_prompts_never_receive_an_automatic_exact_candidate() {
    for prompt in [
        "Round to two decimal places",
        "Estimate the distance",
        "To the nearest integer",
        "Give an approximate value",
    ] {
        assert_eq!(candidate(prompt, "2").0, None);
    }
    assert_eq!(candidate("Compute", "≈ 2").0, None);
    assert_eq!(
        candidate("Find the ground distance", "2").0,
        Some(AnswerContract::Exact)
    );
    assert_eq!(
        candidate("Compute exactly", "2/3").0,
        Some(AnswerContract::Exact)
    );
}
