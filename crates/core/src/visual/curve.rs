//! The curve figure: a coordinate plane with one continuous function drawn on
//! it, plus the exact key points and asymptotes that describe its shape.
//!
//! The drawn line is a sampled approximation, the same way any graphing tool
//! draws a smooth curve. What is never approximate is the mathematics an
//! author asserts: every [`LabeledPoint`] in `key_points` must sit exactly on
//! the curve, checked with exact rational arithmetic wherever the family
//! allows it (a polynomial at any rational `x`, an exponential or logarithm at
//! the integer steps of its base, a reciprocal at any rational `x` off its
//! asymptote, a sine or cosine at a quarter-period). A key point the check
//! cannot confirm exactly counts as wrong, never as unchecked.

use num_rational::BigRational;
use num_traits::{One, Signed, Zero};
use serde::{Deserialize, Serialize};

use super::{LabeledPoint, Scalar, VisualError, inside, tick_count};

mod helpers;

use helpers::{
    TrigFamily, checked_pow, exact_integer, integer_log, nonzero, polynomial_text,
    positive_and_not_one, trig_exact, trig_f64,
};

/// The largest exponent magnitude an exact check computes.
///
/// Curriculum content never needs more; the bound keeps validation bounded
/// work regardless of what an author writes.
const MAX_EXPONENT: i64 = 64;

/// One continuous function family a curve figure can draw.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case", deny_unknown_fields)]
pub enum CurveKind {
    /// `y = c[0] + c[1] x + c[2] x^2 + ...`, exact at every rational `x`.
    Polynomial {
        /// The coefficients, constant term first.
        coefficients: Vec<Scalar>,
    },
    /// `y = a * b^(x - h) + k`, exact whenever `x - h` is an integer.
    Exponential {
        /// The vertical scale. Never zero.
        a: Scalar,
        /// The base. Positive, and never one.
        b: Scalar,
        /// The horizontal shift.
        h: Scalar,
        /// The vertical shift.
        k: Scalar,
    },
    /// `y = a * e^(rate * (x - h)) + k`, where `e` is the named natural
    /// exponential constant. Exact key-point validation is available at
    /// `x = h`, where the exponent is exactly zero.
    NaturalExponential {
        /// The vertical scale. Never zero.
        a: Scalar,
        /// The coefficient of the exponent. Never zero.
        rate: Scalar,
        /// The horizontal shift.
        h: Scalar,
        /// The vertical shift.
        k: Scalar,
    },
    /// `y = a * log_b(x - h) + k`, exact whenever `x - h` is an integer power
    /// of `b`.
    Logarithm {
        /// The vertical scale. Never zero.
        a: Scalar,
        /// The base. Positive, and never one.
        base: Scalar,
        /// The horizontal shift; the vertical asymptote sits at `x = h`.
        h: Scalar,
        /// The vertical shift.
        k: Scalar,
    },
    /// `y = a / (x - h) + k`, exact at every rational `x` off the asymptote.
    Reciprocal {
        /// The scale. Never zero.
        a: Scalar,
        /// The horizontal shift; the vertical asymptote sits at `x = h`.
        h: Scalar,
        /// The vertical shift; the horizontal asymptote sits at `y = k`.
        k: Scalar,
    },
    /// `y = amplitude * sin(2π (x - phase) / period) + midline`.
    Sine {
        /// The height above and below the midline. Never zero.
        amplitude: Scalar,
        /// The horizontal length of one full cycle. Positive.
        period: Scalar,
        /// The horizontal shift.
        phase: Scalar,
        /// The vertical shift: the horizontal line the curve oscillates about.
        midline: Scalar,
    },
    /// `y = amplitude * cos(2π (x - phase) / period) + midline`.
    Cosine {
        /// The height above and below the midline. Never zero.
        amplitude: Scalar,
        /// The horizontal length of one full cycle. Positive.
        period: Scalar,
        /// The horizontal shift.
        phase: Scalar,
        /// The vertical shift: the horizontal line the curve oscillates about.
        midline: Scalar,
    },
}

/// A dashed reference line a curve approaches but never reaches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "axis", rename_all = "snake_case", deny_unknown_fields)]
pub enum Asymptote {
    /// A horizontal asymptote, `y = at`.
    Horizontal {
        /// Where the line sits.
        at: Scalar,
    },
    /// A vertical asymptote, `x = at`.
    Vertical {
        /// Where the line sits.
        at: Scalar,
    },
}

/// A coordinate plane with one drawn curve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurveFigure {
    /// The left end of the drawn x range.
    pub x_min: Scalar,
    /// The right end of the drawn x range.
    pub x_max: Scalar,
    /// The bottom end of the drawn y range.
    pub y_min: Scalar,
    /// The top end of the drawn y range.
    pub y_max: Scalar,
    /// The distance between two x ticks.
    pub x_tick: Scalar,
    /// The distance between two y ticks.
    pub y_tick: Scalar,
    /// The function the figure draws.
    pub curve: CurveKind,
    /// The exact points the curve is checked to pass through.
    #[serde(default)]
    pub key_points: Vec<LabeledPoint>,
    /// The asymptotes the curve is drawn approaching.
    #[serde(default)]
    pub asymptotes: Vec<Asymptote>,
    /// The sentence that leads the accessible equivalent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

