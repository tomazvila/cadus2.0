//! Logarithms: one value for every spelling of one number.
//!
//! `ln(8)` and `3*ln(2)` are one value, and so are `log(45)`, `log10(45)`,
//! `ln(45)/ln(10)`, and `log(5)+log(9)`. A positive rational argument splits
//! into the natural logarithms of its prime factors, and every other argument
//! stays one `ln` atom. `log(x)` has base 10, `log(x, b)` has base `b`, and
//! each of them is a quotient of natural logarithms.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

use super::sum::atom_value;
use super::{Atom, Canon, TRIAL_DIVISION_LIMIT, Undecidable, Work};

impl Work {
    /// Read `ln(argument)`, `log(argument)`, or `log(argument, base)`.
    pub(super) fn logarithm(
        &mut self,
        name: &str,
        arguments: &[Canon],
    ) -> Result<Canon, Undecidable> {
        let ten = Canon::Rational(BigRational::from_integer(BigInt::from(10)));
        match (name, arguments) {
            ("ln", [argument]) => self.natural_log(argument),
            ("log", [Canon::Rational(value)])
                if exact_power(value, &BigRational::from_integer(BigInt::from(10))).is_some() =>
            {
                let k = exact_power(value, &BigRational::from_integer(BigInt::from(10)));
                Ok(Canon::Rational(BigRational::from_integer(BigInt::from(
                    k.unwrap_or(0),
                ))))
            }
            ("log", [Canon::Rational(value), Canon::Rational(base)])
                if exact_power(value, base).is_some() =>
            {
                let k = exact_power(value, base).unwrap_or(0);
                Ok(Canon::Rational(BigRational::from_integer(BigInt::from(k))))
            }
            ("log", [argument]) => {
                let top = self.natural_log(argument)?;
                let bottom = self.natural_log(&ten)?;
                self.divide(&top, &bottom)
            }
            ("log", [argument, base]) => {
                let top = self.natural_log(argument)?;
                let bottom = self.natural_log(base)?;
                self.divide(&top, &bottom)
            }
            _ => Ok(atom_value(Atom::Call(name.to_string(), arguments.to_vec()))),
        }
    }

    /// The natural logarithm of one canonical value.
    fn natural_log(&mut self, argument: &Canon) -> Result<Canon, Undecidable> {
        self.spend(1)?;
        let opaque =
            |argument: &Canon| atom_value(Atom::Call("ln".to_string(), vec![argument.clone()]));
        let Canon::Rational(value) = argument else {
            return Ok(opaque(argument));
        };
        if !value.is_positive() {
            return Ok(opaque(argument));
        }
        if value.is_one() {
            return Ok(Canon::Rational(BigRational::zero()));
        }
        let (Some(top), Some(bottom)) = (value.numer().to_u64(), value.denom().to_u64()) else {
            return Ok(opaque(argument));
        };
        let mut total = Canon::Rational(BigRational::zero());
        for (prime, power) in factor(top)
            .into_iter()
            .map(|(p, k)| (p, i64::from(k)))
            .chain(factor(bottom).into_iter().map(|(p, k)| (p, -i64::from(k))))
        {
            let scale = Canon::Rational(BigRational::from_integer(BigInt::from(power)));
            // The logarithm of 5 is written as `ln(10) - ln(2)`, so `ln(10)` is
            // one atom and the base 10 divides without a sum in the denominator.
            let log = if prime == 5 {
                let ten = ln_atom(10);
                let two = ln_atom(2);
                let minus_two = self.multiply(&Canon::Rational(-BigRational::one()), &two)?;
                self.add(&ten, &minus_two)?
            } else {
                ln_atom(prime)
            };
            let term = self.multiply(&scale, &log)?;
            total = self.add(&total, &term)?;
        }
        Ok(total)
    }
}

/// The atom `ln(n)` for a whole number `n`.
fn ln_atom(n: u64) -> Canon {
    let number = Canon::Rational(BigRational::from_integer(BigInt::from(n)));
    atom_value(Atom::Call("ln".to_string(), vec![number]))
}

/// The whole number `k` with `base^k = value`, when one exists with `|k| <= 64`.
fn exact_power(value: &BigRational, base: &BigRational) -> Option<i64> {
    if !base.is_positive() || base.is_one() || !value.is_positive() {
        return None;
    }
    let mut power = BigRational::one();
    for k in 0..=64_i64 {
        if &power == value {
            return Some(k);
        }
        if power.numer().bits() > 256 || power.denom().bits() > 256 {
            break;
        }
        power *= base;
    }
    let inverse = base.recip();
    power = BigRational::one();
    for k in 0..=64_i64 {
        if &power == value {
            return Some(-k);
        }
        if power.numer().bits() > 256 || power.denom().bits() > 256 {
            break;
        }
        power *= &inverse;
    }
    None
}

/// The prime factors of a whole number with their counts. A cofactor that the
/// trial division does not finish stays whole as one factor.
fn factor(mut value: u64) -> Vec<(u64, u32)> {
    let mut factors = Vec::new();
    let mut divisor = 2_u64;
    while divisor <= u64::try_from(TRIAL_DIVISION_LIMIT).unwrap_or(10_000)
        && divisor * divisor <= value
    {
        let mut count = 0;
        while value.is_multiple_of(divisor) {
            value /= divisor;
            count += 1;
        }
        if count > 0 {
            factors.push((divisor, count));
        }
        divisor += 1;
    }
    if value > 1 {
        factors.push((value, 1));
    }
    factors
}
