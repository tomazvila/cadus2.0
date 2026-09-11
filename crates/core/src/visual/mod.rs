//! The mathematical visuals of unit f9: the four figure families a Foundations
//! objective needs, the checks that keep a figure meaningful, and the accessible
//! equivalent every figure carries.
//!
//! # Why the module exists
//!
//! The readiness audit of D-F5 asks whether a knowledge point that needs a
//! picture has one. Before this module the answer was hardcoded: `visual_present`
//! was `false` for every knowledge point, because no authored visual and no
//! renderer existed. The audit therefore reported a real shortage and had no way
//! to clear it.
//!
//! This module gives the missing half. An author writes a [`VisualSpec`] beside
//! a knowledge point. [`VisualSpec::validate`] answers whether the figure carries
//! mathematical meaning — an ascending range, a tick that divides it, a point
//! inside it, a polygon with area. [`VisualSpec::text_equivalent`] gives the
//! accessible equivalent, which is the same facts in one sentence per element.
//! [`render`] draws the figure as SVG for a browser.
//!
//! # Purity
//!
//! Nothing here opens a socket, reads a clock, or calls a model (R3). The render
//! is deterministic: the same specification always gives the same bytes.
//!
//! # What the module does NOT do
//!
//! It never approves content. An authored visual that fails
//! [`VisualSpec::validate`] counts as absent, so a broken figure keeps the
//! knowledge point unready.

mod curve;
mod fraction;
mod geometry;
mod number_line;
mod plane;
mod render;
mod scalar;
mod special_triangle;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use curve::{Asymptote, CurveFigure, CurveKind};
pub use fraction::{FractionFigure, FractionShape};
pub use geometry::{AngleMark, GeometryFigure, GeometryShape};
pub use number_line::{MarkedInterval, MarkedPoint, MarkedRay, NumberLineFigure, RayDirection};
pub use plane::{CoordinateFigure, LabeledPoint, Segment, ShadedHalfPlane};
pub use render::{RenderOptions, RenderedVisual, render, render_all};
pub use scalar::Scalar;
pub use special_triangle::{SpecialTriangleFigure, SpecialTriangleShape};

/// Exact bounds and tick spacing of a figure drawn on a coordinate grid.
pub(super) trait PlaneAxes {
    fn plane_axes(&self) -> ((&Scalar, &Scalar, &Scalar), (&Scalar, &Scalar, &Scalar));
}

macro_rules! plane_axes {
    ($figure:ty) => {
        impl PlaneAxes for $figure {
            fn plane_axes(&self) -> ((&Scalar, &Scalar, &Scalar), (&Scalar, &Scalar, &Scalar)) {
                (
                    (&self.x_min, &self.x_max, &self.x_tick),
                    (&self.y_min, &self.y_max, &self.y_tick),
                )
            }
        }
    };
}

plane_axes!(CoordinateFigure);
plane_axes!(CurveFigure);

/// The largest number of ticks one axis draws.
///
/// An axis with more ticks than this is unreadable on a screen and unreadable
/// through a screen reader, so the check refuses it.
pub const MAX_TICKS: i64 = 200;

/// The largest number of equal parts a fraction figure divides one whole into.
pub const MAX_PARTS: i64 = 64;

/// The largest number of wholes a fraction figure draws.
pub const MAX_WHOLES: i64 = 4;