impl CurveFigure {
    /// The tick counts of the two axes.
    pub fn ticks(&self) -> Result<(i64, i64), VisualError> {
        Ok((
            tick_count("x", &self.x_min, &self.x_max, &self.x_tick)?,
            tick_count("y", &self.y_min, &self.y_max, &self.y_tick)?,
        ))
    }

    /// Whether the figure carries mathematical meaning.
    pub fn validate(&self) -> Result<(), VisualError> {
        self.ticks()?;
        self.curve.validate_parameters()?;
        for point in &self.key_points {
            inside("a key point", &self.x_min, &self.x_max, &point.x)?;
            inside("a key point", &self.y_min, &self.y_max, &point.y)?;
            let (x, y) = point.value()?;
            let exact = self
                .curve
                .exact_value_at(&x)
                .ok_or_else(|| off_curve(point))?;
            if exact != y {
                return Err(off_curve(point));
            }
        }
        for asymptote in &self.asymptotes {
            match asymptote {
                Asymptote::Horizontal { at } => {
                    inside("an asymptote", &self.y_min, &self.y_max, at)?
                }
                Asymptote::Vertical { at } => inside("an asymptote", &self.x_min, &self.x_max, at)?,
            }
        }
        Ok(())
    }

    /// The accessible equivalent.
    #[must_use]
    pub fn text_equivalent(&self) -> String {
        let mut out = format!(
            "A coordinate plane with x from {} to {} and y from {} to {}. \
             The x ticks step by {} and the y ticks step by {}. The curve {}.",
            self.x_min,
            self.x_max,
            self.y_min,
            self.y_max,
            self.x_tick,
            self.y_tick,
            self.curve.describe()
        );
        for point in &self.key_points {
            out.push_str(&format!(" A key point at {}.", point.spoken()));
        }
        for asymptote in &self.asymptotes {
            match asymptote {
                Asymptote::Horizontal { at } => {
                    out.push_str(&format!(" A horizontal asymptote at y = {at}."));
                }
                Asymptote::Vertical { at } => {
                    out.push_str(&format!(" A vertical asymptote at x = {at}."));
                }
            }
        }
        out
    }
}

/// The error a key point that misses the curve gives.
fn off_curve(point: &LabeledPoint) -> VisualError {
    VisualError::KeyPointOffCurve {
        point: point.spoken(),
    }
}

impl CurveKind {
    /// Whether the function's own parameters carry mathematical meaning,
    /// independent of any key point.
    fn validate_parameters(&self) -> Result<(), VisualError> {
        match self {
            Self::Polynomial { coefficients } => {
                if coefficients.is_empty() {
                    return Err(VisualError::Degenerate {
                        reason: "a polynomial curve has no coefficients".to_owned(),
                    });
                }
                for c in coefficients {
                    c.value()?;
                }
                Ok(())
            }
            Self::Exponential { a, b, h, k } => {
                let (av, bv) = (a.value()?, b.value()?);
                h.value()?;
                k.value()?;
                nonzero("the exponential scale a", &av)?;
                positive_and_not_one("the exponential base b", &bv)
            }
            Self::NaturalExponential { a, rate, h, k } => {
                let (av, ratev) = (a.value()?, rate.value()?);
                h.value()?;
                k.value()?;
                nonzero("the natural exponential scale a", &av)?;
                nonzero("the natural exponential rate", &ratev)
            }
            Self::Logarithm { a, base, h, k } => {
                let (av, basev) = (a.value()?, base.value()?);
                h.value()?;
                k.value()?;
                nonzero("the logarithm scale a", &av)?;
                positive_and_not_one("the logarithm base", &basev)
            }
            Self::Reciprocal { a, h, k } => {
                let av = a.value()?;
                h.value()?;
                k.value()?;
                nonzero("the reciprocal scale a", &av)
            }
            Self::Sine {
                amplitude,
                period,
                phase,
                midline,
            }
            | Self::Cosine {
                amplitude,
                period,
                phase,
                midline,
            } => {
                let (amp, per) = (amplitude.value()?, period.value()?);
                phase.value()?;
                midline.value()?;
                nonzero("the amplitude", &amp)?;
                if !per.is_positive() {
                    return Err(VisualError::Degenerate {
                        reason: "the period is not positive".to_owned(),
                    });
                }
                Ok(())
            }
        }
    }

