//! The arithmetic of the canonicalizer: sums, products, powers, and atoms.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, ToPrimitive, Zero};

use super::sum::{
    bound_terms, extract_square, from_sum, insert_atom, is_one, one_poly, one_term, term,
};
use super::{Atom, Canon, MAX_BITS, Monomial, Poly, Undecidable, Work};

impl Work {
    /// Add two canonical values, over the common denominator.
    ///
    /// Two equal denominators stay one denominator, and two different ones give
    /// the product. The rule is what puts `2/x + 1/(x+1)` and `(3*x+2)/(x*(x+1))`
    /// into one form (M2 review 3, finding 13).
    pub(super) fn add(&mut self, left: &Canon, right: &Canon) -> Result<Canon, Undecidable> {
        let left = self.frac_of(left)?;
        let right = self.frac_of(right)?;
        if left.den == right.den {
            let num = self.poly_add(&left.num, &right.num)?;
            return self.value(num, left.den);
        }
        let left_part = self.poly_mul(&left.num, &right.den)?;
        let right_part = self.poly_mul(&right.num, &left.den)?;
        let num = self.poly_add(&left_part, &right_part)?;
        let den = self.poly_mul(&left.den, &right.den)?;
        self.value(num, den)
    }

    /// Multiply two canonical values.
    pub(super) fn multiply(&mut self, left: &Canon, right: &Canon) -> Result<Canon, Undecidable> {
        let left = self.frac_of(left)?;
        let right = self.frac_of(right)?;
        let num = self.poly_mul(&left.num, &right.num)?;
        let den = self.poly_mul(&left.den, &right.den)?;
        self.value(num, den)
    }

    /// Divide one canonical value by another.
    pub(super) fn divide(&mut self, left: &Canon, right: &Canon) -> Result<Canon, Undecidable> {
        let left = self.frac_of(left)?;
        let right = self.frac_of(right)?;
        let num = self.poly_mul(&left.num, &right.den)?;
        let den = self.poly_mul(&left.den, &right.num)?;
        self.value(num, den)
    }

    /// Raise a canonical value to an integer power.
    pub(super) fn power(&mut self, base: &Canon, exponent: i64) -> Result<Canon, Undecidable> {
        let base = self.frac_of(base)?;
        if base.num.is_empty() {
            return if exponent > 0 {
                Ok(Canon::Rational(BigRational::zero()))
            } else {
                Err(Undecidable::new("a zero base with a non-positive exponent"))
            };
        }
        if exponent == 0 {
            return Ok(Canon::Rational(BigRational::one()));
        }
        if exponent < 0 {
            let magnitude = exponent
                .checked_neg()
                .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
            let num = self.poly_pow(&base.den, magnitude)?;
            let den = self.poly_pow(&base.num, magnitude)?;
            return self.value(num, den);
        }
        let num = self.poly_pow(&base.num, exponent)?;
        let den = self.poly_pow(&base.den, exponent)?;
        self.value(num, den)
    }

    /// Add two sums.
    fn poly_add(&mut self, left: &Poly, right: &Poly) -> Result<Poly, Undecidable> {
        self.rebuild(left)?;
        let mut sum = left.clone();
        for (monomial, coefficient) in right {
            self.spend(1)?;
            self.insert_term(&mut sum, monomial.clone(), coefficient.clone())?;
        }
        bound_terms(&sum)?;
        Ok(sum)
    }

    /// Multiply two sums, term by term.
    ///
    /// The charge is the count of terms of the one sum times the count of terms
    /// of the other, so the common denominators of the rational-function form
    /// stay inside the work bound.
    pub(super) fn poly_mul(&mut self, left: &Poly, right: &Poly) -> Result<Poly, Undecidable> {
        // A factor of 1 is the common case of the form: every value whose
        // denominator is one term carries this exact sum. The short cut skips
        // the term-by-term product, and it still copies the whole sum, so it
        // charges the copy (M2 review 4, finding 2).
        if is_one(left) {
            self.rebuild(right)?;
            return Ok(right.clone());
        }
        if is_one(right) {
            self.rebuild(left)?;
            return Ok(left.clone());
        }
        self.spend(left.len().saturating_mul(right.len()))?;
        let mut product = Poly::new();
        for (left_monomial, left_coefficient) in left {
            for (right_monomial, right_coefficient) in right {
                let (monomial, coefficient) = self.multiply_terms(
                    left_monomial,
                    left_coefficient,
                    right_monomial,
                    right_coefficient,
                )?;
                self.insert_term(&mut product, monomial, coefficient)?;
                bound_terms(&product)?;
            }
        }
        Ok(product)
    }

