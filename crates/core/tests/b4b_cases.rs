//! The repair cases for lane B4b (pack v11, D42 to D47).
//!
//! Copied from `FLOW/state/reviews/B4b-cases.rs` at 54464e2f. Each row pins the
//! result that the repaired rules give. A row with the mark `DEFECT` had a
//! wrong result at 54464e2f; the repair changed it to the result that the
//! review states. Rows whose result the D45 default domain changes carry a
//! `REPAIRED-DOMAIN` note.
//! Run: `cbuild cargo test -p cadus-core --test b4b_cases`.
//! The bar notation `|x|` is from lane B3, which is not in `flow/B4b`; the rows
//! use `abs(x)`.

#![allow(clippy::unwrap_used, clippy::panic, clippy::too_many_lines)]

use cadus_core::answer::contract::function::{FunctionSpec, eval};
use cadus_core::answer::{AnswerContract, Outcome, check_contract, normalize, parse};
use std::collections::BTreeMap;

const X: &str = r#"{"kind":"function","vars":["x"]}"#;
const XC: &str = r#"{"kind":"function","vars":["x"],"up_to_constant":true}"#;
const X_AND_C: &str = r#"{"kind":"function","vars":["x","C"]}"#;
const XY: &str = r#"{"kind":"function","vars":["x","y"]}"#;
const XYZ: &str = r#"{"kind":"function","vars":["x","y","z"]}"#;
const NEGATIVE: &str = r#"{"kind":"function","vars":["x"],"domain":{"x":["-3","-1"]}}"#;
const SYMMETRIC: &str = r#"{"kind":"function","vars":["x"],"domain":{"x":["-1","1"]}}"#;
const ODD_INTEGERS: &str = r#"{"kind":"function","vars":["x"],"domain":{"x":["0","16"]}}"#;
const HALVES: &str = r#"{"kind":"function","vars":["x"],"domain":{"x":["0","8"]}}"#;

#[derive(Debug, PartialEq, Clone, Copy)]
enum Seen {
    Correct,
    Wrong,
    Ungraded,
}
use Seen::{Correct, Ungraded, Wrong};

fn verdict(doc: &str, key: &str, learner: &str) -> Seen {
    let contract: AnswerContract = serde_json::from_str(doc).unwrap();
    match check_contract(key, learner, contract) {
        Outcome::Decided(verdict) if verdict.correct => Correct,
        Outcome::Decided(_) => Wrong,
        Outcome::Undecidable(_) => Ungraded,
    }
}

fn run(rows: &[(&str, &str, &str, Seen)]) {
    for (doc, key, learner, seen) in rows {
        assert_eq!(
            verdict(doc, key, learner),
            *seen,
            "{doc} | {key} | {learner}"
        );
    }
}

/// DEFECT 1 (critical): with `up_to_constant`, a learner text with a constant of
/// 1e17 or more is CORRECT for each key. `key - learner` is one `f64` value at
/// each point, because the key value is below the last bit of the constant.
#[test]
fn defect_a_large_constant_is_correct_for_each_antiderivative() {
    run(&[
        (XC, "x^2/2 + C", "99999999999999999999", Wrong), // DEFECT 1, D42: wrong
        (XC, "x^2/2 + C", "10^17", Wrong),                // DEFECT
        (XC, "x^2/2 + C", "-10^30", Wrong),               // DEFECT
        (XC, "x^2/2 + C", "e^40", Wrong),                 // DEFECT
        (XC, "x^2/2 + C", "x + 10^20", Wrong),            // DEFECT
        (XC, "x^2/2 + C", "10^20 + sin(x)", Wrong),       // DEFECT
        (XC, "sin(x) + C", "10^17", Wrong),               // DEFECT
        (XC, "ln(x) + C", "10^17", Wrong),                // DEFECT
        (XC, "x e^x - e^x + C", "10^18", Wrong),          // DEFECT
        (XC, "e^(3x)/3 + C", "10^20", Wrong),             // DEFECT
        (XC, "x^2/2 + C", "10^16", Wrong),
        (X, "x^2/2", "10^20", Wrong),
    ]);
}

