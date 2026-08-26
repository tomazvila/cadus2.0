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

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use cadus_core::answer::check::same_answer;
use cadus_core::answer::{
    Ast, Atom, Basis, Canon, Monomial, Outcome, Verdict, canon, canonical_form, check,
};
use cadus_core::curriculum::AnswerKind;
use num_bigint::BigInt;
use num_rational::BigRational;

/// The `numeric` answer kind.
const N: AnswerKind = AnswerKind::Numeric;
/// The `expression` answer kind.
const E: AnswerKind = AnswerKind::Expression;

/// The outcome of a decided check.
fn decided(correct: bool, notation: bool) -> Outcome {
    Outcome::Decided(Verdict { correct, notation })
}

/// Run one table of `(expected, learner, kind, correct)` rows.
fn run_table(rows: &[(&str, &str, AnswerKind, bool)]) {
    for (expected, learner, kind, correct) in rows {
        assert_eq!(
            check(expected, learner, *kind),
            decided(*correct, false),
            "{expected:?} against {learner:?} on {kind}"
        );
    }
}

/// Build an exact rational literal.
fn ratio(numerator: i64, denominator: i64) -> BigRational {
    BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
}

/// Build a whole-number rational literal.
fn whole(value: i64) -> BigRational {
    BigRational::from_integer(BigInt::from(value))
}

/// Build a monomial literal from atoms and exponents.
fn monomial(atoms: &[(Atom, i64)]) -> Monomial {
    atoms.iter().cloned().collect()
}

/// Build a variable atom literal.
fn var(name: &str) -> Atom {
    Atom::Var(name.to_string())
}

/// Canonicalize one answer, and fail the test when the grammar refuses it.
fn form(text: &str) -> Canon {
    canonical_form(text).unwrap_or_else(|e| panic!("{text:?}: {}", e.reason))
}

/// Build a radical literal from one radicand and its coefficient.
fn radical(radicand: i64, coefficient: BigRational) -> Canon {
    Canon::Radical(BTreeMap::from([(
        Basis {
            radicand: BigInt::from(radicand),
            pi: 0,
            e: 0,
        },
        coefficient,
    )]))
}

/// Whether the run asks for the release budget of L2.
///
/// A debug build runs the exact arithmetic about ten times slower than a release
/// build, so the two builds carry two budgets. `CADUS_RELEASE_BENCH` selects the
/// release budget of 5 ms per check and 1 s per corpus pass. A plain
/// `cargo test` run keeps the debug budget of 50 ms and 5 s, which measures the
/// work and not the scheduler (`docs/reviews/M2-review-1.md`, finding 20).
fn release_bench() -> bool {
    std::env::var_os("CADUS_RELEASE_BENCH").is_some()
}

/// The wall-clock budget of one check.
fn one_check_budget() -> Duration {
    if release_bench() {
        Duration::from_millis(5)
    } else {
        Duration::from_millis(50)
    }
}

/// The wall-clock budget of one check of a crafted worst-case answer.
///
/// The round-4 ruling of finding #2 puts the reviewer's 4,000-character case at
/// 50 ms in a release build. A debug build runs the exact arithmetic about ten
/// times slower, so the debug budget is ten times the release one, the same
/// scale as [`one_check_budget`] carries.
fn bomb_budget() -> Duration {
    if release_bench() {
        Duration::from_millis(50)
    } else {
        Duration::from_millis(500)
    }
}

/// The wall-clock budget of one pass over the whole corpus.
fn corpus_budget() -> Duration {
    if release_bench() {
        Duration::from_secs(1)
    } else {
        Duration::from_secs(5)
    }
}

// ---------------------------------------------------------------------------
// Spec section 6.1 — tests/test_sympy_check.py
// ---------------------------------------------------------------------------

