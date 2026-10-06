//! Required numeric notation, distinct from approximate value acceptance.

use num_integer::Integer;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Serialize};

use crate::answer::{Ast, Atom, Canon, Monomial, Poly, canon, normalize, parse};

/// An explicit numeric output form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NumericForm {
    Integer,
    Decimal,
    ReducedFraction,
    /// A greatest integer factor multiplying a primitive linear sum.
    FactoredLinear,
    /// A complete factorization over the integers: constants and monomials
    /// times primitive sums, with as many sum factors as the authored key.
    FactoredPolynomial,
    /// A sum of monomials with nonnegative integer exponents and no like terms.
    ExpandedPolynomial,
}

/// Whether the learner `text` is written in `form`. `expected` is the authored
/// key; only [`NumericForm::FactoredPolynomial`] reads it, for its factor count.
pub(super) fn accepts(form: NumericForm, text: &str, expected: &str) -> bool {
    let Ok(tree) = parse(&normalize(text).source) else {
        return false;
    };
    match form {
        NumericForm::FactoredPolynomial => return factored_polynomial(&tree, expected),
        NumericForm::ExpandedPolynomial => return expanded_polynomial(&tree),
        NumericForm::Integer
        | NumericForm::Decimal
        | NumericForm::ReducedFraction
        | NumericForm::FactoredLinear => {}
    }
    let mut node = &tree;
    while let Ast::Neg(inner) = node {
        node = inner;
    }
    match (form, node) {
        (NumericForm::Integer, Ast::Integer(_)) | (NumericForm::Decimal, Ast::Decimal { .. }) => {
            true
        }
        (
            NumericForm::ReducedFraction,
            Ast::Fraction {
                numerator,
                denominator,
            },
        ) => denominator.is_positive() && numerator.gcd(denominator).is_one(),
        (NumericForm::FactoredLinear, node) => factored_linear(node),
        _ => false,
    }
}

fn factored_linear(node: &Ast) -> bool {
    let node = match node {
        Ast::Neg(inner) => inner.as_ref(),
        other => other,
    };
    let Ast::Mul(factors) = node else {
        return false;
    };
    let [outer_node, Ast::Add(terms)] = factors.as_slice() else {
        return false;
    };
    let Some(outer) = signed_integer(outer_node) else {
        return false;
    };
    if outer.abs() < 2.into() || terms.len() < 2 {
        return false;
    }
    let Some(coefficients) = terms
        .iter()
        .map(linear_coefficient)
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    coefficients
        .iter()
        .find(|(_, symbolic)| *symbolic)
        .is_some_and(|(coefficient, _)| coefficient.is_positive())
        && coefficients.iter().any(|(_, symbolic)| *symbolic)
        && coefficients
            .iter()
            .map(|(coefficient, _)| coefficient.abs())
            .reduce(|left, right| left.gcd(&right))
            .is_some_and(|gcd| gcd.is_one())
}

fn linear_coefficient(node: &Ast) -> Option<(num_bigint::BigInt, bool)> {
    match node {
        Ast::Integer(value) => Some((value.clone(), false)),
        Ast::Var(_) | Ast::Pow(_, 1) => Some((1.into(), true)),
        Ast::Neg(inner) => linear_coefficient(inner).map(|(value, symbolic)| (-value, symbolic)),
        Ast::Mul(factors) => {
            let mut coefficient = num_bigint::BigInt::one();
            let mut symbolic_count = 0_u8;
            for factor in factors {
                match factor {
                    Ast::Integer(value) => coefficient *= value,
                    Ast::Var(_) | Ast::Pow(_, 1) => symbolic_count += 1,
                    _ => return None,
                }
            }
            (symbolic_count == 1).then_some((coefficient, true))
        }
        _ => None,
    }
}

fn signed_integer(node: &Ast) -> Option<num_bigint::BigInt> {
    match node {
        Ast::Integer(value) => Some(value.clone()),
        Ast::Neg(inner) => match inner.as_ref() {
            Ast::Integer(value) => Some(-value),
            _ => None,
        },
        _ => None,
    }
}

/// The largest power of a sum factor the factor count expands.
const MAX_FACTOR_POWER: i64 = 16;

