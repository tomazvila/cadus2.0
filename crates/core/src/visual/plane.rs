//! The coordinate plane: two ranges, two tick steps, points, and segments.

use num_rational::BigRational;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};

use super::number_line::label_of;
use super::{Scalar, VisualError, inside, plane_ticks};

/// One point of the plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LabeledPoint {
    /// The horizontal coordinate.
    pub x: Scalar,
    /// The vertical coordinate.
    pub y: Scalar,
    /// The text beside the point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl LabeledPoint {
    /// A point with no label.
    #[must_use]
    pub fn new(x: impl Into<Scalar>, y: impl Into<Scalar>) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            label: None,
        }
    }

    /// A point with a label.
    #[must_use]
    pub fn labeled(x: impl Into<Scalar>, y: impl Into<Scalar>, label: &str) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            label: Some(label.to_owned()),
        }
    }

    /// The exact pair.
    pub fn value(&self) -> Result<(BigRational, BigRational), VisualError> {
        Ok((self.x.value()?, self.y.value()?))
    }

    /// The pair as floats, for the render pass only.
    pub fn to_pair(&self) -> Result<(f64, f64), VisualError> {
        Ok((self.x.to_f64()?, self.y.to_f64()?))
    }

    /// The pair as words: `(2, 3)`, and `(2, 3) labeled A` with a label.
    #[must_use]
    pub fn spoken(&self) -> String {
        match label_of(self.label.as_deref()) {
            Some(label) => format!("({}, {}) labeled {label}", self.x, self.y),
            None => format!("({}, {})", self.x, self.y),
        }
    }
}

/// One straight segment of the plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Segment {
    /// One end.
    pub from: LabeledPoint,
    /// The other end.
    pub to: LabeledPoint,
    /// The text beside the segment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// One shaded half-plane: the solution set of a linear inequality in two
/// variables.
///
/// The boundary is the line through `through_a` and `through_b`. An author
/// never writes the line's equation, so there is nothing to get wrong about it:
/// the two points fix it exactly. `shade_toward` is a point strictly on the
/// shaded side; it is not required to sit inside the drawn range, only to sit
/// off the boundary line, so a reader (sighted or not) can rebuild the exact
/// inequality from three plain facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShadedHalfPlane {
    /// One point the boundary line passes through.
    pub through_a: LabeledPoint,
    /// A second, distinct point the boundary line passes through.
    pub through_b: LabeledPoint,
    /// A solid boundary belongs to the solution set (`≤` or `≥`). A dashed
    /// boundary does not (`<` or `>`).
    #[serde(default = "solid")]
    pub solid: bool,
    /// A point strictly on the shaded side of the boundary.
    pub shade_toward: LabeledPoint,
    /// The text beside the shaded region.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The serde default of `ShadedHalfPlane::solid`.
const fn solid() -> bool {
    true
}

impl ShadedHalfPlane {
    /// Twice the signed area of the triangle `through_a`, `through_b`, `point`.
    ///
    /// The sign says which side of the boundary line `point` sits on; zero says
    /// `point` sits on the line.
    fn side(&self, point: &LabeledPoint) -> Result<BigRational, VisualError> {
        let (ax, ay) = self.through_a.value()?;
        let (bx, by) = self.through_b.value()?;
        let (px, py) = point.value()?;
        Ok((bx - ax.clone()) * (py - ay.clone()) - (by - ay) * (px - ax))
    }
}

/// A coordinate plane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinateFigure {
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
    /// The plotted points, in authored order.
    #[serde(default)]
    pub points: Vec<LabeledPoint>,
    /// The drawn segments, in authored order.
    #[serde(default)]
    pub segments: Vec<Segment>,
    /// The shaded half-planes, in authored order.
    #[serde(default)]
    pub shaded_half_planes: Vec<ShadedHalfPlane>,
    /// The sentence that leads the accessible equivalent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

impl CoordinateFigure {
    /// A square plane from `-half` to `half` on both axes, with a tick of one.
    #[must_use]
    pub fn square(half: i64) -> Self {
        Self {
            x_min: Scalar::from_i64(-half),
            x_max: Scalar::from_i64(half),
            y_min: Scalar::from_i64(-half),
            y_max: Scalar::from_i64(half),
            x_tick: Scalar::from_i64(1),
            y_tick: Scalar::from_i64(1),
            points: Vec::new(),
            segments: Vec::new(),
            shaded_half_planes: Vec::new(),
            caption: None,
        }
    }

