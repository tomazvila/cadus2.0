//! Implied multiplication binds tighter than division (2026-11-02).
//!
//! `V/lh` is `V/(l*h)`, the reading of the textbook convention and of the
//! compute engine of the editor. An explicit `*` keeps left-to-right order.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::answer::{AnswerContract, Ast, Outcome, check_contract, normalize, parse};

fn tree(text: &str) -> Ast {
    parse(&normalize(text).source).unwrap_or_else(|reason| panic!("{text:?}: {reason:?}"))
}

fn same(left: &str, right: &str) {
    assert_eq!(tree(left), tree(right), "{left:?} must read as {right:?}");
}

fn differ(left: &str, right: &str) {
    assert_ne!(
        tree(left),
        tree(right),
        "{left:?} must not read as {right:?}"
    );
}

#[test]
fn glued_names_after_a_slash_are_one_divisor() {
    same("a/bc", "a/(b*c)");
    differ("a/b c", "a/(b*c)");
    same("a/b c", "(a/b)*c");
    differ("a/bc", "(a/b)*c");
}

#[test]
fn a_number_and_a_name_after_a_slash_are_one_divisor() {
    same("1/2x", "1/(2*x)");
    same("x/2y^2", "x/(2*y^2)");
    same("1/2x", "1/(2x)");
}

#[test]
fn a_bracket_after_a_divisor_belongs_to_it() {
    same("6/2(1+2)", "6/(2*(1+2))");
}

#[test]
fn an_explicit_sign_ends_the_divisor() {
    same("a/b*c", "(a/b)*c");
    differ("a/b*c", "a/(b*c)");
    same("a/b/c", "(a/b)/c");
    same("2x/3y", "(2*x)/(3*y)");
}

#[test]
fn a_factor_before_the_slash_stays_in_the_dividend() {
    same("2x/y", "(2*x)/y");
}

/// The verdict of the `function` contract over the variables `V`, `l`, `h`.
fn formula_verdict(key: &str, learner: &str) -> bool {
    let contract: AnswerContract =
        serde_json::from_str(r#"{"kind":"function","vars":["V","l","h"]}"#).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => verdict.correct,
        Outcome::Undecidable(refusal) => panic!("{learner:?}: {refusal:?}"),
    }
}

#[test]
fn the_rearranging_formulas_defect_grades_correct() {
    assert!(formula_verdict("V/(l*h)", "V/lh"));
    assert!(formula_verdict("V/(l*h)", "V/(lh)"));
    assert!(formula_verdict("V/(l*h)", "V/l/h"));
    assert!(!formula_verdict("V/(l*h)", "V/l*h"));
    assert!(!formula_verdict("V/(l*h)", "V*l*h"));
    assert!(formula_verdict("V/lh", "V/(l*h)"));
    assert!(formula_verdict("(V/l)*h", "V/l*h"));
}

#[test]
fn a_spaced_factor_after_a_fraction_of_numbers_is_its_coefficient() {
    same("1/sqrt(2) atan(x)", "(1/sqrt(2))*atan(x)");
    same("1/2 x", "(1/2)*x");
    same("3/5 x", "(3/5)*x");
    same("1/2x", "1/(2*x)");
    differ("1/2 x", "1/2x");
}
