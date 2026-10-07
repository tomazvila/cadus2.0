//! Logarithms compare by value: `log` has base 10, `ln` has base e, `log_b(x)`
//! reads, and equal forms of one number are one answer. Every accepted row has a
//! wrong row beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

const EXACT: &str = r#"{"kind":"exact"}"#;
const X: &str = r#"{"kind":"function","vars":["x"],"domain":{"x":["1/4","11/4"]}}"#;

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

#[test]
fn log_is_base_ten_and_ln_is_base_e() {
    rows(
        "log(45)",
        EXACT,
        &[
            "log10(45)",
            "lg(45)",
            "ln(45)/ln(10)",
            "log(5)+log(9)",
            "log_10(45)",
        ],
        &["ln(45)", "log(54)"],
    );
}

#[test]
fn logarithms_of_different_bases_compare_by_value() {
    rows(
        "ln(10)/ln(2)",
        EXACT,
        &[
            "log_2(10)",
            "log2(10)",
            "log_{2}(10)",
            "log(10, 2)",
            "log_2 10",
        ],
        &["log_3(10)", "log(10)", "ln(10)"],
    );
}

#[test]
fn a_logarithm_of_a_product_splits_into_prime_logarithms() {
    rows(
        "ln(8)",
        EXACT,
        &["3 ln 2", "3*ln(2)", "ln(2)+ln(4)"],
        &["ln 9", "2 ln 3"],
    );
    rows("3*ln(2)", EXACT, &["ln(8)", "ln 8"], &["ln(9)", "ln(6)"]);
    rows("ln(9)", EXACT, &["2 ln 3"], &["3 ln 2"]);
}

#[test]
fn logarithms_with_a_variable_base_read_and_compare() {
    rows(
        "log_b(x)+log_b(y)",
        EXACT,
        &["log_b(y)+log_b(x)"],
        &["log_b(x)-log_b(y)"],
    );
    rows("log_b(x)", EXACT, &["ln(x)/ln(b)"], &["log_c(x)", "log(x)"]);
}

#[test]
fn a_function_contract_compares_logarithms_by_value() {
    rows("ln(x^3)", X, &["3 ln x", "3*ln(x)"], &["2 ln x", "ln(x)"]);
    let xyb = r#"{"kind":"function","vars":["x","y","b"],"domain":{"x":["2","5"],"y":["2","5"],"b":["2","5"]}}"#;
    rows(
        "log_b(x)+log_b(y)",
        xyb,
        &["log_b(x*y)", "log_b(y)+log_b(x)"],
        &["log_b(x)-log_b(y)", "log_b(x+y)"],
    );
    rows("log(x)", X, &["ln(x)/ln(10)", "log10(x)"], &["ln(x)"]);
}
