//! The rational-exponent production of D-F3 (unit f2-grammar, V1, C4).
//!
//! `a^(p/q)` and `a^{p/q}` read into a root when the written `q` is 2 to 6 and
//! the written `|p|` is at most 12. The parse tests pin the tree and the refusal
//! reasons, the canon tests pin the form, and the check tests pin the verdicts
//! on both answer kinds. `docs/reference/undecidable-answers.md` names the 15
//! corpus rows the production recovers.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::grammar::*;

// ---------------------------------------------------------------------------
// Parse
// ---------------------------------------------------------------------------
#[test]
fn a_bracketed_rational_exponent_is_one_node_in_lowest_terms() {
    assert_eq!(ast("x^(1/2)"), root_pow(v("x"), 1, 2));
    assert_eq!(ast("x^{1/2}"), root_pow(v("x"), 1, 2));
    assert_eq!(ast("x**(1/2)"), root_pow(v("x"), 1, 2));
    assert_eq!(ast("x^(-1/3)"), root_pow(v("x"), -1, 3));
    assert_eq!(ast("x^(+5/6)"), root_pow(v("x"), 5, 6));
    assert_eq!(ast("2^(2/3)"), root_pow(int(2), 2, 3));
    // Lowest terms: `2/4` is `1/2`, and `4/2` is the whole exponent 2.
    assert_eq!(ast("x^(2/4)"), root_pow(v("x"), 1, 2));
    assert_eq!(ast("x^(4/2)"), Ast::Pow(Box::new(v("x")), 2));
    assert_eq!(ast("x^(0/3)"), Ast::Pow(Box::new(v("x")), 0));
    assert_eq!(ast("x^(3/1)"), Ast::Pow(Box::new(v("x")), 3));
    // The written numerator carries the bound: `12/6` is the whole exponent 2.
    assert_eq!(ast("x^(12/6)"), Ast::Pow(Box::new(v("x")), 2));
    assert_eq!(ast("x^(-12/5)"), root_pow(v("x"), -12, 5));
}

#[test]
fn the_exponent_binds_to_the_atom_in_front_of_it() {
    // `(5/2)x^(3/2)` is a product, and the root takes `x` alone.
    assert_eq!(
        ast("(5/2)x^(3/2)"),
        Ast::Mul(vec![
            Ast::Fraction {
                numerator: BigInt::from(5),
                denominator: BigInt::from(2),
            },
            root_pow(v("x"), 3, 2),
        ])
    );
    // A bracketed base takes the root as a whole.
    assert_eq!(
        ast("(x + 1)^(5/2)"),
        root_pow(Ast::Add(vec![v("x"), int(1)]), 5, 2)
    );
    // The power of a letter run binds to the last letter, as a whole power does.
    assert_eq!(
        ast("xy^(1/2)"),
        Ast::Mul(vec![v("x"), root_pow(v("y"), 1, 2)])
    );
    // A function name takes the house spelling `sec^(1/2) x`.
    assert_eq!(
        ast("sin^(1/2) x"),
        root_pow(Ast::Func("sin".to_string(), vec![v("x")]), 1, 2)
    );
}

#[test]
fn an_unbracketed_rational_exponent_keeps_the_1_0_reading() {
    // `x^1/2` is `(x^1)/2`, which is the reading 1.0 gives it.
    assert_eq!(
        ast("x^1/2"),
        Ast::Div(Box::new(Ast::Pow(Box::new(v("x")), 1)), Box::new(int(2)))
    );
}

#[test]
fn a_rational_exponent_outside_the_written_bound_is_refused() {
    let out_of_bound = "a rational exponent outside the bound";
    assert_eq!(refusal("x^(1/7)"), out_of_bound);
    assert_eq!(refusal("x^(1/12)"), out_of_bound);
    assert_eq!(refusal("x^(13/2)"), out_of_bound);
    assert_eq!(refusal("x^(-13/2)"), out_of_bound);
    assert_eq!(refusal("x^(1/0)"), out_of_bound);
    // A written `6/12` reduces to a half, and the bound reads the written form.
    assert_eq!(refusal("x^(6/12)"), out_of_bound);
    // The same refusals in the brace spelling.
    assert_eq!(refusal("x^{1/7}"), out_of_bound);
    assert_eq!(refusal("x^{13/2}"), out_of_bound);
}

