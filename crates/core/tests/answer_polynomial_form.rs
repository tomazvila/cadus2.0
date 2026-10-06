//! The `factored_polynomial` and `expanded_polynomial` required forms: a
//! factoring item refuses the expanded or partly factored polynomial, and an
//! expansion item refuses the product or uncombined like terms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, NumericForm, Outcome, check_contract};

fn graded(form: NumericForm, expected: &str, learner: &str) -> bool {
    let outcome = check_contract(expected, learner, AnswerContract::RequiredForm { form });
    matches!(outcome, Outcome::Decided(verdict) if verdict.correct)
}

fn factored(expected: &str, learner: &str) -> bool {
    graded(NumericForm::FactoredPolynomial, expected, learner)
}

fn expanded(expected: &str, learner: &str) -> bool {
    graded(NumericForm::ExpandedPolynomial, expected, learner)
}

#[test]
fn a_complete_factorization_in_any_order_and_sign_arrangement_is_correct() {
    assert!(factored("(x + 3)(x + 4)", "(x+4)(x+3)"));
    assert!(factored("(x + 3)(x + 4)", "(x + 3)*(x + 4)"));
    assert!(factored("(x - 2)(x - 3)", "(2 - x)(3 - x)"));
    assert!(factored("-(x - 2)(x + 3)", "(2 - x)(x + 3)"));
    assert!(factored("(x + 5)^2", "(x+5)(x+5)"));
    assert!(factored("(x + 5)(x + 5)", "(x+5)^2"));
    assert!(factored("2x(x + 3)", "2*x*(x+3)"));
    assert!(factored("3(2x - 1)(x + 4)", "3(x+4)(2x-1)"));
    assert!(factored("(x^2 + 4)(x + 2)(x - 2)", "(x-2)(x+2)(x^2+4)"));
    assert!(factored("5a^2b(2a - 3b)", "5a^2 b (2a - 3b)"));
    assert!(factored("(2y - 7)(2y + 7)", "(2y+7)(2y-7)"));
}

#[test]
fn an_expanded_or_partly_factored_polynomial_is_wrong() {
    assert!(!factored("(x + 3)(x + 4)", "x^2 + 7x + 12"));
    assert!(!factored("2x(x + 3)", "2(x^2 + 3x)"));
    assert!(!factored("2x(x + 3)", "x(2x + 6)"));
    assert!(!factored("3(2x - 1)(x + 4)", "(6x - 3)(x + 4)"));
    assert!(!factored("(x^2 + 4)(x + 2)(x - 2)", "(x^2 + 4)(x^2 - 4)"));
    assert!(!factored("(x + 3)(x + 4)", "(x + 2)(x + 6)"));
    assert!(!factored("(x + 3)(x + 4)", "(x + 3)(x + 4)/1 + 0"));
}

#[test]
fn an_expansion_with_like_terms_combined_is_correct() {
    assert!(expanded("x^2 + 5x + 6", "x^2+5x+6"));
    assert!(expanded("x^2 + 5x + 6", "6 + 5x + x^2"));
    assert!(expanded("6x^3 - 4x^2", "6x^3-4x^2"));
    assert!(expanded("a^2 - 2ab + b^2", "a^2 - 2*a*b + b^2"));
    assert!(expanded("4x^2 - 9", "-9 + 4x^2"));
    assert!(expanded("2x^3", "2x^3"));
}

#[test]
fn a_product_or_uncombined_like_terms_are_wrong() {
    assert!(!expanded("x^2 + 5x + 6", "(x + 2)(x + 3)"));
    assert!(!expanded("x^2 + 5x + 6", "x^2 + 2x + 3x + 6"));
    assert!(!expanded("x^2 + 5x + 6", "x(x + 5) + 6"));
    assert!(!expanded("4x^2 - 9", "4x^2 - 9 + 0"));
    assert!(!expanded("x^2 + 5x + 6", "x^2 + 5x + 7"));
    assert!(!expanded("15x^6", "(3x^2)(5x^4)"));
    assert!(!expanded("15x^6", "3x^2*5x^4"));
    assert!(!expanded("4x^2 + 1", "(2x)^2 + 1"));
    assert!(expanded("15x^6", "15x^6"));
    assert!(expanded("x^2/2 - 3x", "x^2/2 - 3x"));
}
