//! The root law of the canonicalizer: a rational exponent `p/q` (D-F3).
//!
//! `a^(p/q)` reads into a root, and the base decides the form:
//!
//! - A positive rational base factors into primes, and every prime carries its
//!   own rational exponent. The whole part of that exponent moves into the
//!   coefficient, a half becomes [`Atom::Sqrt`], and every other part becomes
//!   [`Atom::Root`] of the prime. `8^(2/3)` is therefore 4, `2^(1/2)` is
//!   `sqrt(2)`, and `2^(1/3) * 4^(1/3)` is 2. The whole part is the floor, so
//!   `2^(-1/3)` and `2^(2/3)/2` are one value.
//! - A base that is one atom carries the exponent on that atom: the whole part
//!   stays on the atom and the rest becomes [`Atom::Root`] of the atom.
//!   `x^(3/2)` and `x*sqrt(x)` are one value, and `sqrt(x)*sqrt(x)` is `x`. The
//!   whole part is the truncation, so `1/sqrt(x)` is the root with the exponent
//!   `-1`, which is the form the function call had before this module.
//! - Every other base is opaque under the root. `(x+1)^(3/2)` is one atom with
//!   the exponent `3/2`, and `(x+1)*sqrt(x+1)` is another form: the form
//!   multiplies no sum into a root, the way it takes no polynomial factor.
//!
//! A square root of a rational keeps the path of [`Work::root_of_rational`], so
//! every value that module decided before keeps its form. A root of index 2 and
//! a root of index 3 or more of one number stay two atoms: `2^(1/2) * 2^(1/3)`
//! and `2^(5/6)` are two forms of one value, which is a documented narrowing.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::sum::{from_sum, insert_atom, is_one, one_term, term};
use super::{Atom, Basis, Canon, Monomial, TRIAL_DIVISION_LIMIT, Undecidable, Work};
use crate::answer::ast::Ast;

/// The refusal of an exponent the root law cannot hold in a machine word.
const EXPONENT_BOUND: &str = "an exponent past the size bound";

impl Work {
    /// Read `base^(p/q)`.
    pub(super) fn rational_exponent_power(
        &mut self,
        base: &Ast,
        numerator: i64,
        denominator: i64,
    ) -> Result<Canon, Undecidable> {
        let base = self.node(base)?;
        self.root_power(&base, numerator, denominator)
    }

    /// Raise a canonical value to the exponent `p/q`, with `q` at least 2.
    ///
    /// A positive rational coefficient of a one-term base takes the root on its
    /// own, so `sqrt(4*x)` is `2*sqrt(x)` and `sqrt(2*x)` is `sqrt(2)*sqrt(x)`.
    pub(super) fn root_power(
        &mut self,
        base: &Canon,
        numerator: i64,
        denominator: i64,
    ) -> Result<Canon, Undecidable> {
        self.spend(1)?;
        if let Canon::Rational(value) = base {
            return self.rational_root(value, numerator, denominator);
        }
        let Some((monomial, coefficient)) = self.scaled_term(base)? else {
            return self.root_term(base, numerator, denominator);
        };
        let scale = self.rational_root(&coefficient, numerator, denominator)?;
        let rest = from_sum(term(monomial, BigRational::one()));
        let rest = self.root_term(&rest, numerator, denominator)?;
        self.multiply(&scale, &rest)
    }

    /// Read the one term of a base whose coefficient is positive and not one.
    fn scaled_term(
        &mut self,
        base: &Canon,
    ) -> Result<Option<(Monomial, BigRational)>, Undecidable> {
        let quotient = self.frac_of(base)?;
        if !is_one(&quotient.den) {
            return Ok(None);
        }
        Ok(one_term(&quotient.num)
            .filter(|(_, coefficient)| coefficient.is_positive() && !coefficient.is_one()))
    }

    /// Build the one-term value `base^(p/q)` of a base that is no rational.
    fn root_term(
        &mut self,
        base: &Canon,
        numerator: i64,
        denominator: i64,
    ) -> Result<Canon, Undecidable> {
        let mut monomial = Monomial::new();
        let mut coefficient = BigRational::one();
        self.add_root(
            &mut monomial,
            &mut coefficient,
            base,
            denominator,
            numerator,
        )?;
        Ok(from_sum(term(monomial, coefficient)))
    }

    /// Raise a rational to `p/q`.
    ///
    /// A square root keeps the path of [`Work::root_of_rational`]. A negative base
    /// under an even or an odd index is no real value the grammar holds, so it
    /// stays one opaque root and compares structurally, the way a negative
    /// radicand does.
    fn rational_root(
        &mut self,
        value: &BigRational,
        numerator: i64,
        denominator: i64,
    ) -> Result<Canon, Undecidable> {
        if value.is_zero() {
            if numerator > 0 {
                return Ok(Canon::Rational(BigRational::zero()));
            }
            return Err(Undecidable::new("a zero base with a non-positive exponent"));
        }
        if denominator == 2 {
            let root = self.root_of_rational(value, vec![Canon::Rational(value.clone())])?;
            return self.power(&root, numerator);
        }
        let mut monomial = Monomial::new();
        let mut coefficient = BigRational::one();
        self.add_root(
            &mut monomial,
            &mut coefficient,
            &Canon::Rational(value.clone()),
            denominator,
            numerator,
        )?;
        Ok(from_sum(term(monomial, coefficient)))
    }