    /// The exact value at `x`, or `None` when the family cannot check that `x`
    /// exactly (an irrational exponential input, a logarithm argument that is
    /// not an exact power of the base, a reciprocal at its asymptote).
    fn exact_value_at(&self, x: &BigRational) -> Option<BigRational> {
        match self {
            Self::Polynomial { coefficients } => {
                let mut total = BigRational::zero();
                let mut power = BigRational::one();
                for c in coefficients {
                    total += c.value().ok()? * &power;
                    power *= x.clone();
                }
                Some(total)
            }
            Self::Exponential { a, b, h, k } => {
                let (av, bv, hv, kv) = (
                    a.value().ok()?,
                    b.value().ok()?,
                    h.value().ok()?,
                    k.value().ok()?,
                );
                let exponent = x - hv;
                let n = exact_integer(&exponent)?;
                Some(av * checked_pow(&bv, n)? + kv)
            }
            Self::NaturalExponential { a, rate, h, k } => {
                let (av, ratev, hv, kv) = (
                    a.value().ok()?,
                    rate.value().ok()?,
                    h.value().ok()?,
                    k.value().ok()?,
                );
                let exponent = ratev * (x - hv);
                exponent.is_zero().then(|| av + kv)
            }
            Self::Logarithm { a, base, h, k } => {
                let (av, basev, hv, kv) = (
                    a.value().ok()?,
                    base.value().ok()?,
                    h.value().ok()?,
                    k.value().ok()?,
                );
                let argument = x - hv;
                let n = integer_log(&basev, &argument)?;
                Some(av * BigRational::from_integer(n.into()) + kv)
            }
            Self::Reciprocal { a, h, k } => {
                let (av, hv, kv) = (a.value().ok()?, h.value().ok()?, k.value().ok()?);
                let denominator = x - hv;
                if denominator.is_zero() {
                    return None;
                }
                Some(av / denominator + kv)
            }
            Self::Sine {
                amplitude,
                period,
                phase,
                midline,
            } => trig_exact(amplitude, period, phase, midline, x, TrigFamily::Sine),
            Self::Cosine {
                amplitude,
                period,
                phase,
                midline,
            } => trig_exact(amplitude, period, phase, midline, x, TrigFamily::Cosine),
        }
    }

    /// The formula the accessible equivalent names.
    fn describe(&self) -> String {
        match self {
            Self::Polynomial { coefficients } => {
                format!("y = {}", polynomial_text(coefficients))
            }
            Self::Exponential { a, b, h, k } => {
                format!("y = {a} · {b}^(x - {h}) + {k}")
            }
            Self::NaturalExponential { a, rate, h, k } => {
                format!("y = {a} · e^({rate} · (x - {h})) + {k}")
            }
            Self::Logarithm { a, base, h, k } => {
                format!("y = {a} · log base {base} of (x - {h}) + {k}")
            }
            Self::Reciprocal { a, h, k } => format!("y = {a} / (x - {h}) + {k}"),
            Self::Sine {
                amplitude,
                period,
                phase,
                midline,
            } => format!("y = {amplitude} · sin(2π (x - {phase}) / {period}) + {midline}"),
            Self::Cosine {
                amplitude,
                period,
                phase,
                midline,
            } => format!("y = {amplitude} · cos(2π (x - {phase}) / {period}) + {midline}"),
        }
    }

    /// The value at `x`, approximated for the render pass only.
    ///
    /// `None` means `x` sits outside the family's domain: below a logarithm's
    /// shift, or at a reciprocal's own asymptote. The render pass breaks the
    /// drawn curve there instead of drawing a false continuation.
    pub(super) fn eval_f64(&self, x: f64) -> Option<f64> {
        match self {
            Self::Polynomial { coefficients } => {
                let mut total = 0.0;
                let mut power = 1.0;
                for c in coefficients {
                    total += c.to_f64().ok()? * power;
                    power *= x;
                }
                Some(total)
            }
            Self::Exponential { a, b, h, k } => {
                let (av, bv, hv, kv) = (
                    a.to_f64().ok()?,
                    b.to_f64().ok()?,
                    h.to_f64().ok()?,
                    k.to_f64().ok()?,
                );
                Some(av * bv.powf(x - hv) + kv)
            }
            Self::NaturalExponential { a, rate, h, k } => {
                let (av, ratev, hv, kv) = (
                    a.to_f64().ok()?,
                    rate.to_f64().ok()?,
                    h.to_f64().ok()?,
                    k.to_f64().ok()?,
                );
                Some(av * (ratev * (x - hv)).exp() + kv)
            }
            Self::Logarithm { a, base, h, k } => {
                let (av, basev, hv, kv) = (
                    a.to_f64().ok()?,
                    base.to_f64().ok()?,
                    h.to_f64().ok()?,
                    k.to_f64().ok()?,
                );
                let argument = x - hv;
                if argument <= 0.0 {
                    return None;
                }
                Some(av * argument.log(basev) + kv)
            }
            Self::Reciprocal { a, h, k } => {
                let (av, hv, kv) = (a.to_f64().ok()?, h.to_f64().ok()?, k.to_f64().ok()?);
                let denominator = x - hv;
                if denominator.abs() < 1e-9 {
                    return None;
                }
                Some(av / denominator + kv)
            }
            Self::Sine {
                amplitude,
                period,
                phase,
                midline,
            } => trig_f64(amplitude, period, phase, midline, x, f64::sin),
            Self::Cosine {
                amplitude,
                period,
                phase,
                midline,
            } => trig_f64(amplitude, period, phase, midline, x, f64::cos),
        }
    }
}

#[cfg(test)]
mod tests;
