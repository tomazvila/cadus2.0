//! The required forms of grader pass 3: lowest terms, vertex form, expanded
//! place value, repeated multiplication, a fraction with the key's denominator,
//! and the three forms of a line. Every accepted row has a wrong row beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

fn verdict(key: &str, json: &str, learner: &str) -> Option<bool> {
    let contract: AnswerContract = serde_json::from_str(json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

fn rows(key: &str, form: &str, right: &[&str], wrong: &[&str]) {
    let json = format!(r#"{{"kind":"required_form","form":"{form}"}}"#);
    for learner in right {
        assert_eq!(
            verdict(key, &json, learner),
            Some(true),
            "{form}: {key} / {learner}"
        );
    }
    for learner in wrong {
        assert_ne!(
            verdict(key, &json, learner),
            Some(true),
            "{form}: {key} / {learner}"
        );
    }
}

#[test]
fn a_rational_expression_in_lowest_terms() {
    rows(
        "3x/4",
        "simplified_rational",
        &["3x/4", "(3x)/4"],
        &["6x/8", "12x/16", "3/4 + 0"],
    );
    rows(
        "ab/(a+b)",
        "simplified_rational",
        &["ab/(a+b)", "(a*b)/(a+b)"],
        &["1/(1/a+1/b)", "(a^2 b)/(a^2+ab)"],
    );
    rows(
        "x+1",
        "simplified_rational",
        &["x+1", "1+x"],
        &["(x^2-1)/(x-1)", "(2x+2)/2"],
    );
    rows(
        "(x+2)/(x+3)",
        "simplified_rational",
        &["(x+2)/(x+3)"],
        &["(x^2+4x+4)/(x^2+5x+6)", "(2x+4)/(2x+6)"],
    );
    rows("2/3", "simplified_rational", &["2/3"], &["4/6"]);
}

#[test]
fn the_vertex_form_of_a_parabola() {
    rows(
        "y = (x+3)^2 - 4",
        "vertex_form",
        &["y = (x+3)^2 - 4", "(x+3)^2-4", "y=(x + 3)^2 - 4"],
        &["y = x^2 + 6x + 5", "x^2+6x+5", "y = (x+3)^2 - 5"],
    );
    rows(
        "2(x-1)^2 + 3",
        "vertex_form",
        &["2(x-1)^2+3", "y = 2(x-1)^2 + 3"],
        &["2x^2-4x+5", "2(x-1)^2+4", "2(x-1)(x-1)+3"],
    );
}

#[test]
fn an_expanded_place_value_sum() {
    rows(
        "4000 + 500 + 6",
        "expanded_place_value",
        &[
            "4000+500+6",
            "4*1000 + 5*100 + 6",
            "4×10^3 + 5×10^2 + 6",
            "6 + 500 + 4000",
        ],
        &[
            "4506",
            "4000+506",
            "4000+400+100+6",
            "4000+500+6+0",
            "4000+500+7",
        ],
    );
    rows(
        "0.5 + 0.07",
        "expanded_place_value",
        &["0.5+0.07"],
        &["0.57", "0.4+0.17"],
    );
}

#[test]
fn a_product_of_one_repeated_factor() {
    rows(
        "5*5*5",
        "repeated_multiplication",
        &["5*5*5", "5 x 5 x 5", "5×5×5", "5 * 5 * 5"],
        &["125", "5^3", "5*5", "5*25", "5*5*5*1"],
    );
    rows(
        "x*x*x",
        "repeated_multiplication",
        &["x*x*x"],
        &["x^3", "x*x"],
    );
}

#[test]
fn a_fraction_with_the_denominator_of_the_key() {
    rows(
        "6/9",
        "fraction_with_denominator",
        &["6/9"],
        &["2/3", "4/6", "12/18", "0.666", "7/9"],
    );
}

#[test]
fn the_equation_of_a_line_in_each_named_form() {
    rows(
        "2x - y = 3",
        "standard_form_line",
        &["2x - y = 3", "2x-y=3"],
        &[
            "y = 2x - 3",
            "4x - 2y = 6",
            "-2x + y = -3",
            "2x - y - 3 = 0",
            "2x - y = 4",
        ],
    );
    rows(
        "y = 4x - 5",
        "slope_intercept_form",
        &["y = 4x - 5", "y=4x-5"],
        &["4x - y = 5", "y = 4x - 6", "y = -5 + 4x"],
    );
    rows(
        "y = x/2 + 1",
        "slope_intercept_form",
        &["y = x/2 + 1", "y = 0.5x + 1"],
        &["x - 2y = -2"],
    );
    rows(
        "y - 3 = 2(x - 1)",
        "point_slope_form",
        &["y-3=2(x-1)", "y - 3 = 2(x - 1)"],
        &["y = 2x + 1", "y - 3 = 2(x - 2)", "2x - y = -1"],
    );
}

#[test]
fn a_monic_relation_refuses_a_non_monic_multiple() {
    let monic = r#"{"kind":"monic_polynomial_relation"}"#;
    for learner in [
        "x^2 + 2x - 15 = 0",
        "x^2+2x=15",
        "0 = x^2 + 2x - 15",
        "x^2 + 2x - 15 = 0 ",
    ] {
        assert_eq!(
            verdict("x^2 + 2x - 15 = 0", monic, learner),
            Some(true),
            "{learner}"
        );
    }
    for learner in [
        "2x^2 + 4x - 30 = 0",
        "-x^2 - 2x + 15 = 0",
        "x^2 + 2x - 14 = 0",
    ] {
        assert_ne!(
            verdict("x^2 + 2x - 15 = 0", monic, learner),
            Some(true),
            "{learner}"
        );
    }
    // The plain contract still takes every nonzero multiple.
    let plain = r#"{"kind":"polynomial_relation"}"#;
    assert_eq!(
        verdict("x^2 + 2x - 15 = 0", plain, "2x^2 + 4x - 30 = 0"),
        Some(true)
    );
}

#[test]
fn a_unit_contract_takes_a_radical_sum_and_a_reduced_fraction_form() {
    let sum = r#"{"kind":"unit","quantity":"length","unit":"m","allow_omitted":true,"form":"simplest_radical_sum"}"#;
    assert_eq!(verdict("30 + 10*sqrt(3) m", sum, "30 + 10√3 m"), Some(true));
    assert_ne!(
        verdict("30 + 10*sqrt(3) m", sum, "30 + sqrt(300) m"),
        Some(true)
    );
    let fraction = r#"{"kind":"unit","quantity":"volume","unit":"L","allow_omitted":true,"form":"reduced_fraction"}"#;
    assert_eq!(verdict("1/6 L", fraction, "1/6 L"), Some(true));
    assert_eq!(verdict("1/6 L", fraction, "1/6"), Some(true));
    assert_ne!(verdict("1/6 L", fraction, "2/12 L"), Some(true));
    assert_ne!(verdict("1/6 L", fraction, "1/5 L"), Some(true));
}
