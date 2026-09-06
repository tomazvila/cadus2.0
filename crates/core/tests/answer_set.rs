//! The unordered-set production of D-F3 (unit f2-grammar, V1, C4).
//!
//! `{a, b, c}` reads into a set: order does not matter, repeated members
//! collapse, and a set against a list or a tuple gets no verdict, because the
//! two shapes hold one member set and the answer contract decides the shape.
//! The parse tests pin the tree, the canon tests pin the form, and the check
//! tests pin the verdicts on both answer kinds. The production recovers no
//! corpus row: every set of values parsed before this unit, and a label set
//! such as `{HH, HT}` stays outside the grammar.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::grammar::*;

/// Build the canonical set of whole numbers.
fn set_of(members: &[i64]) -> Canon {
    Canon::Set(
        members
            .iter()
            .map(|member| Canon::Rational(whole(*member)))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------
#[test]
fn a_brace_pair_reads_into_a_set_in_the_written_order() {
    assert_eq!(ast("{1, 2, 3}"), Ast::Set(vec![int(1), int(2), int(3)]));
    assert_eq!(ast("{3, 1, 2}"), Ast::Set(vec![int(3), int(1), int(2)]));
    assert_eq!(ast("{1,2}"), Ast::Set(vec![int(1), int(2)]));
    assert_eq!(ast("{ 1 , 2 }"), Ast::Set(vec![int(1), int(2)]));
    assert_eq!(ast("{2}"), Ast::Set(vec![int(2)]));
    assert_eq!(ast("{2, 2}"), Ast::Set(vec![int(2), int(2)]));
    assert_eq!(ast("\\{1, 2\\}"), Ast::Set(vec![int(1), int(2)]));
    assert_eq!(ast("$\\{-3, 3\\}$"), ast("{-3, 3}"));
    assert_eq!(
        ast("{x, 2x, sqrt(2)}"),
        Ast::Set(vec![
            v("x"),
            Ast::Mul(vec![int(2), v("x")]),
            Ast::Sqrt(Box::new(int(2))),
        ])
    );
    assert_eq!(
        ast("{{1, 2}, {3}}"),
        Ast::Set(vec![Ast::Set(vec![int(1), int(2)]), Ast::Set(vec![int(3)]),])
    );
}

#[test]
fn a_set_that_is_not_a_set_of_values_is_refused() {
    assert_eq!(refusal("{}"), "an empty set");
    assert_eq!(refusal("{1, 2"), "a set with no closing brace");
    assert_eq!(refusal("{1, }"), "a symbol where a value belongs");
    assert_eq!(refusal("{, 1}"), "a symbol where a value belongs");
    // A label set stays outside the grammar: `HT` is one outcome, not `H*T`.
    assert_eq!(
        refusal("{HH, HT, TH, TT}"),
        "a name that is not a function or variable"
    );
    assert_eq!(
        refusal("{AB, AC, BD}"),
        "a name that is not a function or variable"
    );
}

// ---------------------------------------------------------------------------
// Canon
// ---------------------------------------------------------------------------
#[test]
fn a_set_is_unordered_and_its_repeated_members_collapse() {
    assert_eq!(form("{1, 2}"), set_of(&[1, 2]));
    assert_eq!(form("{1, 2}"), form("{2, 1}"));
    assert_eq!(form("{3, 1, 2}"), form("{1, 2, 3}"));
    assert_eq!(form("{2, 2}"), set_of(&[2]));
    assert_eq!(form("{1, 1, 2, 2, 1}"), set_of(&[1, 2]));
    // Members compare by value, so two spellings of one member are one member.
    assert_eq!(form("{1/2, 0.5}"), form("{0.5}"));
    assert_eq!(form("{2, 4/2, 2.0}"), set_of(&[2]));
    assert_eq!(form("{sqrt(4), 2}"), set_of(&[2]));
    assert_eq!(form("{x, 2x}"), form("{2x, x}"));
    assert_eq!(form("{x + 1, x - 1}"), form("{-1 + x, 1 + x}"));
    assert_eq!(form("{{1, 2}, {3}}"), form("{{3}, {2, 1}}"));
    // Different members are different sets.
    assert_ne!(form("{1, 2}"), form("{1, 3}"));
    assert_ne!(form("{1, 2}"), form("{1}"));
    assert_ne!(form("{1, 2}"), form("{1, 2, 3}"));
    // A set is not a list, a tuple, or a number.
    assert_ne!(form("{2}"), form("2"));
    assert_ne!(form("{1, 2}"), form("[1, 2]"));
    assert_ne!(form("{1, 2}"), form("(1, 2)"));
}

// ---------------------------------------------------------------------------
// Check
// ---------------------------------------------------------------------------
#[test]
fn the_task_pairs_of_d_f3_are_decided_on_both_kinds() {
    run_table(&[
        ("{1, 2}", "{2, 1}", N, true),
        ("{1, 2}", "{2, 1}", E, true),
        ("{1, 2}", "{1,2}", N, true),
        ("{1, 2}", "{ 2 , 1 }", N, true),
        ("{1, 2}", "\\{2, 1\\}", N, true),
        ("{1, 2}", "{1, 2, 2}", N, true),
        ("{1, 2}", "{1, 1, 2}", N, true),
        ("{1, 2}", "{1.0, 4/2}", N, true),
        ("{-3, 3}", "{3, -3}", E, true),
        ("{0, 3, 6, 9}", "{9, 6, 3, 0}", N, true),
        ("{x, 2x}", "{2x, x}", E, true),
        ("{x + 1, x - 1}", "{x - 1, 1 + x}", E, true),
        ("{{1, 2}, {3}}", "{{3}, {2, 1}}", E, true),
        ("{1, 2}", "{1, 3}", N, false),
        ("{1, 2}", "{1}", N, false),
        ("{1, 2}", "{1, 2, 3}", N, false),
        ("{1, 2}", "{2}", N, false),
        ("{2}", "2", N, false),
        ("2", "{2}", N, false),
        ("{x, 2x}", "{x, 3x}", E, false),
    ]);
}

#[test]
fn a_set_against_an_ordered_collection_gets_no_verdict() {
    assert_undecidable("{1, 2}", "[1, 2]", N, "a set against a list");
    assert_undecidable("[1, 2]", "{1, 2}", N, "a set against a list");
    assert_undecidable("{1, 2}", "[2, 1]", E, "a set against a list");
    assert_undecidable("{1, 2}", "(1, 2)", N, "a set against a tuple");
    assert_undecidable("(1, 2)", "{1, 2}", N, "a set against a tuple");
    // A bare comma list is a tuple, so it takes the tuple reason.
    assert_undecidable("{1, 2}", "1, 2", N, "a set against a tuple");
    assert_undecidable("1, 2", "{2, 1}", N, "a set against a tuple");
    // A label falls away first, so a labeled set takes the same rule.
    assert_undecidable("S = {1, 2}", "[1, 2]", E, "a set against a list");
    // A list against a tuple is a decided miss, as before (M2 review 1, #14).
    assert_eq!(check("[1, 2]", "(1, 2)", N), decided(false, false));
    // The string rung still decides an equal spelling first (rung 2).
    assert_eq!(check("{1, 2}", "{1, 2}", N), decided(true, false));
}
