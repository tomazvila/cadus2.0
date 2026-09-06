//! The body of each of the four figure families.

use std::fmt::Write as _;

use super::{
    MARGIN, RenderOptions, dot_at, inner_height, inner_width, line_at, num_text, point_label, px,
    text_at,
};
use crate::visual::plane::exact_text;
use crate::visual::special_triangle::radical_text;
use crate::visual::{
    AngleMark, Asymptote, CoordinateFigure, CurveFigure, FractionFigure, FractionShape,
    GeometryFigure, GeometryShape, LabeledPoint, NumberLineFigure, RayDirection, Scalar,
    SpecialTriangleFigure, SpecialTriangleShape, VisualError,
};

/// The largest tick count that still carries a printed label.
const LABELED_TICKS: i64 = 21;

/// The body of a number line.
pub(super) fn number_line_body(
    figure: &NumberLineFigure,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    let (min, max) = (figure.min.to_f64()?, figure.max.to_f64()?);
    let step = figure.tick.to_f64()?;
    let ticks = figure.ticks()?;
    let width = inner_width(options);
    let baseline = options.height as f64 / 2.0;
    let at = |value: f64| MARGIN + (value - min) / (max - min) * width;

    let mut out = String::new();
    line_at(
        &mut out,
        (MARGIN, baseline),
        (MARGIN + width, baseline),
        "cadus-visual-axis",
    );
    for index in 0..ticks {
        let value = step.mul_add(index as f64, min);
        let x = at(value);
        line_at(
            &mut out,
            (x, baseline - 6.0),
            (x, baseline + 6.0),
            "cadus-visual-tick",
        );
        if ticks <= LABELED_TICKS {
            text_at(&mut out, x, baseline + 22.0, "middle", &num_text(value));
        }
    }
    for interval in &figure.intervals {
        let (from, to) = (interval.from.to_f64()?, interval.to.to_f64()?);
        line_at(
            &mut out,
            (at(from), baseline - 14.0),
            (at(to), baseline - 14.0),
            "cadus-visual-interval",
        );
        end_cap(&mut out, at(from), baseline - 14.0, interval.closed_start);
        end_cap(&mut out, at(to), baseline - 14.0, interval.closed_end);
        if let Some(label) = interval.label.as_deref() {
            text_at(
                &mut out,
                at(from).midpoint(at(to)),
                baseline - 22.0,
                "middle",
                label.trim(),
            );
        }
    }
    for point in &figure.points {
        let x = at(point.at.to_f64()?);
        let class = if point.filled {
            "cadus-visual-point"
        } else {
            "cadus-visual-point-open"
        };
        dot_at(&mut out, (x, baseline), 5.0, class);
        if let Some(label) = point
            .label
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            text_at(&mut out, x, baseline - 24.0, "middle", label);
        }
    }
    for ray in &figure.rays {
        let origin = at(ray.from.to_f64()?);
        let edge = match ray.direction {
            RayDirection::Left => MARGIN,
            RayDirection::Right => MARGIN + width,
        };
        let track = baseline - 14.0;
        line_at(&mut out, (origin, track), (edge, track), "cadus-visual-ray");
        arrowhead(&mut out, edge, track, ray.direction);
        end_cap(&mut out, origin, track, ray.filled);
        if let Some(label) = label_of(ray.label.as_deref()) {
            text_at(
                &mut out,
                origin.midpoint(edge),
                track - 8.0,
                "middle",
                label,
            );
        }
    }
    Ok(out)
}

/// The label text, or `None` for an absent or blank label.
fn label_of(label: Option<&str>) -> Option<&str> {
    label.map(str::trim).filter(|text| !text.is_empty())
}

/// The small triangle that caps a ray at the drawn frame edge.
fn arrowhead(out: &mut String, x: f64, y: f64, direction: RayDirection) {
    let tip = match direction {
        RayDirection::Left => x - 7.0,
        RayDirection::Right => x + 7.0,
    };
    let points = [(x, y - 5.0), (tip, y), (x, y + 5.0)];
    let coords: Vec<String> = points
        .iter()
        .map(|(px_, py_)| format!("{},{}", px(*px_), px(*py_)))
        .collect();
    let _ = write!(
        out,
        "<polygon points=\"{}\" class=\"cadus-visual-ray\"/>",
        coords.join(" ")
    );
}

