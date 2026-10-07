//! A multipart answer written without part names reads in the key's order.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

const FACTORS_VERDICT: &str = r#"{"kind":"multipart","parts":[
    {"name":"factors","contract":{"kind":"exact"}},
    {"name":"verdict","contract":{"kind":"label","options":[["prime"],["composite"]]}}]}"#;

const KEY: &str = "factors = 6; verdict = composite";

fn correct(learner: &str) -> bool {
    let contract: AnswerContract = serde_json::from_str(FACTORS_VERDICT).unwrap();
    match check_contract(KEY, learner, contract) {
        Outcome::Decided(verdict) => verdict.correct,
        other => panic!("{learner}: {other:?}"),
    }
}

#[test]
fn unnamed_parts_in_order_are_read() {
    assert!(correct("6, composite"));
    assert!(correct("6; composite"));
    assert!(correct(" 6 ;composite "));
}

#[test]
fn named_parts_still_read_in_any_order() {
    assert!(correct("factors = 6; verdict = composite"));
    assert!(correct("verdict = composite; factors = 6"));
}

#[test]
fn unnamed_parts_in_another_order_or_with_a_free_word_are_right() {
    assert!(correct("composite, 6"));
    assert!(correct("factors = 6, composite"));
}

#[test]
fn unnamed_parts_that_are_wrong_or_misshapen_are_wrong() {
    assert!(!correct("6, prime"));
    assert!(!correct("5, composite"));
    assert!(!correct("6"));
    assert!(!correct("6, composite, 12"));
    assert!(!correct("2 x 6, 3 x 4, 1 x 12"));
}

#[test]
fn commas_inside_brackets_do_not_split_parts() {
    let contract: AnswerContract = serde_json::from_str(
        r#"{"kind":"multipart","parts":[
            {"name":"point","contract":{"kind":"coordinates","arity":2}},
            {"name":"d","contract":{"kind":"exact"}}]}"#,
    )
    .unwrap();
    let outcome = check_contract("point = (1, 2); d = 5", "(1, 2), 5", contract);
    let Outcome::Decided(verdict) = outcome else {
        panic!("{outcome:?}");
    };
    assert!(verdict.correct);
}

fn decided(contract: &str, key: &str, learner: &str) -> bool {
    let contract: AnswerContract = serde_json::from_str(contract).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => verdict.correct,
        other => panic!("{learner}: {other:?}"),
    }
}

const TWO_EXACT: &str = r#"{"kind":"multipart","parts":[
    {"name":"a","contract":{"kind":"exact"}},
    {"name":"b","contract":{"kind":"exact"}}]}"#;

#[test]
fn a_comma_that_may_group_thousands_does_not_separate_parts() {
    assert!(!decided(TWO_EXACT, "a = 1; b = 0", "1,000"));
    assert!(decided(TWO_EXACT, "a = 1; b = 0", "1, 0"));
    assert!(decided(TWO_EXACT, "a = 1; b = 100", "1, 100"));
    assert!(decided(TWO_EXACT, "a = 1000; b = 5", "1,000; 5"));
}

#[test]
fn an_interval_does_not_split_on_its_comma() {
    assert!(!decided(FACTORS_VERDICT, KEY, "(1, 2], composite"));
}

const LIST_VERDICT: &str = r#"{"kind":"multipart","parts":[
    {"name":"factors","contract":{"kind":"list","ordered":false,"member":{"kind":"exact"}}},
    {"name":"verdict","contract":{"kind":"label","options":[["composite"],["prime"]]}}]}"#;

const LIST_KEY: &str = "factors = 1, 2, 4, 8, 16; verdict = composite";

#[test]
fn surplus_commas_belong_to_the_one_list_part() {
    assert!(decided(LIST_VERDICT, LIST_KEY, "1, 2, 4, 8, 16, composite"));
    assert!(decided(LIST_VERDICT, LIST_KEY, "16, 8, 4, 2, 1, composite"));
    assert!(decided(LIST_VERDICT, LIST_KEY, "1, 2, 4, 8, 16; composite"));
    assert!(!decided(LIST_VERDICT, LIST_KEY, "1, 2, 4, 16, composite"));
    assert!(!decided(LIST_VERDICT, LIST_KEY, "1, 2, 4, 8, 16, prime"));
    assert!(!decided(
        LIST_VERDICT,
        LIST_KEY,
        "composite, 1, 2, 4, 8, 16"
    ));
}
