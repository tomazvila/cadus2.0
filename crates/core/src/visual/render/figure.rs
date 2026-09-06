//! SVG bodies, split by visual family.

mod curve;
mod fraction;
mod geometry;
mod number_line;
mod plane;
mod special_triangle;

pub(super) use curve::curve_body;
pub(super) use fraction::fraction_body;
pub(super) use geometry::geometry_body;
pub(super) use number_line::number_line_body;
pub(super) use plane::coordinate_body;
pub(super) use special_triangle::special_triangle_body;
