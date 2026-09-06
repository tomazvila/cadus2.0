//! Literal integer division prompts supply an independently verified divisor.
#![allow(clippy::unwrap_used)]
mod common;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::load_raw_curriculum;
use serde_json::Value;

#[test]
fn every_reviewed_integer_division_policy_matches_its_authored_operands() {
    let (raw, findings) = load_raw_curriculum(&common::paths::curriculum_root()).unwrap();
    assert!(findings.is_empty());
    let manifest = include_str!("../../../docs/reports/foundations-reviewed-remainders.jsonl");
    let mut count = 0;
    for line in manifest.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        let item = common::fixtures::reviewed_exemplar(&raw, &row);
        let divisor = row["divisor"].as_u64().unwrap();
        let dividend = row["dividend"].as_u64().unwrap();
        let quotient = row["quotient"].as_u64().unwrap();
        let remainder = row["remainder"].as_u64().unwrap();
        assert_eq!(dividend, divisor * quotient + remainder);
        assert!(remainder < divisor);
        let policy = AnswerContract::QuotientRemainder {
            divisor: Some(divisor),
        };
        assert_eq!(item.answer_contract.as_ref(), Some(&policy));
        assert!(matches!(
            check_contract(&item.answer, &item.answer, policy.clone()),
            Outcome::Decided(verdict) if verdict.correct
        ));
        // This pair represents the same total but violates Euclidean remainder bounds.
        let oversized = format!("{} R{}", quotient - 1, remainder + divisor);
        for wrong in [
            oversized,
            format!("{quotient} R{}", remainder + 1),
            dividend.to_string(),
        ] {
            assert!(!matches!(
                check_contract(&item.answer, &wrong, policy.clone()),
                Outcome::Decided(verdict) if verdict.correct
            ));
        }
        count += 1;
    }
    assert_eq!(count, 8);
}

#[test]
fn polynomial_remainders_are_not_given_an_integer_divisor_contract() {
    let (raw, _) = load_raw_curriculum(&common::paths::curriculum_root()).unwrap();
    let mut count = 0;
    for entry in raw.topics().filter(|entry| {
        matches!(
            entry.topic.id.as_str(),
            "polynomial-division" | "synthetic-division"
        )
    }) {
        for kp in &entry.topic.knowledge_points {
            for item in &kp.exemplars {
                if item.answer.contains("remainder") {
                    assert!(item.answer_contract.is_none());
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 4);
}
