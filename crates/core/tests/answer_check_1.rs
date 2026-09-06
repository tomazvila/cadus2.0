//! U2 acceptance: canonical forms (V1, D6) and the checker (A3, C4, V4).
//!
//! Every pair in this file is a literal of
//! `docs/reference/checker-1.0-spec.md`, and every expected verdict is the 1.0
//! verdict that the spec records. No expected value is read back from the code
//! under test.
//!
//! The pairs where 2.0 deliberately leaves 1.0 are NOT here. They live in
//! `answer_divergence.rs`, one test each, with the recorded 1.0 verdict and the
//! reason for the change.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::check::*;

// ---------------------------------------------------------------------------
// Spec section 6.1 — tests/test_sympy_check.py
// ---------------------------------------------------------------------------
#[test]
fn spec_6_1_numeric_equivalence() {
    // The first row of the 1.0 table, `2/3` against `0.667`, is not here: 2.0
    // reads it as the exact rounding of two thirds and grades it correct with a
    // notation tag (ruling `D6-dec`). The pair lives in `answer_divergence.rs`
    // with its 1.0 verdict, which is the rule of this file's header.
    run_table(&[
        ("2*x+1", "1 + 2*x", E, true),
        ("x**2 - 1", "(x-1)*(x+1)", E, true),
        ("3", "4", N, false),
        ("7", "49/7", N, true),
        ("1/2", "0.5", N, true),
    ]);
}

#[test]
fn spec_6_1_unicode_maths_is_parseable() {
    run_table(&[
        ("15√3", "15*sqrt(3)", E, true),
        ("1/(2√x)", "1/(2*sqrt(x))", E, true),
        ("x/√(x^2 + 9)", "x/sqrt(x**2+9)", E, true),
        (
            "1/(4√x · √(1 + √x))",
            "1/(4*sqrt(x)*sqrt(1+sqrt(x)))",
            E,
            true,
        ),
        ("2√3/3", "2*sqrt(3)/3", E, true),
        ("π/6", "pi/6", N, true),
        ("2π", "2*pi", N, true),
        ("x²+1", "x**2+1", E, true),
        ("½", "1/2", N, true),
        ("15*sqrt(3)", "15√3", E, true),
        ("pi/6", "π/6", N, true),
        ("15√3", "15*sqrt(2)", E, false),
        ("π/6", "pi/3", N, false),
        ("x²+1", "x**3+1", E, false),
        ("√2", "2", N, false),
    ]);
    // `30°` against `30` was correct here while the lexer deleted the degree
    // sign. The value-with-unit production of D-F3 (unit f2-grammar) reads
    // `30°` as a quantity, so the bare number gets no verdict until the
    // contract of D-F1 decides it; `answer_unit.rs` pins the production.
    assert_undecidable("30°", "30", N, "a unit is missing");
}

#[test]
fn spec_6_1_non_numeric_pairs_compare_structurally() {
    run_table(&[
        ("(4, 17)", "(4,17)", N, true),
        ("(4, 17)", "(4, 18)", N, false),
        ("(-6, 2)", "(-6,2)", N, true),
        ("{1, 2}", "{2,1}", E, true),
        ("{1, 2}", "{1,3}", E, false),
        ("1 + I", "1+I", E, true),
        ("1 + I", "1 - I", E, false),
        // Spec section 7.6: 1.0 reads a bare comma list and a parenthesized tuple
        // as one answer, and so does 2.0.
        ("4, 17", "(4, 17)", N, true),
    ]);
}

#[test]
fn spec_6_1_grouped_integers_are_accepted() {
    for learner in [
        "7,329",
        "7329",
        "7,329.",
        "7329.",
        "7 329",
        "7\u{00a0}329",
        "7\u{202f}329",
        "$7,329$",
        " 7,329 ",
    ] {
        assert_eq!(
            check("7329", learner, N),
            decided(true, false),
            "grouped {learner:?}"
        );
    }
}

