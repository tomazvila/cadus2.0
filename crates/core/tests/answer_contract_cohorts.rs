//! Reviewed choices and physical quantities retain their closed semantics.

#![allow(clippy::unwrap_used, clippy::panic)]

mod common;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::load_raw_curriculum;
use serde_json::Value;

fn correct(expected: &str, learner: &str, contract: &AnswerContract) -> bool {
    match check_contract(expected, learner, contract.clone()) {
        Outcome::Decided(verdict) => verdict.correct,
        Outcome::Undecidable(reason) => panic!("{expected:?}, {learner:?}: {reason:?}"),
    }
}

#[test]
fn reviewed_manifest_matches_curriculum_and_rejects_other_choices() {
    let (raw, findings) = load_raw_curriculum(&common::paths::curriculum_root()).unwrap();
    assert!(findings.is_empty());
    let manifest = include_str!("../../../docs/reports/foundations-reviewed-choice-units.jsonl");
    let mut counts = (0, 0);
    for line in manifest.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        let item = common::fixtures::reviewed_exemplar(&raw, &row);
        let contract: AnswerContract =
            serde_json::from_value(row["answer_contract"].clone()).unwrap();
        assert_eq!(item.answer_contract.as_ref(), Some(&contract));
        assert!(correct(&item.answer, &item.answer, &contract));
        match &contract {
            AnswerContract::Label { options } => {
                counts.0 += 1;
                assert!(correct(
                    &item.answer,
                    &format!("  {}  ", item.answer.to_uppercase()),
                    &contract
                ));
                for option in options {
                    assert_eq!(
                        correct(&item.answer, &option[0], &contract),
                        item.answer == option[0]
                    );
                }
                for learner in [
                    "yes or no",
                    "true and false",
                    "x = yes",
                    "yes because it looks right",
                    "",
                ] {
                    assert!(!correct(&item.answer, learner, &contract));
                }
            }
            AnswerContract::Unit { .. } => {
                counts.1 += 1;
                assert!(!correct(&item.answer, "0", &contract));
                assert!(!correct(&item.answer, "0 kg", &contract));
            }
            other => panic!("unexpected reviewed policy: {other:?}"),
        }
    }
    assert_eq!(counts, (71, 27));
}

#[test]
fn quantity_cohorts_preserve_conversions_exact_cents_and_radicals() {
    for (expected, learner, wrong, contract) in [
        (
            "30 L",
            "30000 ml",
            "3000 ml",
            r#"{"kind":"unit","quantity":"volume","unit":"L"}"#,
        ),
        (
            "€1081.60",
            "1081.6 €",
            "€1081.61",
            r#"{"kind":"unit","quantity":"euro","unit":"€"}"#,
        ),
        (
            "10√13 km",
            "10000√13 m",
            "36.06 km",
            r#"{"kind":"unit","quantity":"length","unit":"km"}"#,
        ),
        (
            "120°",
            "360/3°",
            "120 m",
            r#"{"kind":"unit","quantity":"angle","unit":"°"}"#,
        ),
    ] {
        let contract = serde_json::from_str(contract).unwrap();
        assert!(correct(expected, learner, &contract));
        assert!(!correct(expected, wrong, &contract));
    }
}
