//! Table tests of `grade_item`: the answer contract decides before the topic kind.

use super::*;
use cadus_core::answer::AnswerContract;
use cadus_core::pool::PoolAnswer;

/// The verdict that one row of the table expects.
#[derive(Debug, PartialEq, Eq)]
enum Want {
    Correct,
    Incorrect,
    ProofUngraded,
}

/// The four `Step <n>` options of a proof item that asks for the wrong step.
fn steps() -> AnswerContract {
    AnswerContract::Label {
        options: (1..=4).map(|step| vec![format!("Step {step}")]).collect(),
    }
}

fn item(answer: &str, answer_contract: Option<AnswerContract>) -> PoolAnswer {
    PoolAnswer {
        answer_contract,
        v: cadus_core::pool::POOL_ROW_VERSION,
        answer: answer.to_owned(),
    }
}

fn verdict(grade: &Grade) -> Want {
    match &grade.outcome {
        AttemptOutcome::Correct => Want::Correct,
        AttemptOutcome::Incorrect => Want::Incorrect,
        AttemptOutcome::Ungraded { reason } => {
            assert_eq!(reason, PROOF_UNGRADED);
            Want::ProofUngraded
        }
    }
}

/// Each row: the contract, the key, the kind, the learner text, the verdict.
#[test]
fn the_contract_decides_before_the_topic_kind() {
    let matrix = AnswerContract::Matrix { rows: 2, cols: 2 };
    let proof_text = "Base n = 0 holds. Assume n = k. Then n = k + 1 follows.";
    let rows = [
        // `Some(contract)`, kind `Proof`: the contract gives the verdict.
        (
            Some(steps()),
            "Step 3",
            AnswerKind::Proof,
            "Step 3",
            Want::Correct,
        ),
        (
            Some(steps()),
            "Step 3",
            AnswerKind::Proof,
            "step  3",
            Want::Correct,
        ),
        (
            Some(steps()),
            "Step 3",
            AnswerKind::Proof,
            "Step 2",
            Want::Incorrect,
        ),
        (
            Some(steps()),
            "Step 3",
            AnswerKind::Proof,
            "Step 9",
            Want::Incorrect,
        ),
        (
            Some(AnswerContract::Exact),
            "n^2+n+1",
            AnswerKind::Proof,
            "1+n+n^2",
            Want::Correct,
        ),
        (
            Some(AnswerContract::Exact),
            "n^2+n+1",
            AnswerKind::Proof,
            "n^2+n",
            Want::Incorrect,
        ),
        // `Some(contract)`, a different kind: as before this change.
        (
            Some(matrix.clone()),
            "[[1,2],[3,4]]",
            AnswerKind::MultiStep,
            "[[1,2],[3,4]]",
            Want::Correct,
        ),
        (
            Some(matrix),
            "[[1,2],[3,4]]",
            AnswerKind::MultiStep,
            "[[1,2],[3,5]]",
            Want::Incorrect,
        ),
        // `Some(AnswerContract::None)`, kind `Proof`: the proof reason (D6).
        (
            Some(AnswerContract::None),
            proof_text,
            AnswerKind::Proof,
            proof_text,
            Want::ProofUngraded,
        ),
        // `None`, kind `Proof`: each learner text is ungraded.
        (
            None,
            proof_text,
            AnswerKind::Proof,
            proof_text,
            Want::ProofUngraded,
        ),
        (None, proof_text, AnswerKind::Proof, "", Want::ProofUngraded),
        (
            None,
            proof_text,
            AnswerKind::Proof,
            "42",
            Want::ProofUngraded,
        ),
        // `None`, a different kind: the deterministic checker.
        (None, "1/2", AnswerKind::Numeric, "0.5", Want::Correct),
        (None, "1/2", AnswerKind::Numeric, "0.4", Want::Incorrect),
    ];
    for (contract, key, kind, learner, want) in rows {
        let grade = grade_item(&item(key, contract.clone()), learner, kind);
        assert_eq!(verdict(&grade), want, "{contract:?} {kind:?} {learner:?}");
        assert_eq!(grade.correct, want == Want::Correct);
    }
}

/// `Some(AnswerContract::None)` on a kind that is not `Proof` stays as before:
/// `check_contract` refuses, and the reason is not the proof reason.
#[test]
fn the_none_contract_on_a_different_kind_keeps_the_checker_reason() {
    let expected = item("7", Some(AnswerContract::None));
    let grade = grade_item(&expected, "7", AnswerKind::Numeric);
    let by_contract = cadus_core::answer::check_contract("7", "7", AnswerContract::None);
    assert!(grade.outcome.is_ungraded());
    assert!(!grade.correct);
    assert_ne!(grade.outcome.reason(), Some(PROOF_UNGRADED));
    assert!(matches!(by_contract, Outcome::Undecidable(_)));
}

/// A wrong-form answer carries the learner-facing notation text; a right-form one carries none.
#[test]
fn a_wrong_form_answer_carries_notation_text() {
    let contract: AnswerContract = serde_json::from_value(
        serde_json::json!({"kind": "required_form", "form": "mixed_number"}),
    )
    .unwrap();
    let pool = item("4 2/5", Some(contract));
    let kind = AnswerKind::Numeric;
    for (answer, want) in [
        (
            "22/5",
            Some(
                "Write the mixed number as a whole number, a space, then the fraction, like 4 2/5",
            ),
        ),
        ("4 * 2/5", None),
        ("4 2/5", None),
    ] {
        let grade = grade_item(&pool, answer, kind);
        let text = notation_text(&pool, answer, kind, &grade);
        match want {
            Some(want) => assert_eq!(text.as_deref(), Some(want), "{answer}"),
            None if answer == "4 * 2/5" => {
                assert!(text.unwrap().contains("means 4 times 2/5"));
            }
            None => assert_eq!(text, None),
        }
    }
}

/// The degree and leading coefficient item reads the natural spellings.
#[test]
fn the_degree_and_leading_coefficient_item_reads_natural_spellings() {
    let contract: AnswerContract = serde_json::from_value(serde_json::json!({
        "kind": "multipart",
        "parts": [
            {"name": "degree", "contract": {"kind": "exact"}},
            {"name": "leading_coefficient", "contract": {"kind": "exact"}}
        ]
    }))
    .unwrap();
    let pool = item("degree = 3; leading_coefficient = 4", Some(contract));
    for answer in [
        "degree = 3, leading coefficient = 4",
        "degree = 3, leading_coefficient = 4",
        "3, 4",
        "degree 3, lc 4",
        "degree: 3; leading coefficient: 4",
    ] {
        let grade = grade_item(&pool, answer, AnswerKind::Numeric);
        assert_eq!(verdict(&grade), Want::Correct, "{answer}");
    }
    let wrong = grade_item(&pool, "4, 3", AnswerKind::Numeric);
    assert_eq!(verdict(&wrong), Want::Incorrect);
}