/// A complete factorization over the integers, read against the authored key.
///
/// Every factor is a constant, a monomial, or a sum that is primitive: integer
/// coefficients with no common integer factor and no variable common to every
/// term. The caller compares the value with the key, so as many primitive sum
/// factors as the key holds means each one is irreducible (unique
/// factorization): `2(x^2+3x)` (a common `x`), `(x^2-4)(x^2+4)` (two sums
/// against three) and the expanded trinomial (one sum against two) fail.
fn factored_polynomial(tree: &Ast, expected: &str) -> bool {
    let Ok(key) = parse(&normalize(expected).source) else {
        return false;
    };
    let mut learner_sums = Vec::new();
    let mut key_sums = Vec::new();
    sum_factors(tree, &mut learner_sums).is_some()
        && sum_factors(&key, &mut key_sums).is_some()
        && !key_sums.is_empty()
        && learner_sums.len() == key_sums.len()
}

/// Push the primitive sum factors of `node`, with multiplicity. `None` when a
/// factor is neither a constant, a monomial nor a primitive integer sum.
fn sum_factors(node: &Ast, out: &mut Vec<Poly>) -> Option<()> {
    match node {
        Ast::Neg(inner) => sum_factors(inner, out),
        Ast::Mul(factors) => factors
            .iter()
            .try_for_each(|factor| sum_factors(factor, out)),
        Ast::Pow(base, power) if (1..=MAX_FACTOR_POWER).contains(power) => {
            let mut inner = Vec::new();
            sum_factors(base, &mut inner)?;
            for _ in 0..*power {
                out.extend(inner.iter().cloned());
            }
            Some(())
        }
        other => match canon(other).ok()? {
            Canon::Rational(_) => Some(()),
            Canon::Poly(poly) if poly.len() == 1 => poly
                .keys()
                .all(|monomial| monomial.values().all(|exponent| *exponent > 0))
                .then_some(()),
            Canon::Poly(poly) => primitive(&poly).then(|| out.push(poly)),
            _ => None,
        },
    }
}

/// Integer coefficients without a common factor, variables with positive
/// exponents, and no variable in every term.
fn primitive(poly: &Poly) -> bool {
    let integral = poly.values().all(num_rational::BigRational::is_integer);
    let content = poly
        .values()
        .map(|coefficient| coefficient.numer().abs())
        .reduce(|left, right| left.gcd(&right));
    let variables_only = poly.keys().all(|monomial| {
        monomial
            .iter()
            .all(|(atom, exponent)| matches!(atom, Atom::Var(_)) && *exponent > 0)
    });
    let common_variable = poly.keys().next().is_some_and(|first| {
        first
            .keys()
            .any(|atom| poly.keys().all(|monomial| monomial.contains_key(atom)))
    });
    integral && variables_only && content.is_some_and(|gcd| gcd.is_one()) && !common_variable
}

/// A sum of monomial terms with nonnegative integer exponents, no sum inside a
/// term, and no two terms with the same monomial.
fn expanded_polynomial(tree: &Ast) -> bool {
    let terms: Vec<&Ast> = match tree {
        Ast::Add(terms) => terms.iter().collect(),
        other => vec![other],
    };
    let single = terms.len() == 1;
    let mut seen = std::collections::BTreeSet::new();
    for term in terms {
        if holds_sum(term) {
            return false;
        }
        let monomial = match canon(term) {
            Ok(Canon::Rational(value)) if single || !value.is_zero() => Monomial::new(),
            Ok(Canon::Poly(poly)) if poly.len() == 1 => {
                let Some(monomial) = poly.into_keys().next() else {
                    return false;
                };
                if monomial.values().any(|exponent| *exponent < 0) {
                    return false;
                }
                monomial
            }
            _ => return false,
        };
        if !seen.insert(monomial) {
            return false;
        }
    }
    true
}

/// Whether a sum sits anywhere inside `node`.
fn holds_sum(node: &Ast) -> bool {
    match node {
        Ast::Add(_) => true,
        Ast::Neg(inner) | Ast::Pow(inner, _) | Ast::Sqrt(inner) => holds_sum(inner),
        Ast::RationalPow { base, .. } => holds_sum(base),
        Ast::Mul(factors) => factors.iter().any(holds_sum),
        Ast::Div(numerator, denominator) => holds_sum(numerator) || holds_sum(denominator),
        _ => false,
    }
}
