//! The coordinate pair `(x, y)` under the tuple production (D-F3, unit
//! f2-grammar, V1, C4).
//!
//! The plan asks the grammar to hold coordinates. The tuple production of M2
//! already holds them: `(3, 4)` is an ordered tuple of two values, the members
//! compare by value, and the order matters. This file adds the tests that pin
//! that reading on both answer kinds; it adds no production.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::grammar::*;

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------
#[test]
fn a_bracketed_pair_is_an_ordered_tuple() {
    assert_eq!(ast("(3, 4)"), Ast::Tuple(vec![int(3), int(4)]));
    assert_eq!(ast("(3,4)"), Ast::Tuple(vec![int(3), int(4)]));
    assert_eq!(ast("( 3 , 4 )"), Ast::Tuple(vec![int(3), int(4)]));
    assert_eq!(ast("3, 4"), Ast::Tuple(vec![int(3), int(4)]));
    assert_eq!(ast("$(3, 4)$"), Ast::Tuple(vec![int(3), int(4)]));
    assert_eq!(ast("\\left(3, 4\\right)"), Ast::Tuple(vec![int(3), int(4)]));
    assert_eq!(
        ast("(-6, 2)"),
        Ast::Tuple(vec![Ast::Neg(Box::new(int(6))), int(2)])
    );
    assert_eq!(
        ast("(1/2, 8)"),
        Ast::Tuple(vec![
            Ast::Fraction {
                numerator: BigInt::from(1),
                denominator: BigInt::from(2),
            },
            int(8),
        ])
    );
    assert_eq!(
        ast("(2, sqrt(2))"),
        Ast::Tuple(vec![int(2), Ast::Sqrt(Box::new(int(2)))])
    );
    assert_eq!(
        ast("(x, 2x)"),
        Ast::Tuple(vec![v("x"), Ast::Mul(vec![int(2), v("x")])])
    );
    assert_eq!(
        ast("(4, -1, 0)"),
        Ast::Tuple(vec![int(4), Ast::Neg(Box::new(int(1))), int(0)])
    );
    // A label in front of the pair stays a label.
    assert_eq!(
        ast("P = (3, 4)"),
        Ast::Assign {
            var: "P".to_string(),
            value: Box::new(Ast::Tuple(vec![int(3), int(4)])),
        }
    );
    // One bracketed value is that value, not a tuple of one.
    assert_eq!(ast("(3)"), int(3));
}

#[test]
fn a_pair_that_is_not_two_values_is_refused() {
    assert_eq!(refusal("(3, 4"), "a group with no closing bracket");
    assert_eq!(refusal("(3, )"), "a symbol where a value belongs");
    assert_eq!(refusal("(, 4)"), "a symbol where a value belongs");
    assert_eq!(refusal("3, 4)"), "trailing text after the answer");
    assert_eq!(refusal("(3; 4)"), "a character outside the grammar");
    // A mixed bracket pair is an interval, and it needs two ends.
    assert_eq!(refusal("(3, 4, 5]"), "an interval that has no two ends");
}

// ---------------------------------------------------------------------------
// Canon
// ---------------------------------------------------------------------------
#[test]
fn a_pair_keeps_its_order_and_compares_its_members_by_value() {
    let three_four = Canon::Tuple(vec![Canon::Rational(whole(3)), Canon::Rational(whole(4))]);
    assert_eq!(form("(3, 4)"), three_four);
    assert_eq!(form("3, 4"), three_four);
    assert_eq!(form("(3.0, 8/2)"), three_four);
    assert_eq!(form("(6/2, 4.00)"), three_four);
    assert_ne!(form("(4, 3)"), three_four);
    assert_ne!(form("(3, 4, 0)"), three_four);
    assert_ne!(form("[3, 4]"), three_four);
    assert_ne!(form("{3, 4}"), three_four);
    assert_eq!(form("(1/2, 8)"), form("(0.5, 8)"));
    assert_eq!(form("(2, sqrt(2))"), form("(2, 2^(1/2))"));
    assert_eq!(form("(-6, 2)"), form("(-6,2)"));
    assert_eq!(form("(x, 2x)"), form("(x, x*2)"));
    assert_eq!(form("(x + 1, 2)"), form("(1 + x, 2)"));
    assert_eq!(form("(4, -1, 0)"), form("(4.0, -1, 0/5)"));
}

// ---------------------------------------------------------------------------
// Check
// ---------------------------------------------------------------------------
#[test]
fn the_coordinate_pairs_are_decided_on_both_kinds() {
    run_table(&[
        ("(3, 4)", "(3, 4)", N, true),
        ("(3, 4)", "(3,4)", N, true),
        ("(3, 4)", "( 3 , 4 )", N, true),
        ("(3, 4)", "3, 4", N, true),
        ("3, 4", "(3, 4)", N, true),
        ("(3, 4)", "(3.0, 4)", N, true),
        ("(3, 4)", "(6/2, 4)", N, true),
        ("(3, 4)", "(3, 4)", E, true),
        ("(1/2, 8)", "(0.5, 8)", N, true),
        ("(-6, 2)", "(-6,2)", N, true),
        ("(2, sqrt(2))", "(2, √2)", E, true),
        ("(2, 2π/3)", "(2, 2*pi/3)", E, true),
        ("(x, 2x)", "(x, x*2)", E, true),
        ("(4, -1, 0)", "(4, -1, 0)", N, true),
        ("(3, 4)", "(4, 3)", N, false),
        ("(3, 4)", "(3, 5)", N, false),
        ("(3, 4)", "(3, 4, 0)", N, false),
        ("(3, 4)", "3", N, false),
        ("(3, 4)", "7", N, false),
        ("(3, 4)", "[3, 4]", N, false),
        ("(-6, 2)", "(6, -2)", N, false),
        ("(x, 2x)", "(2x, x)", E, false),
    ]);
}

#[test]
fn a_rounded_coordinate_takes_no_notation_verdict() {
    // The rounding rung reads one learner decimal and nothing else, so a
    // rounded member inside a pair is a decided miss (ruling `D6-dec`).
    assert_eq!(check("(1/3, 2)", "(0.33, 2)", N), decided(false, false));
    assert_eq!(check("(1/3, 2)", "(1/3, 2.0)", N), decided(true, false));
    // A pair against a set gets no verdict (the set production of D-F3).
    assert_undecidable("(3, 4)", "{3, 4}", N, "a set against a tuple");
}
