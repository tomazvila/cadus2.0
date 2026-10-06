//! Cube roots and nth roots in the answer grammar: `\sqrt[n]{a}`, `root(n, a)`,
//! `cbrt(a)`, `∛a` and `∜a` all read as `a^(1/n)`. Every accepted spelling has
//! a wrong answer beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

const EXACT: &str = r#"{"kind":"exact"}"#;
const RADICAL: &str = r#"{"kind":"required_form","form":"radical"}"#;
const RATIONAL_EXPONENT: &str = r#"{"kind":"required_form","form":"rational_exponent"}"#;

fn verdict(key: &str, json: &str, learner: &str) -> Option<bool> {
    let contract: AnswerContract = serde_json::from_str(json).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

fn rows(key: &str, json: &str, right: &[&str], wrong: &[&str]) {
    for learner in right {
        assert_eq!(verdict(key, json, learner), Some(true), "{learner}");
    }
    for learner in wrong {
        assert_ne!(verdict(key, json, learner), Some(true), "{learner}");
    }
}

#[test]
fn a_cube_root_of_a_power_equals_the_fractional_exponent() {
    rows(
        "x^(2/3)",
        EXACT,
        &[
            r"\sqrt[3]{x^2}",
            "cbrt(x^2)",
            "∛(x^2)",
            "root(3, x^2)",
            r"(\sqrt[3]{x})^2",
            "(x^2)^(1/3)",
            "x^{2/3}",
        ],
        &[r"\sqrt[3]{x}", "x^(3/2)", r"\sqrt[4]{x^2}", "root(3, x^3)"],
    );
}

#[test]
fn a_root_of_a_number_reads_its_value() {
    rows(
        "2",
        EXACT,
        &[r"\sqrt[3]{8}", "cbrt(8)", "root(3, 8)", "∛8", "∛(8)"],
        &[r"\sqrt[3]{9}", "cbrt(9)", "root(3, 9)", "∛9"],
    );
    rows(
        "3",
        EXACT,
        &[r"\sqrt[4]{81}", "∜81", "root(4, 81)"],
        &[r"\sqrt[4]{80}", "∜16"],
    );
    rows(
        "2",
        EXACT,
        &[r"\sqrt[2]{4}", "root(2, 4)"],
        &[r"\sqrt[2]{5}"],
    );
    rows(
        "x^(1/2)",
        EXACT,
        &[r"\sqrt[2]{x}", "sqrt(x)"],
        &[r"\sqrt[3]{x}"],
    );
}

#[test]
fn the_radical_form_needs_a_root_sign() {
    rows(
        "root(3, x^2)",
        RADICAL,
        &[
            r"\sqrt[3]{x^2}",
            "cbrt(x^2)",
            "∛(x^2)",
            "root(3,x^2)",
            r"(\sqrt[3]{x})^2",
        ],
        &["x^(2/3)", r"\sqrt[3]{x}", "x^(3/2)"],
    );
    rows(
        "root(4, x^3)",
        RADICAL,
        &[r"\sqrt[4]{x^3}", "∜(x^3)", "root(4,x^3)"],
        &["x^(3/4)", r"\sqrt[4]{x^2}"],
    );
}

#[test]
fn the_rational_exponent_form_refuses_a_root_sign() {
    rows(
        "x^(2/3)",
        RATIONAL_EXPONENT,
        &["x^{2/3}", "x^(2/3)"],
        &[r"\sqrt[3]{x^2}", "cbrt(x^2)", "x^(3/2)"],
    );
}

#[test]
fn an_index_outside_two_to_nine_is_not_read() {
    for learner in [
        r"\sqrt[1]{8}",
        r"\sqrt[0]{8}",
        r"\sqrt[10]{8}",
        "root(1, 8)",
        "root(10, 8)",
        "root(x, 8)",
        "root(3)",
        "cbrt(2, 8)",
        r"\sqrt[x]{8}",
    ] {
        assert_ne!(verdict("2", EXACT, learner), Some(true), "{learner}");
    }
}

#[test]
fn an_odd_root_takes_a_power_apart_and_an_even_root_does_not() {
    rows(
        "x",
        EXACT,
        &["cbrt(x^3)", r"\sqrt[3]{x^3}"],
        &["sqrt(x^2)", r"\sqrt[4]{x^4}", "cbrt(x^2)"],
    );
    rows("x^(1/3)", EXACT, &["cbrt(x)"], &["sqrt(x)", "(x^2)^(1/6)"]);
}