/// DEFECT 2 (high): rule (5) uses one scale, `max|key|` of all points. A key
/// that grows to 1e9 or more hides a term that is absent.
#[test]
fn defect_a_large_key_hides_an_absent_term_with_up_to_constant() {
    run(&[
        (XC, "e^(10x)/10 + x + C", "e^(10x)/10 + C", Wrong), // DEFECT 2, D42: the term x is absent
        (XC, "e^(10x)/10 + C", "e^(10x)/10 + x", Wrong),     // DEFECT
        (XC, "e^(8x)/8 + x^2/2 + C", "e^(8x)/8 + C", Wrong),
        (XC, "e^(5x)/5 + x + C", "e^(5x)/5 + C", Wrong),
        (XC, "x^20/20 + x + C", "x^20/20 + C", Wrong),
        (X, "e^(10x)", "e^(10x) + x", Wrong), // rule (4) is relative at each point: no defect
    ]);
}

/// The repair that REV proposes for DEFECT 1 and 2: compare the change of the
/// two formulas from the first point, with the tolerance of each point.
fn variation_rule(spec: &FunctionSpec, key: &str, learner: &str) -> bool {
    let key = parse(&normalize(key).source).unwrap();
    let learner = parse(&normalize(learner).source).unwrap();
    [0.0, 1.0].iter().all(|constant| {
        let mut pairs = Vec::new();
        for mut env in spec.sample_points() {
            for name in ["C", "c", "K", "k"] {
                env.insert(name.to_string(), *constant);
            }
            if let Some(a) = eval(&key, &env) {
                match eval(&learner, &env) {
                    Some(b) => pairs.push((a, b)),
                    None => return false,
                }
            }
        }
        let (a0, b0) = pairs[0];
        pairs
            .iter()
            .all(|(a, b)| ((a - a0) - (b - b0)).abs() <= 1e-9 * a.abs().max(a0.abs()).max(1.0))
    })
}

#[test]
fn the_proposed_variation_rule_repairs_defect_1_and_2() {
    let spec = FunctionSpec::new(&["x".to_string()], true, &BTreeMap::new()).unwrap();
    for (key, learner, correct) in [
        ("x^2/2 + C", "10^17", false),
        ("x^2/2 + C", "x + 10^20", false),
        ("sin(x) + C", "e^40", false),
        ("e^(10x)/10 + x + C", "e^(10x)/10 + C", false),
        ("e^(10x)/10 + C", "e^(10x)/10 + x", false),
        ("x^2/2 + C", "x^2/2 + C*x", false),
        ("x^2/2 + C", "x^2/2 + x", false),
        ("x^2/2 + C", "x^2/2", true),
        ("x^2/2 + C", "x^2/2 + 7", true),
        ("x^2/2 + C", "x^2/2 - C", true),
        ("x^2/2 + C", "x^2/2 + 2C", true),
        ("sin(x)^2/2 + C", "-cos(2x)/4 + C", true),
        ("ln(x) + C", "ln(2x) + c", true),
        ("e^(10x)/10 + C", "(e^(5x))^2/10 + 3", true),
    ] {
        assert_eq!(
            variation_rule(&spec, key, learner),
            correct,
            "{key} | {learner}"
        );
    }
}

/// DEFECT 3 (high, authoring): `up_to_constant` on a key with a constant that is
/// not one additive term accepts a wrong general solution. 22 shipped keys of
/// differential-equations have this form (`y = 2 + Ce^{-3x}`).
#[test]
fn defect_up_to_constant_on_a_multiplicative_constant() {
    run(&[
        (XC, "2 + C e^(-3x)", "7 + C e^(-3x)", Ungraded), // DEFECT 3, D44: the key is refused
        (XC, "C e^(2x)", "C e^(2x) + 5", Ungraded),       // DEFECT
        (XC, "2 + C e^(-3x)", "2", Ungraded),             // the same refusal (D44)
        (XC, "C e^(2x)", "3 e^(2x)", Ungraded),           // the same refusal (D44)
        // The form that works: the constant is a variable of the contract.
        (X_AND_C, "2 + C e^(-3x)", "7 + C e^(-3x)", Wrong),
        (X_AND_C, "2 + C e^(-3x)", "2 + C/e^(3x)", Correct),
        (X_AND_C, "2 + C e^(-3x)", "2 + e^(-3x)", Wrong),
        (X_AND_C, "y = x ln(x) + C x", "x (ln(x) + C)", Correct),
        (X_AND_C, "2 + C e^(-3x)", "2 + K e^(-3x)", Wrong), // a different letter: a known cost
    ]);
}

