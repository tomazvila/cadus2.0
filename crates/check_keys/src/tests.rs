//! Tests of `check_answer`: the verdict rule, then the grade rungs.

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;

use super::{Failure, Loc, Status, check_answer};

/// The place of each test item.
fn at() -> Loc {
    Loc {
        file: "t/unit.yaml".to_owned(),
        line: 1,
        course: "t".to_owned(),
        topic: "topic".to_owned(),
        kp: "kp1".to_owned(),
    }
}

/// Run one check; give the status name and the failure reasons.
fn run(answer: &str, contract: Option<&AnswerContract>, kind: AnswerKind) -> (String, Vec<String>) {
    let mut failures: Vec<Failure> = Vec::new();
    let status = check_answer(answer, contract, kind, &at(), "exemplar#0", &mut failures);
    let name = match status {
        Status::Passed(_) => "passed",
        Status::TeachOnly => "teach-only",
        Status::Failed => "failed",
    };
    let reasons = failures.into_iter().map(|failure| failure.reason).collect();
    (name.to_owned(), reasons)
}

#[test]
fn an_item_with_no_verdict_and_no_checker_is_teach_only() {
    let none = AnswerContract::None;
    assert_eq!(run("5", Some(&none), AnswerKind::Numeric).0, "teach-only");
    assert_eq!(run("5", None, AnswerKind::Proof).0, "teach-only");
    assert_eq!(run("prose", None, AnswerKind::MultiStep).0, "teach-only");
}

#[test]
fn a_contract_with_a_key_that_does_not_validate_fails() {
    let exact = AnswerContract::Exact;
    for kind in [AnswerKind::Numeric, AnswerKind::Proof] {
        let (status, reasons) = run("many words", Some(&exact), kind);
        assert_eq!(status, "failed");
        assert_eq!(reasons.len(), 1);
        assert!(
            reasons[0].starts_with("authored answer does not parse under its answer contract"),
            "{reasons:?}"
        );
    }
    let (status, reasons) = run("many words", None, AnswerKind::Numeric);
    assert_eq!(status, "failed");
    assert!(reasons[0].contains("kind 'numeric'"), "{reasons:?}");
}

#[test]
fn a_verdict_item_runs_the_grade_rungs() {
    let exact = AnswerContract::Exact;
    assert_eq!(
        run("5", None, AnswerKind::Numeric),
        ("passed".to_owned(), vec![])
    );
    assert_eq!(
        run("5", Some(&exact), AnswerKind::Proof),
        ("passed".to_owned(), vec![])
    );
    // A key with no numeric component has no +1 mutant.
    assert_eq!(run("x", None, AnswerKind::Expression).0, "passed");
}

#[test]
fn a_label_key_mutates_to_a_different_option() {
    let label = AnswerContract::Label {
        options: vec![vec!["Yes".to_owned()], vec!["No".to_owned()]],
    };
    assert_eq!(
        run("Yes", Some(&label), AnswerKind::Proof),
        ("passed".to_owned(), vec![])
    );
}
