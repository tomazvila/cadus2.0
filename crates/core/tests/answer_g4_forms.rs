//! Grader pass 4, the forms families: equation keys, required forms, name
//! prefixes, multipart spellings, and the `Nr-N` remainder misread. Each row
//! names the item key of the triage; each accepted answer has a wrong twin.

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
const RELATION: &str = r#"{"kind":"polynomial_relation"}"#;
const FX: &str = r#"{"kind":"function","vars":["x"]}"#;
const FT: &str = r#"{"kind":"function","vars":["t"]}"#;

fn form(name: &str) -> String {
    format!(r#"{{"kind":"required_form","form":"{name}"}}"#)
}

fn multipart(names: &[&str]) -> String {
    let parts: Vec<String> = names
        .iter()
        .map(|name| format!(r#"{{"name":"{name}","contract":{{"kind":"exact"}}}}"#))
        .collect();
    format!(r#"{{"kind":"multipart","parts":[{}]}}"#, parts.join(","))
}

#[test]
fn row_175_and_57_a_rational_relation_clears_its_denominator() {
    rows(
        "y = 1/x + 1",
        RELATION,
        &["xy = 1 + x", "y = 1/x + 1"],
        &["xy = 1", "y = 1 + x"],
    );
    rows(
        "y = 36/x",
        RELATION,
        &["xy = 36", "36 = xy", "yx = 36"],
        &["xy = 6", "y = 36x"],
    );
}

#[test]
fn row_57_an_expression_is_not_a_relation() {
    rows("y = 0", RELATION, &["y = 0"], &["0", "x"]);
    assert_eq!(verdict("y = 0", RELATION, "0"), Some(false));
}

#[test]
fn row_6_195_95_20_45_an_equation_key_with_a_function_name() {
    rows(
        "f(x) = 3x-10",
        EXACT,
        &["f(x) = 3x - 10", "3x - 10"],
        &["f(x) = 3x - 11", "g(x) = 3x - 10"],
    );
    rows(
        "f(x, y) = x - 2y",
        EXACT,
        &["f(x, y) = x - 2y", "x - 2y"],
        &["f(x, y) = x - 3y", "x - 3y"],
    );
    rows(
        "h = 5t^2",
        FT,
        &["h(t) = 5t^2", "h = 5t^2"],
        &["h(t) = 5t^3"],
    );
    rows(
        "y = 2x+1",
        FX,
        &["f(x) = 2x+1", "y = 2x + 1"],
        &["f(x) = 2x+2"],
    );
    rows(
        "f(-5) = 16",
        FX,
        &["f(-5) = 16", "16"],
        &["f(-5) = 17", "f(3) = 16"],
    );
}

#[test]
fn row_187_equations_with_one_solution_set_are_one_answer() {
    rows(
        "x = 9 - 4y",
        EXACT,
        &["x + 4y = 9", "2x + 8y = 18"],
        &["x + 4y = 10", "x - 4y = 9"],
    );
    rows(
        "y = 2x+1",
        EXACT,
        &["2x - y + 1 = 0", "y = 1 + 2x"],
        &["2x - y + 2 = 0"],
    );
}

#[test]
fn row_107_a_plus_minus_equation_names_both_branches() {
    let key = "y = (7/6)x or y = -(7/6)x";
    rows(key, EXACT, &["y = ±(7/6)x"], &["y = ±(5/6)x", "y = (7/6)x"]);
}

#[test]
fn row_89_the_hat_names_of_a_regression_line() {
    rows(
        "yhat = 20 + 3x",
        RELATION,
        &["ŷ = 20 + 3x", "yhat = 20 + 3x", "ŷ = 3x + 20"],
        &["ŷ = 20 + 4x"],
    );
}

#[test]
fn row_207_a_trace_names_its_plane() {
    rows(
        "x + y = 3",
        RELATION,
        &["x + y = 3, z = 0", "z = 0, x + y = 3"],
        &["x + y = 3, z = 1", "x + y = 4, z = 0", "x + y = 3, x = 0"],
    );
}

#[test]
fn row_179_182_a_line_in_vector_form() {
    rows(
        "t(-1,1,1)+(1,2,0)",
        EXACT,
        &[
            "(1,2,0)+t(-1,1,1)",
            "t(-1,1,1) + (1,2,0)",
            "(1, 2, 0) + s(-1, 1, 1)",
        ],
        &["(1,2,0)+t(1,1,1)", "(1,2,1)+t(-1,1,1)"],
    );
    rows("t(-2,1)", EXACT, &["t(-2, 1)"], &["t(2, 1)"]);
    rows(
        "t(-2,1)+(4,0)",
        EXACT,
        &["x = 4 - 2t, y = t"],
        &["x = 4 + 2t, y = t"],
    );
}

#[test]
fn row_179_254_a_word_for_a_value() {
    rows("z", EXACT, &["z is free"], &["w is free"]);
    rows("0", EXACT, &["zero", "Zero."], &["one"]);
    rows("pi", EXACT, &["Pi", "pi"], &["e"]);
}

#[test]
fn row_254_an_equation_where_an_expression_is_asked_has_no_verdict() {
    let expanded = form("expanded_polynomial");
    assert_eq!(verdict("x^2 - 18", &expanded, "x^2 - 18 = 0"), None);
    rows("x^2 - 18", &expanded, &["x^2 - 18"], &["x^2 - 17 = 0"]);
}

#[test]
fn row_194_201_268_130_multipart_with_tuple_and_and() {
    rows(
        "v_1 = 2, v_2 = 3",
        &multipart(&["v_1", "v_2"]),
        &["v_1=2 and v_2=3", "v_1 = 2, v_2 = 3"],
        &["v_1=3 and v_2=2"],
    );
    rows(
        "v1 = 2, v2 = 3",
        &multipart(&["v1", "v2"]),
        &["v1 = 2 and v2 = 3"],
        &["v1 = 2 and v2 = 4"],
    );
    rows(
        "a = 1, b = 2, c = 3",
        &multipart(&["a", "b", "c"]),
        &["(1, 2, 3)", "(a, b, c) = (1, 2, 3)"],
        &["(1, 2, 4)", "(a, b, c) = (1, 2, 4)", "(3, 2, 1)"],
    );
    rows(
        "a = 3, b = 2",
        &multipart(&["a", "b"]),
        &["(3, 2)"],
        &["(2, 3)"],
    );
    rows(
        "x = 6; y = -17",
        &multipart(&["x", "y"]),
        &["(6, -17)"],
        &["(-17, 6)", "(6, 17)"],
    );
}

#[test]
fn row_211_a_part_name_with_call_syntax() {
    let json = r#"{"kind":"multipart","parts":[{"name":"g","contract":{"kind":"function","vars":["x"]}}]}"#;
    rows("g = x^2", json, &["g(x) = x^2", "g = x^2"], &["g(x) = x^3"]);
}

#[test]
fn row_99_107_a_part_name_without_a_sign() {
    rows(
        "vertical = x = 1; slant = y = x + 2",
        &multipart(&["vertical", "slant"]),
        &[
            "vertical = x = 1; slant = y = x + 2",
            "vertical x = 1, slant y = x + 2",
        ],
        &["vertical x = 2, slant y = x + 2"],
    );
    let json = r#"{"kind":"multipart","parts":[{"name":"center","contract":{"kind":"coordinates","arity":2}},{"name":"radius","contract":{"kind":"exact"}}]}"#;
    rows(
        "center = (-4, 1); radius = 5",
        json,
        &["center (-4,1), radius 5"],
        &["center (-4,1), radius 6", "center (4,1), radius 5"],
    );
}

#[test]
fn row_153_a_verdict_sentence_with_a_leading_pronoun() {
    let json = r#"{"kind":"multipart","parts":[{"name":"kind","contract":{"kind":"label","options":[["overestimates"],["underestimates"]]}},{"name":"amount","contract":{"kind":"unit","quantity":"volume","unit":"liters"}}]}"#;
    rows(
        "kind = overestimates; amount = 12 liters",
        json,
        &[
            "It overestimates by 12 litres",
            "It overestimates by 12 liters",
        ],
        &[
            "It underestimates by 12 litres",
            "It overestimates by 13 liters",
        ],
    );
}

#[test]
fn row_19_a_labelled_list() {
    rows(
        "3, pi/2",
        EXACT,
        &["amplitude 3, period π/2"],
        &["amplitude 3, period π/3"],
    );
    rows(
        "amplitude = 3; period = pi/2",
        &multipart(&["amplitude", "period"]),
        &["amplitude 3, period π/2"],
        &["amplitude 3, period π/3"],
    );
}

#[test]
fn row_275_151_name_prefixes() {
    rows("x", EXACT, &["(g∘f)^-1(x) = x"], &["(g∘f)^-1(x) = 2x"]);
    rows("x/2", FX, &["(g∘f)^-1(x) = x/2"], &["(g∘f)^-1(x) = x/3"]);
    rows("3x^2 dx", EXACT, &["dy = 3x^2 dx"], &["dy = 3x dx"]);
    rows(
        "0.3",
        EXACT,
        &["P(X=1) = 0.3", "P(X = 1) = 0.3"],
        &["P(X=1) = 0.4"],
    );
}

#[test]
fn row_39_place_value_with_a_comma_of_thousands() {
    let place = form("expanded_place_value");
    rows(
        "4000 + 500 + 6",
        &place,
        &["4,000 + 500 + 6", "4000 + 500 + 6"],
        &["4,506", "4,000 + 500 + 7"],
    );
}

#[test]
fn row_52_the_simplest_radical_form() {
    let radical = form("simplest_radical");
    rows(
        "9*sqrt(2)",
        &radical,
        &["9√2", "x = 9√2", "9*sqrt(2)"],
        &["sqrt(162)", "3*sqrt(18)", "9√3"],
    );
}

#[test]
fn row_61_the_log_forms() {
    let expanded = form("expanded_log");
    rows("3ln(x)", &expanded, &["3ln(x)", "3*ln(x)"], &["ln(x^3)"]);
    let condensed = form("condensed_log");
    rows(
        "ln(x^3)",
        &condensed,
        &["ln(x^3)"],
        &["3ln(x)", "ln(x)+ln(x)+ln(x)"],
    );
}

#[test]
fn row_219_a_function_factor_is_a_factor() {
    let factored = form("factored_polynomial");
    rows(
        "sin(x)(x + 1)",
        &factored,
        &["sin(x)(x + 1)", "(x + 1)sin(x)"],
        &["x sin(x) + sin(x)"],
    );
}

#[test]
fn row_279_a_single_power_of_a_variable() {
    let power = r#"{"kind":"required_single_power"}"#;
    rows("a^12", power, &["a^12"], &["a^7 a^5", "a^13"]);
    rows("2^5", power, &["2^5"], &["4*8", "32"]);
}

#[test]
fn the_remainder_marker_before_a_minus_is_algebra_outside_a_division_item() {
    rows(
        "3*r-10",
        EXACT,
        &["3r-10", "3r - 10", "3 r -10"],
        &["3r+10"],
    );
    rows(
        "r^2+3*r-10",
        EXACT,
        &["r^2+3r-10", "r^2 + 3r - 10"],
        &["r^2+3r+10"],
    );
    let division = r#"{"kind":"quotient_remainder"}"#;
    rows(
        "3 r 10",
        division,
        &["3 r 10", "3R10", "(3, 10)"],
        &["3 r 11"],
    );
}

#[test]
fn routed_a9_simplified_rational_refuses_unreduced_fractions() {
    let simple = form("simplified_rational");
    rows("3x/5", &simple, &["3x/5", "3*x/5"], &["6x/10", "6x/8"]);
}

#[test]
fn routed_a85_a_not_equal_statement_in_any_spelling() {
    rows(
        "beta != 0",
        EXACT,
        &["beta ≠ 0", "β != 0", "0 ≠ β"],
        &["x != 0", "beta != 1"],
    );
    rows("β ≠ 0", EXACT, &["beta != 0", "β ≠ 0"], &["beta != 2"]);
}
