//! U2 divergences: every pair where 2.0 does not give the 1.0 verdict.
//!
//! Each test records the 1.0 verdict as a literal, pins the 2.0 verdict as a
//! literal, and states the reason. The 1.0 verdicts come from
//! `cadus_web.sympy_check.answers_equivalent`, run on 2026-08-26 with
//! `/home/deploy/dev/cadus/.venv/bin/python`, and they match
//! `docs/reference/checker-1.0-spec.md` sections 6, 7.6, and 7.7.
//!
//! A divergence is not a defect until someone shows it is one. The four classes
//! below are the classes of spec section 9.3:
//!
//! 1. The answer leaves the decidable grammar, so 2.0 refuses a verdict (V2).
//! 2. 2.0 holds exact values only, so no float rung decides an answer (D6).
//! 3. 2.0 decides a wrong verifiable answer, and 1.0 handed it to the model (A3).
//! 4. 2.0 canonicalizes instead of simplifying, so it fixes some 1.0 misses and
//!    narrows some 1.0 hits (V1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_core::answer::{Outcome, Verdict, check};
use cadus_core::curriculum::AnswerKind;

/// The `numeric` answer kind.
const N: AnswerKind = AnswerKind::Numeric;
/// The `expression` answer kind.
const E: AnswerKind = AnswerKind::Expression;

/// The outcome of a decided check.
fn decided(correct: bool, notation: bool) -> Outcome {
    Outcome::Decided(Verdict { correct, notation })
}

