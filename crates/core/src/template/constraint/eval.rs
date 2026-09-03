//! The evaluator of the constraint language, on exact rationals.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::wire::write_rational;
use super::{Cmp, Constraint, ConstraintError, MAX_DIGITS, Term};
use crate::template::domain::{Bindings, gcd_of, is_whole};

/// Evaluate one term against a bound tuple.
///
/// # Errors
///
/// Returns [`ConstraintError`] for an undeclared name, a text binding, a
/// non-whole operand of a whole-number term, a zero divisor, and a number past
/// [`MAX_DIGITS`] digits.
pub fn eval_term(term: &Term, bindings: &Bindings) -> Result<BigRational, ConstraintError> {
    match term {
        Term::Param(name) => bound_number(name, bindings),
        Term::Lit(number) => Ok(number.clone()),
        Term::Add(items) => sum(items, bindings),
        Term::Mul(items) => product(items, bindings),
        Term::Sub(left, right) => difference(left, right, bindings),
        Term::Abs(inner) => Ok(eval_term(inner, bindings)?.abs()),
        Term::Mod(left, right) => remainder(left, right, bindings),
        Term::DigitSum(inner) => digit_sum(inner, bindings),
    }
}

/// The number a parameter is bound to, or the error of a missing or text binding.
fn bound_number(name: &str, bindings: &Bindings) -> Result<BigRational, ConstraintError> {
    let Some(value) = bindings.get(name) else {
        return Err(ConstraintError::UnknownParam {
            name: name.to_string(),
        });
    };
    match value.as_rational() {
        Some(number) => Ok(number.clone()),
        None => Err(ConstraintError::NotNumeric {
            name: name.to_string(),
            text: value.canonical_string(),
        }),
    }
}

/// The sum of the terms, inside the width bound at every step.
fn sum(items: &[Term], bindings: &Bindings) -> Result<BigRational, ConstraintError> {
    let mut total = BigRational::from(BigInt::from(0u8));
    for item in items {
        total += eval_term(item, bindings)?;
        width_ok(&total)?;
    }
    Ok(total)
}

/// The product of the terms, inside the width bound at every step.
fn product(items: &[Term], bindings: &Bindings) -> Result<BigRational, ConstraintError> {
    let mut product = BigRational::from(BigInt::from(1u8));
    for item in items {
        product *= eval_term(item, bindings)?;
        width_ok(&product)?;
    }
    Ok(product)
}

/// The left term less the right term.
fn difference(
    left: &Term,
    right: &Term,
    bindings: &Bindings,
) -> Result<BigRational, ConstraintError> {
    let value = eval_term(left, bindings)? - eval_term(right, bindings)?;
    width_ok(&value)?;
    Ok(value)
}

/// The remainder of the left whole number by the right whole number.
///
/// The remainder takes the sign of the divisor, so `mod(-7, 3)` is 2.
fn remainder(
    left: &Term,
    right: &Term,
    bindings: &Bindings,
) -> Result<BigRational, ConstraintError> {
    let dividend = whole("mod", &eval_term(left, bindings)?)?;
    let divisor = whole("mod", &eval_term(right, bindings)?)?;
    if divisor.is_zero() {
        return Err(ConstraintError::ModByZero);
    }
    Ok(BigRational::from(dividend.mod_floor(&divisor)))
}

/// The sum of the decimal digits of the magnitude of a whole number.
fn digit_sum(inner: &Term, bindings: &Bindings) -> Result<BigRational, ConstraintError> {
    let value = whole("digit_sum", &eval_term(inner, bindings)?)?;
    let digits = decimal_digits(&value)?;
    let total: u64 = digits.iter().map(|digit| u64::from(*digit)).sum();
    Ok(BigRational::from(BigInt::from(total)))
}