    /// The root law: multiply `base^(exponent/index)` into a monomial.
    ///
    /// The base decides the rule, as the module header says. A root of a root
    /// multiplies the two indices, and a root of `e` is an exponential.
    pub(super) fn add_root(
        &mut self,
        monomial: &mut Monomial,
        coefficient: &mut BigRational,
        base: &Canon,
        index: i64,
        exponent: i64,
    ) -> Result<(), Undecidable> {
        self.spend(1)?;
        if exponent == 0 {
            return Ok(());
        }
        match single_atom(base) {
            Some(Atom::Root(inner, inner_index)) => {
                let index = checked_product(index, inner_index)?;
                self.add_root(monomial, coefficient, &inner, index, exponent)
            }
            Some(Atom::Sqrt(radicand)) => {
                let index = checked_product(index, 2)?;
                let radicand = Canon::Rational(BigRational::from_integer(radicand));
                self.add_root(monomial, coefficient, &radicand, index, exponent)
            }
            Some(Atom::E) => self.add_exp(monomial, &Canon::Rational(ratio(exponent, index)), 1),
            Some(Atom::Exp(inner)) => {
                let scaled = self.multiply(&Canon::Rational(ratio(exponent, index)), &inner)?;
                self.add_exp(monomial, &scaled, 1)
            }
            Some(atom) => add_atom_root(monomial, base, &atom, index, exponent),
            None => match base {
                Canon::Rational(value) if value.is_positive() => {
                    self.add_rational_root(monomial, coefficient, value, index, exponent)
                }
                _ => add_opaque_root(monomial, base, index, exponent),
            },
        }
    }

    /// Multiply the root of a positive rational into a monomial, prime by prime.
    fn add_rational_root(
        &mut self,
        monomial: &mut Monomial,
        coefficient: &mut BigRational,
        value: &BigRational,
        index: i64,
        exponent: i64,
    ) -> Result<(), Undecidable> {
        for (prime, count) in prime_factors(value.numer())? {
            let exponent = checked_product(count, exponent)?;
            self.add_prime_root(monomial, coefficient, &prime, exponent, index)?;
        }
        for (prime, count) in prime_factors(value.denom())? {
            let exponent = checked_product(count, exponent)?;
            let exponent = exponent
                .checked_neg()
                .ok_or_else(|| Undecidable::new(EXPONENT_BOUND))?;
            self.add_prime_root(monomial, coefficient, &prime, exponent, index)?;
        }
        Ok(())
    }

    /// Multiply `prime^(numerator/denominator)` into a monomial.
    ///
    /// The whole part of the exponent is the floor, and it moves into the
    /// coefficient. A half becomes [`Atom::Sqrt`], which is the form
    /// [`Work::root_of_rational`] builds, so `2^(3/6)` and `sqrt(2)` are one
    /// value.
    fn add_prime_root(
        &mut self,
        monomial: &mut Monomial,
        coefficient: &mut BigRational,
        prime: &BigInt,
        numerator: i64,
        denominator: i64,
    ) -> Result<(), Undecidable> {
        let base = Canon::Rational(BigRational::from_integer(prime.clone()));
        let present = take_root(monomial, &base);
        let (numerator, denominator) = exponent_sum(numerator, denominator, present)?;
        let (whole, part) = numerator.div_mod_floor(&denominator);
        if whole != 0 {
            let factor = self.int_power(prime, whole)?;
            *coefficient = self.bounded(&*coefficient * factor)?;
        }
        if part == 0 {
            return Ok(());
        }
        if denominator == 2 {
            return self.add_atom(monomial, coefficient, &Atom::Sqrt(prime.clone()), 1);
        }
        insert_atom(monomial, &Atom::Root(Box::new(base), denominator), part)
    }
}

/// Multiply the root of one plain atom into a monomial.
///
/// The whole part of the exponent is the truncation, and it stays on the atom.
/// The rest becomes the root atom, so `x^(3/2)` is `x * x^(1/2)` and
/// `x^(-3/2)` is `x^-1 * x^(-1/2)`.
fn add_atom_root(
    monomial: &mut Monomial,
    base: &Canon,
    atom: &Atom,
    index: i64,
    exponent: i64,
) -> Result<(), Undecidable> {
    let present = take_root(monomial, base);
    let (numerator, denominator) = exponent_sum(exponent, index, present)?;
    let whole = numerator / denominator;
    let part = numerator % denominator;
    if whole != 0 {
        insert_atom(monomial, atom, whole)?;
    }
    if part != 0 {
        insert_atom(
            monomial,
            &Atom::Root(Box::new(base.clone()), denominator),
            part,
        )?;
    }
    Ok(())
}

