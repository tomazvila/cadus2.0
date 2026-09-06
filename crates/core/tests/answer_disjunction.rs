//! Finite disjunctions preserve solution completeness and unknown identity (C4).

use cadus_core::answer::{AnswerContract, Outcome, canonical_form, check_contract};

#[test]
fn finite_solutions_are_unordered_exact_sets() {
    for (expected, learner) in [
        ("x = 7 or x = -7", "x = -7 or x = 7"),
        ("x = 7 or x = -7", "{-7, 7}"),
        ("2 or 3", "3 or 2 or 2"),
        ("x = sqrt(2) or x = -sqrt(2)", "x = -2^(1/2) or x = 2^(1/2)"),
        ("x = 2 or 3", "X = 3 or X = 2"),
        ("area = 12", "AREA = 24/2"),
    ] {
        assert!(
            matches!(check_contract(expected, learner, AnswerContract::Exact), Outcome::Decided(v) if v.correct),
            "{expected} vs {learner}"
        );
    }
}

#[test]
fn disjunction_wrong_values_and_wrong_unknowns_are_incorrect() {
    for learner in [
        "x = 7",
        "x = 7 or x = 8",
        "y = 7 or y = -7",
        "{-7, 7, 8}",
        "(-7, 7)",
    ] {
        assert!(
            matches!(check_contract("x = 7 or x = -7", learner, AnswerContract::Exact), Outcome::Decided(v) if !v.correct),
            "{learner}"
        );
    }
    assert!(
        matches!(check_contract("area = 12", "perimeter = 12", AnswerContract::Exact), Outcome::Decided(v) if !v.correct)
    );
}

#[test]
fn malformed_or_nonfinite_disjunctions_remain_ungraded() {
    for text in [
        "2 or",
        "or 2",
        "2 or or 3",
        "x=2 or y=3",
        "x<2 or x>3",
        "{1,2} or 3",
        "2 or 1/0",
        "x=2=3 or 4",
        "(2 or 3)",
        "yes or no",
    ] {
        assert!(canonical_form(text).is_err(), "{text}");
    }
    assert!(canonical_form(&vec!["2"; 17].join(" or ")).is_err());
    assert!(canonical_form("unreviewedword = 12").is_err());
}