/// Whether one constraint holds on a bound tuple.
///
/// # Errors
///
/// Returns [`ConstraintError`] for every term the evaluator cannot decide, and
/// for a whole-number comparison over a fractional term.
pub fn holds(constraint: &Constraint, bindings: &Bindings) -> Result<bool, ConstraintError> {
    let left = eval_term(&constraint.left, bindings)?;
    let right = eval_term(&constraint.right, bindings)?;
    match constraint.op {
        Cmp::Eq => Ok(left == right),
        Cmp::Ne => Ok(left != right),
        Cmp::Lt => Ok(left < right),
        Cmp::Le => Ok(left <= right),
        Cmp::Gt => Ok(left > right),
        Cmp::Ge => Ok(left >= right),
        Cmp::Divides => {
            let divisor = whole("divides", &left)?;
            let dividend = whole("divides", &right)?;
            if divisor.is_zero() {
                return Err(ConstraintError::DividesByZero);
            }
            Ok((dividend % divisor).is_zero())
        }
        Cmp::Coprime => {
            let first = whole("coprime", &left)?;
            let second = whole("coprime", &right)?;
            Ok(gcd_of(&first, &second).is_one())
        }
        Cmp::Carries => {
            let first = whole("carries", &left)?;
            let second = whole("carries", &right)?;
            carries(&first, &second)
        }
    }
}

/// Whether every constraint holds on a bound tuple.
///
/// # Errors
///
/// Returns the first [`ConstraintError`] a constraint raises.
pub fn all_hold(constraints: &[Constraint], bindings: &Bindings) -> Result<bool, ConstraintError> {
    for constraint in constraints {
        if !holds(constraint, bindings)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether the column addition of the two magnitudes carries at least once.
///
/// The predicate reads the decimal digit runs of the two magnitudes, from the
/// ones column upward, and it reports a carry when a column sum reaches ten. The
/// incoming carry needs no state: the first column that carries ends the walk,
/// so every column the walk reads has an incoming carry of zero.
///
/// The predicate reads the magnitudes, because a decimal digit run carries no
/// sign: `carries(-59, 63)` reads the columns of 59 and 63 and reports the carry
/// of `9 + 3`.
fn carries(left: &BigInt, right: &BigInt) -> Result<bool, ConstraintError> {
    let first = decimal_digits(left)?;
    let second = decimal_digits(right)?;
    let width = first.len().max(second.len());
    for column in 0..width {
        let a = column_digit(&first, column);
        let b = column_digit(&second, column);
        if u16::from(a) + u16::from(b) >= 10 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// The digit of one decimal column, counting the ones column as column zero.
///
/// A column past the leading digit reads zero, the way column addition pads the
/// shorter number with leading zeros.
fn column_digit(digits: &[u8], column: usize) -> u8 {
    digits
        .len()
        .checked_sub(column + 1)
        .and_then(|index| digits.get(index).copied())
        .unwrap_or(0)
}

/// The decimal digits of the magnitude of a whole number, most significant first.
fn decimal_digits(value: &BigInt) -> Result<Vec<u8>, ConstraintError> {
    let text = value.magnitude().to_string();
    if text.len() > MAX_DIGITS {
        return Err(ConstraintError::TooWide);
    }
    let mut digits = Vec::with_capacity(text.len());
    for character in text.chars() {
        match character
            .to_digit(10)
            .and_then(|digit| u8::try_from(digit).ok())
        {
            Some(digit) => digits.push(digit),
            None => return Err(ConstraintError::TooWide),
        }
    }
    Ok(digits)
}

/// Read a whole number out of an exact rational, or refuse it.
fn whole(op: &'static str, number: &BigRational) -> Result<BigInt, ConstraintError> {
    if !is_whole(number) {
        return Err(ConstraintError::NotWhole {
            op,
            value: write_rational(number),
        });
    }
    width_ok(number)?;
    Ok(number.numer().clone())
}

/// Refuse a number of more than [`MAX_DIGITS`] decimal digits on either side.
fn width_ok(number: &BigRational) -> Result<(), ConstraintError> {
    let bits = number.numer().bits().max(number.denom().bits());
    // A decimal digit is more than three bits, so this bound is never tighter
    // than MAX_DIGITS digits and it costs no decimal conversion.
    let limit = u64::try_from(MAX_DIGITS)
        .unwrap_or(u64::MAX)
        .saturating_mul(4);
    if bits > limit {
        return Err(ConstraintError::TooWide);
    }
    Ok(())
}
