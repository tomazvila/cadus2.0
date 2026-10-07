//! The `mixed_number` required form: a whole number, a separator, and a proper
//! fraction in lowest terms. Every spelling has a wrong-form row beside it.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract, format_hint};

const CONTRACT: &str = r#"{"kind":"required_form","form":"mixed_number"}"#;

fn contract() -> AnswerContract {
    serde_json::from_str(CONTRACT).unwrap()
}

fn verdict(key: &str, learner: &str) -> Option<bool> {
    match check_contract(key, learner, contract()) {
        Outcome::Decided(verdict) => Some(verdict.correct),
        Outcome::Undecidable(_) => None,
    }
}

#[test]
fn every_spelling_of_the_mixed_number_is_correct() {
    for learner in [
        "4 2/5",
        "4+2/5",
        "4 + 2/5",
        "4 and 2/5",
        "4_2/5",
        "4⅖",
        "4 ⅖",
        "$4 2/5$",
    ] {
        assert_eq!(verdict("4 2/5", learner), Some(true), "{learner}");
    }
}

#[test]
fn the_value_in_another_form_is_wrong_with_the_form_hint() {
    for learner in ["22/5", "4.4", "4 4/10", "88/20"] {
        assert_eq!(verdict("4 2/5", learner), Some(false), "{learner}");
        let hint = format_hint("4 2/5", learner, &contract()).unwrap();
        assert_eq!(
            hint,
            "Write the mixed number as a whole number, a space, then the fraction, like 4 2/5",
            "{learner}"
        );
    }
}

#[test]
fn a_different_value_is_wrong_with_no_hint() {
    for learner in ["5 2/5", "4 3/5", "23/5", "4", "4 1/5"] {
        assert_ne!(verdict("4 2/5", learner), Some(true), "{learner}");
        assert_eq!(
            format_hint("4 2/5", learner, &contract()),
            None,
            "{learner}"
        );
    }
}

#[test]
fn a_times_sign_stays_wrong_and_carries_the_notation_hint() {
    for (learner, typed) in [
        ("4 * 2/5", "4 * 2/5"),
        ("4*2/5", "4*2/5"),
        ("4 x 2/5", "4 x 2/5"),
    ] {
        assert_eq!(verdict("4 2/5", learner), Some(false), "{learner}");
        assert_eq!(
            format_hint("4 2/5", learner, &contract()).unwrap(),
            format!("{typed} means 4 times 2/5. For the mixed number 4⅖ write 4 2/5."),
            "{learner}"
        );
    }
    // A product that is not a plus reading of the key carries no hint.
    assert_eq!(format_hint("4 2/5", "3 * 2/5", &contract()), None);
}

#[test]
fn a_negative_mixed_number_and_a_second_key() {
    assert_eq!(verdict("-4 2/5", "-4 2/5"), Some(true));
    assert_eq!(verdict("-4 2/5", "-22/5"), Some(false));
    assert_eq!(verdict("1 1/2", "1½"), Some(true));
    assert_eq!(verdict("1 1/2", "3/2"), Some(false));
}

#[test]
fn the_spellings_read_as_a_mixed_number_under_the_exact_contract() {
    let exact = AnswerContract::Exact;
    for learner in ["4_2/5", "4 and 2/5", "4⅖", "4 2/5"] {
        assert!(
            matches!(check_contract("22/5", learner, exact.clone()), Outcome::Decided(v) if v.correct),
            "{learner}"
        );
    }
}
