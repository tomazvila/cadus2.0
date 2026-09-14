//! Ordering answers that repeat the problem's own thousands notation (ISSUE-3).
//!
//! The problem displays `$1{,}205$` and the learner answers with the same
//! notation, `89, 698, 712, 1,205`. The comma is the list separator and the
//! thousands separator at once, so the plain grammar refuses the whole answer
//! and the attempt loops on "not marked". These tests pin the fold the parser
//! and the ordered-list contract now take: a comma with no space and three
//! digits behind it is a thousands group inside the member, while a spaced comma
//! stays a list separator.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, canonical_form, check, check_contract};
use cadus_core::curriculum::AnswerKind;

fn is_correct(outcome: Outcome, correct: bool) {
    assert!(
        matches!(&outcome, Outcome::Decided(verdict) if verdict.correct == correct),
        "expected correct = {correct}, got {outcome:?}"
    );
}

#[test]
fn a_bare_ordering_list_folds_a_glued_thousands_group() {
    // The grouped member and the plain member are one value.
    assert_eq!(
        canonical_form("89, 698, 712, 1,205").unwrap(),
        canonical_form("89, 698, 712, 1205").unwrap()
    );
    // The checker grades the display notation on the item's answer kind.
    is_correct(
        check(
            "89, 698, 712, 1205",
            "89, 698, 712, 1,205",
            AnswerKind::Expression,
        ),
        true,
    );
    // A different order is still wrong.
    is_correct(
        check(
            "89, 698, 712, 1205",
            "1,205, 89, 698, 712",
            AnswerKind::Expression,
        ),
        false,
    );
    // Three groups in one list fold too.
    is_correct(
        check(
            "5136, 5316, 5361",
            "5,136, 5,316, 5,361",
            AnswerKind::Expression,
        ),
        true,
    );
}

#[test]
fn a_spaced_comma_stays_a_list_separator() {
    // `1, 205` carries a space after the comma, so it stays the two members
    // 1 and 205 and does not fold into the grouped number 1205.
    assert_ne!(
        canonical_form("1, 205").unwrap(),
        canonical_form("1,205").unwrap()
    );
    is_correct(check("1, 205", "1, 205", AnswerKind::Expression), true);
    is_correct(check("1205", "1, 205", AnswerKind::Expression), false);
}

#[test]
fn the_ordered_list_contract_reads_the_grouped_answer() {
    let ordered = AnswerContract::List {
        ordered: true,
        member: Box::new(AnswerContract::Exact),
    };
    is_correct(
        check_contract("89, 698, 712, 1205", "89, 698, 712, 1,205", ordered.clone()),
        true,
    );
    is_correct(
        check_contract("5136, 5316, 5361", "5,136, 5,316, 5,361", ordered.clone()),
        true,
    );
    is_correct(
        check_contract("89, 698, 712, 1205", "89, 698, 712, 1205", ordered.clone()),
        true,
    );
    is_correct(
        check_contract("89, 698, 712, 1205", "89, 698, 1205, 712", ordered),
        false,
    );
}

#[test]
fn a_thousands_group_inside_a_longer_expression_stays_undecidable() {
    // The group is a whole-answer value or a list member and nothing else, so
    // `1,500%` and `3 + 1,500` keep the existing refusal and no operator gains a
    // second reading.
    assert!(matches!(
        check("15", "1,500%", AnswerKind::Numeric),
        Outcome::Undecidable(_)
    ));
    assert!(matches!(
        check("(4, 500)", "3 + 1,500", AnswerKind::Expression),
        Outcome::Undecidable(_)
    ));
}