/// Multiply the root of an opaque base into a monomial.
///
/// The exponent stays one rational on one atom. An index of 1 is a whole power
/// of a sum that the monomial cannot multiply out, so the atom keeps it.
fn add_opaque_root(
    monomial: &mut Monomial,
    base: &Canon,
    index: i64,
    exponent: i64,
) -> Result<(), Undecidable> {
    let present = take_root(monomial, base);
    let (numerator, denominator) = exponent_sum(exponent, index, present)?;
    if numerator != 0 {
        insert_atom(
            monomial,
            &Atom::Root(Box::new(base.clone()), denominator),
            numerator,
        )?;
    }
    Ok(())
}

/// Read the base as one atom with exponent 1 and coefficient 1, if it is one.
fn single_atom(base: &Canon) -> Option<Atom> {
    match base {
        Canon::Poly(sum) if sum.len() == 1 => {
            let (monomial, coefficient) = sum.iter().next()?;
            if !coefficient.is_one() || monomial.len() != 1 {
                return None;
            }
            let (atom, exponent) = monomial.iter().next()?;
            (*exponent == 1).then(|| atom.clone())
        }
        Canon::Radical(parts) if parts.len() == 1 => {
            let (basis, coefficient) = parts.iter().next()?;
            if !coefficient.is_one() {
                return None;
            }
            basis_atom(basis)
        }
        Canon::Func(name, arguments) => Some(Atom::Call(name.clone(), arguments.clone())),
        _ => None,
    }
}

/// Read a basis of one root, one `pi`, or one `e` as that atom.
fn basis_atom(basis: &Basis) -> Option<Atom> {
    let root = !basis.radicand.is_one();
    match (root, basis.pi, basis.e) {
        (true, 0, 0) => Some(Atom::Sqrt(basis.radicand.clone())),
        (false, 1, 0) => Some(Atom::Pi),
        (false, 0, 1) => Some(Atom::E),
        _ => None,
    }
}

/// Take the root atom of `base` out of a monomial, and return its index and exponent.
fn take_root(monomial: &mut Monomial, base: &Canon) -> Option<(i64, i64)> {
    let found = monomial.iter().find_map(|(atom, exponent)| match atom {
        Atom::Root(inner, index) if inner.as_ref() == base => Some((*index, *exponent)),
        _ => None,
    })?;
    monomial.remove(&Atom::Root(Box::new(base.clone()), found.0));
    Some(found)
}

/// Add a rational exponent to the one a root atom carried, in lowest terms.
fn exponent_sum(
    numerator: i64,
    denominator: i64,
    present: Option<(i64, i64)>,
) -> Result<(i64, i64), Undecidable> {
    let mut total = ratio(numerator, denominator);
    if let Some((index, exponent)) = present {
        total += ratio(exponent, index);
    }
    if total.is_zero() {
        return Ok((0, 1));
    }
    match (total.numer().to_i64(), total.denom().to_i64()) {
        (Some(numerator), Some(denominator)) => Ok((numerator, denominator)),
        _ => Err(Undecidable::new(EXPONENT_BOUND)),
    }
}

/// Build the exact rational `numerator / denominator`.
fn ratio(numerator: i64, denominator: i64) -> BigRational {
    BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
}

/// Multiply two exponents, or refuse an overflow.
fn checked_product(left: i64, right: i64) -> Result<i64, Undecidable> {
    left.checked_mul(right)
        .ok_or_else(|| Undecidable::new(EXPONENT_BOUND))
}

/// Factor a positive integer into primes by trial division.
///
/// The division runs to [`TRIAL_DIVISION_LIMIT`]. A remainder with no factor
/// below its own square root is 1 or a prime. A remainder past the square of
/// the limit is one the division cannot prove prime, so the root is refused: a
/// refusal is safe, and an unreduced radicand would compare wrong (C4).
///
/// # Errors
///
/// Returns [`Undecidable`] for a value wider than `u128` and for a remainder
/// past the factoring bound.
fn prime_factors(value: &BigInt) -> Result<Vec<(BigInt, i64)>, Undecidable> {
    let past_bound = || Undecidable::new("a radicand past the factoring bound");
    let mut remainder = value.to_u128().ok_or_else(past_bound)?;
    let mut factors = Vec::new();
    let mut divisor: u128 = 2;
    let mut exhausted = false;
    while divisor <= TRIAL_DIVISION_LIMIT {
        if divisor > remainder / divisor {
            exhausted = true;
            break;
        }
        let mut count = 0_i64;
        while remainder.is_multiple_of(divisor) {
            remainder /= divisor;
            count += 1;
        }
        if count > 0 {
            factors.push((BigInt::from(divisor), count));
        }
        divisor += 1;
    }
    if remainder > 1 {
        if !exhausted && remainder > TRIAL_DIVISION_LIMIT * TRIAL_DIVISION_LIMIT {
            return Err(past_bound());
        }
        factors.push((BigInt::from(remainder), 1));
    }
    Ok(factors)
}
