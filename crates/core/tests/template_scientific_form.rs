//! The scientific writer preserves an exact value in normalized standard form.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;

use cadus_core::{
    answer::{AnswerContract, Outcome, check_contract},
    template::{answer_for_contract, parse_answer_expr},
};

#[test]
fn writes_normalized_scientific_notation_exactly() {
    let policy = AnswerContract::RequiredNormalizedScientificNotation;
    for (source, expected) in [
        ("60000000", "6 x 10^7"),
        ("1/4000", "2.5 x 10^-4"),
        ("-1/4000", "-2.5 x 10^-4"),
        ("100000", "1 x 10^5"),
        ("6", "6 x 10^0"),
        ("123/100000", "1.23 x 10^-3"),
    ] {
        let answer = answer_for_contract(
            &parse_answer_expr(source).unwrap(),
            &BTreeMap::new(),
            Some(&policy),
        )
        .unwrap();
        assert_eq!(answer.text, expected, "{source}");
        assert!(matches!(
            check_contract(expected, &answer.text, policy.clone()),
            Outcome::Decided(verdict) if verdict.correct
        ));
    }
}

#[test]
fn refuses_values_without_a_bounded_normalized_decimal_rendering() {
    let policy = AnswerContract::RequiredNormalizedScientificNotation;
    for source in [
        "0",
        "1/3",
        "sqrt(2)",
        "1/((2^1000)*(2^1000))",
        "(10^1000)*(10^200)",
    ] {
        let result = parse_answer_expr(source)
            .map(|ast| answer_for_contract(&ast, &BTreeMap::new(), Some(&policy)));
        assert!(result.is_err() || result.unwrap().is_err(), "{source}");
    }
}

#[test]
fn leaves_exact_output_unchanged() {
    let answer = answer_for_contract(
        &parse_answer_expr("60000000").unwrap(),
        &BTreeMap::new(),
        Some(&AnswerContract::Exact),
    )
    .unwrap();
    assert_eq!(answer.text, "60000000");
}
