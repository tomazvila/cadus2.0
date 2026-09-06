//! The body of each of the four figure families.

use std::fmt::Write as _;

use super::{
    MARGIN, RenderOptions, dot_at, inner_height, inner_width, line_at, num_text, point_label, px,
    text_at,
};
use crate::visual::{
    CoordinateFigure, FractionFigure, FractionShape, GeometryFigure, GeometryShape, LabeledPoint,
    NumberLineFigure, VisualError,
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
    Ok(out)
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
    for index in 0..x_ticks {
        let value = x_step.mul_add(index as f64, x_min);
        let (x, _) = at(value, y_min);
        line_at(
            &mut out,
            (x, MARGIN),
            (x, MARGIN + height),
            "cadus-visual-grid",
        );
        if x_ticks <= LABELED_TICKS {
            text_at(
                &mut out,
                x,
                MARGIN + height + 16.0,
                "middle",
                &num_text(value),
            );
        }
    }
    for index in 0..y_ticks {
        let value = y_step.mul_add(index as f64, y_min);
        let (_, y) = at(x_min, value);
        line_at(
            &mut out,
            (MARGIN, y),
            (MARGIN + width, y),
            "cadus-visual-grid",
        );
        if y_ticks <= LABELED_TICKS {
            text_at(&mut out, MARGIN - 8.0, y + 4.0, "end", &num_text(value));
        }
    }
    axes(&mut out, (x_min, x_max, y_min, y_max), width, height, &at);
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

/// The body of a geometry diagram.
pub(super) fn geometry_body(
    figure: &GeometryFigure,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    match &figure.figure {
        GeometryShape::Polygon {
            vertices,
            right_angles,
        } => polygon_body(vertices, right_angles, options),
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

/// The polygon outline, its vertex labels, and its right-angle marks.
fn polygon_body(
    vertices: &[LabeledPoint],
    right_angles: &[usize],
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
    Ok(out)
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