/// DEFECT 4 (medium, authoring): the default domain is positive, and below 3.
/// A sign error is CORRECT there. A domain with negative values repairs it.
#[test]
fn defect_the_default_domain_hides_a_sign_error() {
    run(&[
        (X, "abs(x - 3)", "3 - x", Ungraded), // DEFECT 4, D45: the key is refused
        (X, "sqrt((x-3)^2)", "3 - x", Correct), // D45 refuses abs nodes only; on [-3, 3] this is |x-3| = 3-x
        (X, "x", "abs(x)", Wrong),              // DEFECT: the symmetric default of D45
        (X, "abs(x)", "x", Wrong),              // DEFECT
        (X, "x", "sqrt(x^2)", Wrong),           // DEFECT
        (X, "x/abs(x)", "1", Wrong),            // DEFECT
        (X, "x abs(x)", "x^2", Wrong),          // DEFECT
        (X, "x^(1/3)", "abs(x)^(1/3)", Wrong),  // DEFECT
        (X, "sqrt(x^2 + 2x + 1)", "x + 1", Wrong), // DEFECT
        (X, "ln(abs(x))", "ln(x)", Wrong), // REPAIRED-DOMAIN: ln(x) is not finite at the negative points
        (XC, "ln(abs(x)) + C", "ln(x) + C", Wrong), // REPAIRED-DOMAIN: the same
        (X, "abs(x - 1)", "x - 1", Wrong), // the sign change is in the domain
        (X, "sqrt(x^2 - 2x + 1)", "x - 1", Wrong),
        (X, "x", "atan(tan(x))", Wrong),
        (X, "x", "asin(sin(x))", Wrong),
        (XC, "ln(abs(sec(x))) + C", "ln(sec(x)) + C", Wrong),
        (XC, "ln(abs(sec(x))) + C", "-ln(abs(cos(x))) + C", Correct),
        (NEGATIVE, "x", "abs(x)", Wrong),
        (NEGATIVE, "abs(x)", "-x", Correct),
        (NEGATIVE, "ln(abs(x))", "ln(x)", Wrong),
        (NEGATIVE, "ln(abs(x))", "ln(-x)", Correct),
        (SYMMETRIC, "x", "abs(x)", Wrong),
        (SYMMETRIC, "sqrt(x^2)", "x", Wrong),
        (SYMMETRIC, "sqrt(x^2)", "abs(x)", Correct),
        (SYMMETRIC, "x^(1/3)", "abs(x)^(1/3)", Wrong),
        (SYMMETRIC, "x^3", "abs(x)^3", Wrong),
        (SYMMETRIC, "x^(2/3)", "abs(x)^(2/3)", Correct),
        (SYMMETRIC, "ln(abs(x))", "ln(x)", Wrong),
    ]);
}

/// DEFECT 5 (medium, authoring): an author domain with a width of 1, 2, 4, 8 or
/// 16 puts each sample point on a dyadic grid where `sin` and `cos` of a
/// multiple of `pi x` are 0 or -1. The default domain aliases at `16 pi x` only.
#[test]
fn defect_a_dyadic_author_domain_aliases_pi_keys() {
    run(&[
        (ODD_INTEGERS, "cos(pi x)", "-1", Ungraded), // DEFECT 5, D46: the domain is refused
        (ODD_INTEGERS, "sin(pi x)", "0", Ungraded),  // DEFECT
        (ODD_INTEGERS, "x + sin(pi x)", "x", Ungraded), // DEFECT
        (ODD_INTEGERS, "cos(pi x/2)", "0", Ungraded), // DEFECT
        (HALVES, "cos(pi x)", "0", Ungraded),        // DEFECT
        (HALVES, "sin(2 pi x)", "0", Ungraded),      // DEFECT
        (ODD_INTEGERS, "x + 1", "(x^2-1)/(x-1)", Ungraded), // REPAIRED-DOMAIN: the D46 refusal covers each key
        (X, "x + 1", "(x^2-1)/(x-1)", Correct),
        (X, "sin(pi x)", "0", Wrong),
        (X, "cos(pi x)", "-1", Wrong),
        (X, "sin(2 pi x)", "0", Wrong),
        (X, "cos(8 pi x)", "0", Wrong),
        (X, "cos(16 pi x)", "0", Wrong), // REPAIRED-DOMAIN: 16x is 6 times an odd number
        (X, "sin(x)", "sin(x) + sin(32pi x)", Correct), // no natural answer
        (X, "sin(x)", "sin(x + 2pi)", Correct), // right
        (X, "sin(x)", "sin(x + pi)", Wrong),
    ]);
}

