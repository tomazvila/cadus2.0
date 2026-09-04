//! The value list of one domain, and its count.

use std::collections::BTreeSet;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::Signed;

use super::{Domain, DomainError, IntRange, MAX_DECIMAL_SCALE, MAX_DOMAIN_SIZE, Scalar, Value};

impl Domain {
    /// The count of distinct values the domain holds.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] for an empty range, an empty choice list, a
    /// denominator range that holds zero, and a domain past [`MAX_DOMAIN_SIZE`].
    pub fn size(&self, name: &str) -> Result<u64, DomainError> {
        match self {
            Self::Int { low, high } => bounded(name, range_count(*low, *high)?),
            Self::Choice { values } => bounded(name, choice_count(values)?),
            Self::Rational { .. } => Ok(self.values(name)?.len() as u64),
            Self::Decimal { low, high, scale } => {
                bounded(name, decimal_count(*low, *high, *scale)?)
            }
        }
    }

    /// Every value the domain holds, in a deterministic order.
    ///
    /// A rational domain reduces every pair and drops the repeats, so the list
    /// holds each fraction once and the count is a count of distinct problems.
    ///
    /// # Errors
    ///
    /// Returns [`DomainError`] for the same five cases [`Domain::size`] names.
    pub fn values(&self, name: &str) -> Result<Vec<Value>, DomainError> {
        match self {
            Self::Int { low, high } => int_values(name, *low, *high),
            Self::Choice { values } => choice_values(name, values),
            Self::Rational { num, den } => rational_values(name, *num, *den),
            Self::Decimal { low, high, scale } => decimal_values(name, *low, *high, *scale),
        }
    }
}

/// The count of an inclusive whole-number range, or the error of an empty one.
fn range_count(low: i64, high: i64) -> Result<u64, DomainError> {
    IntRange { low, high }
        .count()
        .ok_or(DomainError::EmptyRange { low, high })
}

/// The count of a choice list, or the error of an empty one.
fn choice_count(values: &[Scalar]) -> Result<u64, DomainError> {
    if values.is_empty() {
        return Err(DomainError::EmptyChoice);
    }
    Ok(values.len() as u64)
}

/// The count of a decimal domain, after the scale bound.
fn decimal_count(low: i64, high: i64, scale: u32) -> Result<u64, DomainError> {
    if scale > MAX_DECIMAL_SCALE {
        return Err(DomainError::DecimalScale { scale });
    }
    range_count(low, high)
}

/// Every whole number of an inclusive range, in order.
fn int_values(name: &str, low: i64, high: i64) -> Result<Vec<Value>, DomainError> {
    let count = bounded(name, range_count(low, high)?)?;
    let mut out = Vec::with_capacity(count as usize);
    let mut current = i128::from(low);
    while current <= i128::from(high) {
        out.push(Value::Num(BigRational::from(BigInt::from(current))));
        current += 1;
    }
    Ok(out)
}

/// Every choice, in the order the document writes them.
fn choice_values(name: &str, values: &[Scalar]) -> Result<Vec<Value>, DomainError> {
    bounded(name, choice_count(values)?)?;
    Ok(values.iter().map(Scalar::value).collect())
}

/// Every reduced fraction of a numerator range over a denominator range, once each.
fn rational_values(name: &str, num: IntRange, den: IntRange) -> Result<Vec<Value>, DomainError> {
    if den.low <= 0 && den.high >= 0 {
        return Err(DomainError::ZeroDenominator {
            low: den.low,
            high: den.high,
        });
    }
    let numerators = range_count(num.low, num.high)?;
    let denominators = range_count(den.low, den.high)?;
    bounded(name, numerators.saturating_mul(denominators))?;
    let mut seen = BTreeSet::new();
    let mut numerator = i128::from(num.low);
    while numerator <= i128::from(num.high) {
        let mut denominator = i128::from(den.low);
        while denominator <= i128::from(den.high) {
            seen.insert(reduced(numerator, denominator));
            denominator += 1;
        }
        numerator += 1;
    }
    Ok(seen.into_iter().map(Value::Num).collect())
}

/// Every decimal of a step range, with the spelling of its scale.
fn decimal_values(name: &str, low: i64, high: i64, scale: u32) -> Result<Vec<Value>, DomainError> {
    let count = bounded(name, decimal_count(low, high, scale)?)?;
    let denominator = BigInt::from(10u8).pow(scale);
    let mut out = Vec::with_capacity(count as usize);
    let mut step = i128::from(low);
    while step <= i128::from(high) {
        let mantissa = BigInt::from(step);
        out.push(Value::Spelled {
            text: write_decimal(&mantissa, scale),
            number: BigRational::new(mantissa, denominator.clone()),
        });
        step += 1;
    }
    Ok(out)
}

/// Write a whole number of steps as a decimal of `scale` places.
///
/// The writer pads the magnitude with leading zeros, splits it at the scale, and
/// puts the sign in front: 5 at scale 1 writes `0.5`, and -5 writes `-0.5`.
fn write_decimal(mantissa: &BigInt, scale: u32) -> String {
    let sign = if mantissa.is_negative() { "-" } else { "" };
    let digits = mantissa.magnitude().to_string();
    if scale == 0 {
        return format!("{sign}{digits}");
    }
    let places = scale as usize;
    let padded = if digits.len() <= places {
        format!("{}{digits}", "0".repeat(places - digits.len() + 1))
    } else {
        digits
    };
    let split = padded.len() - places;
    let (whole, fraction) = padded.split_at(split);
    format!("{sign}{whole}.{fraction}")
}

/// Refuse a domain of more than [`MAX_DOMAIN_SIZE`] values.
fn bounded(name: &str, count: u64) -> Result<u64, DomainError> {
    if count > MAX_DOMAIN_SIZE {
        return Err(DomainError::TooLarge {
            name: name.to_string(),
            count,
        });
    }
    Ok(count)
}

/// Build the reduced fraction of a numerator over a non-zero denominator.
fn reduced(numerator: i128, denominator: i128) -> BigRational {
    BigRational::new(BigInt::from(numerator), BigInt::from(denominator))
}