#[test]
fn an_exponent_that_is_not_a_rational_literal_stays_refused() {
    let not_whole = "an exponent that is not a whole number";
    assert_eq!(refusal("2^0.5"), not_whole);
    assert_eq!(refusal("x^(1.5/2)"), not_whole);
    assert_eq!(refusal("x^(1/2.0)"), not_whole);
    assert_eq!(refusal("x^(1/-2)"), not_whole);
    assert_eq!(refusal("x^(1/y)"), not_whole);
    assert_eq!(refusal("x^(1/2"), not_whole);
    // A percent inside an exponent stays refused (review round 3, #3, #4).
    assert_eq!(refusal("2^50%"), not_whole);
    assert_eq!(refusal("x^(1/2%)"), not_whole);
    // A tower stays refused, with a rational exponent as well.
    assert_eq!(refusal("x^(1/2)^2"), "a tower of powers");
    assert_eq!(refusal("x^(1/2)^(1/2)"), "a tower of powers");
    // A whole exponent past the evaluation bound stays refused.
    assert_eq!(
        refusal("x^(2000/1)"),
        "an exponent outside the evaluation bound"
    );
}

// ---------------------------------------------------------------------------
// Canon
// ---------------------------------------------------------------------------
#[test]
fn a_rational_base_reads_into_the_radical_form() {
    // A half is the square root the canonicalizer already holds.
    assert_eq!(form("2^(1/2)"), form("sqrt(2)"));
    assert_eq!(form("2^(1/2)"), radical(2, whole(1)));
    assert_eq!(form("8^(1/2)"), form("2*sqrt(2)"));
    assert_eq!(form("2^(3/2)"), form("2*sqrt(2)"));
    assert_eq!(form("2^(-1/2)"), form("sqrt(2)/2"));
    assert_eq!(form("(1/2)^(1/2)"), form("sqrt(2)/2"));
    assert_eq!(form("4^(1/2)"), Canon::Rational(whole(2)));
    assert_eq!(form("(9/4)^(3/2)"), Canon::Rational(ratio(27, 8)));
    // A whole root of a perfect power is a rational.
    assert_eq!(form("8^(2/3)"), Canon::Rational(whole(4)));
    assert_eq!(form("8^(1/3)"), Canon::Rational(whole(2)));
    assert_eq!(form("27^(-1/3)"), Canon::Rational(ratio(1, 3)));
    assert_eq!(form("16^(1/4)"), Canon::Rational(whole(2)));
    assert_eq!(form("32^(3/5)"), Canon::Rational(whole(8)));
    assert_eq!(form("64^(1/6)"), Canon::Rational(whole(2)));
    assert_eq!(form("(8/27)^(2/3)"), Canon::Rational(ratio(4, 9)));
    assert_eq!(form("1^(1/3)"), Canon::Rational(whole(1)));
    assert_eq!(form("0^(1/3)"), Canon::Rational(whole(0)));
}

#[test]
fn a_higher_root_of_a_number_is_one_root_atom_per_prime() {
    let root_two_cubed = Canon::Poly(BTreeMap::from([(
        monomial(&[(Atom::Root(Box::new(Canon::Rational(whole(2))), 3), 1)]),
        whole(1),
    )]));
    assert_eq!(form("2^(1/3)"), root_two_cubed);
    // The prime carries the exponent, so `4^(1/3)` is `2^(2/3)`, and a product
    // of two roots of one prime merges: `2^(1/3) * 4^(1/3)` is 2.
    assert_eq!(form("4^(1/3)"), form("2^(2/3)"));
    assert_eq!(form("2^(1/3)*4^(1/3)"), Canon::Rational(whole(2)));
    assert_eq!(form("2^(1/3)*2^(1/3)*2^(1/3)"), Canon::Rational(whole(2)));
    // The whole part of the exponent is the floor, and it moves into the
    // coefficient: `2^(-1/3)` is `2^(2/3)/2`.
    assert_eq!(form("2^(-1/3)"), form("2^(2/3)/2"));
    assert_eq!(form("1/2^(1/3)"), form("2^(2/3)/2"));
    assert_eq!(form("2^(4/3)"), form("2*2^(1/3)"));
    // A sixth root that reduces to a cube root or a square root takes that form.
    assert_eq!(form("4^(1/6)"), form("2^(1/3)"));
    assert_eq!(form("8^(1/6)"), form("sqrt(2)"));
    assert_eq!(form("2^(3/6)"), form("sqrt(2)"));
    // A sum with a root is a polynomial over the root atom.
    assert_eq!(form("3 + 3*2^(1/3)"), form("3*2^(1/3) + 3"));
    assert_ne!(form("3 + 3*2^(1/3)"), form("3 + 3*2^(1/2)"));
    assert_ne!(form("2^(1/3)"), form("3^(1/3)"));
    assert_ne!(form("2^(1/3)"), form("2^(1/4)"));
}

