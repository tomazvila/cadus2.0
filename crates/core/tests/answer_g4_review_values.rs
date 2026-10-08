//! Grader pass 4, review rows: function domains, required forms, units,
//! natural logarithms, and column vectors. Every accepted pair has a wrong twin.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

const EXACT: &str = r#"{"kind":"exact"}"#;

fn verdict(key: &str, json: &str, learner: &str) -> Option<bool> {
    let contract: AnswerContract = serde_json::from_str(json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

fn right(key: &str, json: &str, learners: &[&str]) {
    for learner in learners {
        assert_eq!(verdict(key, json, learner), Some(true), "{key} / {learner}");
    }
}

fn wrong(key: &str, json: &str, learners: &[&str]) {
    for learner in learners {
        assert_eq!(
            verdict(key, json, learner),
            Some(false),
            "{key} / {learner}"
        );
    }
}

const FUNCTION: &str = r#"{"kind":"function","vars":["x"]}"#;
const SIMPLIFIED: &str = r#"{"kind":"required_form","form":"simplified_rational"}"#;
const REPEATED: &str = r#"{"kind":"required_form","form":"repeated_multiplication"}"#;
const SLOPE: &str = r#"{"kind":"required_form","form":"slope_intercept_form"}"#;
const POINT: &str = r#"{"kind":"required_form","form":"point_slope_form"}"#;
const TEMPERATURE: &str = r#"{"kind":"unit","quantity":"temperature","unit":"°C"}"#;
const DOLLAR: &str = r#"{"kind":"unit","quantity":"dollar","unit":"dollar"}"#;
const COLUMN: &str = r#"{"kind":"scalar_multiple","rows":2,"cols":1}"#;

#[test]
fn a_learner_must_be_defined_wherever_the_key_is() {
    right("sqrt(x)", FUNCTION, &["sqrt(|x|)"]);
    right("sqrt(x-1)", FUNCTION, &["sqrt(|x-1|)"]);
    right("ln(x)", FUNCTION, &["ln(|x|)", "ln(x^2)/2"]);
    right("ln(x-1)", FUNCTION, &["ln(|x-1|)"]);
    right("1/sqrt(x)", FUNCTION, &["1/sqrt(|x|)"]);
    wrong("x", FUNCTION, &["sqrt(x)^2"]);
    right("sqrt(x)^2", FUNCTION, &["x"]);
    wrong("x^2", FUNCTION, &["x^2*(x/x)"]);
    right("sqrt(x)", FUNCTION, &["sqrt(x)", "x^(1/2)"]);
    right("ln(x)", FUNCTION, &["ln(x)", "log_e(x)"]);
}

#[test]
fn a_written_domain_keeps_the_probe_inside_it() {
    let negative = r#"{"kind":"function","vars":["x"],"domain":{"x":["-3","-1"]}}"#;
    right("ln(abs(x))", negative, &["ln(-x)"]);
    wrong("ln(abs(x))", FUNCTION, &["ln(-x)"]);
    right(
        "log_b(x)+log_b(y)",
        r#"{"kind":"function","vars":["x","y","b"]}"#,
        &["log_b(x*y)"],
    );
}

#[test]
fn a_removable_singularity_is_not_the_same_function() {
    wrong("x+1", FUNCTION, &["(x^2-1)/(x-1)"]);
    right("(x^2-1)/(x-1)", FUNCTION, &["x+1"]);
    right("(x^2-1)/(x-1)", FUNCTION, &["(x-1)(x+1)/(x-1)"]);
}

#[test]
fn simplified_rational_refuses_decimals() {
    wrong("1/2", SIMPLIFIED, &["0.5", "0.50", "1/2.0", "2/4"]);
    wrong("3x/5", SIMPLIFIED, &["0.6x", "0.6*x"]);
    wrong("x/2", SIMPLIFIED, &["0.5x", "x*0.5"]);
    right("1/2", SIMPLIFIED, &["1/2"]);
    right("3x/5", SIMPLIFIED, &["3x/5", "(3/5)x", "3/5 x"]);
    right("x/2", SIMPLIFIED, &["x/2", "1/2 x"]);
}

#[test]
fn repeated_multiplication_keeps_the_factors_of_the_key() {
    wrong("2*2*2*2", REPEATED, &["4*4", "2*2*2", "16"]);
    right("2*2*2*2", REPEATED, &["2*2*2*2"]);
}

#[test]
fn slope_intercept_needs_simplified_coefficients_and_takes_labels() {
    wrong(
        "y = 2x + 3",
        SLOPE,
        &["y = 4x/2 + 3", "y = 6x/3 + 3", "y = 2x + 6/2"],
    );
    wrong("y = x/2 + 3", SLOPE, &["y = 2x/4 + 3", "y = 3x/6 + 3"]);
    right(
        "y = 2x + 3",
        SLOPE,
        &["y = 3 + 2x", "y=3+2x", "f(x) = 2x + 3", "g(x) = 2x + 3"],
    );
    wrong("y = 2x + 3", SLOPE, &["f(x) = 2x + 4", "y = 2x + 4"]);
    right("y = -x + 3", SLOPE, &["y = 3 - x"]);
}

#[test]
fn point_slope_takes_either_side_first() {
    right(
        "y - 1 = 2(x - 1)",
        POINT,
        &["y - 1 = 2(x - 1)", "2(x - 1) = y - 1"],
    );
    wrong(
        "y - 1 = 2(x - 1)",
        POINT,
        &["2(x - 1) = y - 2", "y - 1 = 3(x - 1)"],
    );
}

#[test]
fn a_one_letter_unit_reads_glued_or_spaced() {
    right("3m", EXACT, &["3 m", "3m"]);
    right("3s", EXACT, &["3 s"]);
    right("3L", EXACT, &["3 L"]);
    right("3 m", EXACT, &["3m"]);
    wrong("3m", EXACT, &["4 m", "3 s"]);
}

#[test]
fn cents_are_hundredths_of_a_dollar() {
    right("5 dollar", DOLLAR, &["500 cents", "5 dollars"]);
    right("0.5 dollar", DOLLAR, &["50 cents"]);
    wrong("5 dollar", DOLLAR, &["50 cents", "500 euro"]);
}

#[test]
fn a_bare_degree_sign_does_not_name_a_temperature_scale() {
    wrong("100 °C", TEMPERATURE, &["100°", "100 degrees"]);
    right("100 °C", TEMPERATURE, &["100 °C", "100°C"]);
}

#[test]
fn the_natural_logarithm_of_e_is_one() {
    right("ln(e)", EXACT, &["1", "log_e(e)"]);
    right("ln(e^2)", EXACT, &["2"]);
    right("ln(2e)", EXACT, &["ln(2) + 1"]);
    wrong("ln(e)", EXACT, &["0", "2", "e"]);
    wrong("ln(3)", EXACT, &["ln(3) + 1"]);
}

#[test]
fn a_flat_list_is_a_column_vector() {
    right("[[1],[2]]", COLUMN, &["[1,2]", "[2,4]", "[[1],[2]]"]);
    wrong("[[1],[2]]", COLUMN, &["[2,5]", "[1,2,3]"]);
}

#[test]
fn an_exact_value_that_rounds_to_the_key_is_correct() {
    let one = r#"{"kind":"approx","decimals":1}"#;
    let three = r#"{"kind":"approx","decimals":3}"#;
    let two = r#"{"kind":"approx","decimals":2}"#;
    right("8.9", one, &["sqrt(80)"]);
    right("0.333", three, &["1/3"]);
    right("0.667", three, &["2/3"]);
    right("3.14", two, &["pi"]);
    right("0.67", two, &["2/3"]);
    wrong("8.9", one, &["sqrt(78)", "sqrt(82)"]);
    wrong("0.333", three, &["2/3"]);
    wrong("3.14", two, &["e"]);
}

#[test]
fn a_tie_at_the_last_digit_takes_both_roundings() {
    let one = r#"{"kind":"approx","decimals":1}"#;
    let two = r#"{"kind":"approx","decimals":2}"#;
    right("8.9", one, &["8.85", "8.9"]);
    right("1.005", two, &["1.0049", "1.00", "1.01"]);
    wrong("8.9", one, &["8.75", "8.8"]);
    wrong("1.005", two, &["1.02", "0.99"]);
}

#[test]
fn standard_form_line_reads_each_spelling_of_the_same_line() {
    let standard = r#"{"kind":"required_form","form":"standard_form_line"}"#;
    right(
        "2x - y = -3",
        standard,
        &[
            "2x - y = -3",
            "2x - y = -3.0",
            "x*2 - y = -3",
            "-y + 2x = -3",
        ],
    );
    wrong(
        "2x - y = -3",
        standard,
        &["4x - 2y = -6", "y = 2x + 3", "2x - y = 3"],
    );
}

#[test]
fn a_learner_with_no_value_where_the_key_has_one_is_wrong() {
    wrong("x+1", FUNCTION, &["(x^2-1)/(x-1)"]);
    wrong("x", FUNCTION, &["sqrt(x)^2"]);
}
