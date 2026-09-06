//! Adversarial semantic negative controls for `00-arithmetic-core.yaml`.
//!
//! The other `arithmetic_core_*` tests prove the authored content is
//! decidable and correct. This file proves the opposite direction: that the
//! real checker (`check_contract`) actually REJECTS a wrong answer to this
//! unit's own authored problems, for every answer-contract family this unit
//! uses (`exact`, `quotient_remainder`, `label`). A checker that accepted
//! anything would make every other green test here meaningless.
#![allow(clippy::unwrap_used)]
use std::path::Path;

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::{Unit, load_curriculum};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn unit() -> Unit {
    let text =
        std::fs::read_to_string(root().join("curriculum/foundations/00-arithmetic-core.yaml"))
            .unwrap();
    serde_norway::from_str(&text).unwrap()
}

fn exemplars_with_contract<'a>(
    doc: &'a Unit,
    matches: impl Fn(&AnswerContract) -> bool,
) -> Vec<(&'a str, &'a AnswerContract)> {
    let mut out = Vec::new();
    for topic in &doc.topics {
        for kp in &topic.knowledge_points {
            for ex in &kp.exemplars {
                if let Some(contract) = ex.answer_contract.as_ref() {
                    if matches(contract) {
                        out.push((ex.answer.as_str(), contract));
                    }
                }
            }
        }
    }
    out
}

fn decided_correct(outcome: Outcome) -> bool {
    matches!(outcome, Outcome::Decided(v) if v.correct)
}

#[test]
fn quotient_remainder_items_reject_a_swapped_and_an_oversized_remainder() {
    let doc = unit();
    let mut checked = 0;
    for (answer, contract) in exemplars_with_contract(&doc, |c| {
        matches!(c, AnswerContract::QuotientRemainder { .. })
    }) {
        let AnswerContract::QuotientRemainder {
            divisor: Some(divisor),
        } = contract
        else {
            continue;
        };
        let (q, r) = answer.split_once(" R").unwrap();
        let quotient: i64 = q.parse().unwrap();
        let remainder: i64 = r.parse().unwrap();
        // the true answer is accepted
        assert!(
            decided_correct(check_contract(answer, answer, contract.clone())),
            "{answer} vs itself under {contract:?}"
        );
        // a swapped quotient/remainder is rejected
        let swapped = format!("{remainder} R{quotient}");
        if swapped != *answer {
            assert!(
                !decided_correct(check_contract(answer, &swapped, contract.clone())),
                "swapped {swapped} wrongly accepted against {answer}"
            );
        }
        // a remainder at or past the divisor is rejected (an invalid division fact)
        let oversized = format!("{quotient} R{divisor}");
        assert!(
            !decided_correct(check_contract(answer, &oversized, contract.clone())),
            "remainder == divisor wrongly accepted: {oversized} vs {answer}"
        );
        checked += 1;
    }
    assert!(
        checked >= 10,
        "only {checked} quotient_remainder items checked"
    );
}

#[test]
fn label_items_reject_the_other_option_and_an_unlisted_word() {
    let doc = unit();
    let mut checked = 0;
    for (answer, contract) in
        exemplars_with_contract(&doc, |c| matches!(c, AnswerContract::Label { .. }))
    {
        let AnswerContract::Label { options } = contract else {
            continue;
        };
        assert!(
            decided_correct(check_contract(answer, answer, contract.clone())),
            "{answer} vs itself under {contract:?}"
        );
        // every OTHER labeled option is rejected against this item's true answer
        for group in options {
            let alias = &group[0];
            if alias != answer {
                assert!(
                    !decided_correct(check_contract(answer, alias, contract.clone())),
                    "{alias} wrongly accepted against {answer}"
                );
            }
        }
        // a word outside the closed option list is undecidable, never silently correct
        let outside = "maybe-adversarial-not-an-option";
        assert!(
            !decided_correct(check_contract(answer, outside, contract.clone())),
            "an unlisted word was wrongly accepted against {answer}"
        );
        checked += 1;
    }
    assert!(checked >= 10, "only {checked} label items checked");
}

#[test]
fn exact_numeric_items_reject_an_off_by_one_answer() {
    let doc = unit();
    let mut checked = 0;
    for (answer, contract) in exemplars_with_contract(&doc, |c| matches!(c, AnswerContract::Exact))
    {
        let Ok(value) = answer.parse::<i64>() else {
            continue;
        };
        assert!(
            decided_correct(check_contract(answer, answer, contract.clone())),
            "{answer} vs itself under exact"
        );
        let off_by_one = (value + 1).to_string();
        assert!(
            !decided_correct(check_contract(answer, &off_by_one, contract.clone())),
            "{off_by_one} wrongly accepted against {answer}"
        );
        checked += 1;
        if checked >= 30 {
            break;
        }
    }
    assert!(checked >= 20, "only {checked} exact numeric items checked");
}

#[test]
fn a_free_text_judgment_answer_with_no_contract_is_undecidable_not_silently_correct() {
    // The exact defect this unit's own coverage test caught and fixed
    // (a "larger"/"smaller" answer needs an explicit label contract): prove
    // the UNCONTRACTED case really is undecidable, so the fix was load-bearing.
    assert!(matches!(
        check_contract("smaller", "smaller", AnswerContract::Exact),
        Outcome::Undecidable(_)
    ));
}

#[test]
fn load_curriculum_still_finds_no_findings_after_this_units_edits() {
    let (_curriculum, findings) = load_curriculum(&root().join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
}
