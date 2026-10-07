//! A multipart answer grades in every natural spelling: any leading word, any
//! separator, any order, and a count noun; wrong values stay wrong.

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};

fn grade(key: &str, contract: &str, learner: &str) -> bool {
    let contract: AnswerContract = serde_json::from_str(contract).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) => verdict.correct,
        Outcome::Undecidable(reason) => panic!("{learner:?}: {reason:?}"),
    }
}

const FACTORS: &str = r#"{"kind":"multipart","parts":[{"name":"count","contract":{"kind":"exact"}},{"name":"type","contract":{"kind":"label","options":[["prime","it is prime","prime number","a prime"],["composite","it is composite","composite number","a composite"]]}}]}"#;
const KEY: &str = "count = 3; type = composite";

#[test]
fn natural_spellings_of_the_factor_count_grade_correct() {
    for learner in [
        "factors = 3, composite",
        "factors = 3; composite",
        "3 composite",
        "composite, 3 factors",
        "composite, 3",
        "composite 3",
        "3 factors, composite",
        "3 and composite",
        "factors: 3, type: composite",
        "3, composite",
        "3; composite",
        "count = 3; type = composite",
        "type = composite, count = 3",
    ] {
        assert!(grade(KEY, FACTORS, learner), "{learner}");
    }
}

#[test]
fn wrong_factor_count_answers_stay_wrong() {
    for learner in [
        "4, composite",
        "3, prime",
        "3",
        "composite",
        "3, composite, 5",
        "factors = 4, composite",
        "4 factors, composite",
        "composite, 4",
    ] {
        assert!(!grade(KEY, FACTORS, learner), "{learner}");
    }
}

const CIRCLE: &str = r#"{"kind":"multipart","parts":[{"name":"center","contract":{"kind":"coordinates","arity":2}},{"name":"radius","contract":{"kind":"exact"}}]}"#;
const CIRCLE_KEY: &str = "center = (-4, 1); radius = 5";

#[test]
fn a_point_and_a_number_grade_in_natural_spellings() {
    for learner in [
        "(-4, 1), 5",
        "(-4, 1); 5",
        "5, (-4, 1)",
        "c = (-4, 1), r = 5",
        "centre = (-4, 1) and radius = 5",
        "(-4, 1) 5",
        "5 (-4, 1)",
    ] {
        assert!(grade(CIRCLE_KEY, CIRCLE, learner), "{learner}");
    }
    for learner in ["(-4, 1), 4", "(1, -4), 5", "(-4, 1)", "(-4, 1), 5, 2"] {
        assert!(!grade(CIRCLE_KEY, CIRCLE, learner), "{learner}");
    }
}

const PAIR: &str = r#"{"kind":"multipart","parts":[{"name":"x","contract":{"kind":"exact"}},{"name":"y","contract":{"kind":"exact"}}]}"#;

#[test]
fn two_numbers_keep_the_authored_order() {
    assert!(grade("x = 3; y = 5", PAIR, "3, 5"));
    assert!(grade("x = 3; y = 5", PAIR, "a = 3, b = 5"));
    assert!(!grade("x = 3; y = 5", PAIR, "5, 3"));
}