    /// Raise one sum to a power of one or more.
    fn poly_pow(&mut self, base: &Poly, exponent: i64) -> Result<Poly, Undecidable> {
        if let Some((monomial, coefficient)) = one_term(base) {
            self.spend(monomial.len().saturating_add(1))?;
            let (monomial, coefficient) = self.power_of_term(&monomial, &coefficient, exponent)?;
            return Ok(term(monomial, coefficient));
        }
        self.rebuild(base)?;
        let mut result = one_poly();
        let mut square = base.clone();
        let mut left = exponent;
        while left > 0 {
            if left % 2 == 1 {
                result = self.poly_mul(&result, &square)?;
            }
            left /= 2;
            if left > 0 {
                square = self.poly_mul(&square, &square)?;
            }
        }
        Ok(result)
    }

    /// Add one non-zero term into a sum, and drop a term whose coefficient cancels to zero.
    pub(super) fn insert_term(
        &mut self,
        sum: &mut Poly,
        monomial: Monomial,
        coefficient: BigRational,
    ) -> Result<(), Undecidable> {
        let total = match sum.get(&monomial) {
            Some(present) => present.clone() + coefficient,
            None => coefficient,
        };
        let total = self.bounded(total)?;
        if total.is_zero() {
            sum.remove(&monomial);
        } else {
            sum.insert(monomial, total);
        }
        Ok(())
    }

    /// Multiply two terms into one term.
    fn multiply_terms(
        &mut self,
        left_monomial: &Monomial,
        left_coefficient: &BigRational,
        right_monomial: &Monomial,
        right_coefficient: &BigRational,
    ) -> Result<(Monomial, BigRational), Undecidable> {
        let mut monomial = left_monomial.clone();
        let product = left_coefficient * right_coefficient;
        let mut coefficient = self.bounded(product)?;
        for (atom, exponent) in right_monomial {
            self.add_atom(&mut monomial, &mut coefficient, atom, *exponent)?;
        }
        Ok((monomial, coefficient))
    }

    /// Raise one term to an integer power.
    pub(super) fn power_of_term(
        &mut self,
        monomial: &Monomial,
        coefficient: &BigRational,
        exponent: i64,
    ) -> Result<(Monomial, BigRational), Undecidable> {
        let mut out_monomial = Monomial::new();
        let mut out_coefficient = self.rational_power(coefficient, exponent)?;
        for (atom, atom_exponent) in monomial {
            let scaled = atom_exponent
                .checked_mul(exponent)
                .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
            self.add_atom(&mut out_monomial, &mut out_coefficient, atom, scaled)?;
        }
        Ok((out_monomial, out_coefficient))
    }

    /// Multiply one atom power into a monomial, and move every square into the
    /// coefficient.
    ///
    /// A monomial holds at most one [`Atom::Sqrt`], and that atom always has
    /// exponent 1. `sqrt(2)**3` moves a factor 2 out, and `sqrt(2)*sqrt(3)`
    /// becomes `sqrt(6)`. A monomial holds at most one [`Atom::Exp`] too, and a
    /// power of it moves into its argument.
    pub(super) fn add_atom(
        &mut self,
        monomial: &mut Monomial,
        coefficient: &mut BigRational,
        atom: &Atom,
        exponent: i64,
    ) -> Result<(), Undecidable> {
        if let Atom::Exp(inner) = atom {
            let inner = inner.as_ref().clone();
            return self.add_exp(monomial, &inner, exponent);
        }
        let Atom::Sqrt(radicand) = atom else {
            return insert_atom(monomial, atom, exponent);
        };
        let (squares, rest) = exponent.div_mod_floor(&2);
        let factor = self.int_power(radicand, squares)?;
        *coefficient = self.bounded(&*coefficient * factor)?;
        if rest == 0 {
            return Ok(());
        }
        let present = monomial.iter().find_map(|(key, _)| match key {
            Atom::Sqrt(value) => Some(value.clone()),
            _ => None,
        });
        let Some(present) = present else {
            monomial.insert(Atom::Sqrt(radicand.clone()), 1);
            return Ok(());
        };
        monomial.remove(&Atom::Sqrt(present.clone()));
        let (outside, merged) = extract_square(&(present * radicand))?;
        *coefficient = self.bounded(&*coefficient * BigRational::from_integer(outside))?;
        if !merged.is_one() {
            monomial.insert(Atom::Sqrt(merged), 1);
        }
        Ok(())
    }

