//! Coordinate-plane SVG body and shared grid helpers.

use std::fmt::Write as _;

use super::super::{
    MARGIN, RenderOptions, dot_at, inner_height, inner_width, line_at, num_text, point_label, px,
    text_at,
};
use crate::visual::{CoordinateFigure, LabeledPoint, Segment, ShadedHalfPlane, VisualError};

const LABELED_TICKS: i64 = 21;

pub(in crate::visual::render) fn coordinate_body(
    figure: &CoordinateFigure,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    let (x_min, x_max) = (figure.x_min.to_f64()?, figure.x_max.to_f64()?);
    let (y_min, y_max) = (figure.y_min.to_f64()?, figure.y_max.to_f64()?);
    let (x_ticks, y_ticks) = figure.ticks()?;
    let (x_step, y_step) = (figure.x_tick.to_f64()?, figure.y_tick.to_f64()?);
    let width = inner_width(options);
    let height = inner_height(options);
    let at = |x: f64, y: f64| {
        (
            MARGIN + (x - x_min) / (x_max - x_min) * width,
            MARGIN + (y_max - y) / (y_max - y_min) * height,
        )
    };

    let mut out = String::new();
    grid_and_axes(
        &mut out,
        (x_min, x_max, y_min, y_max),
        (x_ticks, y_ticks),
        (x_step, y_step),
        width,
        height,
        &at,
    );
    draw_regions(
        &mut out,
        &figure.shaded_half_planes,
        (x_min, x_max, y_min, y_max),
        &at,
    )?;
    draw_segments(&mut out, &figure.segments, &at)?;
    draw_points(&mut out, &figure.points, &at)?;
    Ok(out)
}

fn draw_regions(
    out: &mut String,
    regions: &[ShadedHalfPlane],
    bounds: (f64, f64, f64, f64),
    at: &impl Fn(f64, f64) -> (f64, f64),
) -> Result<(), VisualError> {
    let (x_min, x_max, y_min, y_max) = bounds;
    for region in regions {
        let a = region.through_a.to_pair()?;
        let b = region.through_b.to_pair()?;
        let toward = region.shade_toward.to_pair()?;
        let corners = [
            (x_min, y_min),
            (x_max, y_min),
            (x_max, y_max),
            (x_min, y_max),
        ];
        let clipped = clip_half_plane(&corners, a, b, toward);
        if clipped.len() >= 3 {
            let points: Vec<String> = clipped
                .iter()
                .map(|(x, y)| {
                    let (px_, py_) = at(*x, *y);
                    format!("{},{}", px(px_), px(py_))
                })
                .collect();
            let _ = write!(
                out,
                "<polygon points=\"{}\" class=\"cadus-visual-shaded\"/>",
                points.join(" ")
            );
        }
        let boundary_class = if region.solid {
            "cadus-visual-boundary-solid"
        } else {
            "cadus-visual-boundary-dashed"
        };
        let (bx0, by0, bx1, by1) = extend_to_frame(a, b, bounds);
        line_at(out, at(bx0, by0), at(bx1, by1), boundary_class);
        if let Some(label) = region
            .label
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            let mid = at(bx0.midpoint(bx1), by0.midpoint(by1));
            text_at(out, mid.0, mid.1 - 8.0, "middle", label);
        }
    }
    Ok(())
}

fn draw_segments(
    out: &mut String,
    segments: &[Segment],
    at: &impl Fn(f64, f64) -> (f64, f64),
) -> Result<(), VisualError> {
    for segment in segments {
        let from = segment.from.to_pair()?;
        let to = segment.to.to_pair()?;
        let start = at(from.0, from.1);
        let end = at(to.0, to.1);
        line_at(out, start, end, "cadus-visual-segment");
        if let Some(label) = segment
            .label
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            text_at(
                out,
                start.0.midpoint(end.0),
                start.1.midpoint(end.1) - 8.0,
                "middle",
                label,
            );
        }
    }
    Ok(())
}

fn draw_points(
    out: &mut String,
    points: &[LabeledPoint],
    at: &impl Fn(f64, f64) -> (f64, f64),
) -> Result<(), VisualError> {
    for point in points {
        let pair = point.to_pair()?;
        let spot = at(pair.0, pair.1);
        dot_at(out, spot, 4.0, "cadus-visual-point");
        let label = point_label(point);
        if !label.is_empty() {
            text_at(out, spot.0 + 8.0, spot.1 - 8.0, "start", label);
        }
    }
    Ok(())
}

