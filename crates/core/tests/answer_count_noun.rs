//! A counted answer to a "how many" question grades its number; units,
//! variables and grammar words are never swallowed.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract, count_answer};

const PACKS: &str =
    "A collection of $2^9$ stamps is split into packs of $2^6$ stamps. How many packs are there?";

fn graded(problem: &str, key: &str, learner: &str) -> Option<bool> {
    let number = count_answer(problem, key, learner)?;
    match check_contract(key, &number, AnswerContract::Exact) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(reason) => panic!("{learner:?}: {reason:?}"),
    }
}

#[test]
fn a_counted_noun_grades_its_number() {
    assert_eq!(graded(PACKS, "8", "8 packs"), Some(true));
    assert_eq!(graded(PACKS, "8", "8 packs."), Some(true));
    assert_eq!(graded(PACKS, "2^3", "8 packs"), Some(true));
    assert_eq!(graded(PACKS, "8", "2^3 packs"), Some(true));
    assert_eq!(graded(PACKS, "8", "8 small packs"), Some(true));
    assert_eq!(
        graded("How many sections are there?", "12", "12 sections"),
        Some(true)
    );
    assert_eq!(
        graded("How many times larger is it?", "80", "80 times"),
        Some(true)
    );
    assert_eq!(count_answer(PACKS, "8", "8 times 3"), None);
    assert_eq!(graded(PACKS, "8", "9 packs"), Some(false));
}

#[test]
fn units_variables_and_grammar_words_are_not_swallowed() {
    for learner in [
        "8 cm",
        "8 m",
        "8 kg",
        "8 seconds",
        "8 euros",
        "8 x",
        "8 xy",
        "8 ab",
        "8 or 9",
        "8 and 9",
        "8 pi",
        "8 sin",
        "8",
        "packs",
        "8 packs of 9",
        "8 packs and 2 boxes",
        "8 a b c d",
        "8 -packs",
    ] {
        assert_eq!(count_answer(PACKS, "8", learner), None, "{learner}");
    }
}

#[test]
fn only_a_how_many_question_with_an_integer_key_reads_a_count() {
    assert_eq!(count_answer("Simplify $x^8/x^3$.", "8", "8 packs"), None);
    assert_eq!(count_answer("What is the length?", "8", "8 packs"), None);
    assert_eq!(count_answer(PACKS, "x^5", "8 packs"), None);
    assert_eq!(count_answer(PACKS, "5/2", "5/2 packs"), None);
    assert_eq!(count_answer(PACKS, "8 cm", "8 packs"), None);
}