/// Why one visual specification carries no mathematical meaning.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VisualError {
    /// One authored number is not an integer, a decimal, or a fraction.
    #[error("the number {text:?} is not an integer, a decimal, or a fraction")]
    NotANumber {
        /// The text the author wrote.
        text: String,
    },
    /// One range starts at or after its end.
    #[error("the {axis} range does not ascend")]
    RangeNotAscending {
        /// The axis name: `x`, `y`, or `line`.
        axis: &'static str,
    },
    /// One tick step is zero or negative.
    #[error("the {axis} tick step is not positive")]
    TickNotPositive {
        /// The axis name.
        axis: &'static str,
    },
    /// One axis draws more than [`MAX_TICKS`] ticks.
    #[error("the {axis} axis draws {ticks} ticks and the limit is {MAX_TICKS}")]
    TooManyTicks {
        /// The axis name.
        axis: &'static str,
        /// The tick count the specification asks for.
        ticks: i64,
    },
    /// One element sits outside the drawn range.
    #[error("{what} sits outside the drawn range")]
    OutOfRange {
        /// The element the check rejected.
        what: String,
    },
    /// A fraction divides a whole into zero or fewer parts.
    #[error("a fraction figure divides a whole into {parts} parts")]
    BadPartCount {
        /// The authored part count.
        parts: i64,
    },
    /// A fraction shades a negative count, or more than [`MAX_WHOLES`] wholes.
    #[error("a fraction figure shades {shaded} of {parts} parts")]
    BadShadedCount {
        /// The authored shaded count.
        shaded: i64,
        /// The authored part count.
        parts: i64,
    },
    /// A polygon has fewer than three vertices.
    #[error("a polygon has {count} vertices and needs three")]
    TooFewVertices {
        /// The authored vertex count.
        count: usize,
    },
    /// A figure collapses: a polygon with zero area, a repeated vertex, or a
    /// circle with a radius of zero.
    #[error("the figure collapses: {reason}")]
    Degenerate {
        /// What collapsed.
        reason: String,
    },
    /// A mark names a vertex the figure does not hold.
    #[error("a mark names vertex {at} and the figure has {count} vertices")]
    NoSuchVertex {
        /// The authored index.
        at: usize,
        /// The vertex count.
        count: usize,
    },
    /// An authored curve key point does not sit exactly on the curve, or sits
    /// at an `x` the family cannot check exactly.
    #[error("the key point at {point} does not sit exactly on the curve")]
    KeyPointOffCurve {
        /// The point, in words.
        point: String,
    },
}

/// One authored mathematical visual.
///
/// The four families are the families the assignment names: a number line, a
/// fraction representation, a coordinate graph, and a geometry diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VisualSpec {
    /// A number line with ticks, marked points, and marked intervals.
    NumberLine(NumberLineFigure),
    /// A part-of-a-whole figure: a bar or a circle divided into equal parts.
    Fraction(FractionFigure),
    /// A coordinate plane with points and segments.
    Coordinate(CoordinateFigure),
    /// A geometry diagram: a polygon or a circle.
    Geometry(GeometryFigure),
    /// A coordinate plane with one drawn curve.
    Curve(CurveFigure),
    /// A 45-45-90 or 30-60-90 right triangle, or a two-sides-and-an-angle
    /// triangle area, with every irrational side or area computed rather
    /// than author-typed.
    SpecialTriangle(SpecialTriangleFigure),
}

impl VisualSpec {
    /// The wire name of the family.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NumberLine(_) => "number_line",
            Self::Fraction(_) => "fraction",
            Self::Coordinate(_) => "coordinate",
            Self::Geometry(_) => "geometry",
            Self::Curve(_) => "curve",
            Self::SpecialTriangle(_) => "special_triangle",
        }
    }

    /// The authored caption, or `None`.
    #[must_use]
    pub fn caption(&self) -> Option<&str> {
        let caption = match self {
            Self::NumberLine(figure) => figure.caption.as_deref(),
            Self::Fraction(figure) => figure.caption.as_deref(),
            Self::Coordinate(figure) => figure.caption.as_deref(),
            Self::Geometry(figure) => figure.caption.as_deref(),
            Self::Curve(figure) => figure.caption.as_deref(),
            Self::SpecialTriangle(figure) => figure.caption.as_deref(),
        };
        caption.filter(|text| !text.trim().is_empty())
    }

    /// Whether the figure carries mathematical meaning.
    ///
    /// The caller treats an error as an ABSENT visual. The check never repairs a
    /// figure and never approves one.
    pub fn validate(&self) -> Result<(), VisualError> {
        match self {
            Self::NumberLine(figure) => figure.validate(),
            Self::Fraction(figure) => figure.validate(),
            Self::Coordinate(figure) => figure.validate(),
            Self::Geometry(figure) => figure.validate(),
            Self::Curve(figure) => figure.validate(),
            Self::SpecialTriangle(figure) => figure.validate(),
        }
    }

    /// The accessible equivalent: the facts of the figure in words.
    ///
    /// A screen reader reads this text in place of the picture. The text names
    /// every drawn element, so a learner who never sees the picture answers the
    /// same question.
    #[must_use]
    pub fn text_equivalent(&self) -> String {
        let body = match self {
            Self::NumberLine(figure) => figure.text_equivalent(),
            Self::Fraction(figure) => figure.text_equivalent(),
            Self::Coordinate(figure) => figure.text_equivalent(),
            Self::Geometry(figure) => figure.text_equivalent(),
            Self::Curve(figure) => figure.text_equivalent(),
            Self::SpecialTriangle(figure) => figure.text_equivalent(),
        };
        match self.caption() {
            Some(caption) => format!("{}. {body}", caption.trim()),
            None => body,
        }
    }
}