#[test]
fn a_variable_base_carries_the_exponent_on_its_atom() {
    let x = Canon::Poly(BTreeMap::from([(monomial(&[(var("x"), 1)]), whole(1))]));
    let root_of_x = Atom::Root(Box::new(x), 2);
    // `sqrt(x)` and `x^(1/2)` are one form.
    assert_eq!(form("x^(1/2)"), form("sqrt(x)"));
    assert_eq!(form("x^(1/2)"), form("\\sqrt{x}"));
    assert_eq!(form("x^(1/2)"), form("√x"));
    // The whole part stays on the variable: `x^(3/2)` is `x*sqrt(x)`.
    assert_eq!(
        form("x^(3/2)"),
        Canon::Poly(BTreeMap::from([(
            monomial(&[(var("x"), 1), (root_of_x.clone(), 1)]),
            whole(1),
        )]))
    );
    assert_eq!(form("x^(3/2)"), form("x*sqrt(x)"));
    assert_eq!(form("x^(5/2)"), form("x^2*sqrt(x)"));
    assert_eq!(form("x^(-3/2)"), form("1/(x*sqrt(x))"));
    assert_eq!(form("x^(-1/2)"), form("1/sqrt(x)"));
    // Two roots of one variable add their exponents.
    assert_eq!(form("sqrt(x)*sqrt(x)"), form("x"));
    assert_eq!(form("x^(1/2)*x^(1/3)"), form("x^(5/6)"));
    assert_eq!(form("x^(2/3)*x^(1/3)"), form("x"));
    assert_eq!(form("x^(1/2)/x^(1/2)"), Canon::Rational(whole(1)));
    assert_eq!(form("(x^(1/2))^2"), form("x"));
    assert_eq!(form("(x^(1/3))^(1/2)"), form("x^(1/6)"));
    assert_eq!(form("sqrt(sqrt(x))"), form("x^(1/4)"));
    // A coefficient takes its own root: `sqrt(4x)` is `2*sqrt(x)`.
    assert_eq!(form("sqrt(4x)"), form("2*sqrt(x)"));
    assert_eq!(form("(4x)^(1/2)"), form("2*x^(1/2)"));
    assert_eq!(form("sqrt(2x)"), form("sqrt(2)*sqrt(x)"));
    assert_eq!(form("(8x^3)^(1/3)"), form("2*(x^3)^(1/3)"));
    // Two variables stay two roots.
    assert_ne!(form("x^(1/2)"), form("y^(1/2)"));
    assert_ne!(form("x^(1/2)"), form("x^(1/3)"));
    assert_ne!(form("x^(1/2)"), form("x"));
}

#[test]
fn a_sum_under_a_root_is_one_opaque_atom() {
    // `(x+1)^(3/2)` is one atom with the exponent `3/2`.
    assert_eq!(form("(x + 1)^(3/2)"), form("(1 + x)^(3/2)"));
    assert_eq!(form("(x + 1)^(3/2)"), form("sqrt(x + 1)^3"));
    assert_eq!(form("(x + 1)^(1/2)"), form("sqrt(x + 1)"));
    assert_eq!(form("(x + 1)^(1/2)*(x + 1)^(1/3)"), form("(x + 1)^(5/6)"));
    // The documented narrowing: the form multiplies no sum into a root, so a
    // whole power of a sum under a root stays one atom of index 1.
    assert_eq!(
        form("(x^3 + 1)^(3/2)/(x^3 + 1)^(1/2)"),
        form("(x^3 + 1)^(3/2)*(x^3 + 1)^(-1/2)")
    );
    assert_ne!(form("(x^3 + 1)^(3/2)/(x^3 + 1)^(1/2)"), form("x^3 + 1"));
    assert_ne!(form("(x + 1)^(3/2)"), form("(x + 1)*sqrt(x + 1)"));
    assert_ne!(form("sqrt(x + 1)^2"), form("x + 1"));
    // A power of a monomial is opaque too: `sqrt(x^2)` is not `x`.
    assert_ne!(form("sqrt(x^2)"), form("x"));
    assert_eq!(form("sqrt(x^2)"), form("(x^2)^(1/2)"));
    // A negative base stays one structural root.
    assert_eq!(form("(-8)^(1/3)"), form("(-8)^(1/3)"));
    assert_ne!(form("(-8)^(1/3)"), form("-2"));
    assert_eq!(form("sqrt(-4)"), form("(-4)^(1/2)"));
    // A root of `e` is the exponential the canonicalizer holds.
    assert_eq!(form("e^(1/2)"), form("sqrt(e)"));
    assert_eq!(form("(e^x)^(1/2)"), form("e^(x/2)"));
    assert_eq!(form("pi^(1/2)"), form("sqrt(pi)"));
}

