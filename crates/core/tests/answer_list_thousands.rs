//! Ordering answers written with every natural spelling of a whole-number list.
//!
//! The owner typed `89, 698, 712, 1,205` for "Order from least to greatest" and
//! was marked wrong. Each spelling below is one reading of the authored key
//! `89, 698, 712, 1205`: commas, a thousands group, semicolons, lines, a `<`
//! chain and plain spaces. A group such as `1,205` and a separator such as
//! `1, 205` never both fit one correct answer: the glued group is one number
//! only when the list then has the authored count of numbers, so no correct
//! answer has two readings.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{Outcome, check};
use cadus_core::curriculum::AnswerKind;

const KEY: &str = "89, 698, 712, 1205";

fn verdict(learner: &str) -> Option<bool> {
    match check(KEY, learner, AnswerKind::Expression) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

#[test]
fn every_spelling_of_the_ordered_whole_numbers_is_correct() {
    for learner in [
        "89, 698, 712, 1205",
        "89,698,712,1205",
        "89, 698, 712, 1,205",
        "89,698,712,1,205",
        "89; 698; 712; 1205",
        "89; 698; 712; 1,205",
        "89\n698\n712\n1205",
        "89\n698\n712\n1,205",
        "89 < 698 < 712 < 1205",
        "89 < 698 < 712 < 1,205",
        "89 698 712 1205",
        "89 698 712 1,205",
        "89, 698, 712, and 1,205",
    ] {
        assert_eq!(verdict(learner), Some(true), "{learner:?}");
    }
}

#[test]
fn a_wrong_order_is_wrong_in_every_spelling() {
    for learner in [
        "89, 698, 1,205, 712",
        "89, 698, 1205, 712",
        "89; 698; 1,205; 712",
        "1205 > 712 > 698 > 89",
        "89 712 698 1205",
        "1,205, 89, 698, 712",
    ] {
        assert_eq!(verdict(learner), Some(false), "{learner:?}");
    }
}

#[test]
fn a_group_that_does_not_restore_the_count_stays_split() {
    // `1, 205` is two numbers, so the list has five and does not match.
    assert_eq!(verdict("89, 698, 712, 1, 205"), Some(false));
    // Without a group, three digits after the comma still split on the plain reading.
    assert_eq!(
        check("1, 205", "1,205", AnswerKind::Expression),
        check("1, 205", "1, 205", AnswerKind::Expression)
    );
}

#[test]
fn an_ordered_list_contract_reads_the_same_spellings() {
    use cadus_core::answer::{AnswerContract, check_contract};
    let contract: AnswerContract =
        serde_json::from_str(r#"{"kind":"list","ordered":true,"member":{"kind":"exact"}}"#)
            .unwrap();
    let graded = |learner: &str| match check_contract(KEY, learner, contract.clone()) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    };
    for learner in [
        "89, 698, 712, 1,205",
        "89 < 698 < 712 < 1,205",
        "89; 698; 712; 1205",
    ] {
        assert_eq!(graded(learner), Some(true), "{learner:?}");
    }
    assert_eq!(graded("89, 698, 1,205, 712"), Some(false));
}