    /// The tick counts of the two axes.
    pub fn ticks(&self) -> Result<(i64, i64), VisualError> {
        plane_ticks(self)
    }

    /// Whether the plane carries mathematical meaning.
    pub fn validate(&self) -> Result<(), VisualError> {
        self.ticks()?;
        for point in &self.points {
            self.hold("a point", point)?;
        }
        for segment in &self.segments {
            self.hold("a segment end", &segment.from)?;
            self.hold("a segment end", &segment.to)?;
            if segment.from.value()? == segment.to.value()? {
                return Err(VisualError::Degenerate {
                    reason: format!("a segment starts and ends at {}", segment.from.spoken()),
                });
            }
        }
        for region in &self.shaded_half_planes {
            self.hold("a boundary point", &region.through_a)?;
            self.hold("a boundary point", &region.through_b)?;
            if region.through_a.value()? == region.through_b.value()? {
                return Err(VisualError::Degenerate {
                    reason: format!(
                        "a shaded boundary line's two points both sit at {}",
                        region.through_a.spoken()
                    ),
                });
            }
            if region.side(&region.shade_toward)?.is_zero() {
                return Err(VisualError::Degenerate {
                    reason: format!(
                        "the shaded-side point {} sits on the boundary line itself",
                        region.shade_toward.spoken()
                    ),
                });
            }
        }
        Ok(())
    }

    /// Whether one point sits inside the drawn plane.
    fn hold(&self, what: &str, point: &LabeledPoint) -> Result<(), VisualError> {
        inside(what, &self.x_min, &self.x_max, &point.x)?;
        inside(what, &self.y_min, &self.y_max, &point.y)
    }

    /// The accessible equivalent.
    #[must_use]
    pub fn text_equivalent(&self) -> String {
        let mut out = format!(
            "A coordinate plane with x from {} to {} and y from {} to {}. \
             The x ticks step by {} and the y ticks step by {}.",
            self.x_min, self.x_max, self.y_min, self.y_max, self.x_tick, self.y_tick
        );
        for point in &self.points {
            out.push_str(&format!(" A point at {}.", point.spoken()));
        }
        for segment in &self.segments {
            out.push_str(&format!(
                " A segment from {} to {}",
                segment.from.spoken(),
                segment.to.spoken()
            ));
            if let Some(label) = label_of(segment.label.as_deref()) {
                out.push_str(&format!(", labeled {label}"));
            }
            out.push('.');
        }
        for region in &self.shaded_half_planes {
            let style = if region.solid { "solid" } else { "dashed" };
            out.push_str(&format!(
                " A {style} boundary line through {} and {}. The region containing {} is shaded",
                region.through_a.spoken(),
                region.through_b.spoken(),
                region.shade_toward.spoken(),
            ));
            if let Some(label) = label_of(region.label.as_deref()) {
                out.push_str(&format!(", labeled {label}"));
            }
            out.push('.');
        }
        out
    }
}

/// The exact value as text: an integer, or the fraction `a/b`.
pub(super) fn exact_text(value: &BigRational) -> String {
    if value.denom().is_one() {
        value.numer().to_string()
    } else {
        format!("{}/{}", value.numer(), value.denom())
    }
}

#[cfg(test)]
mod tests {
    use super::{CoordinateFigure, LabeledPoint, Segment, ShadedHalfPlane};
    use crate::visual::plane::exact_text;
    use crate::visual::{Scalar, VisualError};
    use num_bigint::BigInt;
    use num_rational::BigRational;

    fn plane_with_point(x: &str, y: &str) -> CoordinateFigure {
        let mut plane = CoordinateFigure::square(5);
        plane.points = vec![LabeledPoint::new(x, y)];
        plane
    }

    #[test]
    fn a_plane_with_a_point_and_a_segment_validates_and_reads_out_loud() {
        let mut plane = CoordinateFigure::square(5);
        plane.points = vec![LabeledPoint::labeled(2_i64, 3_i64, "A")];
        plane.segments = vec![Segment {
            from: LabeledPoint::new(0_i64, 0_i64),
            to: LabeledPoint::new(4_i64, 2_i64),
            label: Some("m".to_owned()),
        }];
        assert!(plane.validate().is_ok());
        assert_eq!(plane.ticks().unwrap(), (11, 11));
        let text = plane.text_equivalent();
        assert!(text.starts_with("A coordinate plane with x from -5 to 5 and y from -5 to 5."));
        assert!(text.contains("The x ticks step by 1 and the y ticks step by 1."));
        assert!(text.contains("A point at (2, 3) labeled A."));
        assert!(text.contains("A segment from (0, 0) to (4, 2), labeled m."));
    }

