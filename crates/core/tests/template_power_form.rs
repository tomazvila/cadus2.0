//! The power-form writer keeps structure and fails closed at the contract boundary.
#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;

use cadus_core::{
    answer::{AnswerContract, Outcome, canonical_form, check_contract},
    template::{answer_for_contract, parse_answer_expr},
};

#[test]
fn writes_exact_unevaluated_powers_with_computed_parts() {
    for (source, expected) in [
        ("powerform(850,[1+0.06/12,12*6])", "850*(1.005)^72"),
        ("powerform(3/2,[2,-3])", "3/16"),
        ("powerform(-7,[-2,3])", "56"),
    ] {
        let result = answer_for_contract(
            &parse_answer_expr(source).unwrap(),
            &BTreeMap::new(),
            Some(&AnswerContract::Exact),
        )
        .unwrap();
        assert!(result.text.contains('^'));
        assert_eq!(result.canon, canonical_form(expected).unwrap());
        if source.starts_with("powerform(850") {
            assert_eq!(result.text, "(850)*(201/200)^(72)");
        }
    }
}

#[test]
fn writes_one_power_for_the_required_policy() {
    for (source, expected) in [
        ("powerform(1,[2,3+4])", "(2)^(7)"),
        ("powerform(1,[1/2,3])", "(1/2)^(3)"),
        ("powerform(1,[-2,3])", "(-2)^(3)"),
    ] {
        let policy = AnswerContract::RequiredSinglePower;
        let result = answer_for_contract(
            &parse_answer_expr(source).unwrap(),
            &BTreeMap::new(),
            Some(&policy),
        )
        .unwrap();
        assert_eq!(result.text, expected);
        assert!(matches!(
            check_contract(expected, &result.text, policy),
            Outcome::Decided(verdict) if verdict.correct
        ));
    }
    for source in ["powerform(2,[3,4])", "powerform(0,[3,4])"] {
        assert!(
            answer_for_contract(
                &parse_answer_expr(source).unwrap(),
                &BTreeMap::new(),
                Some(&AnswerContract::RequiredSinglePower),
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn refuses_wrong_contract_arity_non_numeric_and_unsafe_exponents() {
    for source in [
        "powerform(1,2)",
        "powerform(1,2,3,4)",
        "powerform(1,[2,1/2])",
        "powerform(1,[2,1001])",
        "powerform(1,[2,-1001])",
        "powerform(1,[0,-1])",
        "powerform(1,[x,3])",
        "powerform(sqrt(2),[2,3])",
        "powerform(1,[999999999999999999999,1000])",
    ] {
        let result = parse_answer_expr(source)
            .map(|ast| answer_for_contract(&ast, &BTreeMap::new(), Some(&AnswerContract::Exact)));
        assert!(result.is_err() || result.unwrap().is_err(), "{source}");
    }
    let ast = parse_answer_expr("powerform(2,[3,4])").unwrap();
    for policy in [
        None,
        Some(AnswerContract::Approx { decimals: 1 }),
        Some(AnswerContract::Label {
            options: vec![vec!["(2)*(3)^(4)".into()]],
        }),
    ] {
        assert!(answer_for_contract(&ast, &BTreeMap::new(), policy.as_ref()).is_err());
    }
}
