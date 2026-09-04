//! The sum helpers of the canonicalizer, and the demotion to the narrowest form.

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};

use super::{
    Atom, Basis, Canon, Frac, MAX_TERMS, Monomial, Poly, SQUAREFREE_CERTAIN, TRIAL_DIVISION_LIMIT,
    Undecidable,
};

/// Build the canonical value of one atom with exponent 1 and coefficient 1.
pub(super) fn atom_value(atom: Atom) -> Canon {
    let mut monomial = Monomial::new();
    monomial.insert(atom, 1);
    from_sum(term(monomial, BigRational::one()))
}

/// Multiply a plain atom power into a monomial, and drop a zero exponent.
///
/// The atom is a plain one: a variable, a function call, `pi`, or `e`. The two
/// atoms that carry a law of their own ([`Atom::Sqrt`] and [`Atom::Exp`]) go
/// through [`Work::add_atom`] instead.
pub(super) fn insert_atom(
    monomial: &mut Monomial,
    atom: &Atom,
    exponent: i64,
) -> Result<(), Undecidable> {
    let previous = monomial.get(atom).copied().unwrap_or(0);
    let total = previous
        .checked_add(exponent)
        .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
    if total == 0 {
        monomial.remove(atom);
    } else {
        monomial.insert(atom.clone(), total);
    }
    Ok(())
}

/// Build a one-term sum, or the empty sum when the coefficient is zero.
pub(super) fn term(monomial: Monomial, coefficient: BigRational) -> Poly {
    let mut sum = Poly::new();
    if !coefficient.is_zero() {
        sum.insert(monomial, coefficient);
    }
    sum
}

/// Build the sum that holds the number 1.
pub(super) fn one_poly() -> Poly {
    term(Monomial::new(), BigRational::one())
}

/// Whether a sum is the number 1.
pub(super) fn is_one(sum: &Poly) -> bool {
    match single_term(sum) {
        Some((monomial, coefficient)) => monomial.is_empty() && coefficient.is_one(),
        None => false,
    }
}

/// Read the only term of a sum, when the sum has exactly one.
fn single_term(sum: &Poly) -> Option<(&Monomial, &BigRational)> {
    if sum.len() == 1 {
        sum.iter().next()
    } else {
        None
    }
}

/// Copy the only term of a sum, when the sum has exactly one.
///
/// The copy frees the borrow of the sum, so the caller keeps the work budget.
pub(super) fn one_term(sum: &Poly) -> Option<(Monomial, BigRational)> {
    single_term(sum).map(|(monomial, coefficient)| (monomial.clone(), coefficient.clone()))
}

/// Refuse a sum that goes past the term bound.
pub(super) fn bound_terms(sum: &Poly) -> Result<(), Undecidable> {
    if sum.len() > MAX_TERMS {
        return Err(Undecidable::new("the answer goes past the term bound"));
    }
    Ok(())
}

/// Build the reciprocal of a non-zero rational.
pub(super) fn reciprocal_of(value: &BigRational) -> BigRational {
    value.recip()
}

/// Split a positive integer into `outside^2 * radicand` with a squarefree radicand.
///
/// # Errors
///
/// Returns [`Undecidable`] when trial division cannot prove the remainder
/// squarefree. A refusal is safe; an unreduced radical would compare wrong.
pub(super) fn extract_square(value: &BigInt) -> Result<(BigInt, BigInt), Undecidable> {
    let Some(remainder) = value.to_u128() else {
        return wide_square(value);
    };
    let mut split = SquareSplit {
        outside: 1,
        radicand: 1,
        remainder,
    };
    let mut divisor: u128 = 2;
    let mut exhausted = false;
    while divisor <= TRIAL_DIVISION_LIMIT {
        if divisor > split.remainder / divisor {
            exhausted = true;
            break;
        }
        split.divide_out(divisor);
        divisor += 1;
    }
    split.finish(exhausted)?;
    Ok((BigInt::from(split.outside), BigInt::from(split.radicand)))
}

