//! The geometry diagram: a polygon with vertices, or a circle.

use num_rational::BigRational;
use num_traits::{Signed, Zero};
use serde::{Deserialize, Serialize};

use super::plane::exact_text;
use super::{LabeledPoint, Scalar, VisualError};

/// The two figures a geometry diagram draws.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeometryShape {
    /// A closed polygon, in vertex order.
    Polygon {
        /// The vertices, in the order the outline visits them.
        vertices: Vec<LabeledPoint>,
        /// The vertex positions that carry a right-angle mark.
        #[serde(default)]
        right_angles: Vec<usize>,
    },
    /// A circle with a center and a radius.
    Circle {
        /// The center.
        center: LabeledPoint,
        /// The radius.
        radius: Scalar,
    },
}

/// A geometry diagram.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeometryFigure {
    /// The drawn figure.
    pub figure: GeometryShape,
    /// The sentence that leads the accessible equivalent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
}

impl GeometryFigure {
    /// A polygon with no right-angle mark.
    #[must_use]
    pub fn polygon(vertices: Vec<LabeledPoint>) -> Self {
        Self {
            figure: GeometryShape::Polygon {
                vertices,
                right_angles: Vec::new(),
            },
            caption: None,
        }
    }

    /// A circle.
    #[must_use]
    pub fn circle(center: LabeledPoint, radius: impl Into<Scalar>) -> Self {
        Self {
            figure: GeometryShape::Circle {
                center,
                radius: radius.into(),
            },
            caption: None,
        }
    }

    /// Whether the diagram carries mathematical meaning.
    pub fn validate(&self) -> Result<(), VisualError> {
        match &self.figure {
            GeometryShape::Polygon {
                vertices,
                right_angles,
            } => validate_polygon(vertices, right_angles),
            GeometryShape::Circle { center, radius } => {
                center.value()?;
                if !radius.value()?.is_positive() {
                    return Err(VisualError::Degenerate {
                        reason: format!("a circle has radius {radius}"),
                    });
                }
                Ok(())
            }
        }
    }

    /// The accessible equivalent.
    #[must_use]
    pub fn text_equivalent(&self) -> String {
        match &self.figure {
            GeometryShape::Polygon {
                vertices,
                right_angles,
            } => polygon_text(vertices, right_angles),
            GeometryShape::Circle { center, radius } => format!(
                "A circle with center at {} and radius {radius}.",
                center.spoken()
            ),
        }
    }
}

/// The name of a polygon with `count` vertices.
fn polygon_name(count: usize) -> String {
    match count {
        3 => "A triangle".to_owned(),
        4 => "A quadrilateral".to_owned(),
        5 => "A pentagon".to_owned(),
        6 => "A hexagon".to_owned(),
        other => format!("A polygon with {other} vertices"),
    }
}

/// Whether a polygon has three vertices, no repeat, and an area above zero.
fn validate_polygon(vertices: &[LabeledPoint], right_angles: &[usize]) -> Result<(), VisualError> {
    if vertices.len() < 3 {
        return Err(VisualError::TooFewVertices {
            count: vertices.len(),
        });
    }
    let mut exact: Vec<(BigRational, BigRational)> = Vec::with_capacity(vertices.len());
    for vertex in vertices {
        exact.push(vertex.value()?);
    }
    for (first, point) in exact.iter().enumerate() {
        for other in exact.iter().skip(first + 1) {
            if point == other {
                return Err(VisualError::Degenerate {
                    reason: format!(
                        "two vertices sit at ({}, {})",
                        exact_text(&point.0),
                        exact_text(&point.1)
                    ),
                });
            }
        }
    }
    if double_area(&exact).is_zero() {
        return Err(VisualError::Degenerate {
            reason: "the vertices sit on one straight line".to_owned(),
        });
    }
    for at in right_angles {
        if *at >= vertices.len() {
            return Err(VisualError::NoSuchVertex {
                at: *at,
                count: vertices.len(),
            });
        }
    }
    Ok(())
}

/// Twice the signed area of a polygon: the shoelace sum, computed exactly.
fn double_area(vertices: &[(BigRational, BigRational)]) -> BigRational {
    let mut sum = BigRational::zero();
    for (at, point) in vertices.iter().enumerate() {
        let next = &vertices[(at + 1) % vertices.len()];
        sum += point.0.clone() * next.1.clone() - next.0.clone() * point.1.clone();
    }
    sum
}

/// The accessible equivalent of a polygon.
fn polygon_text(vertices: &[LabeledPoint], right_angles: &[usize]) -> String {
    let spoken: Vec<String> = vertices.iter().map(LabeledPoint::spoken).collect();
    let mut out = format!(
        "{} with vertices at {}.",
        polygon_name(vertices.len()),
        spoken.join(", ")
    );
    for side in side_texts(vertices) {
        out.push_str(&side);
    }
    for at in right_angles {
        if let Some(vertex) = vertices.get(*at) {
            out.push_str(&format!(" A right angle at {}.", vertex.spoken()));
        }
    }
    out
}