#[test]
fn a_root_past_a_bound_is_refused() {
    assert_eq!(
        canonical_form("0^(-1/2)").unwrap_err().reason,
        "a zero base with a non-positive exponent"
    );
    assert_eq!(
        canonical_form("0^(-1/3)").unwrap_err().reason,
        "a zero base with a non-positive exponent"
    );
    // A radicand wider than the factoring bound is refused, never guessed.
    assert_eq!(
        canonical_form("(10007*10009*10037)^(1/3)")
            .unwrap_err()
            .reason,
        "a radicand past the factoring bound"
    );
    assert_eq!(
        canonical_form("(2^200)^(1/3)").unwrap_err().reason,
        "a radicand past the factoring bound"
    );
    // A root of a collection is arithmetic on a collection.
    assert_eq!(
        canonical_form("(1, 2)^(1/2)").unwrap_err().reason,
        "arithmetic on a collection"
    );
}

// ---------------------------------------------------------------------------
// Check
// ---------------------------------------------------------------------------
#[test]
fn the_task_pairs_of_d_f3_are_correct_on_both_kinds() {
    run_table(&[
        ("sqrt(2)", "2^(1/2)", N, true),
        ("sqrt(2)", "2^(1/2)", E, true),
        ("2^(1/2)", "sqrt(2)", N, true),
        ("4", "8^(2/3)", N, true),
        ("8^(2/3)", "4", N, true),
        ("8^(2/3)", "4", E, true),
        ("sqrt(x)", "x^(1/2)", E, true),
        ("x^(1/2)", "sqrt(x)", E, true),
        ("x^(1/2)", "\\sqrt{x}", E, true),
        ("x^{1/2}", "√x", E, true),
        // The corpus rows the production recovers, against learner spellings.
        ("(5/2)x^(3/2)", "5/2*x*sqrt(x)", E, true),
        ("(5/2)x^(3/2)", "2.5x^(3/2)", E, true),
        ("(2/3)x^(-1/3)", "2/(3x^(1/3))", E, true),
        ("(1/3)x^(-2/3)", "1/(3x^(2/3))", E, true),
        ("x^(5/6)", "x^(1/2)*x^(1/3)", E, true),
        ("3 + 3*2^(1/3)", "3*2^(1/3) + 3", N, true),
        ("3 + 3*2^(1/3)", "3 + 3*4^(1/3)/2^(1/3)", N, true),
        ("(2/3)x^(3/2) + C", "C + (2/3)x*sqrt(x)", E, true),
        ("x^(2/3)", "(x^2)^(1/3)", E, false),
        // Wrong values stay wrong (C4).
        ("sqrt(2)", "2^(1/3)", N, false),
        ("4", "8^(1/3)", N, false),
        ("8^(2/3)", "2", N, false),
        ("x^(1/2)", "x^(1/3)", E, false),
        ("x^(1/2)", "x", E, false),
        ("x^(1/2)", "y^(1/2)", E, false),
        ("(5/2)x^(3/2)", "(5/2)x^(1/2)", E, false),
        ("3 + 3*2^(1/3)", "3 + 3*sqrt(2)", N, false),
    ]);
}

#[test]
fn a_rational_exponent_outside_the_bound_is_undecidable_on_either_side() {
    let reason = "a rational exponent outside the bound";
    assert_undecidable("x^(1/7)", "x^(1/7)+0", E, reason);
    assert_undecidable("sqrt(x)", "x^(1/8)", E, reason);
    assert_undecidable("x^(13/2)", "x^6*sqrt(x)", E, reason);
    assert_undecidable("2^(1/12)", "1.06", N, reason);
    // The string rung still decides an equal spelling first (rung 2).
    assert_eq!(check("x^(1/7)", "x^(1/7)", E), decided(true, false));
}

#[test]
fn a_decimal_against_a_higher_root_takes_no_rounding_verdict() {
    // A square root brackets exactly (ruling `D6-dec`); a cube root has no
    // exact bound in the rounding module, so the pair is refused (V2).
    assert_eq!(check("2^(1/2)", "1.414", N), rounded());
    assert_eq!(check("2^(1/2)", "1.415", N), decided(false, false));
    assert_undecidable(
        "2^(1/3)",
        "1.26",
        N,
        "a rounding of a higher root is not decidable",
    );
    assert_undecidable(
        "3 + 3*2^(1/3)",
        "6.78",
        N,
        "a rounding of a higher root is not decidable",
    );
    // A value with a variable takes no rounding at all: the pair is wrong.
    assert_eq!(check("x^(1/3)", "1.26", E), decided(false, false));
}
