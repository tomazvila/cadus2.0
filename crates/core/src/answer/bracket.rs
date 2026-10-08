//! Exact brackets of the constants `pi`, `e`, `ln(n)`, and of roots (D6).
//!
//! The rounding rule of [`super::rounding`] decides a learner decimal against an
//! authored value that is not a rational: `ln(2)`, `0.88*pi`, `e^2`. It needs two
//! exact rationals that hold the value between them, and it refines the pair
//! until the half-unit bound of the last digit falls outside the pair. Every
//! step here is integer arithmetic. The error of each series is counted in
//! units of the last fixed-point place and added to both ends, so a bracket
//! always holds the true value (V2: no guess, no float).

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

/// A closed interval of exact rationals.
pub(super) type Interval = (BigRational, BigRational);

/// The largest power of one atom the bracket accepts.
const MAX_POWER: i64 = 8;

/// The guard bits that absorb the error counts of the series.
const GUARD: usize = 64;

/// Build an interval from a fixed-point value, its error in units, and its scale.
fn around(value: &BigInt, error: u64, bits: usize) -> Interval {
    let unit = BigInt::one() << bits;
    let error = BigInt::from(error);
    (
        BigRational::new(value - &error, unit.clone()),
        BigRational::new(value + &error, unit),
    )
}

/// `arctan(1/n)` as a fixed-point value with `scale` bits, and its error in units.
fn arctan_inverse(n: u32, scale: usize) -> (BigInt, u64) {
    let one = BigInt::one() << scale;
    let square = BigInt::from(n) * BigInt::from(n);
    let mut power = &one / BigInt::from(n);
    let mut total = BigInt::zero();
    let mut k = 0_u64;
    let mut sign = true;
    while !power.is_zero() {
        let term = &power / BigInt::from(2 * k + 1);
        if sign {
            total += term;
        } else {
            total -= term;
        }
        sign = !sign;
        power /= &square;
        k += 1;
    }
    (total, 2 * k + 2)
}

/// Bracket `pi` with `bits` bits.
pub(super) fn pi(bits: usize) -> Interval {
    let scale = bits + GUARD;
    let (five, five_error) = arctan_inverse(5, scale);
    let (n239, n239_error) = arctan_inverse(239, scale);
    // Machin: pi = 16 arctan(1/5) - 4 arctan(1/239).
    let value = five * 16 - n239 * 4;
    let error = five_error * 16 + n239_error * 4;
    scaled_down(&value, error, scale, bits)
}

/// Bracket `e` with `bits` bits.
pub(super) fn e(bits: usize) -> Interval {
    let scale = bits + GUARD;
    let mut term = BigInt::one() << scale;
    let mut total = BigInt::zero();
    let mut k = 1_u64;
    while !term.is_zero() {
        total += &term;
        term /= BigInt::from(k);
        k += 1;
    }
    scaled_down(&total, k + 2, scale, bits)
}

/// Bracket `ln(n)` for a whole number `n` of 2 or more, with `bits` bits.
///
/// The number splits as `2^k * m` with `m` in the half-open range 1 to 2, and
/// `ln(m)` is `2 * arctanh((m - 1) / (m + 1))`, a series with ratio below 1/9.
pub(super) fn ln(n: &BigInt, bits: usize) -> Option<Interval> {
    if n < &BigInt::from(2) || n.bits() > 512 {
        return None;
    }
    let scale = bits + GUARD;
    let k = i64::try_from(n.bits()).ok()? - 1;
    let m = BigRational::new(n.clone(), BigInt::one() << usize::try_from(k).ok()?);
    let (ln_two, ln_two_error) = atanh_ratio(&BigRational::new(1.into(), 3.into()), scale);
    let (ln_m, ln_m_error) = atanh_ratio(
        &((&m - BigRational::one()) / (&m + BigRational::one())),
        scale,
    );
    let value = ln_two * 2 * BigInt::from(k) + ln_m * 2;
    let error = ln_two_error * 2 * k.unsigned_abs() + ln_m_error * 2;
    Some(scaled_down(&value, error, scale, bits))
}

/// `arctanh(z)` for a rational `z` in the range 0 to 1/3, as a fixed-point value.
fn atanh_ratio(z: &BigRational, scale: usize) -> (BigInt, u64) {
    let one = BigInt::one() << scale;
    let mut power = BigRational::from_integer(one) * z;
    let square = z * z;
    let mut total = BigInt::zero();
    let mut k = 0_u64;
    loop {
        let term = (&power / BigRational::from_integer(BigInt::from(2 * k + 1))).floor();
        if term.is_zero() {
            break;
        }
        total += term.to_integer();
        power *= &square;
        k += 1;
    }
    (total, k + 2)
}

/// Move a fixed-point bracket from `scale` bits down to `bits` bits.
fn scaled_down(value: &BigInt, error: u64, scale: usize, bits: usize) -> Interval {
    let exact = around(value, error, scale);
    let unit = BigInt::one() << bits;
    let widen = BigRational::new(BigInt::one(), unit);
    (exact.0 - widen.clone(), exact.1 + widen)
}

/// Bracket `base^(1/index)` for a positive rational, with `bits` bits.
pub(super) fn root(base: &BigRational, index: i64, bits: usize) -> Option<Interval> {
    let index = u32::try_from(index).ok().filter(|index| *index >= 2)?;
    let unit = BigInt::one() << bits;
    let scaled = unit.pow(index);
    // `a^(1/q) * 2^bits` lies in one unit above its floor, and so does the root
    // of the denominator. The quotient lies between the two outer fractions.
    let top = (base.numer() * &scaled).nth_root(index);
    let bottom = (base.denom() * &scaled).nth_root(index);
    if bottom.is_zero() {
        return None;
    }
    let low = BigRational::new(top.clone(), &bottom + 1);
    let high = BigRational::new(top + 1, bottom);
    Some((low, high))
}

/// Multiply two positive intervals.
pub(super) fn times(left: &Interval, right: &Interval) -> Interval {
    (&left.0 * &right.0, &left.1 * &right.1)
}

/// Raise a positive interval to a whole power of `MAX_POWER` in size or less.
pub(super) fn power(base: &Interval, exponent: i64) -> Option<Interval> {
    if exponent.abs() > MAX_POWER || !base.0.is_positive() {
        return None;
    }
    let mut result = (BigRational::one(), BigRational::one());
    for _ in 0..exponent.abs() {
        result = times(&result, base);
    }
    if exponent < 0 {
        result = (result.1.recip(), result.0.recip());
    }
    Some(result)
}

/// Scale an interval by a rational, and keep the ends in order.
pub(super) fn scale(interval: &Interval, factor: &BigRational) -> Interval {
    let (low, high) = (&interval.0 * factor, &interval.1 * factor);
    if factor.is_negative() {
        (high, low)
    } else {
        (low, high)
    }
}

/// Add two intervals.
pub(super) fn plus(left: &Interval, right: &Interval) -> Interval {
    (&left.0 + &right.0, &left.1 + &right.1)
}