/// The tolerance. A difference below 1e-9 (absolute below 1, relative above 1)
/// is CORRECT by design; REV found no natural wrong answer there.
#[test]
fn tolerance_edges() {
    run(&[
        (X, "sin(x)", "sin(x) + x/10^10", Correct),
        (X, "sin(x)", "sin(x) + x/10^9", Wrong),
        (X, "x^2", "x^2 (1 + 10^(-9)/2)", Correct),
        (X, "x^2", "x^2 (1 + 2*10^(-9))", Wrong),
        (X, "10^6 x", "10^6 x + 1/10000", Correct),
        (X, "10^6 x", "10^6 x + 1/1000", Wrong),
        (X, "0.3679", "e^(-1)", Wrong),
        (X, "pi x", "3.14159265 x", Wrong),
        (X, "pi x", "3.1415926535 x", Correct),
        (X, "x/3", "0.333333333 x", Correct), // 9 digits are sufficient
        (X, "10^(-10) x", "0", Correct),      // a key below 1e-9 is equal to 0: authoring rule
        (XY, "x^2 y", "x^2 y + (x - y)/10^10", Correct),
    ]);
}

/// The F8 list and cancellation: no false WRONG.
#[test]
fn equal_forms_are_correct() {
    run(&[
        (X, "sec(x)^2", "1/cos(x)^2", Correct),
        (X, "sec(x)^2", "1 + tan(x)^2", Correct),
        (X, "sin(2x)", "2 sin(x) cos(x)", Correct),
        (X, "tan(x)", "sin(x)/cos(x)", Correct),
        (X, "(x+2)/(x+1)^2", "1/(x+1) + 1/(x+1)^2", Correct),
        (X, "ln(x^2)", "2 ln(x)", Wrong), // REPAIRED-DOMAIN: 2 ln(x) is not finite at the negative points
        (X, "ln(x/(x+1))", "ln(x) - ln(x+1)", Wrong), // REPAIRED-DOMAIN: ln(x) is not finite at the negative points
        (X, "e^(2x)", "(e^x)^2", Correct),
        (X, "e^(2x)", "exp(2x)", Correct),
        (X, "e^(10x)", "(e^(5x))^2", Correct),
        (X, "e^(300x)", "(e^(150x))^2", Correct), // the key is finite at 7 points (1e292 at the last)
        (X, "e^(10x)", "e^(10x) + 1", Wrong),
        (X, "(1 - cos(x))/x^2", "2 sin(x/2)^2/x^2", Correct),
        (
            X,
            "(x-1)^10",
            "x^10 - 10x^9 + 45x^8 - 120x^7 + 210x^6 - 252x^5 + 210x^4 - 120x^3 + 45x^2 - 10x + 1",
            Correct,
        ),
        (X, "e^(30x) + x - e^(30x)", "x", Correct),
        (X, "cosh(10x)^2 - sinh(10x)^2", "1", Wrong), // the KEY loses its digits: authoring rule
        (X, "1", "x/x", Correct),
        (X, "1/(x - 33/32)", "32/(32x - 33)", Correct), // a pole on a sample point: 7 points
        (X, "sqrt(x - 1)", "(x-1)^(1/2)", Correct),     // 6 finite points
        (X, "sqrt(x - 1)", "sqrt(abs(x - 1))", Correct), // the 2 other points do not count
        (X, "asin(x/2)", "pi/2 - acos(x/2)", Correct),
        (X, "x^(3/2)", "x sqrt(x)", Correct),
        (X, "1/sqrt(x)", "sqrt(x)/x", Correct),
        (
            X,
            "x + 1",
            "(x^2 - (33/32)^2)/(x - 33/32) + 1 - 33/32",
            Correct,
        ), // REPAIRED-DOMAIN: the pole is off the grid
        (XC, "x^2/2 + C", "x^2/2 + k", Correct),
        (XC, "x^2/2 + C", "x^2/2", Correct),
        (XC, "x^2/2 + C", "x^2/2 - 1/2", Correct),
        (XC, "x^2/2 + C", "C*x^2/2 + (1-C)*x^2/2", Correct),
        (XC, "x^2/2 + C", "x^2/2 + C - C", Correct),
        (XC, "sin(x)^2/2 + C", "-cos(x)^2/2 + C", Correct),
        (XC, "atan(x) + C", "-atan(1/x) + C", Wrong), // REPAIRED-DOMAIN: the constant jumps by pi at 0
        (XC, "x^2/2 + C", "x^2/2 + C*x", Wrong),
        (XC, "x^2/2 + C", "x^2/2 + C*K*x", Wrong),
        (XC, "x^2/2 + C", "x^2/2 + D", Wrong), // a different letter
        (XC, "x^2/2 + C", "x^2/2 + C1", Ungraded),
        (XC, "x^2/2 + C", "x^2/2 + 1/C", Wrong), // not finite with the constant at 0
        (XC, "x^2/2 + C", "x^2/2 + C(C-1)x", Correct), // low: 0 at the two constant values
        (XC, "x^2/2 + C", "x^2/2 + (C-K)*x", Correct), // low: each name has one value
        (XC, "ln(abs(x)) + C", "ln(abs(C x))", Wrong), // the pinned limit of D34
        (XC, "ln(abs(x)) + C", "ln(abs(2x)) + C", Correct),
    ]);
}

