//! Note 115 (owner defect): a comma-separated list key compares as a multiset
//! unless `ordered: true`; spacing, a trailing period, braces and the word
//! `and` as a separator carry no meaning. The served row of the defect carried
//! no contract, so the key takes the `Exact` path — every case here is Exact.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::Outcome;
use cadus_core::answer::contract::{AnswerContract, check_contract};

fn exact(expected: &str, learner: &str) -> bool {
    match check_contract(expected, learner, AnswerContract::Exact) {
        Outcome::Decided(verdict) => verdict.correct,
        Outcome::Undecidable(reason) => panic!("{expected:?} against {learner:?}: {reason:?}"),
    }
}

/// The defect itself: the served key ascending, the learner's answer in
/// reverse order — one multiset, so it is correct.
#[test]
fn the_reported_pair_is_one_multiset() {
    assert!(exact(
        "1, 2, 3, 4, 6, 9, 12, 18, 36",
        "36, 18, 12, 9, 6, 4, 3, 2, 1",
    ));
}

#[test]
fn the_same_order_stays_correct_and_a_wrong_member_stays_wrong() {
    assert!(exact("1, 2, 3", "1, 2, 3"));
    assert!(!exact("1, 2, 3", "1, 2, 4"));
}

/// A multiset and not a sorted set: repeats count.
#[test]
fn repeats_count() {
    assert!(exact("2, 2, 3", "3, 2, 2"));
    assert!(!exact("2, 2, 3", "2, 3, 3"));
}

#[test]
fn braces_spacing_and_trailing_period_carry_no_meaning() {
    assert!(exact("1, 2, 3", "{3, 2, 1}"));
    assert!(exact("{1, 2, 3}", "3,2,1"));
    assert!(exact("1, 2, 3", "3, 2, 1."));
    assert!(exact("1, 2, 3.", "{1, 2, 3}"));
}

#[test]
fn the_word_and_is_a_separator() {
    assert!(exact("1, 2, 3", "3, 2 and 1"));
    assert!(exact("1, 2, 3", "3, 2, and 1"));
}

#[test]
fn a_member_count_mismatch_is_wrong() {
    assert!(!exact("1, 2, 3", "1, 2"));
    assert!(!exact("1, 2", "1, 2, 2"));
}

/// Grouped thousands stay one member on both sides.
#[test]
fn grouped_thousands_stay_one_member() {
    assert!(exact("1,205", "1205"));
    assert!(exact("1,205, 6", "6, 1,205"));
    assert!(exact("1,205, 6", "1205, 6"));
    assert!(!exact("1,205, 6", "1205"));
}

/// The 1.0 readings that note 115 does not touch: a parenthesized tuple stays
/// ordered, and a bare key against a parenthesized learner keeps its verdict.
#[test]
fn ordered_shapes_keep_the_1_0_reading() {
    assert!(!exact("(1, 2)", "(2, 1)"));
    assert!(exact("(4, 17)", "(4,17)"));
    assert!(!exact("(4, 17)", "(4, 18)"));
    assert!(exact("4, 17", "(4, 17)"));
}

/// Single values and non-list shapes fall through unchanged.
#[test]
fn single_values_are_untouched() {
    assert!(exact("7,329", "7329"));
    assert!(exact("x^2 + 1", "1 + x^2"));
    assert!(!exact("x^2 + 1", "x^2 - 1"));
}