#[test]
fn spec_6_1_a_wrong_value_stays_wrong_however_it_is_grouped() {
    for learner in ["7330", "7,330", "7.32", "73,29", "7329x"] {
        assert_eq!(
            check("7329", learner, N),
            decided(false, false),
            "wrong {learner:?}"
        );
    }
}

#[test]
fn spec_6_1_a_period_grouped_integer_is_a_notation_variant() {
    assert_eq!(check("7329", "7.329", N), decided(true, true));
    assert_eq!(check("2500", "2.500", N), decided(true, true));
}

#[test]
fn spec_6_1_the_american_reading_always_wins_first() {
    assert_eq!(check("0.5", "0.5", N), decided(true, false));
    assert_eq!(check("1", "1.000", N), decided(true, false));
}

#[test]
fn spec_6_1_the_notation_reading_requires_an_exact_value_match() {
    assert_eq!(check("7239", "7.329", N), decided(false, false));
}

#[test]
fn spec_6_1_a_trailing_period_does_not_break_an_expression() {
    assert_eq!(check("2*x+1", "1 + 2x.", E), decided(true, false));
}

#[test]
fn spec_6_1_a_decimal_answer_survives_a_trailing_period() {
    assert_eq!(check("0.5", "0.5.", N), decided(true, false));
    assert_eq!(check("0.5", "0.50", N), decided(true, false));
}

// ---------------------------------------------------------------------------
// Spec section 6.2 — tests/test_deterministic_grade.py, mapped to `check`
// ---------------------------------------------------------------------------
#[test]
fn spec_6_2_a_verified_correct_answer_needs_no_engine() {
    run_table(&[
        ("12", "12", N, true),
        ("12", "12.0", N, true),
        ("12", "sqrt(144)", N, true),
        ("7329", "7,329", N, true),
        ("7400", "7400", N, true),
        ("2*x+1", "1 + 2x", E, true),
    ]);
}

#[test]
fn spec_6_2_a_blank_answer_is_wrong() {
    for learner in ["", "   ", "\t\n"] {
        assert_eq!(
            check("12", learner, N),
            decided(false, false),
            "blank {learner:?}"
        );
    }
    // The blank rung runs before the kind gate, exactly as 1.0
    // `deterministic_grade.py:120-125` orders the two.
    assert_eq!(
        check("anything", "   ", AnswerKind::Proof),
        decided(false, false)
    );
}

#[test]
fn spec_6_2_an_unverifiable_kind_has_no_deterministic_verdict() {
    for kind in [AnswerKind::MultiStep, AnswerKind::Proof] {
        match check("anything", "some answer", kind) {
            Outcome::Undecidable(reason) => {
                assert_eq!(reason.reason, "the answer kind is not decidable");
            }
            other => panic!("{kind} gave {other:?}"),
        }
    }
}

#[test]
fn spec_6_2_a_correct_answer_carries_no_notation_note() {
    assert_eq!(check("0.5", "0.5", N), decided(true, false));
    assert_eq!(check("12", "12", N), decided(true, false));
}

#[test]
fn spec_6_3_security_payloads_are_undecidable_and_run_nothing() {
    for payload in RCE_PAYLOADS {
        assert!(
            matches!(check("4", payload, N), Outcome::Undecidable(_)),
            "payload as the learner answer: {payload:?}"
        );
        assert!(
            matches!(check(payload, "4", N), Outcome::Undecidable(_)),
            "payload as the authored answer: {payload:?}"
        );
    }
    assert!(
        !std::path::Path::new("_rce_marker_should_not_exist").exists(),
        "the checker must never evaluate a learner answer"
    );
}

