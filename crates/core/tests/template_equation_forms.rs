//! Equation writers validate both mathematical roles and the reviewed vocabulary.
#![allow(clippy::unwrap_used)]
use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::template::{answer_for_contract, parse_answer_expr};
use std::collections::BTreeMap;

fn contract(text: &str) -> AnswerContract {
    AnswerContract::Label {
        options: vec![vec![text.into()], vec!["wrong equation".into()]],
    }
}

#[test]
fn writes_both_directions_and_negative_exponents() {
    for (source, expected) in [
        ("logequation(3,[4,81])", "log_3(81) = 4"),
        ("expequation(10,[-2,1/100])", "(10)^(-2) = 1/100"),
        ("logequation(1/2,[-3,8])", "log_(1/2)(8) = -3"),
        ("expequation(5,[0,1])", "(5)^(0) = 1"),
    ] {
        let policy = contract(expected);
        let result = answer_for_contract(
            &parse_answer_expr(source).unwrap(),
            &BTreeMap::new(),
            Some(&policy),
        )
        .unwrap();
        assert_eq!(result.text, expected);
        assert!(
            matches!(check_contract(&result.text, "wrong equation", policy),
            Outcome::Decided(verdict) if !verdict.correct)
        );
    }
}

#[test]
fn wrong_components_are_rejected_even_when_vocabulary_would_accept_them() {
    for (source, wrong) in [
        ("logequation(2,[3,9])", "log_2(9) = 3"),
        ("expequation(3,[2,8])", "(3)^(2) = 8"),
        ("logequation(1,[2,1])", "log_1(1) = 2"),
        ("expequation(-2,[3,-8])", "(-2)^(3) = -8"),
        ("logequation(0,[2,0])", "log_0(0) = 2"),
        ("expequation(4,[1/2,2])", "(4)^(1/2) = 2"),
        ("logequation(2,[1001,8])", "log_2(8) = 1001"),
    ] {
        assert!(
            answer_for_contract(
                &parse_answer_expr(source).unwrap(),
                &BTreeMap::new(),
                Some(&contract(wrong))
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn refuses_missing_vocabulary_wrong_policy_and_malformed_parts() {
    let ast = parse_answer_expr("logequation(2,[3,8])").unwrap();
    for policy in [
        None,
        Some(AnswerContract::Exact),
        Some(contract("log_3(8) = 2")),
    ] {
        assert!(answer_for_contract(&ast, &BTreeMap::new(), policy.as_ref()).is_err());
    }
    for source in [
        "expequation(2,[3])",
        "logequation(2,[3,8,5])",
        "logequation(2,8)",
        "expequation(x,[3,8])",
    ] {
        assert!(
            answer_for_contract(
                &parse_answer_expr(source).unwrap(),
                &BTreeMap::new(),
                Some(&contract("(2)^(3) = 8"))
            )
            .is_err()
        );
    }
}
