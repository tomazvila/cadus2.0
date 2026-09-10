//! A symbolic-base powerform keeps exponent variation bounded at instantiation.
#![allow(clippy::unwrap_used)]
use std::collections::BTreeMap;

use cadus_core::{
    answer::{AnswerContract, canonical_form},
    template::{answer_for_contract, domain::Value, parse_answer_expr},
};
use num_rational::BigRational;

fn bindings(values: &[(&str, i64)]) -> BTreeMap<String, Value> {
    values
        .iter()
        .map(|(name, value)| {
            (
                (*name).to_owned(),
                Value::Num(BigRational::from_integer((*value).into())),
            )
        })
        .collect()
}

#[test]
fn exact_powerform_writes_one_conventional_symbol_with_a_bound_exponent() {
    for (source, bound, expected) in [
        ("powerform(3,[x,a-1])", 6, "3*x^5"),
        ("powerform(1,[y,2*a])", 4, "y^8"),
        ("powerform(2,[z,2*a+3])", 3, "2*z^9"),
    ] {
        let answer = answer_for_contract(
            &parse_answer_expr(source).unwrap(),
            &bindings(&[("a", bound)]),
            Some(&AnswerContract::Exact),
        )
        .unwrap();
        assert_eq!(answer.canon, canonical_form(expected).unwrap());
        assert!(answer.text.contains('^'));
    }
}

#[test]
fn symbolic_powerform_accepts_literal_zero_negative_and_boundary_exponents() {
    for (source, expected_text, expected_value) in [
        ("powerform(1,[x,-1000])", "(1)*(x)^(-1000)", "x^-1000"),
        ("powerform(1,[x,0])", "(1)*(x)^(0)", "1"),
        ("powerform(1,[x,1000])", "(1)*(x)^(1000)", "x^1000"),
    ] {
        let answer = answer_for_contract(
            &parse_answer_expr(source).unwrap(),
            &BTreeMap::new(),
            Some(&AnswerContract::Exact),
        )
        .unwrap();
        assert_eq!(answer.text, expected_text);
        assert_eq!(answer.canon, canonical_form(expected_value).unwrap());
    }
}

#[test]
fn symbolic_powerform_fails_closed_outside_its_narrow_surface() {
    let exact = Some(&AnswerContract::Exact);
    for source in [
        "powerform(1,[x+y,a])",
        "powerform(1,[2*x,a])",
        "powerform(1,[sqrt(x),a])",
        "powerform(1,[q,a])",
        "powerform(1,[x,1001*a])",
        "powerform(1,[x,-1001*a])",
        "powerform(1,[x,a/2])",
    ] {
        let result = answer_for_contract(
            &parse_answer_expr(source).unwrap(),
            &bindings(&[("a", 1)]),
            exact,
        );
        assert!(result.is_err(), "{source}");
    }

    let text_base = BTreeMap::from([
        ("b".to_owned(), Value::Text("x".to_owned())),
        (
            "a".to_owned(),
            Value::Num(BigRational::from_integer(3.into())),
        ),
    ]);
    let ast = parse_answer_expr("powerform(1,[b,a])").unwrap();
    assert!(answer_for_contract(&ast, &text_base, exact).is_err());
}

#[test]
fn numeric_and_required_single_power_behavior_stays_unchanged() {
    let numeric = answer_for_contract(
        &parse_answer_expr("powerform(3/2,[2,-3])").unwrap(),
        &BTreeMap::new(),
        Some(&AnswerContract::Exact),
    )
    .unwrap();
    assert_eq!(numeric.text, "(3/2)*(2)^(-3)");

    let required = answer_for_contract(
        &parse_answer_expr("powerform(1,[2,3+4])").unwrap(),
        &BTreeMap::new(),
        Some(&AnswerContract::RequiredSinglePower),
    )
    .unwrap();
    assert_eq!(required.text, "(2)^(7)");

    let symbolic = parse_answer_expr("powerform(1,[x,a])").unwrap();
    assert!(
        answer_for_contract(
            &symbolic,
            &bindings(&[("a", 3)]),
            Some(&AnswerContract::RequiredSinglePower),
        )
        .is_err()
    );
    assert!(parse_answer_expr("x**(a-1)").is_err());
}
