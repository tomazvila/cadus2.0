//! Exact and sampled curve helpers.

use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::{MAX_EXPONENT, Scalar, VisualError};
use crate::visual::plane::exact_text;

pub(super) fn trig_f64(
    amplitude: &Scalar,
    period: &Scalar,
    phase: &Scalar,
    midline: &Scalar,
    x: f64,
    wave: fn(f64) -> f64,
) -> Option<f64> {
    let (amp, per, ph, mid) = (
        amplitude.to_f64().ok()?,
        period.to_f64().ok()?,
        phase.to_f64().ok()?,
        midline.to_f64().ok()?,
    );
    let angle = std::f64::consts::TAU * (x - ph) / per;
    Some(amp * wave(angle) + mid)
}

/// Whether a trigonometric family is sine or cosine, for the shared exact
/// quarter-period check.
#[derive(Clone, Copy)]
pub(super) enum TrigFamily {
    Sine,
    Cosine,
}

/// The exact value of a sine or cosine curve at `x`, when `x` sits on a
/// quarter of the period from the phase (the only points where the trig value
/// is a small exact rational: `-1`, `0`, or `1`).
pub(super) fn trig_exact(
    amplitude: &Scalar,
    period: &Scalar,
    phase: &Scalar,
    midline: &Scalar,
    x: &BigRational,
    family: TrigFamily,
) -> Option<BigRational> {
    let (amp, per, ph, mid) = (
        amplitude.value().ok()?,
        period.value().ok()?,
        phase.value().ok()?,
        midline.value().ok()?,
    );
    let quarters = (x - ph) / per * BigRational::from_integer(4.into());
    let n = exact_integer(&quarters)?;
    let phase_index = n.rem_euclid(4);
    let value: i32 = match (family, phase_index) {
        (TrigFamily::Sine, 0) => 0,
        (TrigFamily::Sine, 1) => 1,
        (TrigFamily::Sine, 2) => 0,
        (TrigFamily::Sine, _) => -1,
        (TrigFamily::Cosine, 0) => 1,
        (TrigFamily::Cosine, 1) => 0,
        (TrigFamily::Cosine, 2) => -1,
        (TrigFamily::Cosine, _) => 0,
    };
    Some(amp * BigRational::from_integer(value.into()) + mid)
}

/// The integer a rational equals, or `None` when it is not an integer or is
/// larger in magnitude than [`MAX_EXPONENT`].
pub(super) fn exact_integer(value: &BigRational) -> Option<i64> {
    if !value.is_integer() {
        return None;
    }
    let n = value.numer().to_string().parse::<i64>().ok()?;
    if n.unsigned_abs() > MAX_EXPONENT.unsigned_abs() {
        return None;
    }
    Some(n)
}

/// `base` raised to the integer power `exponent`, computed exactly.
pub(super) fn checked_pow(base: &BigRational, exponent: i64) -> Option<BigRational> {
    let magnitude = exponent.unsigned_abs();
    if magnitude > MAX_EXPONENT.unsigned_abs() {
        return None;
    }
    if base.is_zero() && exponent < 0 {
        return None;
    }
    let mut result = BigRational::one();
    for _ in 0..magnitude {
        result *= base.clone();
    }
    if exponent < 0 {
        Some(BigRational::one() / result)
    } else {
        Some(result)
    }
}

/// The integer `n` with `base^n == argument`, searched exactly, or `None` when
/// no such small integer exists.
pub(super) fn integer_log(base: &BigRational, argument: &BigRational) -> Option<i64> {
    if !argument.is_positive() {
        return None;
    }
    (-MAX_EXPONENT..=MAX_EXPONENT).find(|&n| checked_pow(base, n).as_ref() == Some(argument))
}

/// An error unless `value` is nonzero.
pub(super) fn nonzero(what: &'static str, value: &BigRational) -> Result<(), VisualError> {
    if value.is_zero() {
        Err(VisualError::Degenerate {
            reason: format!("{what} is zero"),
        })
    } else {
        Ok(())
    }
}

/// An error unless `value` is positive and not one.
pub(super) fn positive_and_not_one(
    what: &'static str,
    value: &BigRational,
) -> Result<(), VisualError> {
    if !value.is_positive() {
        Err(VisualError::Degenerate {
            reason: format!("{what} is not positive"),
        })
    } else if value.is_one() {
        Err(VisualError::Degenerate {
            reason: format!("{what} is exactly one"),
        })
    } else {
        Ok(())
    }
}

/// The polynomial written as `c0 + c1 x + c2 x^2 + ...`, dropping the terms
/// with a zero coefficient.
pub(super) fn polynomial_text(coefficients: &[Scalar]) -> String {
    let mut terms = Vec::new();
    for (power, c) in coefficients.iter().enumerate() {
        let Ok(value) = c.value() else {
            return "an invalid polynomial".to_owned();
        };
        if value.is_zero() {
            continue;
        }
        let variable = match power {
            0 => String::new(),
            1 => "x".to_owned(),
            _ => format!("x^{power}"),
        };
        terms.push(if power == 0 {
            exact_text(&value)
        } else if value.is_one() {
            variable
        } else if (-&value).is_one() {
            format!("-{variable}")
        } else {
            format!("{} {variable}", exact_text(&value))
        });
    }
    if terms.is_empty() {
        "0".to_owned()
    } else {
        terms.join(" + ")
    }
}
