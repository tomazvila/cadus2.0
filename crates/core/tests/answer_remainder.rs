//! The quotient-and-remainder production of D-F3 (unit f2-grammar, V1, C4).
//!
//! `9 R2`, `9 R 2`, `9R2`, `x + 2 remainder 3`, and the authored tuple `(9, 2)`
//! all read into one canonical pair. The parse tests pin the tree, the canon
//! tests pin the form, and the check tests pin the verdicts on both answer
//! kinds. `docs/reference/undecidable-answers.md` names the 16 corpus rows the
//! production recovers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::grammar::*;

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------
#[test]
fn every_spelling_of_a_quotient_with_a_remainder_is_one_tuple() {
    let nine_two = Ast::Tuple(vec![int(9), int(2)]);
    assert_eq!(ast("9 R2"), nine_two);
    assert_eq!(ast("9 R 2"), nine_two);
    assert_eq!(ast("9R2"), nine_two);
    assert_eq!(ast("9 remainder 2"), nine_two);
    assert_eq!(ast("(9, 2)"), nine_two);
    assert_eq!(ast("9, 2"), nine_two);
    // The corpus spellings.
    assert_eq!(ast("241 R2"), Ast::Tuple(vec![int(241), int(2)]));
    assert_eq!(ast("23 R14"), Ast::Tuple(vec![int(23), int(14)]));
    assert_eq!(
        ast("x + 2 remainder 3"),
        Ast::Tuple(vec![Ast::Add(vec![v("x"), int(2)]), int(3)])
    );
    assert_eq!(
        ast("2x^2 + 4x + 5 remainder 11"),
        Ast::Tuple(vec![
            Ast::Add(vec![
                Ast::Mul(vec![int(2), Ast::Pow(Box::new(v("x")), 2)]),
                Ast::Mul(vec![int(4), v("x")]),
                int(5),
            ]),
            int(11),
        ])
    );
    assert_eq!(
        ast("x^2 - 2x remainder 6"),
        Ast::Tuple(vec![
            Ast::Add(vec![
                Ast::Pow(Box::new(v("x")), 2),
                Ast::Neg(Box::new(Ast::Mul(vec![int(2), v("x")]))),
            ]),
            int(6),
        ])
    );
    // The word marker takes an expression on both sides.
    assert_eq!(
        ast("x + 2 remainder x - 1"),
        Ast::Tuple(vec![
            Ast::Add(vec![v("x"), int(2)]),
            Ast::Add(vec![v("x"), Ast::Neg(Box::new(int(1)))]),
        ])
    );
    // A remainder that is not smaller than the divisor is no concern here.
    assert_eq!(ast("3 R7"), Ast::Tuple(vec![int(3), int(7)]));
    assert_eq!(
        ast("-9 R2"),
        Ast::Tuple(vec![Ast::Neg(Box::new(int(9))), int(2)])
    );
}

#[test]
fn the_letter_r_stays_a_variable_outside_the_marker_position() {
    assert_eq!(ast("R"), v("R"));
    assert_eq!(ast("2R"), Ast::Mul(vec![int(2), v("R")]));
    assert_eq!(ast("2 R"), Ast::Mul(vec![int(2), v("R")]));
    assert_eq!(ast("2 R x"), Ast::Mul(vec![int(2), v("R"), v("x")]));
    assert_eq!(ast("R 2"), Ast::Mul(vec![v("R"), int(2)]));
    assert_eq!(ast("x R 2"), Ast::Mul(vec![v("x"), v("R"), int(2)]));
    // The lower-case `r` is never the marker (the task rule `q r r`).
    assert_eq!(refusal("9 r2"), "a number glued to a name reads as a label");
    assert_eq!(ast("9 r 2"), Ast::Mul(vec![int(9), v("r"), int(2)]));
    // A marker with nothing after it, or a second marker, is refused.
    assert_eq!(
        refusal("9 remainder"),
        "the answer ends where a value belongs"
    );
    assert_eq!(refusal("9 R2 R3"), "trailing text after the answer");
    assert_eq!(
        refusal("9 remainder 2 remainder 3"),
        "trailing text after the answer"
    );
    assert_eq!(
        refusal("remainder 3"),
        "a name that is not a function or variable"
    );
}

