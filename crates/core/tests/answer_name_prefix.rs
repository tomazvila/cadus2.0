//! A name written before the value (`d = 13 km`, `f'(x) = 2x`, `x = 2, y = 3`)
//! is not part of the value. Every accepted row has a wrong row beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

fn verdict(key: &str, json: &str, learner: &str) -> Option<bool> {
    let contract: AnswerContract = serde_json::from_str(json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

fn rows(key: &str, json: &str, right: &[&str], wrong: &[&str]) {
    for learner in right {
        assert_eq!(verdict(key, json, learner), Some(true), "{key} / {learner}");
    }
    for learner in wrong {
        assert_ne!(verdict(key, json, learner), Some(true), "{key} / {learner}");
    }
}

const EXACT: &str = r#"{"kind":"exact"}"#;
const FX: &str = r#"{"kind":"function","vars":["x"]}"#;
const FT: &str = r#"{"kind":"function","vars":["t"]}"#;
const FN: &str = r#"{"kind":"function","vars":["n"]}"#;
const KM: &str = r#"{"kind":"unit","quantity":"length","unit":"km","allow_omitted":true}"#;
const CM: &str = r#"{"kind":"unit","quantity":"length","unit":"cm","allow_omitted":true}"#;
const DEG: &str = r#"{"kind":"unit","quantity":"angle","unit":"°","allow_omitted":true}"#;
const COORDS: &str = r#"{"kind":"coordinates","arity":2}"#;

#[test]
fn a_letter_name_before_a_measured_value_falls_away() {
    rows(
        "13 km",
        KM,
        &["d = 13 km", "d = 13", "d=13000 m"],
        &["d = 14 km", "d = 13 cm"],
    );
    rows("75 cm", CM, &["x = 75 cm", "x = 75"], &["x = 7.5 cm"]);
    rows("66°", DEG, &["α = 66°", "α = 66"], &["α = 67°"]);
    rows("35", EXACT, &["θ = 35", "x = 35"], &["θ = 36"]);
}

#[test]
fn a_function_name_before_a_formula_falls_away() {
    rows(
        "2x",
        FX,
        &["f'(x) = 2x", "dy/dx = 2x", "f(x) = 2x"],
        &["f'(x) = 3x", "f'(x) = 2x + 1"],
    );
    rows(
        "500e^(0.06t)",
        FT,
        &["A(t) = 500e^(0.06t)", "A(t) = 500*e^(0.06*t)"],
        &["A(t) = 500e^(0.6t)"],
    );
    rows(
        "5n - 3",
        FN,
        &["a_n = 5n - 3", "a(n) = 5n - 3", "a_n=5*n-3"],
        &["a_n = 5n - 2"],
    );
    rows(
        "3x - 10",
        EXACT,
        &["f(g(x)) = 3x - 10"],
        &["f(g(x)) = 3x - 9"],
    );
    rows(
        "(x+3)/4",
        EXACT,
        &["f^-1(x) = (x+3)/4"],
        &["f^-1(x) = (x-3)/4"],
    );
    rows(
        "3x - 3",
        EXACT,
        &["(f+g)(x) = 3x - 3"],
        &["(f+g)(x) = 3x + 3"],
    );
    rows("16", EXACT, &["f(-5) = 16"], &["f(-5) = 15"]);
    rows("9x", EXACT, &["P = 9x"], &["P = 8x"]);
}

#[test]
fn a_spoken_name_before_a_value_falls_away() {
    rows("0", EXACT, &["slope is 0"], &["slope is 1"]);
    rows(
        "-5",
        EXACT,
        &["the coefficient is -5"],
        &["the coefficient is 5"],
    );
}

#[test]
fn named_coordinates_read_as_the_pair() {
    rows(
        "(2, 3)",
        COORDS,
        &["x = 2, y = 3", "(x = 2, y = 3)"],
        &["x = 3, y = 2", "x = 2, y = 4"],
    );
    rows("(3, 5)", COORDS, &["m = 3, b = 5"], &["m = 5, b = 3"]);
}

#[test]
fn approx_accepts_a_more_precise_spelling_of_the_key() {
    let two = r#"{"kind":"approx","decimals":2}"#;
    rows(
        "2.81",
        two,
        &["2.81", "2.807", "2.8149", "2.8100"],
        &["2.9", "2.8", "2.82", "2.8049"],
    );
    rows(
        "sqrt(2)",
        two,
        &["1.41", "1.414", "1.4142"],
        &["1.42", "1.415", "1.4"],
    );
    rows("1/3", two, &["0.33", "0.333", "0.3333"], &["0.34", "0.334"]);
}

const SETUP: &str = r#"{"kind":"relation_setup"}"#;

#[test]
fn a_setup_inequality_accepts_every_equal_spelling() {
    rows(
        "m/2 + 3 >= 12",
        SETUP,
        &[
            "0.5m+3>=12",
            "(1/2)m+3>=12",
            "m/2 + 3 >= 12",
            "12 <= m/2 + 3",
            "3 + m/2 >= 12",
        ],
        &["m/2 + 3 > 12", "m/2 + 3 >= 13", "m >= 18", "m/2 >= 9"],
    );
    rows(
        "8x + 10 <= 58",
        SETUP,
        &["58 >= 8x + 10", "10 + 8x <= 58"],
        &["58 <= 8x + 10", "8x + 10 < 58", "x <= 6"],
    );
    rows(
        "|x| < 5",
        SETUP,
        &["|x| < 5", "abs(x) < 5", "5 > |x|"],
        &["|x| <= 5", "|x| < 4", "x < 5"],
    );
    rows(
        "x >= 12",
        SETUP,
        &["12 <= x", "x >= 12"],
        &["x > 12", "12 >= x"],
    );
    rows("6 < y/3", SETUP, &["y/3 > 6", "6 < y/3"], &["y/3 < 6"]);
}

#[test]
fn a_set_takes_a_bare_comma_list_and_refuses_a_repeated_member() {
    let set = r#"{"kind":"set"}"#;
    rows(
        "{2, 5}",
        set,
        &["2, 5", "5, 2", "{2, 5}", "{5,2}"],
        &["2", "2, 2, 5", "{2,2,5}", "2, 5, 7", "2, 6"],
    );
}

#[test]
fn a_power_with_a_variable_exponent_reads_and_compares_by_value() {
    rows(
        "50*3^t",
        FT,
        &["50*3^t", "P(t) = 50*3^t", "50*3^(t)"],
        &["50*2^t", "50*3^(2t)", "3^t"],
    );
    rows(
        "300*(1.005)^(12t)",
        FT,
        &["300*1.005^(12*t)", "A = 300*(1.005)^(12t)"],
        &["300*(1.005)^(11t)", "300*1.05^(12t)"],
    );
    rows(
        "ln(1.05^t)",
        FT,
        &["t ln(1.05)", "t*ln(1.05)"],
        &["t ln(1.5)", "ln(1.05)"],
    );
    rows(
        "6*4^(n-1)",
        FN,
        &["6*4^(n-1)", "1.5*4^n"],
        &["6*4^n", "6*4^(n+1)"],
    );
    rows("2^n", FN, &["2^n", "2^(n)"], &["3^n", "n^2"]);
    // A key that holds `3^t` is read under the exact contract as well.
    rows("3^t", EXACT, &["3^t", "3^(t)"], &["3^(2t)", "t^3"]);
}

#[test]
fn a_polynomial_division_takes_the_pair_and_the_mixed_form() {
    let div = r#"{"kind":"polynomial_division","divisor":"x+1"}"#;
    rows(
        "x + 2 remainder 3",
        div,
        &[
            "x+2+3/(x+1)",
            "x + 2 remainder 3",
            "x+2 r 3",
            "x+2 R 3",
            "quotient x+2, remainder 3",
            "x + 2, remainder 3",
            "(x+2, 3)",
        ],
        &[
            "x+2+3/(x+2)",
            "x + 2 remainder 4",
            "x + 3 remainder 3",
            "x+2",
        ],
    );
    let div2 = r#"{"kind":"polynomial_division","divisor":"x+3"}"#;
    rows(
        "2x - 3 remainder -6",
        div2,
        &["2x-3 r -6", "2x-3 R -6", "2x-3-6/(x+3)"],
        &["2x-3 r 6", "2x-3+6/(x+3)"],
    );
    let plain = r#"{"kind":"quotient_remainder"}"#;
    rows(
        "9 R 2",
        plain,
        &[
            "quotient 9, remainder 2",
            "q = 9, r = 2",
            "9 with remainder 2",
            "9 r 2",
        ],
        &["quotient 2, remainder 9", "q = 9, r = 3"],
    );
}

#[test]
fn a_label_sentence_with_one_alias_names_that_option() {
    let label =
        r#"{"kind":"label","options":[["yes","it is a function"],["no","not a function"]]}"#;
    rows(
        "yes",
        label,
        &[
            "Yes, it is",
            "Yes, it is a function",
            "yes, definitely",
            "Yes it does.",
        ],
        &["No, it is not a function", "no", "yes and no", "maybe so"],
    );
    let label = r#"{"kind":"label","options":[["yes","must be connected"],["no","disconnected"]]}"#;
    rows(
        "yes",
        label,
        &["Yes, it must be connected", "It must be connected."],
        &["it is disconnected"],
    );
}

#[test]
fn inequality_spellings_read_as_the_same_set() {
    let union = r#"{"kind":"inequality_union"}"#;
    rows(
        "x < -4",
        union,
        &["(-inf, -4)", "(-∞, -4)", "x < -4", "-4 > x"],
        &["(-inf, -4]", "x < 4"],
    );
    rows(
        "x < 0 or x > 2",
        union,
        &["(-inf, 0) U (2, inf)", "x < 0 or x > 2", "(-oo,0) U (2,oo)"],
        &["(-inf, 0) U [2, inf)"],
    );
    rows(
        "-7 < x < 7",
        union,
        &["x > -7 and x < 7", "(-7, 7)"],
        &["x > -7 or x < 7", "[-7, 7]"],
    );
    rows(
        "x < 0 or x > 0",
        union,
        &["x != 0", "x ≠ 0", "all real numbers except 0"],
        &["x != 1", "all real numbers except 1"],
    );
    rows(
        "x < 2 or 2 < x < 5 or x > 5",
        union,
        &[
            "x ≠ 2, 5",
            "x != 2 and x != 5",
            "all real numbers except 2 and 5",
        ],
        &["x != 2", "x != 5"],
    );
    rows(
        "y > 0",
        EXACT,
        &[
            "(0, ∞)",
            "(0, infinity)",
            "y is greater than 0",
            "y > 0",
            "(0, inf)",
        ],
        &["[0, ∞)", "x > 0", "y is greater than 1", "(0, 5)"],
    );
    rows(
        "-1 <= y <= 1",
        EXACT,
        &["[-1, 1]", "-1 <= y <= 1"],
        &["(-1, 1)", "[-1, 2]"],
    );
}

#[test]
fn a_list_takes_outer_parentheses_and_spaces() {
    let ordered = r#"{"kind":"list","ordered":true,"member":{"kind":"exact"}}"#;
    let unordered = r#"{"kind":"list","ordered":false,"member":{"kind":"exact"}}"#;
    rows(
        "3, 7, 15",
        ordered,
        &["(3, 7, 15)", "3, 7, 15", "3 7 15"],
        &["(3, 15, 7)", "3 15 7", "(3, 7)"],
    );
    rows(
        "1, 0, 0, 1",
        ordered,
        &["1 0 0 1", "(1, 0, 0, 1)"],
        &["1 0 1 0"],
    );
    rows(
        "1, 2, 3, 4, 6, 12",
        unordered,
        &["1 2 3 4 6 12", "12 6 4 3 2 1"],
        &["1 2 3 4 6"],
    );
}

#[test]
fn a_nested_radical_key_takes_an_equal_value() {
    rows(
        "sqrt(2+sqrt(3))/2",
        EXACT,
        &["(sqrt(6)+sqrt(2))/4", "sqrt(2+sqrt(3))/2", "√(2+√3)/2"],
        &["(sqrt(6)+sqrt(3))/4", "sqrt(2+sqrt(3))", "0.9659"],
    );
}

#[test]
fn a_large_key_does_not_hide_an_extra_term() {
    let anti = r#"{"kind":"function","vars":["x"],"up_to_constant":true}"#;
    rows(
        "(x^4+2)^7/7 + C",
        anti,
        &["(x^4+2)^7/7 + C", "(x^4+2)^7/7 + 5"],
        &["(x^4+2)^7/7 + x", "(x^4+2)^7/7 + x^2 + C"],
    );
    rows(
        "(x^4+7)^6/24 + C",
        anti,
        &["(x^4+7)^6/24 + 3"],
        &["(x^4+7)^6/24 + 0.1x + C"],
    );
    rows(
        "(x^4+2)^7/7",
        FX,
        &["(x^4+2)^7/7"],
        &["(x^4+2)^7/7 + x", "(x^4+2)^7/7 + x^2"],
    );
}

#[test]
fn a_key_finite_on_a_part_of_the_line_samples_there() {
    rows(
        "sqrt(x-2)",
        FX,
        &["(x-2)^(1/2)", "sqrt(x - 2)"],
        &["sqrt(x-1)", "x-2"],
    );
    rows(
        "-ln(cos(x))",
        FX,
        &["ln(sec(x))", "-ln(cos(x))"],
        &["ln(cos(x))", "ln(sin(x))"],
    );
}

#[test]
fn inverse_trig_spellings_and_sech_read() {
    rows(
        "asin(x)",
        FX,
        &["sin^-1(x)", "arcsin(x)", "asin(x)"],
        &["sin(x)", "1/sin(x)"],
    );
    rows(
        "atan(x)",
        FX,
        &["tan^-1(x)", "arctan(x)"],
        &["tan(x)", "1/tan(x)"],
    );
    rows(
        "1/cosh(x)",
        FX,
        &["sech(x)", "sech x"],
        &["cosh(x)", "csch(x)"],
    );
}