#[test]
fn a_variable_swap_is_wrong() {
    run(&[
        (XY, "x*y^2", "x^2*y", Wrong),
        (XY, "x*y^2", "y^2 x", Correct),
        (XY, "x - y", "y - x", Wrong),
        (XY, "x/y", "y/x", Wrong),
        (XY, "x e^y", "y e^x", Wrong),
        (XY, "2x", "2y", Wrong),
        (XY, "2xy", "2x", Wrong),
        (XYZ, "x*y*z", "z*y*x", Correct),
        (XYZ, "x + 2y + 3z", "3x + 2y + z", Wrong),
        (XYZ, "x + 2y + 3z", "x + 3y + 2z", Wrong),
        (XYZ, "x + 2y + 3z", "2x + y + 3z", Wrong),
        (XYZ, "x + 2y + 3z", "2x + 3y + z", Wrong),
        (XYZ, "x + 2y + 3z", "3x + y + 2z", Wrong),
        (XYZ, "e^(x y) sin(z)", "sin(y) e^(z x)", Wrong),
        (
            r#"{"kind":"function","vars":["r","theta"]}"#,
            "r cos(theta)",
            "theta cos(r)",
            Wrong,
        ),
    ]);
}

/// DEFECT 6 (medium): the frozen mutant `2*(E) + v` is equal to the key for
/// `E = -v` (and for `E = a - v` with `up_to_constant`). `check_keys` then
/// reports a legal key as a failure.
#[test]
fn defect_the_mutant_of_minus_v_is_correct() {
    run(&[
        (X, "-x", "2*(-x) + x", Correct),                // DEFECT
        (XC, "-x + C", "2*(-x) + x", Correct),           // DEFECT
        (XC, "y = 5 - x + C", "2*(5 - x) + x", Correct), // DEFECT
        (X, "0", "2*(0) + x", Wrong),
        (XC, "C", "2*(C) + x", Wrong),
        (X, "x", "2*(x) + x", Wrong),
        (X, "7", "2*(7) + x", Wrong),
        (XC, "7", "2*(7) + x", Wrong),
        (XY, "-x + y", "2*(-x + y) + x", Wrong),
        (XY, "y", "2*(y) + x", Wrong),
    ]);
}

#[test]
fn refusals_at_authoring_time() {
    let refused = |doc: &str, key: &str| {
        let contract: AnswerContract = serde_json::from_str(doc).unwrap();
        contract.validate_expected(key).is_err()
    };
    for key in [
        "log(x)",
        "Log(x) + 1",
        "1e-5 x",
        "2E3",
        "ln(x - 5)",
        "sqrt(1 - x)",
        "asin(x)",
        "(1, 2)",
        "x > 2",
        "x + y",
        "x + C",
        "",
        "2^x",
    ] {
        assert!(refused(X, key), "{key}");
    }
    for key in [
        "ln(x)",
        "2e^(3x)",
        "2e^(-3x)",
        "3e^2 x",
        "3e - 2x",
        "x^2e^x",
        "5 + 2e + 3x",
        "ln(x - 1)",
        "sqrt(x - 1)",
        "1/(x - 33/32)",
        "y = x e^(-x)",
        "asin(x/3)",
    ] {
        assert!(!refused(X, key), "{key}");
    }
    // Low: a legal key with no space is refused by the D28 text rule.
    assert!(refused(X, "3e-2x"));
    assert!(refused(X, "5 + 2e+3x"));
    // Low: the reason is "fewer than six finite sample points"; the cause is the constant at 0.
    assert!(refused(XC, "ln(abs(C x))"));
    for doc in [
        r#"{"kind":"function","vars":["x"],"domain":{"x":["-1/16","15/16"]}}"#,
        r#"{"kind":"function","vars":["x"],"domain":{"x":["1","1"]}}"#,
        r#"{"kind":"function","vars":["x"],"domain":{"x":["1","sqrt(2)"]}}"#,
        r#"{"kind":"function","vars":["x"],"domain":{"x":["1","10^400"]}}"#,
        r#"{"kind":"function","vars":["x"],"domain":{"y":["1","2"]}}"#,
        r#"{"kind":"function","vars":["x"],"points":8}"#,
        r#"{"kind":"function","vars":[]}"#,
        r#"{"kind":"function","vars":["x","x"]}"#,
        r#"{"kind":"function","vars":["e"]}"#,
        r#"{"kind":"function","vars":["xy"]}"#,
        r#"{"kind":"function","vars":["x","y","z","w"]}"#,
    ] {
        assert!(
            serde_json::from_str::<AnswerContract>(doc).is_err(),
            "{doc}"
        );
    }
    for doc in [
        NEGATIVE,
        SYMMETRIC,
        ODD_INTEGERS,
        r#"{"kind":"function","vars":["theta"]}"#,
    ] {
        assert!(serde_json::from_str::<AnswerContract>(doc).is_ok(), "{doc}");
    }
}