/// One end of a marked interval: a filled dot for a closed end, an open dot for
/// an open end.
fn end_cap(out: &mut String, x: f64, y: f64, closed: bool) {
    let class = if closed {
        "cadus-visual-point"
    } else {
        "cadus-visual-point-open"
    };
    dot_at(out, (x, y), 5.0, class);
}

/// The body of a fraction figure.
pub(super) fn fraction_body(figure: &FractionFigure, options: &RenderOptions) -> String {
    let wholes = figure.wholes().max(1);
    let width = inner_width(options);
    let height = inner_height(options);
    let gap = 14.0;
    let each = ((width - gap * (wholes - 1) as f64) / wholes as f64).max(1.0);
    let mut out = String::new();
    let mut left = figure.shaded;
    for whole in 0..wholes {
        let origin = MARGIN + (each + gap) * whole as f64;
        let shaded = left.clamp(0, figure.parts);
        left -= shaded;
        match figure.shape {
            FractionShape::Bar => bar_whole(&mut out, origin, each, height, figure.parts, shaded),
            FractionShape::Circle => {
                circle_whole(&mut out, origin, each, height, figure.parts, shaded);
            }
        }
    }
    out
}

/// One divided bar.
fn bar_whole(out: &mut String, origin: f64, each: f64, height: f64, parts: i64, shaded: i64) {
    let part = each / parts as f64;
    let top = MARGIN;
    let tall = height.min(90.0);
    for index in 0..parts {
        let class = if index < shaded {
            "cadus-visual-part-shaded"
        } else {
            "cadus-visual-part"
        };
        let _ = write!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"{class}\"/>",
            px(part.mul_add(index as f64, origin)),
            px(top),
            px(part),
            px(tall)
        );
    }
}

/// One divided circle.
fn circle_whole(out: &mut String, origin: f64, each: f64, height: f64, parts: i64, shaded: i64) {
    let radius = (each / 2.0).min(height / 2.0).max(1.0);
    let center = (origin + each / 2.0, MARGIN + radius);
    let sweep = 360.0 / parts as f64;
    for index in 0..parts {
        let class = if index < shaded {
            "cadus-visual-part-shaded"
        } else {
            "cadus-visual-part"
        };
        let start = sweep.mul_add(index as f64, -90.0).to_radians();
        let end = sweep.mul_add((index + 1) as f64, -90.0).to_radians();
        let large = i32::from(sweep > 180.0);
        let _ = write!(
            out,
            "<path d=\"M {cx} {cy} L {x0} {y0} A {r} {r} 0 {large} 1 {x1} {y1} Z\" \
             class=\"{class}\"/>",
            cx = px(center.0),
            cy = px(center.1),
            x0 = px(radius.mul_add(start.cos(), center.0)),
            y0 = px(radius.mul_add(start.sin(), center.1)),
            r = px(radius),
            x1 = px(radius.mul_add(end.cos(), center.0)),
            y1 = px(radius.mul_add(end.sin(), center.1)),
        );
    }
}

/// The body of a coordinate plane.
pub(super) fn coordinate_body(
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
    for region in &figure.shaded_half_planes {
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
        let (bx0, by0, bx1, by1) = extend_to_frame(a, b, (x_min, x_max, y_min, y_max));
        line_at(&mut out, at(bx0, by0), at(bx1, by1), boundary_class);
        if let Some(label) = region
            .label
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            let mid = at(bx0.midpoint(bx1), by0.midpoint(by1));
            text_at(&mut out, mid.0, mid.1 - 8.0, "middle", label);
        }
    }
    for segment in &figure.segments {
        let from = segment.from.to_pair()?;
        let to = segment.to.to_pair()?;
        let start = at(from.0, from.1);
        let end = at(to.0, to.1);
        line_at(&mut out, start, end, "cadus-visual-segment");
        if let Some(label) = segment
            .label
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            text_at(
                &mut out,
                start.0.midpoint(end.0),
                start.1.midpoint(end.1) - 8.0,
                "middle",
                label,
            );
        }
    }
    for point in &figure.points {
        let pair = point.to_pair()?;
        let spot = at(pair.0, pair.1);
        dot_at(&mut out, spot, 4.0, "cadus-visual-point");
        let label = point_label(point);
        if !label.is_empty() {
            text_at(&mut out, spot.0 + 8.0, spot.1 - 8.0, "start", label);
        }
    }
    Ok(out)
}

