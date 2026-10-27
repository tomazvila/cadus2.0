//! A figure with one learner-set parameter (decision row D-FA1).
//!
//! An author writes the object `motion` beside a figure: one parameter (a name,
//! a unit, a range, a step, a default) and the drives that say which part of the
//! figure the parameter moves. The server draws ONE SVG at the default value.
//! Every moving part sits in a `<g id>`. The server also sends a small table, the
//! [`MotionTable`]: a law per group in the slider index, the exact text of each
//! step, and the exact text equivalent of each step.
//!
//! The browser does no geometry. It sets `transform` and `opacity` from the table.
//!
//! # Exactness
//!
//! The slider walks an index. Every value is `min + i * step` as an exact
//! rational. Every text comes from the rational. No float is printed. A drive
//! is affine in the index, so a law is a pair `[k, c]` per entry.
//!
//! # Limits
//!
//! At most [`MAX_STEPS`] steps, [`MAX_GROUPS`] groups and [`MAX_TABLE_BYTES`]
//! bytes of table JSON. One parameter per figure.

mod apply;
mod error;
mod table;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};

pub use error::MotionError;
pub use table::{GroupLaw, Law, MotionTable, ParameterTable, Readout, affine, round4};

pub(super) use apply::{coincident_points, group_count, readouts, step_specs};

use super::plane::exact_text;
use super::{Scalar, VisualError, VisualSpec};

/// The most steps one slider has.
pub const MAX_STEPS: usize = 60;

/// The most moving groups one figure has.
pub const MAX_GROUPS: usize = 12;

/// The most bytes the motion table has as JSON.
pub const MAX_TABLE_BYTES: usize = 24_000;

/// The parameter a learner sets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    /// The plain name of the slider, for example `slope`.
    pub name: String,
    /// The unit, for example `°`, or `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    /// The least value.
    pub min: Scalar,
    /// The greatest value.
    pub max: Scalar,
    /// The value the server draws.
    pub default: Scalar,
    /// The distance between two steps.
    pub step: Scalar,
}

/// The part of a figure that the parameter moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Drive {
    /// A coordinate figure: the slope of one segment, about its `from` end.
    LineSlope {
        /// The index of the segment.
        segment: usize,
    },
    /// A coordinate figure: the y-intercept of the line of one segment.
    LineIntercept {
        /// The index of the segment.
        segment: usize,
    },
    /// A coordinate figure: the x of one point.
    PointX {
        /// The index of the point.
        point: usize,
    },
    /// A coordinate figure: the y of one point.
    PointY {
        /// The index of the point.
        point: usize,
    },
    /// A number line: the place of one labeled point.
    Point {
        /// The index of the point.
        index: usize,
    },
    /// A fraction figure with one whole: the count of shaded parts.
    Shaded,
    /// A geometry angle: the terminal ray, in degrees.
    Angle,
}

impl Drive {
    /// The family the drive belongs to.
    pub(super) const fn family(&self) -> &'static str {
        match self {
            Self::LineSlope { .. }
            | Self::LineIntercept { .. }
            | Self::PointX { .. }
            | Self::PointY { .. } => "coordinate",
            Self::Point { .. } => "number_line",
            Self::Shaded => "fraction",
            Self::Angle => "geometry",
        }
    }
}

/// The `motion` object of one figure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    /// The slider.
    pub parameter: Parameter,
    /// The parts the slider moves.
    pub drives: Vec<Drive>,
}

/// The exact steps of one parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Steps {
    /// The least value.
    pub min: BigRational,
    /// The distance between two steps.
    pub step: BigRational,
    /// The count of steps: the slider runs from index 0 to `n`.
    pub n: usize,
    /// The index of the default value.
    pub default_index: usize,
}

impl Steps {
    /// The exact value at one index.
    #[must_use]
    pub fn value(&self, index: usize) -> BigRational {
        self.min.clone() + self.step.clone() * BigRational::from_integer(BigInt::from(index))
    }
}

/// Whether the denominator has only the factors 2 and 5.
fn terminates(value: &BigRational) -> bool {
    let mut denominator = value.denom().clone();
    for factor in [2_u8, 5] {
        let factor = BigInt::from(factor);
        while (&denominator % &factor).is_zero() {
            denominator /= &factor;
        }
    }
    denominator.is_one()
}

fn decimal(name: &str, scalar: &Scalar) -> Result<BigRational, VisualError> {
    let value = scalar.value()?;
    if terminates(&value) {
        Ok(value)
    } else {
        Err(MotionError::ParameterNotDecimal {
            text: format!("{name} {scalar}"),
        }
        .into())
    }
}

