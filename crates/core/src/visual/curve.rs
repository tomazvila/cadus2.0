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

use super::plane::exact_text;
use super::{LabeledPoint, Scalar, VisualError, inside, tick_count};

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

/// The float approximation of a sine or cosine curve at `x`, for the render
/// pass only.
fn trig_f64(
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
enum TrigFamily {
    Sine,
    Cosine,
}

/// The exact value of a sine or cosine curve at `x`, when `x` sits on a
/// quarter of the period from the phase (the only points where the trig value
/// is a small exact rational: `-1`, `0`, or `1`).
fn trig_exact(
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
fn exact_integer(value: &BigRational) -> Option<i64> {
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
fn checked_pow(base: &BigRational, exponent: i64) -> Option<BigRational> {
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
fn integer_log(base: &BigRational, argument: &BigRational) -> Option<i64> {
    if !argument.is_positive() {
        return None;
    }
    (-MAX_EXPONENT..=MAX_EXPONENT).find(|&n| checked_pow(base, n).as_ref() == Some(argument))
}

/// An error unless `value` is nonzero.
fn nonzero(what: &'static str, value: &BigRational) -> Result<(), VisualError> {
    if value.is_zero() {
        Err(VisualError::Degenerate {
            reason: format!("{what} is zero"),
        })
    } else {
        Ok(())
    }
}

/// An error unless `value` is positive and not one.
fn positive_and_not_one(what: &'static str, value: &BigRational) -> Result<(), VisualError> {
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
fn polynomial_text(coefficients: &[Scalar]) -> String {
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

#[cfg(test)]
mod tests {
    use super::{Asymptote, CurveFigure, CurveKind};
    use crate::visual::{LabeledPoint, Scalar, VisualError};

    fn parabola() -> CurveFigure {
        CurveFigure {
            x_min: Scalar::from("-5"),
            x_max: Scalar::from("5"),
            y_min: Scalar::from("-6"),
            y_max: Scalar::from("10"),
            x_tick: Scalar::from("1"),
            y_tick: Scalar::from("2"),
            curve: CurveKind::Polynomial {
                coefficients: vec![Scalar::from("-4"), Scalar::from("0"), Scalar::from("1")],
            },
            key_points: vec![
                LabeledPoint::labeled(0_i64, -4_i64, "vertex"),
                LabeledPoint::new(2_i64, 0_i64),
                LabeledPoint::new(-2_i64, 0_i64),
            ],
            asymptotes: Vec::new(),
            caption: None,
        }
    }

    #[test]
    fn a_parabola_validates_its_exact_key_points_and_reads_its_formula() {
        let figure = parabola();
        assert!(figure.validate().is_ok());
        let text = figure.text_equivalent();
        assert!(text.contains("The curve y = -4 + x^2."));
        assert!(text.contains("A key point at (0, -4) labeled vertex."));
    }

    #[test]
    fn a_key_point_off_the_curve_is_refused() {
        let mut figure = parabola();
        figure.key_points = vec![LabeledPoint::new(1_i64, 1_i64)];
        assert!(matches!(
            figure.validate(),
            Err(VisualError::KeyPointOffCurve { .. })
        ));
    }

    #[test]
    fn an_exponential_checks_only_integer_steps_from_its_shift() {
        let figure = CurveFigure {
            x_min: Scalar::from("-3"),
            x_max: Scalar::from("3"),
            y_min: Scalar::from("0"),
            y_max: Scalar::from("10"),
            x_tick: Scalar::from("1"),
            y_tick: Scalar::from("1"),
            curve: CurveKind::Exponential {
                a: Scalar::from("1"),
                b: Scalar::from("2"),
                h: Scalar::from("0"),
                k: Scalar::from("0"),
            },
            key_points: vec![
                LabeledPoint::new(0_i64, 1_i64),
                LabeledPoint::new(1_i64, 2_i64),
                LabeledPoint::new(3_i64, 8_i64),
            ],
            asymptotes: vec![Asymptote::Horizontal {
                at: Scalar::from("0"),
            }],
            caption: None,
        };
        assert!(figure.validate().is_ok());

        let mut half_step = figure.clone();
        half_step.key_points = vec![LabeledPoint::new("0.5", "1.414")];
        assert!(matches!(
            half_step.validate(),
            Err(VisualError::KeyPointOffCurve { .. })
        ));
    }

    #[test]
    fn a_logarithm_checks_exact_powers_of_its_base() {
        let figure = CurveFigure {
            x_min: Scalar::from("0.5"),
            x_max: Scalar::from("10"),
            y_min: Scalar::from("-3"),
            y_max: Scalar::from("3"),
            x_tick: Scalar::from("1"),
            y_tick: Scalar::from("1"),
            curve: CurveKind::Logarithm {
                a: Scalar::from("1"),
                base: Scalar::from("2"),
                h: Scalar::from("0"),
                k: Scalar::from("0"),
            },
            key_points: vec![
                LabeledPoint::new(1_i64, 0_i64),
                LabeledPoint::new(2_i64, 1_i64),
                LabeledPoint::new(4_i64, 2_i64),
            ],
            asymptotes: vec![],
            caption: None,
        };
        assert!(figure.validate().is_ok());

        let mut off_grid = figure.clone();
        off_grid.key_points = vec![LabeledPoint::new("3", "1.585")];
        assert!(matches!(
            off_grid.validate(),
            Err(VisualError::KeyPointOffCurve { .. })
        ));
    }

    #[test]
    fn a_reciprocal_refuses_a_key_point_at_its_own_asymptote() {
        let figure = CurveFigure {
            x_min: Scalar::from("-5"),
            x_max: Scalar::from("5"),
            y_min: Scalar::from("-5"),
            y_max: Scalar::from("5"),
            x_tick: Scalar::from("1"),
            y_tick: Scalar::from("1"),
            curve: CurveKind::Reciprocal {
                a: Scalar::from("2"),
                h: Scalar::from("0"),
                k: Scalar::from("0"),
            },
            key_points: vec![LabeledPoint::new(0_i64, 0_i64)],
            asymptotes: vec![],
            caption: None,
        };
        assert!(matches!(
            figure.validate(),
            Err(VisualError::KeyPointOffCurve { .. })
        ));
    }

    #[test]
    fn a_sine_curve_checks_exact_quarter_period_points_only() {
        let figure = CurveFigure {
            x_min: Scalar::from("-1"),
            x_max: Scalar::from("5"),
            y_min: Scalar::from("-2"),
            y_max: Scalar::from("2"),
            x_tick: Scalar::from("1"),
            y_tick: Scalar::from("1"),
            curve: CurveKind::Sine {
                amplitude: Scalar::from("1"),
                period: Scalar::from("4"),
                phase: Scalar::from("0"),
                midline: Scalar::from("0"),
            },
            key_points: vec![
                LabeledPoint::new(0_i64, 0_i64),
                LabeledPoint::new(1_i64, 1_i64),
                LabeledPoint::new(2_i64, 0_i64),
                LabeledPoint::new(3_i64, -1_i64),
                LabeledPoint::new(4_i64, 0_i64),
            ],
            asymptotes: vec![],
            caption: None,
        };
        assert!(figure.validate().is_ok());

        let mut wrong = figure.clone();
        wrong.key_points = vec![LabeledPoint::new(1_i64, 0_i64)];
        assert!(matches!(
            wrong.validate(),
            Err(VisualError::KeyPointOffCurve { .. })
        ));
    }

    #[test]
    fn degenerate_parameters_are_refused_before_any_key_point_check() {
        let mut zero_amplitude = CurveFigure {
            x_min: Scalar::from("-1"),
            x_max: Scalar::from("1"),
            y_min: Scalar::from("-1"),
            y_max: Scalar::from("1"),
            x_tick: Scalar::from("1"),
            y_tick: Scalar::from("1"),
            curve: CurveKind::Sine {
                amplitude: Scalar::from("0"),
                period: Scalar::from("4"),
                phase: Scalar::from("0"),
                midline: Scalar::from("0"),
            },
            key_points: vec![],
            asymptotes: vec![],
            caption: None,
        };
        assert!(matches!(
            zero_amplitude.validate(),
            Err(VisualError::Degenerate { .. })
        ));

        zero_amplitude.curve = CurveKind::Exponential {
            a: Scalar::from("1"),
            b: Scalar::from("1"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        };
        assert!(matches!(
            zero_amplitude.validate(),
            Err(VisualError::Degenerate { .. })
        ));
    }

    #[test]
    fn an_asymptote_outside_the_drawn_range_is_refused() {
        let mut figure = CurveFigure {
            x_min: Scalar::from("-5"),
            x_max: Scalar::from("5"),
            y_min: Scalar::from("-5"),
            y_max: Scalar::from("5"),
            x_tick: Scalar::from("1"),
            y_tick: Scalar::from("1"),
            curve: CurveKind::Reciprocal {
                a: Scalar::from("1"),
                h: Scalar::from("0"),
                k: Scalar::from("0"),
            },
            key_points: vec![],
            asymptotes: vec![Asymptote::Horizontal {
                at: Scalar::from("50"),
            }],
            caption: None,
        };
        assert!(matches!(
            figure.validate(),
            Err(VisualError::OutOfRange { .. })
        ));
        figure.asymptotes = vec![Asymptote::Vertical {
            at: Scalar::from("0"),
        }];
        assert!(figure.validate().is_ok());
    }
}
