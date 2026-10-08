//! Grader pass 4, VALUES families: logarithms and radicals compare by value,
//! function domains and variable names, approximate answers, and infinity.
//! Every accepted pair has a wrong twin beside it.

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

fn ungraded(key: &str, json: &str, learners: &[&str]) {
    for learner in learners {
        assert_eq!(verdict(key, json, learner), None, "{key} / {learner}");
    }
}

#[test]
fn logarithm_of_a_root_is_a_fraction_of_the_logarithm() {
    right(
        "ln(3)/2",
        EXACT,
        &["ln(sqrt(3))", "0.5 ln 3", "ln(3^(1/2))"],
    );
    wrong("ln(3)/2", EXACT, &["ln(3)", "ln(sqrt(5))", "ln(3)/3"]);
    right("ln(4/3)/2", EXACT, &["ln(2/sqrt(3))", "ln(2) - ln(3)/2"]);
    wrong("ln(4/3)/2", EXACT, &["ln(2/sqrt(5))", "ln(4/3)"]);
    right("ln(2)/3", EXACT, &["ln(cbrt(2))", "ln(2^(1/3))"]);
    wrong("ln(2)/3", EXACT, &["ln(cbrt(3))"]);
}

#[test]
fn a_power_with_a_variable_exponent_is_the_exponential_of_a_logarithm() {
    let key = "ln(2) e^(x ln(2))";
    right(key, EXACT, &["2^x ln 2", "ln(2)*2^x", "ln(2) 2^x"]);
    wrong(key, EXACT, &["2^x", "ln(2)*3^x", "ln(3)*2^x", "ln(2) 4^x"]);
    right("4^x", EXACT, &["2^(2x)"]);
    wrong("4^x", EXACT, &["2^x", "8^x"]);
}

#[test]
fn a_root_of_a_plain_atom_merges_with_the_power_of_that_atom() {
    right("1/sqrt(y)", EXACT, &["sqrt(1/y)", "sqrt(y)/y", "y^(-1/2)"]);
    wrong("1/sqrt(y)", EXACT, &["sqrt(y)", "sqrt(y)/y^2", "1/y"]);
    right("y*sqrt(y)", EXACT, &["sqrt(y^3)", "y^(3/2)"]);
    // An even power keeps its absolute value: sqrt(y^2) is not y.
    wrong("y", EXACT, &["sqrt(y^2)"]);
}

#[test]
fn the_root_of_a_negative_number_squares_back_to_that_number() {
    right("6", EXACT, &["(1+sqrt(-5))(1-sqrt(-5))"]);
    wrong(
        "6",
        EXACT,
        &["(1+sqrt(-5))(1+sqrt(-5))", "(1+sqrt(5))(1-sqrt(5))"],
    );
    right("-5", EXACT, &["sqrt(-5)^2", "sqrt(-5)*sqrt(-5)"]);
}

#[test]
fn a_nested_square_root_equals_its_denested_sum() {
    right(
        "(sqrt(6)+sqrt(2))/4",
        EXACT,
        &["sqrt(2+sqrt(3))/2", "sqrt(2+sqrt(3))/2"],
    );
    wrong(
        "(sqrt(6)+sqrt(2))/4",
        EXACT,
        &["sqrt(2-sqrt(3))/2", "sqrt(2+sqrt(3))/3"],
    );
    right("sqrt(2)-1", EXACT, &["sqrt(3-2sqrt(2))"]);
    wrong("sqrt(2)+1", EXACT, &["sqrt(3-2sqrt(2))"]);
    right("2+sqrt(3)", EXACT, &["sqrt(7+4sqrt(3))"]);
    // A radicand that does not denest stays a value of its own.
    wrong("sqrt(2)", EXACT, &["sqrt(3+sqrt(5))"]);
    let unit = r#"{"kind":"unit","quantity":"length","unit":"km"}"#;
    right(
        "sqrt(2+sqrt(3))/2 km",
        unit,
        &["sqrt(2+sqrt(3))/2 km", "(sqrt(6)+sqrt(2))/4 km"],
    );
}

#[test]
fn a_relation_with_a_root_constant_compares_as_a_relation() {
    let relation = r#"{"kind":"polynomial_relation"}"#;
    right(
        "y = -x + 2sqrt(2)",
        relation,
        &["x + y = 2√2", "x + y = 2sqrt(2)", "2y + 2x = 4sqrt(2)"],
    );
    wrong(
        "y = -x + 2sqrt(2)",
        relation,
        &["x + y = 2sqrt(3)", "x + y = 2", "x - y = 2sqrt(2)"],
    );
}

#[test]
fn a_sample_domain_on_quarters_is_fine_without_pi() {
    let domain = r#"{"kind":"function","vars":["x"],"domain":{"x":["1","9"]}}"#;
    right("x^2", domain, &["x^2", "x*x"]);
    wrong("x^2", domain, &["x^3", "x^2+1"]);
    // With pi in the key the quarter points make sin(pi x) vanish, so the key is refused.
    ungraded("sin(pi*x)", domain, &["sin(pi*x)"]);
}

