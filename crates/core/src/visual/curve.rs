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
use serde::{Deserialize, Serialize};

use super::{LabeledPoint, Scalar, VisualError, inside, tick_count};

mod evaluate;
mod helpers;

use evaluate::{
    exponential_exact, exponential_f64, logarithm_exact, logarithm_f64, natural_exact, natural_f64,
    polynomial_exact, polynomial_f64, reciprocal_exact, reciprocal_f64, validate_polynomial,
    validate_scaled, validate_trig,
};
use helpers::{TrigFamily, nonzero, polynomial_text, positive_and_not_one, trig_exact, trig_f64};

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
            Self::Polynomial { coefficients } => validate_polynomial(coefficients),
            Self::Exponential { a, b, h, k } => {
                validate_scaled(a, h, k, "the exponential scale a")?;
                positive_and_not_one("the exponential base b", &b.value()?)
            }
            Self::NaturalExponential { a, rate, h, k } => {
                validate_scaled(a, h, k, "the natural exponential scale a")?;
                nonzero("the natural exponential rate", &rate.value()?)
            }
            Self::Logarithm { a, base, h, k } => {
                validate_scaled(a, h, k, "the logarithm scale a")?;
                positive_and_not_one("the logarithm base", &base.value()?)
            }
            Self::Reciprocal { a, h, k } => validate_scaled(a, h, k, "the reciprocal scale a"),
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
            } => validate_trig(amplitude, period, phase, midline),
        }
    }

    /// The exact value at `x`, or `None` when the family cannot check that `x`
    /// exactly (an irrational exponential input, a logarithm argument that is
    /// not an exact power of the base, a reciprocal at its asymptote).
    fn exact_value_at(&self, x: &BigRational) -> Option<BigRational> {
        match self {
            Self::Polynomial { coefficients } => polynomial_exact(coefficients, x),
            Self::Exponential { a, b, h, k } => exponential_exact(a, b, h, k, x),
            Self::NaturalExponential { a, rate, h, k } => natural_exact(a, rate, h, k, x),
            Self::Logarithm { a, base, h, k } => logarithm_exact(a, base, h, k, x),
            Self::Reciprocal { a, h, k } => reciprocal_exact(a, h, k, x),
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
            Self::Polynomial { coefficients } => polynomial_f64(coefficients, x),
            Self::Exponential { a, b, h, k } => exponential_f64(a, b, h, k, x),
            Self::NaturalExponential { a, rate, h, k } => natural_f64(a, rate, h, k, x),
            Self::Logarithm { a, base, h, k } => logarithm_f64(a, base, h, k, x),
            Self::Reciprocal { a, h, k } => reciprocal_f64(a, h, k, x),
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
