//! The two power nodes of the tree as `f64` operations.

/// Return `base^exponent` for a whole exponent.
///
/// An exponent in the `i32` range uses `powi`. An exponent outside that range
/// uses `powf` on the magnitude, and the sign comes from the parity of the
/// exponent, because the `f64` form of such an exponent has no parity.
pub(super) fn int_pow(base: f64, exponent: i64) -> f64 {
    if let Ok(small) = i32::try_from(exponent) {
        return base.powi(small);
    }
    let magnitude = base.abs().powf(exponent as f64);
    if base < 0.0 && exponent % 2 != 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// Return the real value of `base^(numerator/denominator)`.
///
/// A negative base with an even denominator has no real root, so it is `None`.
/// A negative base with an odd denominator gives the real odd root. The parser
/// writes a denominator of 2 or more; a denominator below 1 is `None`.
pub(super) fn rational_pow(base: f64, numerator: i64, denominator: i64) -> Option<f64> {
    if denominator < 1 || (base < 0.0 && denominator % 2 == 0) {
        return None;
    }
    let magnitude = base.abs();
    let root = match denominator {
        2 => magnitude.sqrt(),
        3 => magnitude.cbrt(),
        _ => magnitude.powf(1.0 / denominator as f64),
    };
    Some(int_pow(root.copysign(base), numerator))
}