/// The count of ticks one axis draws, or the reason it draws none.
fn tick_count(
    axis: &'static str,
    min: &Scalar,
    max: &Scalar,
    tick: &Scalar,
) -> Result<i64, VisualError> {
    use num_traits::{Signed, ToPrimitive};

    let (low, high, step) = (min.value()?, max.value()?, tick.value()?);
    if high <= low {
        return Err(VisualError::RangeNotAscending { axis });
    }
    if !step.is_positive() {
        return Err(VisualError::TickNotPositive { axis });
    }
    let span = (high - low) / step;
    let ticks = span
        .to_f64()
        .map(|value| value.floor())
        .and_then(|value| {
            if value.is_finite() && value <= i64::MAX as f64 {
                Some(value as i64)
            } else {
                None
            }
        })
        .unwrap_or(i64::MAX);
    if ticks > MAX_TICKS {
        return Err(VisualError::TooManyTicks { axis, ticks });
    }
    Ok(ticks.saturating_add(1))
}

/// The tick counts of the x and y axes of a plane-based figure.
fn plane_ticks(figure: &impl PlaneAxes) -> Result<(i64, i64), VisualError> {
    let (x, y) = figure.plane_axes();
    Ok((
        tick_count("x", x.0, x.1, x.2)?,
        tick_count("y", y.0, y.1, y.2)?,
    ))
}

/// Symmetric coordinate bounds with a unit tick on each axis.
fn square_axes(half: i64) -> (Scalar, Scalar, Scalar, Scalar, Scalar, Scalar) {
    (
        Scalar::from_i64(-half),
        Scalar::from_i64(half),
        Scalar::from_i64(-half),
        Scalar::from_i64(half),
        Scalar::from_i64(1),
        Scalar::from_i64(1),
    )
}

/// Whether one value sits inside the closed range, and the error if it does not.
fn inside(what: &str, min: &Scalar, max: &Scalar, value: &Scalar) -> Result<(), VisualError> {
    let (low, high, at) = (min.value()?, max.value()?, value.value()?);
    if at < low || at > high {
        Err(VisualError::OutOfRange {
            what: format!("{what} at {value}"),
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{FractionFigure, VisualSpec};

    #[test]
    fn the_caption_leads_the_accessible_equivalent_and_blank_text_never_does() {
        let mut figure = FractionFigure::bar(3, 4);
        figure.caption = Some("  Three quarters  ".to_owned());
        let spec = VisualSpec::Fraction(figure.clone());
        assert_eq!(spec.kind(), "fraction");
        assert_eq!(spec.caption(), Some("  Three quarters  "));
        assert!(spec.text_equivalent().starts_with("Three quarters. "));

        figure.caption = Some("   ".to_owned());
        let blank = VisualSpec::Fraction(figure);
        assert_eq!(blank.caption(), None);
        assert!(blank.text_equivalent().starts_with("A fraction bar"));
    }

    #[test]
    fn the_spec_round_trips_through_yaml_with_the_kind_tag() {
        let text = "kind: fraction\nparts: 4\nshaded: 3\nshape: bar\n";
        let spec: VisualSpec = serde_norway::from_str(text).unwrap();
        assert_eq!(spec, VisualSpec::Fraction(FractionFigure::bar(3, 4)));
        assert!(spec.validate().is_ok());
    }
}