#[test]
fn a_function_variable_may_be_a_word_and_a_contract_may_hold_four_variables() {
    let rho = r#"{"kind":"function","vars":["rho"]}"#;
    right("rho^2", rho, &["rho^2", "rho*rho"]);
    wrong("rho^2", rho, &["rho^3"]);
    let pair = r#"{"kind":"function","vars":["nT","V"]}"#;
    right("nT*V", pair, &["nT V", "V nT", "nT*V"]);
    wrong("nT*V", pair, &["nT + V"]);
    let capitals = r#"{"kind":"function","vars":["T","V"]}"#;
    right("T*V", capitals, &["TV", "T V"]);
    let four = r#"{"kind":"function","vars":["a","b","c","d"]}"#;
    right("a+b+c+d", four, &["a+b+c+d", "d+c+b+a"]);
    wrong("a+b+c+d", four, &["a+b+c", "a+b+c+2d"]);
    assert!(
        serde_json::from_str::<AnswerContract>(
            r#"{"kind":"function","vars":["a","b","c","d","e1"]}"#
        )
        .is_err()
    );
}

#[test]
fn a_remainder_mark_never_splits_a_function_of_r() {
    let radius = r#"{"kind":"function","vars":["r"]}"#;
    right("r^2+3*r-10", radius, &["r^2+3r-10", "r^2 + 3r - 10"]);
    right("3*r-10", radius, &["3r-10"]);
    right("2*r-1", radius, &["2r-1"]);
    wrong("3*r-10", radius, &["3r-11", "3r"]);
}

#[test]
fn an_approximate_key_may_be_a_constant() {
    let three = r#"{"kind":"approx","decimals":3}"#;
    right("ln(2)", three, &["0.693", "0.6931", "0.69315"]);
    wrong("ln(2)", three, &["0.692", "0.694", "0.7", "0.6932"]);
    right("ln(4)", three, &["1.386", "1.3863"]);
    wrong("ln(4)", three, &["1.387", "1.385"]);
    let two = r#"{"kind":"approx","decimals":2}"#;
    right("0.88*pi", two, &["2.76", "2.765", "2.7646"]);
    wrong("0.88*pi", two, &["2.77", "2.75", "2.8"]);
    right("e", two, &["2.72", "2.718"]);
    wrong("e", two, &["2.71", "2.73"]);
    right("pi", two, &["3.14", "3.142"]);
    wrong("pi", two, &["3.15", "3.1"]);
    right("e^2", two, &["7.39", "7.389"]);
    wrong("e^2", two, &["7.38", "7.4"]);
    right("pi/4", two, &["0.79", "0.785"]);
    right("2*cbrt(3)", two, &["2.88", "2.884"]);
    wrong("2*cbrt(3)", two, &["2.89", "2.87"]);
}

#[test]
fn a_rounding_tie_accepts_both_neighbours() {
    let two = r#"{"kind":"approx","decimals":2}"#;
    right("1.005", two, &["1.01", "1.00", "1.005"]);
    wrong("1.005", two, &["1.02", "0.99", "1.1"]);
    right("0.125", two, &["0.13", "0.12", "0.125"]);
    wrong("0.125", two, &["0.14", "0.11"]);
    // A value that is not a tie keeps one rounding only.
    right("1.004", two, &["1.00"]);
    wrong("1.004", two, &["1.01"]);
}

#[test]
fn an_estimate_mark_in_front_of_an_approximate_answer_is_not_part_of_it() {
    let two = r#"{"kind":"approx","decimals":2}"#;
    right(
        "8.94",
        two,
        &["about 8.94", "approximately 8.94", "≈ 8.94", "≈8.94"],
    );
    wrong("8.94", two, &["about 8.95", "≈ 8.9"]);
}

#[test]
fn a_unit_the_approximate_contract_does_not_name_leaves_the_pair_ungraded() {
    let two = r#"{"kind":"approx","decimals":2}"#;
    ungraded("2.76", two, &["2.76 cm^3"]);
    right("2.76", two, &["2.76"]);
}

#[test]
fn infinity_has_one_value_per_sign() {
    right(
        "+infinity",
        EXACT,
        &["+infinity", "∞", "+∞", "oo", "infinity", "inf"],
    );
    wrong("+infinity", EXACT, &["-infinity", "-∞", "-oo", "0", "1"]);
    right(
        "-infinity",
        EXACT,
        &["-infinity", "-∞", "-oo", "negative infinity"],
    );
    wrong("-infinity", EXACT, &["infinity", "∞"]);
    right("-oo", EXACT, &["-oo", "-infinity", "-∞"]);
    right("infinity", EXACT, &["infinite", "infinity", "oo"]);
    wrong("infinity", EXACT, &["0", "-infinity"]);
}

#[test]
fn a_union_of_intervals_reads_the_words_union_and_or() {
    let union = r#"{"kind":"inequality_union"}"#;
    right(
        "x < 1 or x > 4",
        union,
        &[
            "(-infinity,1) union (4,infinity)",
            "(-infinity, 1) or (4, infinity)",
            "(-oo,1) U (4,oo)",
        ],
    );
    wrong(
        "x < 1 or x > 4",
        union,
        &[
            "(-infinity,2) union (4,infinity)",
            "(-infinity,1) union [4,infinity)",
        ],
    );
    right("(-inf,inf)", union, &["all real numbers"]);
}

#[test]
fn a_constant_is_an_expanded_polynomial() {
    let expanded = r#"{"kind":"required_form","form":"expanded_polynomial"}"#;
    right("-7", expanded, &["-7"]);
    wrong("-7", expanded, &["7", "-7+0"]);
}
