//! Part 2 of the `answer_check` tests. The header of `answer_check_1.rs` names the sources.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use common::check::*;

#[test]
fn a_chained_inequality_becomes_a_range_over_its_variable() {
    let expected = x_range(true, true);
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
    let open_chain = x_range(false, false);
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
    let upper_closed = x_range(false, true);
    assert_eq!(form("3 >= x > -1"), upper_closed);
    let upper_open = x_range(true, false);
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
