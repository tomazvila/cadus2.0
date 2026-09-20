//! The exact literals of the tree as `f64` values.

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{ToPrimitive, Zero};

/// The count of binary places below which a positive `f64` is zero.
///
/// The smallest positive `f64` is `2^-1074`. The constant has a margin.
const UNDERFLOW_BITS: u64 = 1100;

/// Return `numerator / denominator`. A zero denominator is `None`.
///
/// The division is exact, and one conversion to `f64` follows. A quotient of
/// two large integers thus stays correct when each part is above the `f64` range.
pub(super) fn ratio(numerator: &BigInt, denominator: &BigInt) -> Option<f64> {
    if denominator.is_zero() {
        return None;
    }
    BigRational::new(numerator.clone(), denominator.clone()).to_f64()
}

/// Return `mantissa / 10^scale`.
///
/// A scale that is much larger than the mantissa gives zero, and the function
/// does not build `10^scale` then. `10^scale` is larger than `2^scale`, so the
/// value is below `2^-1100`, which is zero as an `f64`. This keeps the memory
/// in proportion to the mantissa for each scale.
pub(super) fn decimal(mantissa: &BigInt, scale: u32) -> Option<f64> {
    if u64::from(scale) > mantissa.bits() + UNDERFLOW_BITS {
        return Some(0.0);
    }
    ratio(mantissa, &BigInt::from(10_u32).pow(scale))
}

/// Return `whole + numerator / denominator`. A zero denominator is `None`.
pub(super) fn mixed(whole: &BigInt, numerator: &BigInt, denominator: &BigInt) -> Option<f64> {
    ratio(&(whole * denominator + numerator), denominator)
}
