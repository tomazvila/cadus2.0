//! Bounded answer contracts used to make Foundations diagnostics gradeable.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

fn contract(json: &str) -> AnswerContract {
    serde_json::from_str(json).expect("valid contract")
}

fn correct(expected: &str, learner: &str, policy: &str) -> bool {
    matches!(
        check_contract(expected, learner, contract(policy)),
        Outcome::Decided(verdict) if verdict.correct
    )
}

#[test]
fn a_reduced_ratio_keeps_order_and_requires_lowest_terms() {
    let policy = r#"{"kind":"reduced_ratio"}"#;
    assert!(correct("2:3", "2 : 3", policy));
    assert!(!correct("2:3", "3:2", policy));
    assert!(matches!(
        check_contract("2:3", "4:6", contract(policy)),
        Outcome::Undecidable(_)
    ));
}

#[test]
fn an_ascending_chain_accepts_spacing_but_requires_strict_order() {
    let policy = r#"{"kind":"ascending_chain"}"#;
    assert!(correct("-9 < -2 < 5", "-9<-2<5", policy));
    assert!(!correct("-9 < -2 < 5", "-9 < 2 < 5", policy));
    assert!(matches!(
        check_contract("-9 < -2 < 5", "-9 < -9 < 5", contract(policy)),
        Outcome::Undecidable(_)
    ));
}

#[test]
fn polynomial_relations_compare_exact_equivalent_forms() {
    let policy = r#"{"kind":"polynomial_relation"}"#;
    assert!(correct("3x - y = 5", "6x - 2y = 10", policy));
    assert!(correct("3x - y = 5", "y - 3x = -5", policy));
    assert!(!correct("3x - y = 5", "3x - y = 4", policy));
    assert!(correct("n - 3 > 10", "n > 13", policy));
    assert!(correct("n - 3 > 10", "26 < 2n", policy));
    assert!(!correct("n - 3 > 10", "n < 13", policy));
}

#[test]
fn polynomial_relations_refuse_non_polynomial_and_ambiguous_inputs() {
    let policy = contract(r#"{"kind":"polynomial_relation"}"#);
    for learner in ["x = 1 = 2", "sin(x) = 0", "x + 1", "0 = 0"] {
        assert!(matches!(
            check_contract("x = 1", learner, policy.clone()),
            Outcome::Undecidable(_)
        ));
    }
}

#[test]
fn inequality_union_accepts_bounded_interval_notation() {
    let policy = r#"{"kind":"inequality_union"}"#;
    let reordered = check_contract("(-∞, -2) ∪ [3, ∞)", "[3, ∞) ∪ (-∞, -2)", contract(policy));
    assert!(
        matches!(reordered, Outcome::Decided(verdict) if verdict.correct),
        "{reordered:?}"
    );
    assert!(!correct("(-∞, -2) ∪ [3, ∞)", "(-∞, -2] ∪ [3, ∞)", policy));
}