#[test]
fn learner_text_that_is_not_one_formula() {
    run(&[
        (X, "x", "(1, 2)", Ungraded),
        (X, "x", "{x}", Ungraded),
        (X, "x", "x > 2", Ungraded),
        (X, "x", "5 cm", Ungraded),
        (X, "x", "x/sqrt(x^2+9", Ungraded),
        (X, "x", " ", Wrong),
        (X, "2x", "y = 2x", Correct),
        (X, "2x", "f(x) = 2x", Ungraded), // low: a natural learner form; the parser refuses it
        (X, "2x", "dy/dx = 2x", Ungraded),
        (X, "2x", "2X", Wrong),
        (X, "x^2", "x = x^2", Correct), // low: the label is ignored, also the label `x`
        (X, "ln(x)", "log(x)", Correct), // the learner side reads `log` as `ln` (D28 is key side)
    ]);
}

/// No panic on any learner text: the 4,000-character cap, deep nesting, and
/// 20,000 texts from a fixed generator. No text of the generator is CORRECT
/// except a large constant (DEFECT 1).
#[test]
fn no_panic_on_any_learner_text() {
    let contract: AnswerContract = serde_json::from_str(XC).unwrap();
    let mut texts: Vec<String> = Vec::new();
    for size in [3990_usize, 3999, 4000, 4001, 8000, 100_000] {
        texts.push("x".repeat(size));
        texts.push("1+".repeat(size / 2) + "x");
        texts.push("(".repeat(size / 2) + "x" + &")".repeat(size / 2));
        texts.push("-".repeat(size - 1) + "x");
        texts.push("sin(".repeat(size / 4) + "x" + &")".repeat(size / 4));
        texts.push("é".repeat(size));
    }
    for depth in [95_usize, 96, 97, 1000] {
        texts.push("sqrt(".repeat(depth) + "x" + &")".repeat(depth));
        texts.push("e^(".repeat(depth) + "x" + &")".repeat(depth));
        texts.push("1/(".repeat(depth) + "x" + &")".repeat(depth));
    }
    let pieces = [
        "x",
        "C",
        "+",
        "-",
        "*",
        "/",
        "^",
        "(",
        ")",
        "|",
        "=",
        "sin",
        "ln",
        "sqrt",
        "e",
        "pi",
        "1",
        "0",
        ".",
        ",",
        " ",
        "{",
        "}",
        "[",
        "]",
        "<",
        "%",
        "!",
        "\\",
        "'",
        "\u{221e}",
        "\u{2212}",
        "9999999999",
        "^(1/2)",
        "^-1",
        "1e9",
        "NaN",
        "inf",
        "\0",
    ];
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut step = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        usize::try_from(state >> 33).unwrap()
    };
    for _ in 0..20_000 {
        let count = step() % 12 + 1;
        texts.push((0..count).map(|_| pieces[step() % pieces.len()]).collect());
    }
    let mut correct = Vec::new();
    for text in &texts {
        if matches!(
            check_contract("x^2/2 + C", text, contract.clone()),
            Outcome::Decided(verdict) if verdict.correct
        ) {
            correct.push(text.clone());
        }
        let _ = contract.validate_expected(text);
    }
    // D42: no learner text of the generator is correct any more. The two
    // large constants of DEFECT 1 are wrong under the variation rule.
    assert!(correct.is_empty(), "correct texts: {correct:?}");
}