// ---------------------------------------------------------------------------
// Canon
// ---------------------------------------------------------------------------
#[test]
fn every_spelling_reads_into_one_canonical_pair() {
    for spelling in ["9 R2", "9 R 2", "9R2", "9 remainder 2", "(9, 2)", "9, 2"] {
        assert_eq!(form(spelling), pair(9, 2), "{spelling}");
    }
    assert_eq!(form("9 R3"), pair(9, 3));
    assert_ne!(form("9 R2"), pair(9, 3));
    assert_ne!(form("9 R2"), pair(2, 9));
    assert_eq!(
        form("x + 2 remainder 3"),
        Canon::Tuple(vec![form("x + 2"), Canon::Rational(whole(3))])
    );
    assert_eq!(form("x + 2 remainder 3"), form("(x + 2, 3)"));
    assert_eq!(form("x + 2 remainder 3"), form("(2 + x, 3)"));
    // The members compare by value, so `9 R 2.0` is the same pair.
    assert_eq!(form("9 R 2.0"), pair(9, 2));
    assert_eq!(form("9.0 R2"), pair(9, 2));
}

// ---------------------------------------------------------------------------
// Check
// ---------------------------------------------------------------------------
#[test]
fn the_task_pairs_of_d_f3_are_decided_on_both_kinds() {
    run_table(&[
        ("9 R2", "9 R2", N, true),
        ("9 R2", "9 R 2", N, true),
        ("9 R2", "9R2", N, true),
        ("9 R2", "9 remainder 2", N, true),
        ("9 R2", "(9, 2)", N, true),
        ("9 R2", "9, 2", N, true),
        ("(9, 2)", "9 R2", N, true),
        ("9 R2", "9 R3", N, false),
        ("9 R2", "8 R2", N, false),
        ("9 R2", "2 R9", N, false),
        ("9 R2", "9", N, false),
        ("9 R2", "11", N, false),
        ("9 R2", "9.2", N, false),
        ("9 R2", "9 R 2", E, true),
        ("241 R2", "241 R 2", N, true),
        ("23 R14", "23 R 14", N, true),
        ("x + 2 remainder 3", "x + 2 remainder 3", E, true),
        ("x + 2 remainder 3", "(x + 2, 3)", E, true),
        ("x + 2 remainder 3", "2 + x remainder 3", E, true),
        ("x + 2 remainder 3", "x + 2 R 3", E, true),
        ("x + 2 remainder 3", "x + 2 remainder 4", E, false),
        ("2x^2 + 4x + 5 remainder 11", "(2x^2 + 4x + 5, 11)", E, true),
        ("2x^2 + 4x + 5 remainder 11", "2x^2 + 4x + 5", E, false),
    ]);
    // A `9 R 2` learner spelling for the authored tuple, on both kinds.
    assert_eq!(check("(9, 2)", "9 R 2", E), decided(true, false));
    assert_eq!(check("(9, 2)", "9 r 2", E), decided(false, false));
}

#[test]
fn the_lower_case_marker_gets_no_pair_reading() {
    // The string rung reads case-free, so `9 r2` is the string `9 R2` (rung 2).
    assert_eq!(check("9 R2", "9 r2", N), decided(true, false));
    // Against the authored tuple, `9 r2` is the label refusal (V2).
    assert_undecidable(
        "(9, 2)",
        "9 r2",
        N,
        "a number glued to a name reads as a label",
    );
    // `9 r 2` is the product `18*r`, which is not the pair.
    assert_eq!(check("9 R2", "9 r 2", N), decided(false, false));
    assert_eq!(check("(9, 2)", "9 r 2", N), decided(false, false));
    // `9 2/2` is two numbers side by side, as before (review round 1).
    assert_undecidable("9 R2", "9 2/2", N, "two numbers stand side by side");
}