/// The grid lines, tick labels, and axes shared by a coordinate plane and a
/// curve figure.
#[allow(clippy::too_many_arguments)]
pub(super) fn grid_and_axes(
    out: &mut String,
    bounds: (f64, f64, f64, f64),
    ticks: (i64, i64),
    steps: (f64, f64),
    width: f64,
    height: f64,
    at: &impl Fn(f64, f64) -> (f64, f64),
) {
    let (x_min, _, y_min, _) = bounds;
    let (x_ticks, y_ticks) = ticks;
    let (x_step, y_step) = steps;
    for index in 0..x_ticks {
        let value = x_step.mul_add(index as f64, x_min);
        let (x, _) = at(value, y_min);
        line_at(out, (x, MARGIN), (x, MARGIN + height), "cadus-visual-grid");
        if x_ticks <= LABELED_TICKS {
            text_at(out, x, MARGIN + height + 16.0, "middle", &num_text(value));
        }
    }
    for index in 0..y_ticks {
        let value = y_step.mul_add(index as f64, y_min);
        let (_, y) = at(x_min, value);
        line_at(out, (MARGIN, y), (MARGIN + width, y), "cadus-visual-grid");
        if y_ticks <= LABELED_TICKS {
            text_at(out, MARGIN - 8.0, y + 4.0, "end", &num_text(value));
        }
    }
    axes(out, bounds, width, height, at);
}

/// The two axes, drawn only where the plane holds zero.
fn axes(
    out: &mut String,
    bounds: (f64, f64, f64, f64),
    width: f64,
    height: f64,
    at: &impl Fn(f64, f64) -> (f64, f64),
) {
    let (x_min, x_max, y_min, y_max) = bounds;
    if x_min <= 0.0 && x_max >= 0.0 {
        let (x, _) = at(0.0, y_min);
        line_at(out, (x, MARGIN), (x, MARGIN + height), "cadus-visual-axis");
    }
    if y_min <= 0.0 && y_max >= 0.0 {
        let (_, y) = at(x_min, 0.0);
        line_at(out, (MARGIN, y), (MARGIN + width, y), "cadus-visual-axis");
    }
}

/// Twice the signed area of the triangle `a`, `b`, `p`: positive on one side of
/// the line through `a` and `b`, negative on the other, zero on the line.
fn cross(a: (f64, f64), b: (f64, f64), p: (f64, f64)) -> f64 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

/// The point where segment `p`-`q` crosses the line through `a` and `b`.
fn intersect(a: (f64, f64), b: (f64, f64), p: (f64, f64), q: (f64, f64)) -> (f64, f64) {
    let (d1, d2) = (cross(a, b, p), cross(a, b, q));
    let t = d1 / (d1 - d2);
    (p.0 + t * (q.0 - p.0), p.1 + t * (q.1 - p.1))
}

/// Sutherland-Hodgman clip of a convex polygon against one half-plane: the side
/// of the line through `a` and `b` that holds `toward`.
fn clip_half_plane(
    subject: &[(f64, f64)],
    a: (f64, f64),
    b: (f64, f64),
    toward: (f64, f64),
) -> Vec<(f64, f64)> {
    let toward_sign = cross(a, b, toward);
    let inside = |p: (f64, f64)| cross(a, b, p) * toward_sign >= 0.0;
    let count = subject.len();
    let mut out = Vec::with_capacity(count + 1);
    for at in 0..count {
        let current = subject[at];
        let previous = subject[(at + count - 1) % count];
        let (current_in, previous_in) = (inside(current), inside(previous));
        if current_in {
            if !previous_in {
                out.push(intersect(a, b, previous, current));
            }
            out.push(current);
        } else if previous_in {
            out.push(intersect(a, b, previous, current));
        }
    }
    out
}

/// The two points where the infinite line through `a` and `b` crosses the
/// drawn rectangle, given that `a` and `b` both sit inside it.
fn extend_to_frame(
    a: (f64, f64),
    b: (f64, f64),
    bounds: (f64, f64, f64, f64),
) -> (f64, f64, f64, f64) {
    let (x_min, x_max, y_min, y_max) = bounds;
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for (p, q) in [
        (-dx, a.0 - x_min),
        (dx, x_max - a.0),
        (-dy, a.1 - y_min),
        (dy, y_max - a.1),
    ] {
        if p == 0.0 {
            continue;
        }
        let r = q / p;
        if p < 0.0 {
            t0 = t0.max(r);
        } else {
            t1 = t1.min(r);
        }
    }
    (
        dx.mul_add(t0, a.0),
        dy.mul_add(t0, a.1),
        dx.mul_add(t1, a.0),
        dy.mul_add(t1, a.1),
    )
}
