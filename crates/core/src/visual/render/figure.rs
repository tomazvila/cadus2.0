//! SVG bodies, split by visual family.

mod curve;
mod fraction;
mod geometry;
mod number_line;
mod plane;
mod special_triangle;

use super::{MARGIN, RenderOptions, inner_height, inner_width};

pub(super) use curve::curve_body;
pub(super) use fraction::fraction_body;
pub(super) use geometry::geometry_body;
pub(super) use number_line::number_line_body;
pub(super) use plane::coordinate_body;
pub(super) use special_triangle::special_triangle_body;

/// The map from figure coordinates to pixels that keeps the aspect ratio.
struct Fit {
    x_min: f64,
    y_max: f64,
    scale: f64,
    left: f64,
    top: f64,
}

impl Fit {
    /// The fit of one bounding box inside the drawn area.
    fn new(bounds: (f64, f64, f64, f64), options: &RenderOptions) -> Self {
        let (x_min, x_max, y_min, y_max) = bounds;
        let width = inner_width(options);
        let height = inner_height(options);
        let x_span = (x_max - x_min).max(f64::EPSILON);
        let y_span = (y_max - y_min).max(f64::EPSILON);
        let scale = (width / x_span).min(height / y_span);
        Self {
            x_min,
            y_max,
            scale,
            left: MARGIN + (width - x_span * scale) / 2.0,
            top: MARGIN + (height - y_span * scale) / 2.0,
        }
    }

    /// The pixel of one figure coordinate.
    fn at(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.x_min).mul_add(self.scale, self.left),
            (self.y_max - y).mul_add(self.scale, self.top),
        )
    }
}