    #[test]
    fn the_corner_points_stay_inside_and_each_axis_reports_its_own_miss() {
        assert!(plane_with_point("-5", "-5").validate().is_ok());
        assert!(plane_with_point("5", "5").validate().is_ok());
        for (x, y) in [("6", "0"), ("0", "6"), ("-6", "0"), ("0", "-6")] {
            assert!(
                matches!(
                    plane_with_point(x, y).validate(),
                    Err(VisualError::OutOfRange { .. })
                ),
                "point ({x}, {y}) passed the range check"
            );
        }
    }

    #[test]
    fn a_collapsed_axis_names_the_axis_that_collapsed() {
        let mut flat_x = CoordinateFigure::square(5);
        flat_x.x_max = Scalar::from("-5");
        assert!(matches!(
            flat_x.validate(),
            Err(VisualError::RangeNotAscending { axis: "x" })
        ));

        let mut flat_y = CoordinateFigure::square(5);
        flat_y.y_max = Scalar::from("-5");
        assert!(matches!(
            flat_y.validate(),
            Err(VisualError::RangeNotAscending { axis: "y" })
        ));

        let mut fine_y = CoordinateFigure::square(5);
        fine_y.y_tick = Scalar::from("1/1000");
        assert!(matches!(
            fine_y.validate(),
            Err(VisualError::TooManyTicks { axis: "y", .. })
        ));
    }

    #[test]
    fn a_segment_that_starts_and_ends_at_one_point_collapses() {
        let mut plane = CoordinateFigure::square(5);
        plane.segments = vec![Segment {
            from: LabeledPoint::new(1_i64, 1_i64),
            to: LabeledPoint::new("1.0", "1"),
            label: None,
        }];
        let error = plane.validate().unwrap_err();
        assert!(matches!(error, VisualError::Degenerate { .. }));
        assert!(error.to_string().contains("(1, 1)"));
    }

    #[test]
    fn a_shaded_half_plane_reads_its_boundary_and_shaded_side_out_loud() {
        let mut plane = CoordinateFigure::square(5);
        plane.shaded_half_planes = vec![ShadedHalfPlane {
            through_a: LabeledPoint::new(0_i64, 1_i64),
            through_b: LabeledPoint::new(1_i64, 2_i64),
            solid: false,
            shade_toward: LabeledPoint::new(0_i64, 0_i64),
            label: Some("y < x + 1".to_owned()),
        }];
        assert!(plane.validate().is_ok());
        let text = plane.text_equivalent();
        assert!(text.contains(
            "A dashed boundary line through (0, 1) and (1, 2). \
             The region containing (0, 0) is shaded, labeled y < x + 1."
        ));
    }

    #[test]
    fn a_half_plane_with_two_boundary_points_the_same_or_a_shade_point_on_the_line_is_degenerate() {
        let mut same_point = CoordinateFigure::square(5);
        same_point.shaded_half_planes = vec![ShadedHalfPlane {
            through_a: LabeledPoint::new(1_i64, 1_i64),
            through_b: LabeledPoint::new("1.0", "1"),
            solid: true,
            shade_toward: LabeledPoint::new(0_i64, 0_i64),
            label: None,
        }];
        assert!(matches!(
            same_point.validate(),
            Err(VisualError::Degenerate { .. })
        ));

        let mut on_line = CoordinateFigure::square(5);
        on_line.shaded_half_planes = vec![ShadedHalfPlane {
            through_a: LabeledPoint::new(0_i64, 0_i64),
            through_b: LabeledPoint::new(2_i64, 2_i64),
            solid: true,
            shade_toward: LabeledPoint::new(1_i64, 1_i64),
            label: None,
        }];
        let error = on_line.validate().unwrap_err();
        assert!(matches!(error, VisualError::Degenerate { .. }));
        assert!(error.to_string().contains("(1, 1)"));
    }

    #[test]
    fn the_exact_text_prints_an_integer_and_a_fraction() {
        assert_eq!(
            exact_text(&BigRational::from(BigInt::from(4))),
            "4".to_owned()
        );
        assert_eq!(
            exact_text(&BigRational::new(BigInt::from(3), BigInt::from(4))),
            "3/4".to_owned()
        );
    }
}
