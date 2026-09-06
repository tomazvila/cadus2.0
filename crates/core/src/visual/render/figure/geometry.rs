//! Geometry SVG body.

use std::fmt::Write as _;

use super::super::{
    MARGIN, RenderOptions, dot_at, inner_height, inner_width, line_at, point_label, px, text_at,
};
use crate::visual::{AngleMark, GeometryFigure, GeometryShape, LabeledPoint, Scalar, VisualError};

pub(in crate::visual::render) fn geometry_body(
    figure: &GeometryFigure,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    match &figure.figure {
        GeometryShape::Polygon {
            vertices,
            right_angles,
            angle_marks,
        } => polygon_body(vertices, right_angles, angle_marks, options),
        GeometryShape::Angle {
            vertex,
            initial_degrees,
            terminal_degrees,
            ray_length,
            label,
        } => angle_body(
            vertex,
            initial_degrees,
            terminal_degrees,
            ray_length,
            label.as_deref(),
            options,
        ),
        GeometryShape::Circle { center, radius } => {
            let (cx, cy) = center.to_pair()?;
            let r = radius.to_f64()?;
            let fit = Fit::new((cx - r, cx + r, cy - r, cy + r), options);
            let spot = fit.at(cx, cy);
            let mut out = String::new();
            let _ = write!(
                out,
                "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" class=\"cadus-visual-shape\"/>",
                px(spot.0),
                px(spot.1),
                px(r * fit.scale)
            );
            dot_at(&mut out, spot, 3.0, "cadus-visual-point");
            line_at(
                &mut out,
                spot,
                (r.mul_add(fit.scale, spot.0), spot.1),
                "cadus-visual-segment",
            );
            text_at(
                &mut out,
                r.mul_add(fit.scale / 2.0, spot.0),
                spot.1 - 6.0,
                "middle",
                &format!("r = {radius}"),
            );
            Ok(out)
        }
    }
}

/// The polygon outline, its vertex labels, its right-angle marks, and its
/// labeled angle marks.
fn polygon_body(
    vertices: &[LabeledPoint],
    right_angles: &[usize],
    angle_marks: &[AngleMark],
    options: &RenderOptions,
) -> Result<String, VisualError> {
    let mut pairs = Vec::with_capacity(vertices.len());
    for vertex in vertices {
        pairs.push(vertex.to_pair()?);
    }
    let fit = Fit::new(bounds_of(&pairs), options);
    let spots: Vec<(f64, f64)> = pairs.iter().map(|(x, y)| fit.at(*x, *y)).collect();
    let points: Vec<String> = spots
        .iter()
        .map(|(x, y)| format!("{},{}", px(*x), px(*y)))
        .collect();
    let mut out = format!(
        "<polygon points=\"{}\" class=\"cadus-visual-shape\"/>",
        points.join(" ")
    );
    for (index, spot) in spots.iter().enumerate() {
        let label = vertices.get(index).map(point_label).unwrap_or_default();
        if !label.is_empty() {
            text_at(&mut out, spot.0 + 8.0, spot.1 - 8.0, "start", label);
        }
    }
    for at in right_angles {
        if let Some(mark) = right_angle_mark(&spots, *at) {
            out.push_str(&mark);
        }
    }
    for mark in angle_marks {
        vertex_angle_arc(&mut out, &spots, mark.at, mark.label.as_deref());
    }
    Ok(out)
}

/// The small arc that marks one polygon vertex's angle, between the two edges
/// that meet there.
pub(super) fn vertex_angle_arc(
    out: &mut String,
    spots: &[(f64, f64)],
    at: usize,
    label: Option<&str>,
) {
    let count = spots.len();
    let (Some(&corner), Some(&previous), Some(&next)) = (
        spots.get(at),
        spots.get((at + count - 1) % count),
        spots.get((at + 1) % count),
    ) else {
        return;
    };
    let (Some(first), Some(second)) = (unit(corner, previous), unit(corner, next)) else {
        return;
    };
    arc_mark(
        out,
        corner,
        first,
        second,
        16.0,
        "cadus-visual-angle",
        label,
    );
}

