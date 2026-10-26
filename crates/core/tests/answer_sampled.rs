//! Rung 5b of the answer check: a sampled upgrade for plain expressions (D6-expr).
//!
//! Each pair first proves that the exact forms differ, so the pair reaches the rung.

mod common;

use cadus_core::answer::{AnswerContract, Outcome, canonical_form, check_contract};
use common::answer::{E, N, check, decided};

/// Assert that the exact canonical forms of two texts differ.
#[track_caller]
fn forms_differ(expected: &str, learner: &str) {
    assert_ne!(
        canonical_form(expected),
        canonical_form(learner),
        "{expected:?} and {learner:?} must reach rung 5b"
    );
}

#[test]
fn an_identity_with_a_variable_is_correct() {
    let pairs = [
        ("sec(x)^2", "1/cos(x)^2"),
        ("sin(x)**2+cos(x)**2", "1"),
        ("(x^2 - 1)/(x - 1)", "x + 1"),
        ("-4(x + 1)/(x - 1)^3", "-4/(x - 1)**2 - 8/(x - 1)**3"),
        ("x/√(x^2 + 9)", "x*sqrt(x**2 + 9)/(x**2 + 9)"),
        ("1/√(1 - x^2)", "-sqrt(1 - x**2)/(x**2 - 1)"),
        ("x^(1/2)", "(x^2)^(1/4)"),
        ("(x^2+x)/(x+1)", "x"),
    ];
    for (expected, learner) in pairs {
        forms_differ(expected, learner);
        assert_eq!(
            check(expected, learner, E),
            decided(true, false),
            "{expected:?} / {learner:?}"
        );
    }
}

#[test]
fn an_identity_with_log_is_correct() {
    // `log` reads as base 10 in the canon and in the evaluator, so the identity holds.
    forms_differ("log(100*x)", "2 + log(x)");
    assert_eq!(check("log(100*x)", "2 + log(x)", E), decided(true, false));
}

#[test]
fn a_different_function_stays_wrong() {
    let pairs = [
        ("sec(x)^2", "1/cos(x)^3"),
        ("x^2", "x^3"),
        ("e^(x+1/2)", "e*e^x"),
        ("cos 2*x", "x*cos(2)"),
        ("1/(x+1)", "x+1"),
        // The learner has no value at 1, where the key has one.
        ("x + 1", "(x^2 - 1)/(x - 1)"),
        // The difference is above 1e-9.
        ("x^2", "x^2 + 1/10000000"),
        // The key lives on both signs, and the learner differs there.
        ("x", "(x^2)^(1/2)"),
    ];
    for (expected, learner) in pairs {
        assert_eq!(
            check(expected, learner, E),
            decided(false, false),
            "{expected:?} / {learner:?}"
        );
    }
}

#[test]
fn a_refusal_of_the_sampler_keeps_the_miss() {
    // Five variables: the sampler reads four.
    forms_differ("a+b+c+d+x", "x+d+c+b+a+1");
    assert_eq!(check("a+b+c+d+x", "x+d+c+b+a+1", E), decided(false, false));
    // Five variables, and the pair is an identity: the refusal keeps the miss.
    forms_differ("sin(a)^2+cos(a)^2+b+c+d+x", "1+b+c+d+x");
    assert_eq!(
        check("sin(a)^2+cos(a)^2+b+c+d+x", "1+b+c+d+x", E),
        decided(false, false)
    );
    // A key with fewer than six finite points on every default interval.
    assert_eq!(
        check("sqrt(0 - x^2 - 1)", "sqrt(0 - x^2 - 2)", E),
        decided(false, false)
    );
    // A learner text that does not parse is a parse refusal, as before.
    assert!(matches!(
        check("sec(x)^2", "1/cos(x)^", E),
        cadus_core::answer::Outcome::Undecidable(_)
    ));
}

#[test]
fn the_rung_skips_what_it_must_not_touch() {
    let function = AnswerContract::Function {
        vars: vec!["x".into()],
        up_to_constant: false,
        domain: Default::default(),
    };
    assert_eq!(
        check_contract("sec(x)^2", "1/cos(x)^2", function),
        decided(true, false)
    );
    assert_eq!(
        check_contract("sec(x)^2", "1/cos(x)^2", AnswerContract::Exact),
        decided(false, false)
    );
    // D6-dec: a decimal literal is never sampled.
    assert_eq!(check("x/3", "0.3333333333*x", E), decided(false, false));
    assert_eq!(check("1/3", "0.3333333333", E), decided(true, true));
    assert_eq!(check("2*x", "0.5", E), decided(false, false));
    assert_eq!(check("x/3", "0.3333333333", E), decided(false, false));
    assert_eq!(check("x^2", "x^2 + 0.0000001", E), decided(false, false));
    // The label rule: a labeled answer is not sampled.
    assert_eq!(check("x = 4", "y = 4", N), decided(false, false));
    assert_eq!(
        check("y = sec(x)^2", "y = 1/cos(x)^2", E),
        decided(false, false)
    );
    assert_eq!(
        check("sec(x)^2", "y = 1/cos(x)^2", E),
        decided(false, false)
    );
    // The kind: a numeric item is never sampled.
    assert_eq!(check("sec(x)^2", "1/cos(x)^2", N), decided(false, false));
}

#[test]
fn the_rung_is_pure_and_bounded() {
    let first = check("sec(x)^2", "1/cos(x)^2", E);
    assert_eq!(first, decided(true, false));
    for _ in 0..100 {
        assert_eq!(check("sec(x)^2", "1/cos(x)^2", E), first);
    }
    let long = vec!["x"; 400].join("+");
    let started = std::time::Instant::now();
    let outcome = check("400*x", &long, E);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "the 400-term sum took {:?}",
        started.elapsed()
    );
    let Outcome::Undecidable(undecidable) = outcome else {
        unreachable!("expected the work-bound outcome");
    };
    assert_eq!(undecidable.reason, "the answer goes past the work bound");
}