#[test]
fn spec_6_1_numeric_equivalence() {
    run_table(&[
        ("2/3", "0.667", N, false),
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
        ("30°", "30", N, true),
        ("15*sqrt(3)", "15√3", E, true),
        ("pi/6", "π/6", N, true),
        ("15√3", "15*sqrt(2)", E, false),
        ("π/6", "pi/3", N, false),
        ("x²+1", "x**3+1", E, false),
        ("√2", "2", N, false),
    ]);
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

// ---------------------------------------------------------------------------
// Spec section 6.3 — tests/test_sympy_security.py
// ---------------------------------------------------------------------------

/// The payloads of 1.0 `tests/test_sympy_security.py:24-58`.
const RCE_PAYLOADS: [&str; 4] = [
    "__import__('os').system('touch _rce_marker_should_not_exist')",
    "exec(\"open('_rce_marker_should_not_exist','w').write('x')\")",
    "eval(\"__import__('os').getenv('ANTHROPIC_API_KEY')\")",
    "print(open('.env.example').read())",
];

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
fn a_radicand_that_is_not_a_rational_number_stays_a_function() {
    // The reading changed in M2 review 3, findings 9 and 11: a RATIONAL radicand
    // reduces now, and only a radicand outside the rationals keeps the function
    // application. `a_rational_radicand_reduces_to_the_same_value` below owns the
    // reduced half.
    let x = Canon::Poly(BTreeMap::from([(monomial(&[(var("x"), 1)]), whole(1))]));
    assert_eq!(form("sqrt(x)"), Canon::Func("sqrt".to_string(), vec![x]));
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

#[test]
fn a_chained_inequality_becomes_a_range_over_its_variable() {
    let expected = Canon::Interval {
        var: Some("x".to_string()),
        lo: Some(Box::new(Canon::Rational(whole(-1)))),
        lo_closed: true,
        hi: Some(Box::new(Canon::Rational(whole(3)))),
        hi_closed: true,
    };
    assert_eq!(form("-1 ≤ x ≤ 3"), expected);
    assert_eq!(form("-1 <= x <= 3"), expected);
    assert_eq!(form("3 >= x >= -1"), expected);
    let open_below = Canon::Interval {
        var: Some("x".to_string()),
        lo: None,
        lo_closed: false,
        hi: Some(Box::new(Canon::Rational(whole(-1)))),
        hi_closed: true,
    };
    assert_eq!(form("x <= -1"), open_below);
    assert_eq!(form("-1 >= x"), open_below);
}

#[test]
fn a_set_is_unordered_and_a_list_and_a_tuple_are_ordered() {
    assert_eq!(check("{1, 3, 5}", "{5, 3, 1}", E), decided(true, false));
    assert_eq!(check("[-3, 3]", "[3, -3]", E), decided(false, false));
    assert_eq!(check("(4, 17)", "(17, 4)", N), decided(false, false));
    // A tuple compares its members by value, which 1.0 cannot do (spec 7.7).
    assert_eq!(check("(4, 17)", "(4, 17.0)", N), decided(true, false));
}

#[test]
fn a_set_a_list_and_a_tuple_are_three_different_answers() {
    // M2 review 1, finding 14. The test above reorders inside one collection
    // kind only, so nothing pinned the kind itself. 1.0 answers False for every
    // cross-kind pair below and True for the repeated set member.
    let one_three_five = Canon::Set(
        [
            Canon::Rational(whole(1)),
            Canon::Rational(whole(3)),
            Canon::Rational(whole(5)),
        ]
        .into_iter()
        .collect(),
    );
    assert_eq!(form("{1, 3, 5}"), one_three_five);
    assert_eq!(
        form("[1, 3, 5]"),
        Canon::List(vec![
            Canon::Rational(whole(1)),
            Canon::Rational(whole(3)),
            Canon::Rational(whole(5)),
        ])
    );
    assert_eq!(
        form("(1, 3, 5)"),
        Canon::Tuple(vec![
            Canon::Rational(whole(1)),
            Canon::Rational(whole(3)),
            Canon::Rational(whole(5)),
        ])
    );
    assert_eq!(check("{1, 3, 5}", "[1, 3, 5]", E), decided(false, false));
    assert_eq!(check("{1, 3, 5}", "(1, 3, 5)", E), decided(false, false));
    assert_eq!(check("[1, 3, 5]", "(1, 3, 5)", E), decided(false, false));
    assert_eq!(check("[1, 3, 5]", "{1, 3, 5}", E), decided(false, false));
    // A repeated set member collapses, and a repeated list member does not.
    assert_eq!(check("{1, 3, 5}", "{1, 3, 5, 5}", E), decided(true, false));
    assert_eq!(check("[1, 3, 5]", "[1, 3, 5, 5]", E), decided(false, false));
}

#[test]
fn an_open_interval_end_is_not_a_closed_one() {
    // M2 review 1, finding 13. Every range assertion of U2 pinned a closed end,
    // so three closedness mutants lived. 1.0 answers False for every pair below.
    let open_chain = Canon::Interval {
        var: Some("x".to_string()),
        lo: Some(Box::new(Canon::Rational(whole(-1)))),
        lo_closed: false,
        hi: Some(Box::new(Canon::Rational(whole(3)))),
        hi_closed: false,
    };
    assert_eq!(form("-1 < x < 3"), open_chain);
    let open_above = Canon::Interval {
        var: Some("x".to_string()),
        lo: None,
        lo_closed: false,
        hi: Some(Box::new(Canon::Rational(whole(3)))),
        hi_closed: false,
    };
    assert_eq!(form("x < 3"), open_above);
    let open_below = Canon::Interval {
        var: Some("x".to_string()),
        lo: Some(Box::new(Canon::Rational(whole(4)))),
        lo_closed: false,
        hi: None,
        hi_closed: false,
    };
    assert_eq!(form("x > 4"), open_below);
    let half_open_high = Canon::Interval {
        var: None,
        lo: Some(Box::new(Canon::Rational(whole(0)))),
        lo_closed: false,
        hi: Some(Box::new(Canon::Rational(whole(1)))),
        hi_closed: true,
    };
    assert_eq!(form("(0, 1]"), half_open_high);
    let half_open_low = Canon::Interval {
        var: None,
        lo: Some(Box::new(Canon::Rational(whole(0)))),
        lo_closed: true,
        hi: Some(Box::new(Canon::Rational(whole(1)))),
        hi_closed: false,
    };
    assert_eq!(form("[0, 1)"), half_open_low);
    // C4: a strict end never accepts a closed one, in either direction.
    assert_eq!(
        check("-1 <= x <= 3", "-1 < x < 3", E),
        decided(false, false)
    );
    assert_eq!(
        check("-1 < x < 3", "-1 <= x <= 3", E),
        decided(false, false)
    );
    assert_eq!(check("x <= 3", "x < 3", E), decided(false, false));
    assert_eq!(check("x > 4", "x >= 4", E), decided(false, false));
    assert_eq!(check("x >= 4", "x > 4", E), decided(false, false));
    assert_eq!(check("(0, 1]", "[0, 1)", E), decided(false, false));
    assert_eq!(check("(0, 1]", "[0, 1]", E), decided(false, false));
}

#[test]
fn a_descending_chain_carries_its_upper_end_closedness() {
    // M2 review 4, finding #3. The round-1 fix pinned the ASCENDING chain only,
    // so `hi_closed: op == IneqOp::Ge` of the descending arm was never evaluated
    // as false and `hi_closed: true` survived the whole suite. A closed end that
    // accepts a strict one grades a wrong learner answer correct (C4).
    //
    // A descending chain names the same set as its ascending twin, and the
    // FIRST operator of the descending spelling carries the UPPER end.
    assert_eq!(form("3 >= x > -1"), form("-1 < x <= 3"));
    assert_eq!(form("3 > x >= -1"), form("-1 <= x < 3"));
    assert_eq!(form("3 ≥ x > -1"), form("-1 < x ≤ 3"));
    assert_eq!(form("3 > x ≥ -1"), form("-1 ≤ x < 3"));
    let upper_closed = Canon::Interval {
        var: Some("x".to_string()),
        lo: Some(Box::new(Canon::Rational(whole(-1)))),
        lo_closed: false,
        hi: Some(Box::new(Canon::Rational(whole(3)))),
        hi_closed: true,
    };
    assert_eq!(form("3 >= x > -1"), upper_closed);
    let upper_open = Canon::Interval {
        var: Some("x".to_string()),
        lo: Some(Box::new(Canon::Rational(whole(-1)))),
        lo_closed: true,
        hi: Some(Box::new(Canon::Rational(whole(3)))),
        hi_closed: false,
    };
    assert_eq!(form("3 > x >= -1"), upper_open);
    // C4: the four descending spellings are four different sets. 1.0 answers
    // False for every pair below.
    assert_ne!(form("3 >= x > -1"), form("3 > x > -1"));
    assert_ne!(form("3 >= x >= -1"), form("3 >= x > -1"));
    assert_ne!(form("3 > x >= -1"), form("3 > x > -1"));
    assert_ne!(form("3 >= x >= -1"), form("3 > x >= -1"));
    assert_eq!(check("3 >= x > -1", "3 > x > -1", E), decided(false, false));
    assert_eq!(
        check("-1 <= x <= 3", "3 > x >= -1", E),
        decided(false, false)
    );
    assert_eq!(check("-1 <= x < 3", "3 > x >= -1", E), decided(true, false));
}

#[test]
fn ln_and_log_are_one_function() {
    // M2 review 1, finding 8. 1.0 makes `ln` an alias of `log`, and the corpus
    // authors both spellings on the topic `change-of-base-formula`.
    // 1.0: True for the first two pairs.
    assert_eq!(
        check("log(12)/log(5)", "ln(12)/ln(5)", E),
        decided(true, false)
    );
    assert_eq!(
        check("ln(7)/ln(3)", "log(7)/log(3)", E),
        decided(true, false)
    );
    assert_eq!(form("ln(x)"), form("log(x)"));
    // C4: the alias must not accept another value or another function.
    assert_eq!(
        check("log(12)/log(5)", "ln(12)/ln(7)", E),
        decided(false, false)
    );
    assert_eq!(check("ln(2)", "log(3)", E), decided(false, false));
    assert_eq!(check("ln(x)", "sin(x)", E), decided(false, false));
    assert_eq!(check("ln(x)", "log(x, 2)", E), decided(false, false));
}

#[test]
fn an_exponential_obeys_the_exponent_law() {
    // M2 review 1, finding 19. A reciprocal of an exponential is the negative
    // exponent. 1.0 answers True for every pair below.
    assert_eq!(check("e^(-x)", "1/e^x", E), decided(true, false));
    assert_eq!(
        check("e^(-x)(2x - x^2)", "(2x - x^2)/e^x", E),
        decided(true, false)
    );
    assert_eq!(
        check("-2x e^(-x^2)", "-2x/e^(x^2)", E),
        decided(true, false)
    );
    assert_eq!(
        check("$-(x^2 + 2x + 2)/e^x + C$", "-(x^2 + 2x + 2)e^(-x) + C", E),
        decided(true, false)
    );
    assert_eq!(check("exp(x)**3", "exp(3*x)", E), decided(true, false));
    assert_eq!(check("1", "e^x*e^(-x)", E), decided(true, false));
    // A whole argument keeps the atom `e`, so `exp(2)` and `e**2` stay one value.
    assert_eq!(check("e**2", "e^x*e^(2-x)", E), decided(true, false));
    // The canonical form of a symbolic exponential.
    let x = Canon::Poly(BTreeMap::from([(monomial(&[(var("x"), 1)]), whole(1))]));
    let exponential = Canon::Poly(BTreeMap::from([(
        monomial(&[(Atom::Exp(Box::new(x)), 1)]),
        whole(1),
    )]));
    assert_eq!(form("e^x"), exponential);
    assert_eq!(form("exp(x)"), exponential);
    // C4: the exponent law must not accept a different exponent or a sign flip.
    assert_eq!(check("e^x", "e^(2x)", E), decided(false, false));
    assert_eq!(check("e^x", "e^(-x)", E), decided(false, false));
    assert_eq!(check("1/e^x", "e^x", E), decided(false, false));
    assert_eq!(check("e^(x^2)", "e^x", E), decided(false, false));
    assert_eq!(check("e^(2x)", "2*e^x", E), decided(false, false));
    assert_eq!(check("e^x*e^y", "e^x", E), decided(false, false));
}

#[test]
fn an_internal_space_collapses() {
    // Spec section 9.3 names "internal space collapse" as a True generator
    // family, and M2 review 2, finding 16, found it missing from the oracle
    // harness. The literals below are the four examples of that family, plus its
    // neighbors. Every verdict is the 1.0 verdict, measured with
    // `scripts/oracle/check_1_0.py`.
    // 1.0: True.
    run_table(&[
        ("1/2", "1 / 2", N, true),
        ("1+2x", "1 + 2 x", E, true),
        ("(4, 17)", "( 4 , 17 )", E, true),
        ("x^2", "x ^ 2", E, true),
        ("2x^2 - 3x + 1", "2 x ^ 2 - 3 x + 1", E, true),
        ("2x", "2 x", E, true),
        ("{1, 2}", "{ 1 , 2 }", E, true),
        ("1/(x+1)", "1 / ( x + 1 )", E, true),
        ("sqrt(2)/2", "sqrt ( 2 ) / 2", E, true),
        ("e^(x+2)", "e ^ ( x + 2 )", E, true),
        // A sign in front of the answer, and the same pair on both kinds.
        ("-1/2", "- 1 / 2", N, true),
        ("1/2", "1 / 2", E, true),
        ("x^2", "x ^ 2", N, true),
    ]);
    // C4: the space tolerance must admit no other value. 1.0: False for every
    // pair below.
    run_table(&[
        ("1/2", "1 / 3", N, false),
        ("1+2x", "1 + 3 x", E, false),
        ("(4, 17)", "( 17 , 4 )", E, false),
        ("x^2", "x ^ 3", E, false),
        ("2x", "2 y", E, false),
        ("-1/2", "1 / 2", N, false),
    ]);
}

#[test]
fn the_whole_part_of_an_exponent_is_the_atom_e() {
    // M2 review 2, finding 8. `e**(a+k)` for a whole `k` is `e**k * e**a`, so the
    // whole part of the argument folds into the atom `e`. Every pair below is a
    // 1.0 verdict, measured with `scripts/oracle/check_1_0.py`.
    // 1.0: True.
    run_table(&[
        ("e^(x+2)", "e^2*e^x", E, true),
        ("e^2*e^x", "e^(x+2)", E, true),
        ("e^(x+2)", "e^x*e^2", E, true),
        ("e^(x+2)", "e^2 e^x", E, true),
        ("e^(x+1)", "e*e^x", E, true),
        ("e^(x-1)", "e^x/e", E, true),
        ("e^(2x+2)", "e^2*e^(2x)", E, true),
        ("exp(x+2)", "exp(2)*exp(x)", E, true),
        ("2e^(x+2)", "2*e^2*e^x", E, true),
        // The same pair on the other answer kind.
        ("e^(x+2)", "e^2*e^x", N, true),
    ]);
    // The canonical form of `e**(x+2)`: the atom `e` with exponent 2, times the
    // exponential of `x`.
    let x = Canon::Poly(BTreeMap::from([(monomial(&[(var("x"), 1)]), whole(1))]));
    let folded = Canon::Poly(BTreeMap::from([(
        monomial(&[(Atom::E, 2), (Atom::Exp(Box::new(x)), 1)]),
        whole(1),
    )]));
    assert_eq!(form("e^(x+2)"), folded);
    assert_eq!(form("e^2*e^x"), folded);
    // C4: the fold must admit no other value. 1.0: False for every pair below.
    run_table(&[
        ("e^(x+2)", "e^(x+3)", E, false),
        ("e^(x+2)", "e^2*e^(2x)", E, false),
        ("e^(x+2)", "e^2+e^x", E, false),
        ("e^(x+2)", "e^x+2", E, false),
        ("e^(x+2)", "2*e^x", E, false),
        // The sign of the variable part, and the sign of the whole part.
        ("e^(2-x)", "e^2*e^x", E, false),
        ("e^(x+2)", "e^(x+2)*e", E, false),
        // A whole part is a whole number. One half stays inside the exponential.
        ("e^(x+1/2)", "e*e^x", E, false),
        ("e^(x+2)", "e^(x+3)", N, false),
    ]);
    // A fraction in the argument keeps its own exponential, and the two spellings
    // of it still meet.
    assert_eq!(form("e^(x+1/2)"), form("e^(1/2)*e^x"));
    assert_ne!(form("e^(x+1/2)"), form("e^(x+3/2)"));
}

#[test]
fn a_reciprocal_of_a_sum_is_one_denominator() {
    // The name changed with the form: M2 review 3 replaced the `Inverse` ATOM
    // with the quotient `Canon::Value { num, den }`, so a reciprocal is no longer
    // an atom of a monomial. The verdicts of M2 review 2, findings 13 and 17,
    // are unchanged: a power of a reciprocal is the reciprocal of the power, and
    // two reciprocals in one product are the reciprocal of the product. Every
    // verdict below is the 1.0 verdict, measured with
    // `scripts/oracle/check_1_0.py`.
    // 1.0: True.
    run_table(&[
        ("1/(x+1)^2", "(1/(x+1))^2", E, true),
        ("(1/(x+1))^2", "1/(x+1)^2", E, true),
        ("4/((x - 2)(x + 2))", "4/(x - 2) * 1/(x + 2)", E, true),
        ("1/((s - 2)(s - 5))", "(1/(s - 2))(1/(s - 5))", E, true),
        ("1/(x+1)^2", "1/(x+1) * 1/(x+1)", E, true),
        ("1/((x-2)(x+2))", "(1/(x-2))/(x+2)", E, true),
        ("1/((x-2)(x+2))", "1/(x^2-4)", E, true),
        // Three divisors, and one of them already merged.
        ("1/((x+1)(x+2)(x+3))", "1/(x+1) * 1/((x+2)(x+3))", E, true),
        // A divisor that comes back into the numerator.
        ("1/(1/(x+1))", "x+1", E, true),
        ("2/(1/(x+1))", "2x+2", E, true),
        // The two divisors cancel into a rational.
        ("1/(sqrt(2)+1)*1/(sqrt(2)-1)", "1", E, true),
        // The sign travels with the content, not with the divisor.
        ("1/(x+1)*1/(-x-1)", "-1/(x+1)^2", E, true),
        // A space around every operator, and the other answer kind.
        ("1/(x+1)^2", "1 / ( x + 1 ) ^ 2", E, true),
        ("1/(x+1)^2", "(1/(x+1))^2", N, true),
        ("(1/2)^2", "1/2^2", N, true),
    ]);
    // The canonical form of `1/(x+1)**2`: the numerator 1 over the expanded
    // denominator. M2 review 3 replaced the `Inverse` atom with this quotient.
    let reciprocal = Canon::Value {
        num: BTreeMap::from([(Monomial::new(), whole(1))]),
        den: BTreeMap::from([
            (monomial(&[(var("x"), 2)]), whole(1)),
            (monomial(&[(var("x"), 1)]), whole(2)),
            (Monomial::new(), whole(1)),
        ]),
    };
    assert_eq!(form("1/(x+1)^2"), reciprocal);
    assert_eq!(form("(1/(x+1))^2"), reciprocal);
    assert_eq!(form("1/(x+1) * 1/(x+1)"), reciprocal);
    // C4: the merge must admit no other value. 1.0: False for every pair below.
    run_table(&[
        ("1/(x+1)^2", "1/(x+1)", E, false),
        ("1/(x+1)", "1/(x+1)^2", E, false),
        ("1/((x-2)(x+2))", "1/((x-2)(x+3))", E, false),
        ("(1/(x+1))^2", "1/(x+1)^3", E, false),
        ("1/(x+1)^2", "-1/(x+1)^2", E, false),
        ("1/((x-2)(x+2))", "1/(x^2+4)", E, false),
        ("4/((x-2)(x+2))", "5/((x-2)(x+2))", E, false),
        ("1/(x+1)", "x+1", E, false),
        ("1/(x+1)^2", "1/(x+1)", N, false),
    ]);
    // The merge multiplies two divisors and it cancels no common factor, so the
    // documented narrowing stands: `(x**2-1)/(x-1)` and `x+1` stay two values.
    // The pair itself is pinned in `answer_divergence.rs`, with the 1.0 verdict.
    assert_ne!(form("(x**2-1)/(x-1)"), form("x+1"));
    assert_ne!(form("1/(x^2-1)"), form("1/(x-1)"));
}

#[test]
fn a_reciprocal_of_a_product_meets_a_product_of_reciprocals() {
    // M2 review 3, finding 10. A divisor that carries a common monomial factor
    // used to stay whole inside the `Inverse` atom, so `1/(x(x+h))` and
    // `1/x * 1/(x+h)` were two canonical forms of one value. The quotient form
    // clears every negative exponent into the denominator, so the two meet.
    // Every verdict below is the 1.0 verdict, measured with
    // `scripts/oracle/check_1_0.py`.
    // The canonical form of `1/(x*(x+h))`: the numerator 1 over the EXPANDED
    // denominator `x**2 + h*x`.
    let quotient = Canon::Value {
        num: BTreeMap::from([(Monomial::new(), whole(1))]),
        den: BTreeMap::from([
            (monomial(&[(var("x"), 2)]), whole(1)),
            (monomial(&[(var("h"), 1), (var("x"), 1)]), whole(1)),
        ]),
    };
    assert_eq!(form("1/(x(x+h))"), quotient);
    assert_eq!(form("1/x * 1/(x+h)"), quotient);
    assert_eq!(form("1/(x^2+xh)"), quotient);
    // 1.0: True for every pair below. The three corpus answers of the finding
    // are `-1/(x(x + h))` and `-2/(x(x + h))` (curriculum/calculus-1/
    // 01-derivative.yaml, difference-quotients kp3) and `1/(2√x (1 + x))`
    // (02-differentiation-rules.yaml, derivatives-inverse-trig kp2).
    run_table(&[
        ("-1/(x(x + h))", "-1/x * 1/(x + h)", E, true),
        ("-2/(x(x + h))", "-2/x * 1/(x + h)", E, true),
        ("1/(2√x (1 + x))", "1/(2√x) * 1/(1 + x)", E, true),
        ("-1/(x(x + h))", "(-1/x)/(x + h)", E, true),
        ("-1/(x(x + h))", "-(1/x)(1/(x+h))", E, true),
        ("-3/(x(x + h))", "-3/(x^2 + hx)", E, true),
        ("1/(x(x+1))", "1/x/(x+1)", E, true),
        ("1/(2x(x+1))", "1/(2x) * 1/(x+1)", E, true),
        ("h/(x(x+h))", "h/x * 1/(x+h)", E, true),
        ("x/(x(x+1))", "1/(x+1)", E, true),
        ("x^2/(x(x+1))", "x/(x+1)", E, true),
        // Three divisors, in the three groupings a learner writes.
        ("1/(x(x+1)(x+2))", "1/x * 1/(x+1) * 1/(x+2)", E, true),
        ("1/(x(x+1)(x+2))", "1/x * 1/((x+1)(x+2))", E, true),
        ("1/(x(x+1)(x+2))", "1/(x(x+1)) * 1/(x+2)", E, true),
        // A constant, a root, and an exponential in front of the divisor. A root
        // and an exponential never carry a negative exponent, so the denominator
        // alone gives their content.
        ("1/(e(x+1))", "1/e * 1/(x+1)", E, true),
        ("1/(pi(x+1))", "1/pi * 1/(x+1)", E, true),
        ("1/(sqrt(2)(x+1))", "1/sqrt(2) * 1/(x+1)", E, true),
        ("1/(√2(x+1))", "√2/(2(x+1))", E, true),
        ("2/(sqrt(2)(x+1))", "sqrt(2)/(x+1)", E, true),
        ("sqrt(2)/(sqrt(2)(x+1))", "1/(x+1)", E, true),
        ("1/(e^x(x+1))", "1/e^x * 1/(x+1)", E, true),
        ("1/(e^x(x+1))", "e^(-x)/(x+1)", E, true),
        ("1/(sin(x)(x+1))", "1/sin(x) * 1/(x+1)", E, true),
        ("e^x/(x+1)", "e^x * 1/(x+1)", E, true),
        ("pi/(x+1)", "pi * 1/(x+1)", E, true),
        ("√2/(x+1)", "√2 * 1/(x+1)", E, true),
        // The construct under a sign, a power, a division, and a function name.
        ("-1/(x(x+1))", "-(1/x * 1/(x+1))", E, true),
        ("1/(-x(x+1))", "-1/(x(x+1))", E, true),
        ("(1/(x(x+1)))^2", "1/(x(x+1))^2", E, true),
        ("(1/x * 1/(x+1))^2", "1/(x^2(x+1)^2)", E, true),
        ("(1/(sqrt(2)(x+1)))^3", "(1/sqrt(2) * 1/(x+1))^3", E, true),
        ("1/(x(x+1))/2", "1/(2x(x+1))", E, true),
        ("2/(1/(x(x+1)))", "2x^2+2x", E, true),
        ("sin(1/(x(x+1)))", "sin(1/x * 1/(x+1))", E, true),
        ("sqrt(1/(x(x+1)))", "sqrt(1/x * 1/(x+1))", E, true),
        ("log(1/(x(x+1)))", "log(1/x * 1/(x+1))", E, true),
        ("sin(1/(sqrt(2)(x+1)))", "sin(1/sqrt(2) * 1/(x+1))", E, true),
        // A space around every operator, a decimal coefficient, and both kinds.
        ("1/(x(x + h))", "1 / ( x ( x + h ) )", E, true),
        ("1/(2(x+1))", "0.5/(x+1)", E, true),
        ("-1/(x(x + h))", "-1/(x(x + h))", N, true),
        ("1/(x(x+1))", "1/x * 1/(x+1)", N, true),
    ]);
    // C4: the rule must admit no other value. 1.0: False for every pair below.
    // Each learner answer is a WRONG value in the same spelling.
    run_table(&[
        ("-1/(x(x + h))", "-1/x * 1/(x - h)", E, false),
        ("-1/(x(x + h))", "1/x * 1/(x + h)", E, false),
        ("-1/(x(x + h))", "-1/(x(x + h))^2", E, false),
        ("1/(x(x+h))", "1/(x(x-h))", E, false),
        ("1/(x(x+h))", "1/(x^2+2xh)", E, false),
        ("1/(2√x (1 + x))", "1/(2√x) * 1/(1 - x)", E, false),
        ("1/(x(x+1))", "-1/(x(x+1))", E, false),
        ("1/(x(x+1))", "1/(x(x+1)) + 1", E, false),
        ("1/(x^2+x)", "1/(x^2-x)", E, false),
        ("1/(x(x+1)(x+2))", "1/x * 1/(x+1) * 1/(x+3)", E, false),
        ("1/(sqrt(2)(x+1))", "1/sqrt(2) * 1/(x+2)", E, false),
        ("1/(sqrt(3)(x+1))", "1/sqrt(2) * 1/(x+1)", E, false),
        ("1/(e^x(x+1))", "1/e^x * 1/(x+2)", E, false),
        ("1/(e^x(x+1))", "e^(x)/(x+1)", E, false),
        ("1/(sin(x)(x+1))", "1/sin(x) * 1/(x+2)", E, false),
        ("1/(sqrt(2)x+1)", "1/(sqrt(2)(x+1))", E, false),
        ("1/(x(x+1))", "1/x * 1/(x+2)", N, false),
    ]);
    // A denominator of ONE term is negative exponents, not a quotient: `1/x`
    // stays a monomial and `1/(2x)` is one half of it.
    assert_eq!(
        form("1/x"),
        Canon::Poly(BTreeMap::from([(monomial(&[(var("x"), -1)]), whole(1))]))
    );
    assert_eq!(form("1/x"), form("x^-1"));
    assert_ne!(form("1/x"), form("1/x^2"));
    assert_eq!(check("1/(2x)", "0.5/x", E), decided(true, false));
    // The rule cancels a MONOMIAL factor and no polynomial factor. 1.0 answers
    // True for both pairs below and 2.0 answers False for the second: that is
    // the documented narrowing of the module header.
    assert_eq!(check("(x+1)/(x+1)", "1", E), decided(true, false));
    assert_eq!(check("(2x+2)/(x+1)", "2", E), decided(true, false));
    assert_eq!(check("x/(x+1) + 1/(x+1)", "1", E), decided(true, false));
    assert_ne!(form("1/(x+1) + 1/(x+1)^2"), form("(x+2)/(x+1)^2"));
    assert_ne!(form("(x+2)/(x+1)"), form("1"));
}

#[test]
fn a_scalar_ratio_needs_the_same_monomials_on_both_sides() {
    // M2 review 4, finding #4. Every rule-5 test used a numerator and a
    // denominator over the SAME monomials, so the key half of the guard
    // (`num.len() != den.len() || !num.keys().eq(den.keys())`) decided nothing
    // and its loss survived the whole suite. Without the key half,
    // `(x+1)/(y+1)` collapses to the number 1, and the learner answer `1`
    // grades correct against a quotient of two unrelated polynomials (C4).
    // 1.0 answers False for every `assert_ne!` pair below.
    //
    // Two sums of TWO terms each over DIFFERENT monomials. The count of terms
    // matches, so the key test is the deciding half.
    assert_ne!(form("(x+1)/(y+1)"), form("1"));
    assert_ne!(form("(x+2)/(y+2)"), form("1"));
    assert_ne!(form("(x+1)/(y+1)"), form("(x+2)/(y+2)"));
    assert_eq!(check("(x+1)/(y+1)", "1", E), decided(false, false));
    assert_eq!(check("1", "(x+1)/(y+1)", E), decided(false, false));
    // The rule itself still fires where the monomials DO match.
    assert_eq!(check("(2x+2)/(x+1)", "2", E), decided(true, false));
    assert_eq!(check("2", "(2x+2)/(x+1)", E), decided(true, false));
    assert_eq!(form("(2x+2)/(x+1)"), Canon::Rational(whole(2)));
    assert_eq!(form("(x+1)/(x+1)"), Canon::Rational(whole(1)));
    // The form runs no polynomial GCD, so a quotient that a common polynomial
    // factor would reduce stays two values. That is the documented narrowing of
    // the module header, and 1.0 answers True for this pair.
    assert_ne!(form("(x^2+x)/(x+1)"), form("x"));
    assert_eq!(check("(x^2+x)/(x+1)", "x", E), decided(false, false));
    // Same monomials, and no rational multiple: the coefficient half decides.
    assert_ne!(form("(x+y)/(x-y)"), form("(x-y)/(x+y)"));
    assert_ne!(form("(x+y)/(x-y)"), form("1"));
    assert_eq!(
        check("(x+y)/(x-y)", "(x-y)/(x+y)", E),
        decided(false, false)
    );
}

#[test]
fn a_sum_of_two_quotients_goes_over_the_common_denominator() {
    // M2 review 3, finding 13. Two terms with two different divisors were never
    // put over one denominator, so 16 authored corpus answers were two values
    // apart from their own combined spelling. Every verdict below is the 1.0
    // verdict, measured with `scripts/oracle/check_1_0.py`.
    //
    // The 16 authored corpus answers of the finding, in both spellings. The
    // topic of each answer follows it.
    run_table(&[
        // derivatives-natural-log (curriculum/calculus-1/03-transcendental.yaml).
        ("2/x + 1/(x + 1)", "(3x + 2)/(x(x + 1))", E, true),
        ("(3x + 2)/(x(x + 1))", "2/x + 1/(x + 1)", E, true),
        ("2/x - 1/(x + 3)", "(x + 6)/(x(x + 3))", E, true),
        ("3/x + 2x", "(2x^2 + 3)/x", E, true),
        ("5e^x - 2/x", "(5x e^x - 2)/x", E, true),
        // adding-subtracting-rational-expressions.
        ("4/((x - 2)(x + 2))", "1/(x - 2) - 1/(x + 2)", E, true),
        (
            "(5x - 1)/((x + 1)(x - 1))",
            "3/(x + 1) + 2/(x - 1)",
            E,
            true,
        ),
        (
            "(5x - 9)/((x + 3)(x - 3))",
            "4/(x + 3) + 1/(x - 3)",
            E,
            true,
        ),
        ("(x + 3)/((x + 1)(x + 2))", "2/(x + 1) - 1/(x + 2)", E, true),
        (
            "(3x + 8)/((x - 4)(x + 4))",
            "5/(2(x - 4)) + 1/(2(x + 4))",
            E,
            true,
        ),
        ("(2x + 3)/x^2", "2/x + 3/x^2", E, true),
        ("(3 + x)/(3x)", "1/3 + 1/x", E, true),
        // complex-fractions.
        ("(x + 1)/(x - 1)", "1 + 2/(x - 1)", E, true),
        ("(3 - x)/(3 + x)", "-1 + 6/(x + 3)", E, true),
        // dividing-rational-expressions.
        ("(x + 2)/(x - 2)", "1 + 4/(x - 2)", E, true),
        // multiplying-dividing-rational-expressions.
        ("(x - 3)/(x + 1)", "1 - 4/(x + 1)", E, true),
        // The four further topics the finding names, one answer each.
        // rational-expressions-common-denominators.
        ("2x/(x + 1)", "2 - 2/(x + 1)", E, true),
        ("(5x - 1)/(x - 3)", "5 + 14/(x - 3)", E, true),
        // rational-expressions.
        ("(x - 2)/(x + 2)", "1 - 4/(x + 2)", E, true),
        ("(x + 2)/(x + 3)", "1 - 1/(x + 3)", E, true),
        // multiplying-rational-expressions.
        ("(x + 2)/x", "1 + 2/x", E, true),
        ("(x + 3)/(x - 2)", "1 + 5/(x - 2)", E, true),
        // difference-quotients.
        ("1/x + 1/(x + h)", "(2x + h)/(x(x + h))", E, true),
        // Two equal denominators stay one denominator, and a sum that cancels
        // is zero.
        ("1/(x+1) + 1/(x+1)", "2/(x+1)", E, true),
        ("1/(x+1) - 1/(x+1)", "0", E, true),
        ("1/(x-1) + 1/(1-x)", "0", E, true),
        ("(a+b)/(a b)", "1/a + 1/b", E, true),
        // The same pairs on the other answer kind.
        ("2/x + 1/(x + 1)", "(3x + 2)/(x(x + 1))", N, true),
        ("(2x + 3)/x^2", "2/x + 3/x^2", N, true),
        ("4/((x - 2)(x + 2))", "1/(x - 2) - 1/(x + 2)", N, true),
    ]);
    // The canonical form of `2/x + 1/(x+1)`: the expanded numerator `3*x + 2`
    // over the expanded denominator `x**2 + x`.
    let combined = Canon::Value {
        num: BTreeMap::from([
            (monomial(&[(var("x"), 1)]), whole(3)),
            (Monomial::new(), whole(2)),
        ]),
        den: BTreeMap::from([
            (monomial(&[(var("x"), 2)]), whole(1)),
            (monomial(&[(var("x"), 1)]), whole(1)),
        ]),
    };
    assert_eq!(form("2/x + 1/(x + 1)"), combined);
    assert_eq!(form("(3x + 2)/(x(x + 1))"), combined);
    assert_eq!(form("(3x + 2)/(x^2 + x)"), combined);
    // C4: the common denominator must admit no other value. 1.0: False for every
    // pair below. Each learner answer is a wrong value in the same spelling.
    run_table(&[
        ("2/x + 1/(x + 1)", "(3x + 3)/(x(x + 1))", E, false),
        ("2/x + 1/(x + 1)", "(3x + 2)/(x(x - 1))", E, false),
        ("2/x + 1/(x + 1)", "(2x + 3)/(x(x + 1))", E, false),
        ("2/x - 1/(x + 3)", "(x + 6)/(x(x - 3))", E, false),
        ("2/x - 1/(x + 3)", "(x - 6)/(x(x + 3))", E, false),
        ("4/((x - 2)(x + 2))", "1/(x - 2) + 1/(x + 2)", E, false),
        (
            "(5x - 1)/((x + 1)(x - 1))",
            "2/(x + 1) + 3/(x - 1)",
            E,
            false,
        ),
        ("(x + 2)/(x - 2)", "1 + 4/(x + 2)", E, false),
        ("(x + 2)/(x - 2)", "1 - 4/(x - 2)", E, false),
        ("2x/(x + 1)", "2 + 2/(x + 1)", E, false),
        ("(2x + 3)/x^2", "2/x + 3/x", E, false),
        ("(3 + x)/(3x)", "1/3 + 1/(3x)", E, false),
        ("1/x + 1/(x + h)", "(2x + h)/(x(x - h))", E, false),
        ("(a+b)/(a b)", "1/a - 1/b", E, false),
        ("2/x + 1/(x + 1)", "(3x + 3)/(x(x + 1))", N, false),
    ]);
}

#[test]
fn the_integer_part_of_a_fractional_exponent_is_the_atom_e() {
    // M2 review 3, finding 12. The `Atom::E` fold fired only for a WHOLE
    // constant term, so the exponent law failed for a fractional exponent. The
    // fold now takes the integer part, and the integer part is the FLOOR, so a
    // negative exponent has one spelling as well. Every verdict below is the 1.0
    // verdict, measured with `scripts/oracle/check_1_0.py`.
    // 1.0: True.
    run_table(&[
        ("e^(5/2)", "e^2*e^(1/2)", E, true),
        ("e^(3/2)", "e*e^(1/2)", E, true),
        ("e^(x+5/2)", "e^2*e^(x+1/2)", E, true),
        ("e^(x+3/2)", "e*e^(x+1/2)", E, true),
        ("e^(5/2)", "e^(1/2)*e^2", E, true),
        ("e^(5/2)", "e^2 e^(1/2)", E, true),
        ("exp(5/2)", "exp(2)*exp(1/2)", E, true),
        ("e^(7/2)", "e^3*e^(1/2)", E, true),
        ("e^(7/2)", "e^2*e^(3/2)", E, true),
        ("2e^(5/2)", "2*e^2*e^(1/2)", E, true),
        // The floor is what makes the two spellings of a negative exponent meet.
        ("e^(-5/2)", "e^-3*e^(1/2)", E, true),
        ("e^(-5/2)", "e^-2*e^(-1/2)", E, true),
        ("e^(-3/2)", "e^-2*e^(1/2)", E, true),
        ("e^(-1/2)", "1/e^(1/2)", E, true),
        ("1/e^(5/2)", "e^-3*e^(1/2)", E, true),
        // The construct under a division, a product, a power, and a function.
        ("e^(5/2)/2", "e^2*e^(1/2)/2", E, true),
        ("e^(5/2)*x", "x*e^2*e^(1/2)", E, true),
        ("(e^(5/2))^2", "e^5", E, true),
        ("sin(e^(5/2))", "sin(e^2*e^(1/2))", E, true),
        // The integer half of the fold, which FIXM2e added, still holds.
        ("e^(x+2)", "e^2*e^x", E, true),
        ("e^(x+1/2)", "e^(1/2)*e^x", E, true),
        // The same pair on the other answer kind.
        ("e^(5/2)", "e^2*e^(1/2)", N, true),
    ]);
    // The canonical form of `e**(5/2)`: the atom `e` with exponent 2, times the
    // exponential of one half. `e**(-5/2)` takes the floor, so it is the atom
    // `e` with exponent -3 times the same exponential.
    let root_of_e = Canon::Poly(BTreeMap::from([(
        monomial(&[
            (Atom::E, 2),
            (Atom::Exp(Box::new(Canon::Rational(ratio(1, 2)))), 1),
        ]),
        whole(1),
    )]));
    assert_eq!(form("e^(5/2)"), root_of_e);
    assert_eq!(form("e^2*e^(1/2)"), root_of_e);
    let negative = Canon::Poly(BTreeMap::from([(
        monomial(&[
            (Atom::E, -3),
            (Atom::Exp(Box::new(Canon::Rational(ratio(1, 2)))), 1),
        ]),
        whole(1),
    )]));
    assert_eq!(form("e^(-5/2)"), negative);
    assert_eq!(form("e^-2*e^(-1/2)"), negative);
    // C4: the fold must admit no other value. 1.0: False for every pair below.
    run_table(&[
        ("e^(5/2)", "e^2*e^(3/2)", E, false),
        ("e^(5/2)", "e^3*e^(1/2)", E, false),
        ("e^(5/2)", "e^2+e^(1/2)", E, false),
        ("e^(5/2)", "e^(5/3)", E, false),
        ("e^(5/2)", "e^(2/5)", E, false),
        ("e^(5/2)", "-e^2*e^(1/2)", E, false),
        ("e^(x+5/2)", "e^2*e^(x+3/2)", E, false),
        ("e^(x+5/2)", "e^3*e^(x+1/2)", E, false),
        ("e^(x+5/2)", "e^2*e^(2x+1/2)", E, false),
        ("e^(x+1/2)", "e*e^x", E, false),
        ("e^(x+2)", "e^(x+3)", E, false),
    ]);
}

/// Build the canonical form of one labeled whole number, `<var> = <value>`.
fn labeled(var: &str, value: i64) -> Canon {
    let ast = Ast::Assign {
        var: var.to_string(),
        value: Box::new(Ast::Integer(BigInt::from(value))),
    };
    canon(&ast).unwrap_or_else(|e| panic!("{var} = {value}: {}", e.reason))
}

#[test]
fn a_value_label_names_the_unknown_it_answers_for() {
    // M2 review 1, the ruling on findings 2, 10, and 16. A leading `x =` is a
    // label. 1.0 answers False for `x = 4` against `y = 4`.
    assert_eq!(
        labeled("x", 4),
        Canon::Assign {
            var: "x".to_string(),
            value: Box::new(Canon::Rational(whole(4))),
        }
    );
    assert!(same_answer(&labeled("x", 4), &labeled("x", 4)));
    // Two labels: the two names are one name, casefolded, or the answer is wrong.
    assert!(same_answer(&labeled("x", 4), &labeled("X", 4)));
    assert!(!same_answer(&labeled("x", 4), &labeled("y", 4)));
    assert!(!same_answer(&labeled("y", 4), &labeled("x", 4)));
    assert!(!same_answer(&labeled("x", 4), &labeled("x", 5)));
    // One label only: the label falls away (the V4 tolerance of M2.md).
    let four = Canon::Rational(whole(4));
    assert!(same_answer(&four, &labeled("y", 4)));
    assert!(same_answer(&labeled("y", 4), &four));
    assert!(!same_answer(&four, &labeled("y", 5)));
    assert!(!same_answer(&labeled("y", 5), &four));
}

#[test]
fn division_by_a_sum_normalizes_the_content() {
    assert_eq!(check("1/(x+1)", "2/(2*x+2)", E), decided(true, false));
    assert_eq!(check("1/(x+1)", "-1/(-x-1)", E), decided(true, false));
    assert_eq!(check("1/(x+1)", "1/(x+2)", E), decided(false, false));
}

#[test]
fn the_dot_thousands_hole_of_spec_7_2_stays_closed() {
    // `_DOT_GROUPS_RE` needs a non-zero leading group, so a 1000x slip is wrong.
    assert_eq!(check("8", "0.008", N), decided(false, false));
    assert_eq!(check("8", "8.000", N), decided(true, false));
    assert_eq!(check("0.5", "0,5", N), decided(false, false));
}

// ---------------------------------------------------------------------------
// The corpus: coverage and the L2 budget
// ---------------------------------------------------------------------------

/// One corpus row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
struct CorpusRow {
    answer: String,
    answer_kind: String,
}

/// Read the answer corpus.
fn corpus() -> Vec<CorpusRow> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/answers/corpus_1_0.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("row {line}: {e}")))
        .collect()
}