/// The grid lines, tick labels, and axes shared by a coordinate plane and a
/// curve figure.
#[allow(clippy::too_many_arguments)]
fn grid_and_axes(
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

/// The count of points one sampled curve run draws.
const CURVE_SAMPLES: usize = 121;

/// The body of a curve figure: the grid, the asymptotes, the sampled curve,
/// and the exact key points.
pub(super) fn curve_body(
    figure: &CurveFigure,
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
    for asymptote in &figure.asymptotes {
        let (from, to) = match asymptote {
            Asymptote::Horizontal { at: value } => {
                let y = value.to_f64()?;
                (at(x_min, y), at(x_max, y))
            }
            Asymptote::Vertical { at: value } => {
                let x = value.to_f64()?;
                (at(x, y_min), at(x, y_max))
            }
        };
        line_at(&mut out, from, to, "cadus-visual-asymptote");
    }
    for run in curve_runs(&figure.curve, x_min, x_max, y_min, y_max) {
        let points: Vec<String> = run
            .iter()
            .map(|(x, y)| {
                let (px_, py_) = at(*x, *y);
                format!("{},{}", px(px_), px(py_))
            })
            .collect();
        let _ = write!(
            out,
            "<polyline points=\"{}\" class=\"cadus-visual-curve\"/>",
            points.join(" ")
        );
    }
    for point in &figure.key_points {
        let pair = point.to_pair()?;
        let spot = at(pair.0, pair.1);
        dot_at(&mut out, spot, 4.0, "cadus-visual-point");
        let label = point_label(point);
        if !label.is_empty() {
            text_at(&mut out, spot.0 + 8.0, spot.1 - 8.0, "start", label);
        }
    }
    Ok(out)
}

/// The sampled runs of one curve inside the drawn viewport.
///
/// A run breaks wherever the function leaves its domain (a logarithm below
/// its shift, a reciprocal at its asymptote) or leaves the padded vertical
/// viewport, so the render never draws a false continuation or a spike that
/// swamps the rest of the picture.
fn curve_runs(
    kind: &crate::visual::CurveKind,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
) -> Vec<Vec<(f64, f64)>> {
    let pad = (y_max - y_min).max(1e-9);
    let (low, high) = (y_min - pad, y_max + pad);
    let step = (x_max - x_min) / (CURVE_SAMPLES - 1) as f64;
    let mut runs = Vec::new();
    let mut current: Vec<(f64, f64)> = Vec::new();
    for index in 0..CURVE_SAMPLES {
        let x = step.mul_add(index as f64, x_min);
        let sample = kind
            .eval_f64(x)
            .filter(|y| y.is_finite() && *y >= low && *y <= high);
        match sample {
            Some(y) => current.push((x, y)),
            None => {
                if current.len() >= 2 {
                    runs.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
        }
    }
    if current.len() >= 2 {
        runs.push(current);
    }
    runs
}

/// The body of a geometry diagram.
pub(super) fn geometry_body(
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
fn vertex_angle_arc(out: &mut String, spots: &[(f64, f64)], at: usize, label: Option<&str>) {
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
fn right_angle_mark(spots: &[(f64, f64)], at: usize) -> Option<String> {
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
fn bounds_of(pairs: &[(f64, f64)]) -> (f64, f64, f64, f64) {
    let mut bounds = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for (x, y) in pairs {
        bounds.0 = bounds.0.min(*x);
        bounds.1 = bounds.1.max(*x);
        bounds.2 = bounds.2.min(*y);
        bounds.3 = bounds.3.max(*y);
    }
    bounds
}

/// The body of a special-triangle figure: an illustrative triangle (the
/// picture approximates any irrational side with a float, the same as any
/// other rendered shape) with its exact side or angle labels.
pub(super) fn special_triangle_body(
    figure: &SpecialTriangleFigure,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    match &figure.figure {
        SpecialTriangleShape::FortyFiveFortyFiveNinety { leg } => {
            let length = leg.to_f64()?;
            let value = leg.value()?;
            let vertices = [(0.0, 0.0), (length, 0.0), (0.0, length)];
            let fit = Fit::new(bounds_of(&vertices), options);
            let spots: Vec<(f64, f64)> = vertices.iter().map(|(x, y)| fit.at(*x, *y)).collect();
            let mut out = triangle_outline(&spots);
            if let Some(mark) = right_angle_mark(&spots, 0) {
                out.push_str(&mark);
            }
            side_label(&mut out, spots[0], spots[1], leg.as_str());
            side_label(&mut out, spots[2], spots[0], leg.as_str());
            side_label(&mut out, spots[1], spots[2], &radical_text(&value, 2));
            Ok(out)
        }
        SpecialTriangleShape::ThirtySixtyNinety { short_leg } => {
            let short = short_leg.to_f64()?;
            let value = short_leg.value()?;
            let long = short * 3.0_f64.sqrt();
            let vertices = [(0.0, 0.0), (long, 0.0), (0.0, short)];
            let fit = Fit::new(bounds_of(&vertices), options);
            let spots: Vec<(f64, f64)> = vertices.iter().map(|(x, y)| fit.at(*x, *y)).collect();
            let mut out = triangle_outline(&spots);
            if let Some(mark) = right_angle_mark(&spots, 0) {
                out.push_str(&mark);
            }
            vertex_angle_arc(&mut out, &spots, 1, Some("30°"));
            vertex_angle_arc(&mut out, &spots, 2, Some("60°"));
            side_label(&mut out, spots[0], spots[1], &radical_text(&value, 3));
            side_label(&mut out, spots[2], spots[0], short_leg.as_str());
            side_label(
                &mut out,
                spots[1],
                spots[2],
                &exact_text(&(value * num_bigint::BigInt::from(2))),
            );
            Ok(out)
        }
        SpecialTriangleShape::SasArea {
            side_a,
            side_b,
            included_angle_degrees,
        } => {
            let (a, b) = (side_a.to_f64()?, side_b.to_f64()?);
            let angle = included_angle_degrees.to_f64()?.to_radians();
            let vertices = [(0.0, 0.0), (b, 0.0), (a * angle.cos(), a * angle.sin())];
            let fit = Fit::new(bounds_of(&vertices), options);
            let spots: Vec<(f64, f64)> = vertices.iter().map(|(x, y)| fit.at(*x, *y)).collect();
            let mut out = triangle_outline(&spots);
            let label = format!("{included_angle_degrees}°");
            vertex_angle_arc(&mut out, &spots, 0, Some(&label));
            side_label(&mut out, spots[0], spots[1], side_b.as_str());
            side_label(&mut out, spots[2], spots[0], side_a.as_str());
            Ok(out)
        }
    }
}

/// The `<polygon>` outline of a three-point spot list.
fn triangle_outline(spots: &[(f64, f64)]) -> String {
    let points: Vec<String> = spots
        .iter()
        .map(|(x, y)| format!("{},{}", px(*x), px(*y)))
        .collect();
    format!(
        "<polygon points=\"{}\" class=\"cadus-visual-shape\"/>",
        points.join(" ")
    )
}

/// One side label, placed at the midpoint of the two spots.
fn side_label(out: &mut String, from: (f64, f64), to: (f64, f64), label: &str) {
    text_at(
        out,
        from.0.midpoint(to.0),
        from.1.midpoint(to.1) - 6.0,
        "middle",
        label,
    );
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