    /// Multiply `e**(exponent * inner)` into a monomial.
    ///
    /// The exponent law lives here: a power of the atom scales the argument, and
    /// two exponentials add their arguments. A whole argument gives the atom
    /// [`Atom::E`] instead, so `exp(x)*exp(-x)` is 1 and `exp(x)*exp(2-x)` is
    /// `e**2`.
    ///
    /// The INTEGER PART of the rational constant term gives [`Atom::E`] too, so
    /// `e**(x+2)` and `e**2 * e**x` are one value, and `e**(5/2)` and
    /// `e**2 * e**(1/2)` are one value (M2 review 3, finding 12). The integer
    /// part is the floor, not the truncation: the constant that stays behind is
    /// then always in the half-open range 0 to 1, so `e**(-5/2)`,
    /// `e**-3 * e**(1/2)`, and `e**-2 * e**(-1/2)` are one value too.
    pub(super) fn add_exp(
        &mut self,
        monomial: &mut Monomial,
        inner: &Canon,
        exponent: i64,
    ) -> Result<(), Undecidable> {
        let scaled = if exponent == 1 {
            inner.clone()
        } else {
            let factor = Canon::Rational(BigRational::from_integer(BigInt::from(exponent)));
            self.multiply(&factor, inner)?
        };
        let present = monomial.iter().find_map(|(key, _)| match key {
            Atom::Exp(value) => Some(value.as_ref().clone()),
            _ => None,
        });
        let total = match present {
            Some(value) => {
                monomial.remove(&Atom::Exp(Box::new(value.clone())));
                self.add(&value, &scaled)?
            }
            None => scaled,
        };
        // A collection carries no arithmetic, and a quotient carries no constant
        // term of its own, so both keep the whole argument.
        let Ok(argument) = self.frac_of(&total) else {
            monomial.insert(Atom::Exp(Box::new(total)), 1);
            return Ok(());
        };
        if !is_one(&argument.den) {
            monomial.insert(Atom::Exp(Box::new(total)), 1);
            return Ok(());
        }
        let mut argument = argument.num;
        let constant = Monomial::new();
        if let Some(value) = argument.get(&constant).cloned() {
            let whole = value.floor().to_integer();
            if let Some(step) = whole.to_i64() {
                let rest = self.bounded(value - BigRational::from_integer(whole))?;
                if rest.is_zero() {
                    argument.remove(&constant);
                } else {
                    argument.insert(constant, rest);
                }
                if step != 0 {
                    insert_atom(monomial, &Atom::E, step)?;
                }
            }
        }
        if !argument.is_empty() {
            monomial.insert(Atom::Exp(Box::new(from_sum(argument))), 1);
        }
        Ok(())
    }

    /// Raise a non-zero integer to an integer power, as an exact rational.
    fn int_power(&mut self, base: &BigInt, exponent: i64) -> Result<BigRational, Undecidable> {
        if exponent == 0 {
            return Ok(BigRational::one());
        }
        let magnitude = exponent
            .checked_abs()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| Undecidable::new("an exponent past the size bound"))?;
        if base.bits().saturating_mul(u64::from(magnitude)) > MAX_BITS {
            return Err(Undecidable::new("a number past the size bound"));
        }
        let power = base.pow(magnitude);
        let value = if exponent > 0 {
            BigRational::from_integer(power)
        } else {
            BigRational::new(BigInt::one(), power)
        };
        self.bounded(value)
    }

    /// Raise a non-zero rational to an integer power, as an exact rational.
    fn rational_power(
        &mut self,
        value: &BigRational,
        exponent: i64,
    ) -> Result<BigRational, Undecidable> {
        let numerator = self.int_power(value.numer(), exponent)?;
        let denominator = self.int_power(value.denom(), exponent)?;
        self.bounded(numerator / denominator)
    }
}