impl Motion {
    /// The exact steps, or the reason the parameter is wrong.
    pub fn steps(&self) -> Result<Steps, VisualError> {
        let parameter = &self.parameter;
        let min = decimal("min", &parameter.min)?;
        let max = decimal("max", &parameter.max)?;
        let step = decimal("step", &parameter.step)?;
        let default = decimal("default", &parameter.default)?;
        if parameter.name.trim().is_empty() {
            return Err(MotionError::ParameterRange {
                reason: "the parameter has no name".to_owned(),
            }
            .into());
        }
        if max <= min {
            return Err(MotionError::ParameterRange {
                reason: "the range does not ascend".to_owned(),
            }
            .into());
        }
        if !step.is_positive() {
            return Err(MotionError::ParameterStep {
                reason: "the step is not positive".to_owned(),
            }
            .into());
        }
        let count = (max - min.clone()) / step.clone();
        if !count.denom().is_one() {
            return Err(MotionError::ParameterStep {
                reason: "the step does not divide the range".to_owned(),
            }
            .into());
        }
        let n = count.numer().to_usize().unwrap_or(usize::MAX);
        if n > MAX_STEPS {
            return Err(MotionError::ParameterRange {
                reason: format!("the slider has {n} steps and the limit is {MAX_STEPS}"),
            }
            .into());
        }
        let offset = (default - min.clone()) / step.clone();
        let default_index = if offset.denom().is_one() && !offset.is_negative() {
            offset.numer().to_usize().filter(|index| *index <= n)
        } else {
            None
        };
        let Some(default_index) = default_index else {
            return Err(MotionError::ParameterDefault {
                default: parameter.default.to_string(),
            }
            .into());
        };
        Ok(Steps {
            min,
            step,
            n,
            default_index,
        })
    }

    /// The text of one value with the unit: `3/2`, `45°`, `2 cm`.
    #[must_use]
    pub fn value_text(&self, value: &BigRational) -> String {
        let number = exact_text(value);
        match self.parameter.unit.as_deref().map(str::trim) {
            Some(unit) if !unit.is_empty() => {
                if unit.chars().next().is_some_and(char::is_alphanumeric) {
                    format!("{number} {unit}")
                } else {
                    format!("{number}{unit}")
                }
            }
            _ => number,
        }
    }

    /// Whether the motion fits the figure it sits on.
    ///
    /// The figure must hold at the least, the default and the greatest value.
    pub(super) fn validate(&self, spec: &VisualSpec) -> Result<(), VisualError> {
        let steps = self.steps()?;
        if self.drives.is_empty() {
            return Err(MotionError::DriveMismatch {
                reason: "the motion has no drive".to_owned(),
            }
            .into());
        }
        let bare = spec.without_motion();
        for drive in &self.drives {
            if drive.family() != spec.kind() {
                return Err(MotionError::DriveMismatch {
                    reason: format!("a {} drive on a {} figure", drive.family(), spec.kind()),
                }
                .into());
            }
            apply::check_default(&bare, drive, &self.parameter, &steps)?;
        }
        let groups = group_count(&bare, self)?;
        if groups > MAX_GROUPS {
            return Err(MotionError::TooManyGroups {
                groups,
                limit: MAX_GROUPS,
            }
            .into());
        }
        for index in [0, steps.default_index, steps.n] {
            apply::at_index(&bare, self, &steps, index)?.validate()?;
        }
        Ok(())
    }
}

impl VisualSpec {
    /// The `motion` object of the figure, or `None`.
    #[must_use]
    pub const fn motion(&self) -> Option<&Motion> {
        match self {
            Self::NumberLine(figure) => figure.motion.as_ref(),
            Self::Fraction(figure) => figure.motion.as_ref(),
            Self::Coordinate(figure) => figure.motion.as_ref(),
            Self::Geometry(figure) => figure.motion.as_ref(),
            Self::Curve(figure) => figure.motion.as_ref(),
            Self::SpecialTriangle(figure) => figure.motion.as_ref(),
        }
    }

    /// The same figure with no `motion`: the static figure at the default.
    #[must_use]
    pub fn without_motion(&self) -> Self {
        let mut bare = self.clone();
        match &mut bare {
            Self::NumberLine(figure) => figure.motion = None,
            Self::Fraction(figure) => figure.motion = None,
            Self::Coordinate(figure) => figure.motion = None,
            Self::Geometry(figure) => figure.motion = None,
            Self::Curve(figure) => figure.motion = None,
            Self::SpecialTriangle(figure) => figure.motion = None,
        }
        bare
    }

    /// The sentence that names the slider, or an empty text.
    pub(super) fn slider_sentence(&self) -> String {
        let Some(motion) = self.motion() else {
            return String::new();
        };
        let Ok(steps) = motion.steps() else {
            return String::new();
        };
        let (low, high) = (steps.value(0), steps.value(steps.n));
        format!(
            " The slider is named {}. It runs from {} to {}.",
            motion.parameter.name.trim(),
            motion.value_text(&low),
            motion.value_text(&high),
        )
    }
}
