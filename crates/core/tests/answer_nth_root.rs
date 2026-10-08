//! An indexed root is read in every natural spelling, and the radical-form
//! contract marks only a radical as correct.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

fn radical(learner: &str) -> Option<bool> {
    let contract: AnswerContract =
        serde_json::from_str(r#"{"kind":"required_form","form":"radical"}"#).unwrap();
    match check_contract("root(5, y)", learner, contract) {
        Outcome::Decided(v) => Some(v.correct),
        Outcome::Undecidable(_) => None,
    }
}

fn exact(learner: &str) -> Option<bool> {
    let contract: AnswerContract = serde_json::from_str(r#"{"kind":"exact"}"#).unwrap();
    match check_contract("root(5, y)", learner, contract) {
        Outcome::Decided(v) => Some(v.correct),
        Outcome::Undecidable(_) => None,
    }
}

#[test]
fn every_spelling_of_the_fifth_root_is_right_under_the_radical_form() {
    for learner in [
        "root(5,y)",
        "root(5, y)",
        "nthroot(5,y)",
        "\\sqrt[5]{y}",
        "⁵√y",
    ] {
        assert_eq!(radical(learner), Some(true), "{learner}");
    }
}

#[test]
fn the_wrong_answers_are_wrong_under_the_radical_form() {
    for learner in [
        "sqrt(y)",
        "root(3,y)",
        "y^5",
        "1/y^5",
        "y^(1/5)",
        "y^(1/3)",
        "5√y",
    ] {
        assert_ne!(radical(learner), Some(true), "{learner}");
    }
}

#[test]
fn the_power_spelling_is_right_when_the_contract_is_plain_exact() {
    for learner in ["y^(1/5)", "root(5,y)", "nthroot(5,y)", "\\sqrt[5]{y}"] {
        assert_eq!(exact(learner), Some(true), "{learner}");
    }
    for learner in ["sqrt(y)", "root(3,y)", "y^5", "1/y^5"] {
        assert_ne!(exact(learner), Some(true), "{learner}");
    }
}
