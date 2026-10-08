//! A constant glued before a digit run is the product of the two. A glued
//! letter label such as `x2` stays ungraded.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

#[derive(Debug, PartialEq, Eq)]
enum V {
    Right,
    Wrong,
    Ungraded,
}

fn verdict(key: &str, learner: &str) -> V {
    let contract: AnswerContract = serde_json::from_str(r#"{"kind":"exact"}"#).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(v) if v.correct => V::Right,
        Outcome::Decided(_) => V::Wrong,
        Outcome::Undecidable(_) => V::Ungraded,
    }
}

#[test]
fn a_constant_glued_before_digits_is_a_product() {
    for learner in [
        "π3/4", "pi3/4", "π 3/4", "3π/4", "3*pi/4", "(3*pi)/4", "π*3/4",
    ] {
        assert_eq!(verdict("3*pi/4", learner), V::Right, "{learner}");
    }
}

#[test]
fn the_wrong_twins_stay_wrong() {
    for learner in ["π/4", "5π/4", "−13π/4", "11π/4", "π3/5"] {
        assert_eq!(verdict("3*pi/4", learner), V::Wrong, "{learner}");
    }
}

#[test]
fn a_glued_letter_label_stays_ungraded() {
    for learner in ["x2", "n1"] {
        assert_eq!(verdict("3*pi/4", learner), V::Ungraded, "{learner}");
    }
}