#[test]
fn spec_6_3_exponent_bombs_are_undecidable_inside_the_budget() {
    let bombs = [
        "9**9**9",
        "9^9^9",
        "9**9**9**9",
        "2**10000000",
        "(2)**(9999999)",
    ];
    let start = Instant::now();
    for bomb in bombs {
        assert!(
            matches!(check("4", bomb, N), Outcome::Undecidable(_)),
            "bomb as the learner answer: {bomb:?}"
        );
    }
    for bomb in ["9**9**9", "2**10000000"] {
        assert!(
            matches!(check(bomb, "4", N), Outcome::Undecidable(_)),
            "bomb as the authored answer: {bomb:?}"
        );
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(50),
        "the exponent bombs took {elapsed:?}, and the budget is 50 ms"
    );
}

#[test]
fn spec_6_3_the_guard_does_not_reject_legitimate_powers() {
    run_table(&[
        ("8", "2**3", E, true),
        ("1024", "2**10", E, true),
        ("x**2+1", "x^2+1", E, true),
        ("x**2*y**3", "x^2*y^3", E, true),
        ("15√3", "15*sqrt(3)", E, true),
    ]);
}

// ---------------------------------------------------------------------------
// Canonical forms (V1, D6) — the literal shapes of spec section 8.1
// ---------------------------------------------------------------------------
#[test]
fn a_decimal_becomes_an_exact_rational() {
    assert_eq!(form("0.7"), Canon::Rational(ratio(7, 10)));
    assert_eq!(form("0.50"), Canon::Rational(ratio(1, 2)));
    assert_eq!(form("12.0"), Canon::Rational(whole(12)));
    assert_eq!(form("3 1/2"), Canon::Rational(ratio(7, 2)));
    assert_eq!(form("-2 1/4"), Canon::Rational(ratio(-9, 4)));
}

#[test]
fn a_fraction_after_a_divided_or_raised_number_gets_no_verdict() {
    // M2 review 4, finding #1. The `b/c` mixed-number spelling fell back to the
    // product reading when a `/` or a `^` had already taken the number token, so
    // the authored answer `3*t/16` accepted the learner answer `t/4 3/4`. The
    // mixed-number rule reads that answer as `t/(4 + 3/4)` = 4t/19, so 2.0
    // graded a wrong answer correct (C4). The glyph and the `\frac` spellings of
    // the same shape refused it, so one rule gave two verdicts.
    for (expected, learner) in [
        ("3*t/16", "t/4 3/4"),
        ("4*t/19", "t/4 3/4"),
        ("x/4", "x/2 1/2"),
        ("x^2/2", "x^2 1/2"),
        ("cos(x)/4", "cos(x)/2 1/2"),
        ("pi/4", "pi/2 1/2"),
    ] {
        for (left, right) in [(expected, learner), (learner, expected)] {
            match check(left, right, E) {
                Outcome::Undecidable(refused) => assert_eq!(
                    refused.reason, "a fraction stands after a number that is no whole part",
                    "{left:?} against {right:?}"
                ),
                other => panic!("{left:?} against {right:?} gave {other:?}"),
            }
        }
    }
    // The mixed number itself keeps its verdict, in both kinds.
    assert_eq!(check("5/2", "2 1/2", N), decided(true, false));
    assert_eq!(check("5/2", "2 1/2", E), decided(true, false));
    assert_eq!(check("2 1/2", "2.5", N), decided(true, false));
}

#[test]
fn a_radical_is_reduced_to_a_squarefree_radicand() {
    let two_root_two = Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(2),
            pi: 0,
            e: 0,
        },
        whole(2),
    )]));
    assert_eq!(form("sqrt(8)"), two_root_two);
    assert_eq!(form("sqrt(4)"), Canon::Rational(whole(2)));
    assert_eq!(form("sqrt(144)"), Canon::Rational(whole(12)));
    assert_eq!(form("sqrt(0)"), Canon::Rational(whole(0)));
    // sqrt(2)*sqrt(3) is sqrt(6), and 1/sqrt(2) is sqrt(2)/2.
    let root_six = Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(6),
            pi: 0,
            e: 0,
        },
        whole(1),
    )]));
    assert_eq!(form("sqrt(2)*sqrt(3)"), root_six);
    let half_root_two = Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(2),
            pi: 0,
            e: 0,
        },
        ratio(1, 2),
    )]));
    assert_eq!(form("1/sqrt(2)"), half_root_two);
}