/// The three parts of a radicand under trial division.
struct SquareSplit {
    /// The product of the square factors found so far.
    outside: u128,
    /// The product of the odd prime powers found so far.
    radicand: u128,
    /// The part of the value that trial division has not read yet.
    remainder: u128,
}

impl SquareSplit {
    /// Divide one candidate out of the remainder, and move its squares outside.
    ///
    /// The three parts multiply to the value at every step, and the value fits
    /// a `u128`, so no product here overflows.
    fn divide_out(&mut self, divisor: u128) {
        let mut multiplicity: u32 = 0;
        while self.remainder.is_multiple_of(divisor) {
            self.remainder /= divisor;
            multiplicity += 1;
        }
        self.outside *= divisor.pow(multiplicity / 2);
        if multiplicity % 2 == 1 {
            self.radicand *= divisor;
        }
    }

    /// Place the remainder of trial division under the root, or outside it as a square.
    fn finish(&mut self, exhausted: bool) -> Result<(), Undecidable> {
        if self.remainder <= 1 {
            return Ok(());
        }
        if exhausted || self.remainder < SQUAREFREE_CERTAIN {
            self.radicand *= self.remainder;
            return Ok(());
        }
        let root = self.remainder.isqrt();
        if root * root != self.remainder {
            return Err(Undecidable::new("a radicand past the factoring bound"));
        }
        self.outside *= root;
        Ok(())
    }
}

/// Split a radicand wider than `u128`: a perfect square, or a refusal.
fn wide_square(value: &BigInt) -> Result<(BigInt, BigInt), Undecidable> {
    let root = value.sqrt();
    if &(&root * &root) == value {
        Ok((root, BigInt::one()))
    } else {
        Err(Undecidable::new("a radicand past the factoring bound"))
    }
}

/// Demote a quotient to the narrowest canonical variant that holds it.
///
/// The demotion is total and deterministic, which is what makes equality of two
/// canonical forms an equality of two values.
pub(super) fn from_frac(quotient: Frac) -> Canon {
    if quotient.num.is_empty() {
        return Canon::Rational(BigRational::zero());
    }
    if is_one(&quotient.den) {
        return from_sum(quotient.num);
    }
    Canon::Value {
        num: quotient.num,
        den: quotient.den,
    }
}

/// Demote a non-empty sum of terms to the narrowest canonical variant that holds it.
pub(super) fn from_sum(sum: Poly) -> Canon {
    if let Some((monomial, coefficient)) = single_term(&sum) {
        if monomial.is_empty() {
            return Canon::Rational(coefficient.clone());
        }
        if coefficient.is_one()
            && monomial.len() == 1
            && let Some((Atom::Call(name, arguments), 1)) = monomial.iter().next()
        {
            return Canon::Func(name.clone(), arguments.clone());
        }
    }
    match as_radical(&sum) {
        Some(parts) => Canon::Radical(parts),
        None => Canon::Poly(sum),
    }
}

/// Read a sum as a rational combination of roots and constants, if it is one.
fn as_radical(sum: &Poly) -> Option<BTreeMap<Basis, BigRational>> {
    let mut parts = BTreeMap::new();
    for (monomial, coefficient) in sum {
        let mut basis = Basis {
            radicand: BigInt::one(),
            pi: 0,
            e: 0,
        };
        for (atom, exponent) in monomial {
            match atom {
                // A monomial holds one root at most. A second one is not a basis.
                Atom::Sqrt(radicand) if *exponent == 1 && basis.radicand.is_one() => {
                    basis.radicand = radicand.clone();
                }
                Atom::Pi => basis.pi = *exponent,
                Atom::E => basis.e = *exponent,
                _ => return None,
            }
        }
        parts.insert(basis, coefficient.clone());
    }
    Some(parts)
}
