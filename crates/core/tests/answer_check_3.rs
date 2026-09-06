//! Part 3 of the `answer_check` tests. The header of `answer_check_1.rs` names the sources.

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
    // FIXM2a pinned 265 answers as outside the grammar. The rational-exponent
    // production of D-F3 (unit f2-grammar) reads 15 of them (`recovered_2_0.jsonl`),
    // so 3,492 - 250 = 3,242 answers parse. Every one of them canonicalizes.
    assert_eq!(
        canonical,
        3_242,
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