/// One sentence per side the diagram measures exactly: a horizontal side and a
/// vertical side. A slanted side needs a square root, so the text names no
/// length for it.
fn side_texts(vertices: &[LabeledPoint]) -> Vec<String> {
    let mut out = Vec::new();
    for (at, vertex) in vertices.iter().enumerate() {
        let next = &vertices[(at + 1) % vertices.len()];
        let (Ok((x0, y0)), Ok((x1, y1))) = (vertex.value(), next.value()) else {
            return Vec::new();
        };
        let length = if x0 == x1 {
            (y1 - y0).abs()
        } else if y0 == y1 {
            (x1 - x0).abs()
        } else {
            continue;
        };
        out.push(format!(
            " The side from {} to {} is {} units long.",
            vertex.spoken(),
            next.spoken(),
            exact_text(&length)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{GeometryFigure, GeometryShape};
    use crate::visual::{LabeledPoint, VisualError};

    fn right_triangle() -> Vec<LabeledPoint> {
        vec![
            LabeledPoint::labeled(0_i64, 0_i64, "A"),
            LabeledPoint::labeled(4_i64, 0_i64, "B"),
            LabeledPoint::labeled(0_i64, 3_i64, "C"),
        ]
    }

    #[test]
    fn a_right_triangle_validates_and_reads_its_measured_sides_out_loud() {
        let mut figure = GeometryFigure::polygon(right_triangle());
        figure.figure = GeometryShape::Polygon {
            vertices: right_triangle(),
            right_angles: vec![0],
        };
        assert!(figure.validate().is_ok());
        let text = figure.text_equivalent();
        assert!(text.starts_with(
            "A triangle with vertices at (0, 0) labeled A, (4, 0) labeled B, (0, 3) labeled C."
        ));
        assert!(
            text.contains("The side from (0, 0) labeled A to (4, 0) labeled B is 4 units long.")
        );
        assert!(
            text.contains("The side from (0, 3) labeled C to (0, 0) labeled A is 3 units long.")
        );
        assert!(!text.contains("(4, 0) labeled B to (0, 3) labeled C is"));
        assert!(text.ends_with(" A right angle at (0, 0) labeled A."));
    }

    #[test]
    fn a_polygon_needs_three_vertices_no_repeat_and_an_area() {
        assert!(matches!(
            GeometryFigure::polygon(right_triangle()[..2].to_vec()).validate(),
            Err(VisualError::TooFewVertices { count: 2 })
        ));

        let repeated = vec![
            LabeledPoint::new(0_i64, 0_i64),
            LabeledPoint::new(4_i64, 0_i64),
            LabeledPoint::new("0.0", "0"),
        ];
        let error = GeometryFigure::polygon(repeated).validate().unwrap_err();
        assert_eq!(
            error.to_string(),
            "the figure collapses: two vertices sit at (0, 0)"
        );

        let straight = vec![
            LabeledPoint::new(0_i64, 0_i64),
            LabeledPoint::new(1_i64, 1_i64),
            LabeledPoint::new(2_i64, 2_i64),
        ];
        let error = GeometryFigure::polygon(straight).validate().unwrap_err();
        assert_eq!(
            error.to_string(),
            "the figure collapses: the vertices sit on one straight line"
        );
    }

    #[test]
    fn a_right_angle_mark_names_a_vertex_the_polygon_holds() {
        let figure = GeometryFigure {
            figure: GeometryShape::Polygon {
                vertices: right_triangle(),
                right_angles: vec![3],
            },
            caption: None,
        };
        assert!(matches!(
            figure.validate(),
            Err(VisualError::NoSuchVertex { at: 3, count: 3 })
        ));
    }

    #[test]
    fn a_circle_needs_a_radius_above_zero() {
        let good = GeometryFigure::circle(LabeledPoint::new(0_i64, 0_i64), 5_i64);
        assert!(good.validate().is_ok());
        assert_eq!(
            good.text_equivalent(),
            "A circle with center at (0, 0) and radius 5."
        );
        for radius in ["0", "-2"] {
            let bad = GeometryFigure::circle(LabeledPoint::new(0_i64, 0_i64), radius);
            assert!(matches!(
                bad.validate(),
                Err(VisualError::Degenerate { .. })
            ));
        }
    }

    #[test]
    fn the_polygon_name_follows_the_vertex_count() {
        let square = vec![
            LabeledPoint::new(0_i64, 0_i64),
            LabeledPoint::new(2_i64, 0_i64),
            LabeledPoint::new(2_i64, 2_i64),
            LabeledPoint::new(0_i64, 2_i64),
        ];
        assert!(
            GeometryFigure::polygon(square.clone())
                .text_equivalent()
                .starts_with("A quadrilateral")
        );
        let mut seven = square;
        for step in 0..3_i64 {
            seven.push(LabeledPoint::new(-step - 1, step + 3));
        }
        assert!(
            GeometryFigure::polygon(seven)
                .text_equivalent()
                .starts_with("A polygon with 7 vertices")
        );
    }
}
