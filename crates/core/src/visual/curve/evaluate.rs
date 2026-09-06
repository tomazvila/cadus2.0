//! Focused validation and evaluation helpers for curve families.

use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::helpers::{checked_pow, exact_integer, integer_log, nonzero};
use super::{Scalar, VisualError};

pub(super) fn validate_polynomial(coefficients: &[Scalar]) -> Result<(), VisualError> {
    if coefficients.is_empty() {
        return Err(VisualError::Degenerate {
            reason: "a polynomial curve has no coefficients".to_owned(),
        });
    }
    for coefficient in coefficients {
        coefficient.value()?;
    }
    Ok(())
}

pub(super) fn validate_scaled(
    scale: &Scalar,
    h: &Scalar,
    k: &Scalar,
    label: &'static str,
) -> Result<(), VisualError> {
    let scale = scale.value()?;
    h.value()?;
    k.value()?;
    nonzero(label, &scale)
}

pub(super) fn validate_trig(
    amplitude: &Scalar,
    period: &Scalar,
    phase: &Scalar,
    midline: &Scalar,
) -> Result<(), VisualError> {
    let (amplitude, period) = (amplitude.value()?, period.value()?);
    phase.value()?;
    midline.value()?;
    nonzero("the amplitude", &amplitude)?;
    if period.is_positive() {
        Ok(())
    } else {
        Err(VisualError::Degenerate {
            reason: "the period is not positive".to_owned(),
        })
    }
}

pub(super) fn polynomial_exact(coefficients: &[Scalar], x: &BigRational) -> Option<BigRational> {
    let mut total = BigRational::zero();
    let mut power = BigRational::one();
    for coefficient in coefficients {
        total += coefficient.value().ok()? * &power;
        power *= x.clone();
    }
    Some(total)
}

pub(super) fn exponential_exact(
    a: &Scalar,
    b: &Scalar,
    h: &Scalar,
    k: &Scalar,
    x: &BigRational,
) -> Option<BigRational> {
    let (a, b, h, k) = (
        a.value().ok()?,
        b.value().ok()?,
        h.value().ok()?,
        k.value().ok()?,
    );
    let exponent = x - h;
    Some(a * checked_pow(&b, exact_integer(&exponent)?)? + k)
}

pub(super) fn natural_exact(
    a: &Scalar,
    rate: &Scalar,
    h: &Scalar,
    k: &Scalar,
    x: &BigRational,
) -> Option<BigRational> {
    let (a, rate, h, k) = (
        a.value().ok()?,
        rate.value().ok()?,
        h.value().ok()?,
        k.value().ok()?,
    );
    (rate * (x - h)).is_zero().then(|| a + k)
}

pub(super) fn logarithm_exact(
    a: &Scalar,
    base: &Scalar,
    h: &Scalar,
    k: &Scalar,
    x: &BigRational,
) -> Option<BigRational> {
    let (a, base, h, k) = (
        a.value().ok()?,
        base.value().ok()?,
        h.value().ok()?,
        k.value().ok()?,
    );
    let power = integer_log(&base, &(x - h))?;
    Some(a * BigRational::from_integer(power.into()) + k)
}

pub(super) fn reciprocal_exact(
    a: &Scalar,
    h: &Scalar,
    k: &Scalar,
    x: &BigRational,
) -> Option<BigRational> {
    let (a, h, k) = (a.value().ok()?, h.value().ok()?, k.value().ok()?);
    let denominator = x - h;
    (!denominator.is_zero()).then(|| a / denominator + k)
}

pub(super) fn polynomial_f64(coefficients: &[Scalar], x: f64) -> Option<f64> {
    let mut total = 0.0;
    let mut power = 1.0;
    for coefficient in coefficients {
        total += coefficient.to_f64().ok()? * power;
        power *= x;
    }
    Some(total)
}

pub(super) fn exponential_f64(
    a: &Scalar,
    b: &Scalar,
    h: &Scalar,
    k: &Scalar,
    x: f64,
) -> Option<f64> {
    let (a, b, h, k) = (
        a.to_f64().ok()?,
        b.to_f64().ok()?,
        h.to_f64().ok()?,
        k.to_f64().ok()?,
    );
    Some(a * b.powf(x - h) + k)
}

pub(super) fn natural_f64(
    a: &Scalar,
    rate: &Scalar,
    h: &Scalar,
    k: &Scalar,
    x: f64,
) -> Option<f64> {
    let (a, rate, h, k) = (
        a.to_f64().ok()?,
        rate.to_f64().ok()?,
        h.to_f64().ok()?,
        k.to_f64().ok()?,
    );
    Some(a * (rate * (x - h)).exp() + k)
}

pub(super) fn logarithm_f64(
    a: &Scalar,
    base: &Scalar,
    h: &Scalar,
    k: &Scalar,
    x: f64,
) -> Option<f64> {
    let (a, base, h, k) = (
        a.to_f64().ok()?,
        base.to_f64().ok()?,
        h.to_f64().ok()?,
        k.to_f64().ok()?,
    );
    let argument = x - h;
    (argument > 0.0).then(|| a * argument.log(base) + k)
}

pub(super) fn reciprocal_f64(a: &Scalar, h: &Scalar, k: &Scalar, x: f64) -> Option<f64> {
    let (a, h, k) = (a.to_f64().ok()?, h.to_f64().ok()?, k.to_f64().ok()?);
    let denominator = x - h;
    (denominator.abs() >= 1e-9).then(|| a / denominator + k)
}