#[test]
fn a_radical_product_extracts_the_square_of_the_merged_radicand() {
    // M2 review 1, finding 15. `sqrt(2)*sqrt(3)` alone leaves the merge untested:
    // its merged radicand, 6, is already squarefree. Every pair below merges into
    // a radicand that carries a square, so the extraction has to run.
    assert_eq!(form("sqrt(2)*sqrt(8)"), Canon::Rational(whole(4)));
    assert_eq!(form("sqrt(2)*sqrt(2)"), Canon::Rational(whole(2)));
    assert_eq!(form("sqrt(12)*sqrt(3)"), Canon::Rational(whole(6)));
    assert_eq!(form("sqrt(6)*sqrt(3)"), radical(2, whole(3)));
    assert_eq!(form("sqrt(2)*sqrt(6)"), radical(3, whole(2)));
    // 1.0 answers True for every pair below.
    assert_eq!(check("4", "sqrt(2)*sqrt(8)", N), decided(true, false));
    assert_eq!(check("2", "sqrt(2)*sqrt(2)", N), decided(true, false));
    assert_eq!(check("6", "sqrt(12)*sqrt(3)", N), decided(true, false));
    assert_eq!(
        check("3*sqrt(2)", "sqrt(6)*sqrt(3)", E),
        decided(true, false)
    );
    assert_eq!(
        check("2*sqrt(3)", "sqrt(2)*sqrt(6)", E),
        decided(true, false)
    );
    // C4: the merge must not accept a different value. Each learner answer below
    // is the product of two radicals with a different value.
    assert_eq!(check("4", "sqrt(2)*sqrt(6)", N), decided(false, false));
    assert_eq!(check("16", "sqrt(2)*sqrt(8)", N), decided(false, false));
    assert_eq!(
        check("3*sqrt(2)", "sqrt(6)*sqrt(2)", E),
        decided(false, false)
    );
    assert_eq!(
        check("2*sqrt(3)", "sqrt(6)*sqrt(3)", E),
        decided(false, false)
    );
}

#[test]
fn a_radicand_that_is_not_a_rational_number_is_a_root_atom() {
    // The reading changed in M2 review 3, findings 9 and 11: a RATIONAL radicand
    // reduces, and `a_rational_radicand_reduces_to_the_same_value` below owns
    // that half. The rational-exponent production of D-F3 (unit f2-grammar)
    // then made `sqrt(x)` the root atom of `x`, so `sqrt(x)` and `x^(1/2)` are
    // one form.
    let x = Canon::Poly(BTreeMap::from([(monomial(&[(var("x"), 1)]), whole(1))]));
    let root_of_x = Canon::Poly(BTreeMap::from([(
        monomial(&[(Atom::Root(Box::new(x), 2), 1)]),
        whole(1),
    )]));
    assert_eq!(form("sqrt(x)"), root_of_x);
    assert_eq!(form("x^(1/2)"), root_of_x);
    // A negative radicand is not a real number, so it keeps its application too.
    assert_eq!(
        form("sqrt(-1/2)"),
        Canon::Func("sqrt".to_string(), vec![Canon::Rational(ratio(-1, 2))])
    );
    assert_eq!(
        form("sqrt(-4)"),
        Canon::Func("sqrt".to_string(), vec![Canon::Rational(whole(-4))])
    );
}