/// Read the answer kind of a corpus row.
fn kind_of(row: &CorpusRow) -> AnswerKind {
    match row.answer_kind.as_str() {
        "numeric" => AnswerKind::Numeric,
        "expression" => AnswerKind::Expression,
        other => panic!("the corpus holds only verifiable kinds, and this row is {other}"),
    }
}

#[test]
fn every_answer_the_grammar_accepts_also_canonicalizes() {
    let corpus = corpus();
    assert_eq!(corpus.len(), 3_492, "the corpus is 3,492 answers");
    let mut canonical = 0_usize;
    let mut refused: Vec<(&str, &'static str)> = Vec::new();
    for row in &corpus {
        match canonical_form(&row.answer) {
            Ok(_) => canonical += 1,
            Err(reason) => refused.push((&row.answer, reason.reason)),
        }
    }
    // FIXM2a pins 265 answers as outside the grammar (`undecidable_1_0.jsonl`),
    // so 3,492 - 265 = 3,227 answers parse. Every one of them canonicalizes.
    assert_eq!(
        canonical,
        3_227,
        "the first refusals are {:?}",
        refused.iter().take(5).collect::<Vec<_>>()
    );
}

#[test]
fn the_corpus_self_check_holds_the_l2_budget() {
    let corpus = corpus();
    let mut worst = Duration::ZERO;
    let mut worst_answer = "";
    let start = Instant::now();
    for row in &corpus {
        let one = Instant::now();
        let outcome = check(&row.answer, &row.answer, kind_of(row));
        let elapsed = one.elapsed();
        if elapsed > worst {
            worst = elapsed;
            worst_answer = &row.answer;
        }
        assert_eq!(
            outcome,
            decided(true, false),
            "an answer must equal itself: {:?}",
            row.answer
        );
    }
    let total = start.elapsed();
    let corpus_budget = corpus_budget();
    assert!(
        total < corpus_budget,
        "the 3,492 self-checks took {total:?}, and the budget is {corpus_budget:?}"
    );
    let one_check_budget = one_check_budget();
    assert!(
        worst < one_check_budget,
        "the longest single check took {worst:?} on {worst_answer:?}, \
         and the budget is {one_check_budget:?}"
    );
}

#[test]
fn the_corpus_canonicalization_holds_the_l2_budget() {
    // The self-check above stops on the string rung, so it never reaches the
    // arithmetic. This test drives the whole path and asserts the same budget.
    //
    // Measured after the rational-function form of M2 review 3 (release build,
    // this machine): 11.3 ms for the 3,492 answers, and 129.9 µs for the worst
    // one, `24(2x + 1)^2 (1 + (2x + 1)^3)^3`. The worst rational-function PAIR
    // is `(3x + 8)/((x - 4)(x + 4))` against `5/(2(x - 4)) + 1/(2(x + 4))` at
    // 47.2 µs, so the new products cost about one third of the worst answer.
    let corpus = corpus();
    let mut worst = Duration::ZERO;
    let mut worst_answer = "";
    let start = Instant::now();
    for row in &corpus {
        let one = Instant::now();
        let _ = canonical_form(&row.answer);
        let elapsed = one.elapsed();
        if elapsed > worst {
            worst = elapsed;
            worst_answer = &row.answer;
        }
    }
    let total = start.elapsed();
    let corpus_budget = corpus_budget();
    assert!(
        total < corpus_budget,
        "the 3,492 canonicalizations took {total:?}, and the budget is {corpus_budget:?}"
    );
    let one_check_budget = one_check_budget();
    assert!(
        worst < one_check_budget,
        "the longest canonicalization took {worst:?} on {worst_answer:?}, \
         and the budget is {one_check_budget:?}"
    );
}

/// Build the reciprocal bomb of M2 review 1, finding 5.
///
/// The answer is `1/(a/3**250 + b/5**250 + …)`: every term carries a coprime
/// denominator, so the least common multiple of the content normalization grows
/// by about 580 bits per term. The shape is inside the section 8.1 grammar and
/// inside the 4,000-character input cap.
fn reciprocal_bomb(terms: usize) -> String {
    const NAMES: [&str; 40] = [
        "a", "b", "c", "d", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s",
        "u", "v", "w", "x", "y", "z", "A", "B", "C", "D", "F", "G", "H", "I", "J", "K", "L", "M",
        "N", "O", "P", "Q",
    ];
    const PRIMES: [u32; 40] = [
        3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89,
        97, 101, 103, 107, 109, 113, 127, 131, 137, 139, 149, 151, 157, 163, 167, 173, 179,
    ];
    let mut parts: Vec<String> = Vec::new();
    for index in 0..terms {
        let name = NAMES[index % NAMES.len()];
        let prime = PRIMES[index % PRIMES.len()];
        let power = 1 + index / NAMES.len();
        let numerator = if power == 1 {
            name.to_string()
        } else {
            format!("{name}**{power}")
        };
        parts.push(format!("{numerator}/{prime}**250"));
    }
    // The last term brings the answer to the 2,072 characters of the review.
    parts.push("R/2**11".to_string());
    format!("1/({})", parts.join(" + "))
}

#[test]
fn the_reciprocal_bomb_of_review_1_is_refused_inside_the_budget() {
    // M2 review 1, findings 5 and 11. The content normalization folded an
    // unbounded least common multiple, and this one answer cost 8.4 s of CPU in
    // a release build. The fold now runs the size bound and the width charge
    // after every step.
    let bomb = reciprocal_bomb(143);
    assert_eq!(bomb.chars().count(), 2_072, "the reviewer's answer length");
    let start = Instant::now();
    let outcome = check("1", &bomb, E);
    let elapsed = start.elapsed();
    assert!(
        matches!(outcome, Outcome::Undecidable(_)),
        "the bomb gave {outcome:?}"
    );
    let one_check_budget = one_check_budget();
    assert!(
        elapsed < one_check_budget,
        "the 2,072-character bomb took {elapsed:?}, and the budget is {one_check_budget:?}"
    );
}

/// Build the sum-rebuild bomb of M2 review 4, finding 2.
///
/// The answer is one bracket that holds the 280-term sum `e**2 + … + e**281`,
/// and `factors` factors of `1` follow it. Every factor costs one node step, and
/// every factor rebuilt all 280 terms three times over: once in `frac_of`, once
/// in the `is_one` short cut of `poly_mul`, and once in the demotion that
/// `value` runs. `MAX_STEPS` bounded the COUNT of operations and not the COST of
/// one, so 846 factors cost 188 ms of canonicalization in a release build.
///
/// The shape is inside the section 8.1 grammar, holds no LaTeX and no glyph, and
/// stays inside the 4,000-character input cap.
fn sum_rebuild_bomb(factors: usize) -> String {
    let mut bomb = wide_sum(280);
    for _ in 0..factors {
        bomb.push_str("*1");
    }
    bomb
}

/// Build one bracketed sum of `terms` powers of `e`.
fn wide_sum(terms: usize) -> String {
    let parts: Vec<String> = (2..2 + terms).map(|power| format!("e^{power}")).collect();
    format!("({})", parts.join("+"))
}

#[test]
fn the_sum_rebuild_bomb_of_review_4_is_refused_inside_the_budget() {
    // M2 review 4, finding 2. The work bound charges one step per term the
    // arithmetic touches now, so the rebuild of a wide sum costs what it is
    // worth. The reviewer's pair cost 378 ms in a release build, which is 1.26
    // times the whole 300 ms of L2 for one deterministic grade.
    let expected = sum_rebuild_bomb(846);
    assert_eq!(
        expected.chars().count(),
        3_267,
        "the reviewer's first answer length"
    );
    let learner = expected.replacen('(', "(0+", 1);
    assert_eq!(
        learner.chars().count(),
        3_269,
        "the reviewer's second answer length"
    );
    let budget = bomb_budget();
    let start = Instant::now();
    let outcome = check(&expected, &learner, E);
    let elapsed = start.elapsed();
    match &outcome {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("the bomb gave {other:?}"),
    }
    assert!(
        elapsed < budget,
        "the 3,267-character pair took {elapsed:?}, and the budget is {budget:?}"
    );
    // The learner side alone reaches the same cost with a short authored answer,
    // and the learner writes that side.
    let learner_bomb = sum_rebuild_bomb(1_212);
    assert_eq!(
        learner_bomb.chars().count(),
        3_999,
        "the learner-side answer length"
    );
    let start = Instant::now();
    let outcome = check("42", &learner_bomb, N);
    let elapsed = start.elapsed();
    match &outcome {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("the learner-side bomb gave {other:?}"),
    }
    assert!(
        elapsed < budget,
        "the 3,999-character learner answer took {elapsed:?}, and the budget is {budget:?}"
    );
}

#[test]
fn a_wide_sum_costs_its_terms_and_a_narrow_one_does_not() {
    // M2 review 4, finding 2. The charge is one step per term touched, so an
    // ordinary answer of a few terms keeps the cost it had, and a sum of
    // hundreds of terms pays for every rebuild. The two assertions below hold
    // the rule in both directions.
    //
    // A 20-term sum with one factor of `1` is inside the budget and decides.
    let sum = wide_sum(20);
    assert_eq!(
        check(&sum, &format!("{sum}*1"), E),
        decided(true, false),
        "one factor beside a 20-term sum"
    );
    // The same sum with 100 factors is past the budget.
    let wide = format!("{sum}{}", "*1".repeat(100));
    match check("42", &wide, N) {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("100 factors beside a 20-term sum gave {other:?}"),
    }
    // The rule holds for a polynomial sum too, and a polynomial takes another
    // path through the promotion than a sum of powers of `e` takes.
    let powers: Vec<String> = (2..22).map(|power| format!("x^{power}")).collect();
    let polynomial = format!("({})", powers.join("+"));
    assert_eq!(
        check(&polynomial, &format!("{polynomial}*1"), E),
        decided(true, false),
        "one factor beside a 20-term polynomial"
    );
    let wide_polynomial = format!("{polynomial}{}", "*1".repeat(100));
    match check("42", &wide_polynomial, N) {
        Outcome::Undecidable(refused) => {
            assert_eq!(refused.reason, "the answer goes past the work bound");
        }
        other => panic!("100 factors beside a 20-term polynomial gave {other:?}"),
    }
    // A two-term sum keeps its verdict through the same count of factors.
    let narrow = format!("(x+1){}", "*1".repeat(100));
    assert_eq!(check("x+1", &narrow, E), decided(true, false));
}

#[test]
fn a_wide_coefficient_costs_more_than_a_narrow_one() {
    // M2 review 1, finding 11. `MAX_STEPS` charged term operations only, so a
    // 20-character answer spent 155 ms in a release build on 4,096-bit
    // coefficients. The budget now charges the width of every number it builds.
    for bomb in [
        "((7/3)**23*x+y)**616",
        "(x+(7/3)**23)**512",
        "(2*x+3*y)**900",
    ] {
        let start = Instant::now();
        let outcome = check("1", bomb, E);
        let elapsed = start.elapsed();
        assert!(
            matches!(outcome, Outcome::Undecidable(_)),
            "{bomb:?} gave {outcome:?}"
        );
        let one_check_budget = one_check_budget();
        assert!(
            elapsed < one_check_budget,
            "{bomb:?} took {elapsed:?}, and the budget is {one_check_budget:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The checker never panics
// ---------------------------------------------------------------------------

/// A deterministic xorshift generator. The fuzz needs no crate for this.
struct Rng(u64);

impl Rng {
    /// Return the next pseudo-random word.
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }

    /// Return a value below `bound`.
    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        let bound = u64::try_from(bound).unwrap_or(u64::MAX);
        usize::try_from(self.next() % bound).unwrap_or(0)
    }
}

/// The characters a hostile learner reaches for.
const FUZZ_ALPHABET: [&str; 40] = [
    "0", "1", "9", ".", ",", "+", "-", "*", "/", "**", "^", "(", ")", "[", "]", "{", "}", "<", "=",
    ">", "x", "y", "sqrt", "sin", "log", "exp", "pi", "e", "theta", "%", "$", "\\frac", "\\sqrt",
    "√", "π", "∞", "≤", "²", " ", "\u{202f}",
];

/// Build one random string from the alphabet.
fn fuzz_string(rng: &mut Rng, tokens: usize) -> String {
    let mut out = String::new();
    for _ in 0..tokens {
        let index = rng.below(FUZZ_ALPHABET.len());
        out.push_str(FUZZ_ALPHABET.get(index).copied().unwrap_or("0"));
    }
    out
}

/// Build one random answer: half raw bytes, half tokens of the alphabet.
fn fuzz_case(rng: &mut Rng) -> String {
    if rng.next().is_multiple_of(2) {
        let length = 1 + rng.below(64);
        fuzz_bytes(rng, length)
    } else {
        let tokens = 1 + rng.below(24);
        fuzz_string(rng, tokens)
    }
}

/// Build one random byte string, read as lossy UTF-8.
fn fuzz_bytes(rng: &mut Rng, length: usize) -> String {
    let mut bytes = Vec::with_capacity(length);
    for _ in 0..length {
        bytes.push(u8::try_from(rng.next() % 256).unwrap_or(0));
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn the_checker_never_panics_on_random_input() {
    let mut rng = Rng(0x2026_0826_u64 | 1);
    let start = Instant::now();
    let mut cases = 0_u64;
    while start.elapsed() < Duration::from_secs(10) {
        for _ in 0..64 {
            let expected = fuzz_case(&mut rng);
            let learner = fuzz_case(&mut rng);
            let kind = if rng.next().is_multiple_of(2) { N } else { E };
            let probe = (expected.clone(), learner.clone(), kind);
            let result = std::panic::catch_unwind(move || check(&probe.0, &probe.1, probe.2));
            assert!(
                result.is_ok(),
                "the checker panicked on {expected:?} against {learner:?} on {kind}"
            );
            cases += 1;
        }
    }
    assert!(cases > 1_000, "the fuzz ran only {cases} cases");
}

#[test]
fn an_answer_past_the_input_cap_is_undecidable() {
    let long = "1".repeat(4_001);
    assert!(matches!(check("1", &long, N), Outcome::Undecidable(_)));
    assert!(matches!(check(&long, "1", N), Outcome::Undecidable(_)));
    let at_cap = "1".repeat(4_000);
    assert_eq!(check(&at_cap, &at_cap, N), decided(true, false));
}