/// Assert that 2.0 refuses a verdict, and name the refusal.
fn assert_undecidable(expected: &str, learner: &str, kind: AnswerKind, reason: &str) {
    match check(expected, learner, kind) {
        Outcome::Undecidable(refusal) => assert_eq!(
            refusal.reason, reason,
            "{expected:?} against {learner:?} on {kind}"
        ),
        other => panic!("{expected:?} against {learner:?} on {kind} gave {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Class 1 — outside the grammar, so 2.0 claims no verdict (V2)
// ---------------------------------------------------------------------------

#[test]
fn prose_answers_get_no_deterministic_verdict() {
    // 1.0: True for both. Prose parses into a product of one-letter symbols, and
    // multiplication commutes, so every anagram of a word answer is "correct"
    // (spec section 7.6). 167 corpus answers are in this class.
    assert_undecidable("yes", "sey", N, "a name that is not a function or variable");
    assert_undecidable(
        "even",
        "neve",
        N,
        "a name that is not a function or variable",
    );
}

#[test]
fn a_sympy_name_gets_no_deterministic_verdict() {
    // 1.0: `zoo` = `-zoo` is True, and a test pins it
    // (`tests/test_sympy_check.py:123`). `zoo` is SymPy's complex infinity, and
    // every SymPy name is in scope of a learner answer box (spec section 3.1).
    assert_undecidable(
        "zoo",
        "-zoo",
        E,
        "a name that is not a function or variable",
    );
    // 1.0: True. `III` parses as `I*I*I`, which is `-I`, the imaginary unit.
    assert_undecidable("III", "-I", E, "a name that is not a function or variable");
    // 1.0: True for `Interval(0, 1)` = `Interval(0,1)`, False for `Interval(0,2)`.
    // 2.0 has no constructor calls: `Interval` is not a whitelisted function.
    assert_undecidable(
        "Interval(0, 1)",
        "Interval(0,1)",
        E,
        "a name that is not a function or variable",
    );
    assert_undecidable(
        "Interval(0, 1)",
        "Interval(0,2)",
        E,
        "a name that is not a function or variable",
    );
    // 1.0: True. `oo` is SymPy's infinity. 2.0 has no infinity value.
    assert_undecidable("∞", "oo", E, "a name that is not a function or variable");
    assert_undecidable("-∞", "-oo", E, "a name that is not a function or variable");
}

#[test]
fn an_answer_past_the_work_bound_gets_no_verdict() {
    // 1.0: False, in 276 ms of the 300 ms L2 budget (spec section 3.2). That one
    // 12-character answer is the concrete case for V1. 2.0 bounds the work and
    // refuses instead of spending the budget.
    assert_undecidable(
        "(x+1)**200",
        "x**200+1",
        E,
        "the answer goes past the work bound",
    );
}

// ---------------------------------------------------------------------------
// Class 2 — no float rung (D6, V1)
// ---------------------------------------------------------------------------

#[test]
fn a_rounded_decimal_is_not_the_exact_value() {
    // 1.0: True for all four. The SymPy rung compares `evalf()` results at a 1e-6
    // relative tolerance (spec section 3.2), so a learner who rounds is "correct"
    // and a learner who rounds a little more is not. 2.0 has no float in any
    // equality decision (D6), so a rounded decimal is a different value.
    assert_eq!(check("1/3", "0.333333", N), decided(false, false));
    assert_eq!(check("1/3", "0.3333333333", N), decided(false, false));
    assert_eq!(check("sqrt(2)", "1.41421356", N), decided(false, false));
    assert_eq!(check("pi", "3.14159", N), decided(false, false));
    // The 1.0 boundary case stays wrong in both versions.
    assert_eq!(check("2/3", "0.667", N), decided(false, false));
}

// ---------------------------------------------------------------------------
// Class 3 — the kind gate and the wrong-answer verdict (A3)
// ---------------------------------------------------------------------------

#[test]
fn an_undecidable_kind_never_gets_a_string_match() {
    // 1.0: False, because `dot_thousands_variant` gates on the kind. But the 1.0
    // string rung runs BEFORE the kind gate (`sympy_check.py:379-382`), so an
    // exact string match returns True for `proof` and `multi-step` too. 2.0 puts
    // the kind gate first, so an undecidable kind has no verdict at all.
    assert_undecidable(
        "7329",
        "7.329",
        AnswerKind::MultiStep,
        "the answer kind is not decidable",
    );
    assert_undecidable(
        "7329",
        "7.329",
        AnswerKind::Proof,
        "the answer kind is not decidable",
    );
    assert_undecidable(
        "a proof",
        "a proof",
        AnswerKind::Proof,
        "the answer kind is not decidable",
    );
}

#[test]
fn a_wrong_verifiable_answer_is_decided_and_not_deferred() {
    // 1.0 `answers_equivalent`: False. 1.0 `deterministic_grade`: None — a miss
    // went to the model, deliberately (`deterministic_grade.py:26-31`). A3 drops
    // that restriction: the checker answers for a right AND a wrong answer.
    assert_eq!(check("12", "14", N), decided(false, false));
    assert_eq!(check("7239", "7.329", N), decided(false, false));
}

// ---------------------------------------------------------------------------
// Class 4 — canonical forms instead of simplification (V1)
// ---------------------------------------------------------------------------

#[test]
fn the_false_negatives_of_spec_7_7_are_fixed() {
    // 1.0: False for every row. Each miss marked a correct learner wrong.
    let rows: [(&str, &str, AnswerKind); 8] = [
        // Implicit multiplication read `3 1/2` as `3*(1/2)`.
        ("3 1/2", "7/2", N),
        ("3 1/2", "3.5", N),
        // The rewriter deleted the backslash and left `frac{1}{2}`.
        ("1/2", "\\frac{1}{2}", N),
        // The rewriter left `x**{2}`, which SymPy cannot parse.
        ("x**2", "x^{2}", E),
        // 1.0 has no percent reading.
        ("0.5", "50%", N),
        // 1.0 has no `x =` prefix reading.
        ("5", "x=5", N),
        // A chained relational raised inside SymPy.
        ("-1 ≤ x ≤ 3", "-1 <= x <= 3", E),
        // Tuples compared structurally, so `17` and `17.0` were different.
        ("(4, 17)", "(4, 17.0)", N),
    ];
    for (expected, learner, kind) in rows {
        assert_eq!(
            check(expected, learner, kind),
            decided(true, false),
            "{expected:?} against {learner:?} on {kind}"
        );
    }
}

#[test]
fn e_is_eulers_number_on_both_sides() {
    // 1.0: False. `to_sympy_source` keeps the case, and SymPy reads lowercase `e`
    // as a plain symbol while `E` is Euler's number, so `e**2` is `e**2` and
    // `exp(2)` is a number. 2.0 reads `e` and `E` as one constant.
    assert_eq!(check("e**2", "exp(2)", E), decided(true, false));
}

#[test]
fn a_transcendental_identity_is_not_decided() {
    // 1.0: True, through `simplify(lhs - rhs) == 0` in 16 ms (spec section 3.2).
    // 2.0 compares canonical forms and runs no simplification (V1), so a function
    // application equals only the same function application.
    assert_eq!(check("sin(x)**2+cos(x)**2", "1", E), decided(false, false));
}

#[test]
fn a_rational_function_is_not_cancelled() {
    // 1.0: True, through `simplify`. 2.0 normalizes the content of the divisor,
    // so a scalar multiple still matches, but it runs no polynomial greatest
    // common divisor, so a common factor does not cancel.
    assert_eq!(check("(x**2-1)/(x-1)", "x+1", E), decided(false, false));
    assert_eq!(check("1/(x+1)", "2/(2*x+2)", E), decided(true, false));
}

#[test]
fn a_polynomial_identity_is_decided_by_the_canonical_form() {
    // 1.0: True, in 62 ms of `simplify` (spec section 3.2). 2.0: True, by
    // expanding both sides into the same sparse polynomial. Same verdict, and it
    // costs no search.
    assert_eq!(check("(x+1)**2", "x**2+2*x+1", E), decided(true, false));
    assert_eq!(check("x**2 - 1", "(x-1)*(x+1)", E), decided(true, false));
}

#[test]
fn a_range_and_a_list_of_two_numbers_stay_different_answers() {
    // 1.0: False, because the chained relational raises. 2.0: False on purpose. A
    // chained inequality is a predicate over a variable and a list is a pair of
    // numbers, so the two are different answers.
    assert_eq!(check("-1 ≤ x ≤ 3", "[-1, 3]", E), decided(false, false));
}

// ---------------------------------------------------------------------------
// Pinned agreements: the traps of spec section 7 that 2.0 must not reopen
// ---------------------------------------------------------------------------

#[test]
fn the_traps_of_spec_7_keep_the_1_0_verdict() {
    // 1.0: False. The 1000x hole of spec section 7.2, closed by the non-zero
    // leading group of `_DOT_GROUPS_RE`. A learner's unit slip stays wrong.
    assert_eq!(check("8", "0.008", N), decided(false, false));
    // 1.0: False. There is no decimal-comma reading, in 1.0 or in 2.0.
    assert_eq!(check("0.5", "0,5", N), decided(false, false));
    // 1.0: True, and `dot_thousands_variant` is True as well.
    assert_eq!(check("7329", "7.329", N), decided(true, true));
    // 1.0: True. Exact arithmetic decides it in 2.0.
    assert_eq!(check("7", "49/7", N), decided(true, false));
}