#[test]
fn a_rational_radicand_reduces_to_the_same_value() {
    // M2 review 3, findings 9 and 11. `sqrt(p/q)` is `sqrt(p*q)/q`, so a rational
    // radicand reduces the same way a whole one does. Every verdict below is the
    // 1.0 verdict, measured with `scripts/oracle/check_1_0.py`.
    // The canonical forms, as literals.
    assert_eq!(form("sqrt(1/2)"), radical(2, ratio(1, 2)));
    assert_eq!(form("sqrt(1/2)"), form("sqrt(2)/2"));
    assert_eq!(form("sqrt(4/9)"), Canon::Rational(ratio(2, 3)));
    assert_eq!(form("sqrt(9/16)"), Canon::Rational(ratio(3, 4)));
    assert_eq!(form("sqrt(0.25)"), Canon::Rational(ratio(1, 2)));
    assert_eq!(form("sqrt(2/3)"), radical(6, ratio(1, 3)));
    assert_eq!(form("sqrt(8/2)"), Canon::Rational(whole(2)));
    // 1.0: True for every pair below.
    run_table(&[
        ("sqrt(2)/2", "sqrt(1/2)", N, true),
        ("√2/2", "√(1/2)", N, true),
        ("2/3", "sqrt(4/9)", N, true),
        ("3/2", "sqrt(9/4)", N, true),
        ("3/4", "√(9/16)", N, true),
        ("1/2", "sqrt(0.25)", N, true),
        ("1/2", "√(1/4)", N, true),
        ("√3/3", "√(1/3)", N, true),
        ("sqrt(6)/3", "sqrt(2/3)", N, true),
        // 1/sqrt(2) already reduced before this fix; the two neighbors now agree.
        ("√2/2", "1/√2", N, true),
        ("sqrt(1/2)", "1/sqrt(2)", E, true),
        // The same value under a product, a quotient, a power, and a function.
        ("2*sqrt(2)/2", "2*sqrt(1/2)", E, true),
        ("sqrt(2)/4", "sqrt(1/2)/2", E, true),
        ("1/2", "sqrt(1/2)^2", E, true),
        ("sin(sqrt(2)/2)", "sin(sqrt(1/2))", E, true),
        ("x*sqrt(2)/2", "x*sqrt(1/2)", E, true),
        ("sqrt(2)/(2x)", "sqrt(1/2)/x", E, true),
        // The same pair on the other answer kind.
        ("sqrt(2)/2", "sqrt(1/2)", E, true),
    ]);
    // C4: the reduction must admit no other value. 1.0: False for every pair
    // below. Each learner answer is a wrong value in the same spelling.
    run_table(&[
        ("sqrt(2)/2", "sqrt(1/3)", N, false),
        ("sqrt(2)/2", "sqrt(2/2)", N, false),
        ("sqrt(2)/2", "-sqrt(1/2)", N, false),
        ("sqrt(2)/2", "sqrt(1/2)/2", N, false),
        ("2/3", "sqrt(4/3)", N, false),
        ("2/3", "sqrt(9/4)", N, false),
        ("3/4", "√(9/17)", N, false),
        ("1/2", "sqrt(0.26)", N, false),
        ("1/2", "sqrt(1/2)", N, false),
        ("sqrt(6)/3", "sqrt(3/2)", N, false),
        ("sin(sqrt(2)/2)", "sin(sqrt(1/3))", E, false),
        ("x*sqrt(2)/2", "y*sqrt(1/2)", E, false),
        ("sqrt(2)/2", "sqrt(1/3)", E, false),
    ]);
}

#[test]
fn a_polynomial_is_a_sparse_map_of_monomials() {
    let expected = Canon::Poly(BTreeMap::from([
        (monomial(&[]), whole(1)),
        (monomial(&[(var("x"), 1)]), whole(2)),
        (monomial(&[(var("x"), 2)]), whole(1)),
    ]));
    assert_eq!(form("(x+1)**2"), expected);
    assert_eq!(form("x**2+2*x+1"), expected);
    assert_eq!(form("1 + 2x + x^2"), expected);
}