/// The body of a standalone angle diagram: two rays from one vertex, an arc
/// between them, and the label.
fn angle_body(
    vertex: &LabeledPoint,
    initial_degrees: &Scalar,
    terminal_degrees: &Scalar,
    ray_length: &Scalar,
    label: Option<&str>,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    let (vx, vy) = vertex.to_pair()?;
    let length = ray_length.to_f64()?;
    let initial = initial_degrees.to_f64()?.to_radians();
    let terminal = terminal_degrees.to_f64()?.to_radians();
    let ray_end = |angle: f64| {
        (
            length.mul_add(angle.cos(), vx),
            length.mul_add(angle.sin(), vy),
        )
    };
    let (first_end, second_end) = (ray_end(initial), ray_end(terminal));
    let fit = Fit::new(bounds_of(&[(vx, vy), first_end, second_end]), options);
    let corner = fit.at(vx, vy);
    let (first_spot, second_spot) = (
        fit.at(first_end.0, first_end.1),
        fit.at(second_end.0, second_end.1),
    );
    let mut out = String::new();
    line_at(&mut out, corner, first_spot, "cadus-visual-segment");
    line_at(&mut out, corner, second_spot, "cadus-visual-segment");
    dot_at(&mut out, corner, 3.0, "cadus-visual-point");
    let (Some(first_dir), Some(second_dir)) = (unit(corner, first_spot), unit(corner, second_spot))
    else {
        return Ok(out);
    };
    arc_mark(
        &mut out,
        corner,
        first_dir,
        second_dir,
        20.0,
        "cadus-visual-angle",
        label,
    );
    Ok(out)
}

/// The small arc between two unit directions from `corner`, plus a label
/// placed along their bisector.
fn arc_mark(
    out: &mut String,
    corner: (f64, f64),
    first: (f64, f64),
    second: (f64, f64),
    radius: f64,
    class: &str,
    label: Option<&str>,
) {
    let start = (
        radius.mul_add(first.0, corner.0),
        radius.mul_add(first.1, corner.1),
    );
    let end = (
        radius.mul_add(second.0, corner.0),
        radius.mul_add(second.1, corner.1),
    );
    let cross = first.0 * second.1 - first.1 * second.0;
    let sweep = i32::from(cross > 0.0);
    let _ = write!(
        out,
        "<path d=\"M {} {} A {r} {r} 0 0 {sweep} {} {}\" class=\"{class}\" fill=\"none\"/>",
        px(start.0),
        px(start.1),
        px(end.0),
        px(end.1),
        r = px(radius),
    );
    let Some(label) = label.map(str::trim).filter(|t| !t.is_empty()) else {
        return;
    };
    let bisector =
        unit((0.0, 0.0), (first.0 + second.0, first.1 + second.1)).unwrap_or((first.0, first.1));
    let at = (
        (radius + 14.0).mul_add(bisector.0, corner.0),
        (radius + 14.0).mul_add(bisector.1, corner.1),
    );
    text_at(out, at.0, at.1, "middle", label);
}

/// The small square that marks one right angle.
pub(super) fn right_angle_mark(spots: &[(f64, f64)], at: usize) -> Option<String> {
    let count = spots.len();
    let corner = *spots.get(at)?;
    let previous = *spots.get((at + count - 1) % count)?;
    let next = *spots.get((at + 1) % count)?;
    let first = unit(corner, previous)?;
    let second = unit(corner, next)?;
    let size: f64 = 10.0;
    let path = [
        (
            size.mul_add(first.0, corner.0),
            size.mul_add(first.1, corner.1),
        ),
        (
            size.mul_add(first.0 + second.0, corner.0),
            size.mul_add(first.1 + second.1, corner.1),
        ),
        (
            size.mul_add(second.0, corner.0),
            size.mul_add(second.1, corner.1),
        ),
    ];
    let points: Vec<String> = path
        .iter()
        .map(|(x, y)| format!("{},{}", px(*x), px(*y)))
        .collect();
    Some(format!(
        "<polyline points=\"{}\" class=\"cadus-visual-right-angle\"/>",
        points.join(" ")
    ))
}

/// The unit vector from one point toward another.
fn unit(from: (f64, f64), to: (f64, f64)) -> Option<(f64, f64)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return None;
    }
    Some((dx / length, dy / length))
}

/// The bounding box of a vertex list.
pub(super) fn bounds_of(pairs: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut bounds = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for (x, y) in pairs {
        bounds.0 = bounds.0.min(*x);
        bounds.1 = bounds.1.max(*x);
        bounds.2 = bounds.2.min(*y);
        bounds.3 = bounds.3.max(*y);
    }
    bounds
}

/// The map from figure coordinates to pixels that keeps the aspect ratio.
struct Fit {
    x_min: f64,
    y_max: f64,
    scale: f64,
    left: f64,
    top: f64,
}

impl Fit {
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

    fn at(&self, x: f64, y: f64) -> (f64, f64) {
        (
            (x - self.x_min).mul_add(self.scale, self.left),
            (self.y_max - y).mul_add(self.scale, self.top),
        )
    }
}
