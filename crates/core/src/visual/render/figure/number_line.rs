//! Number-line SVG body.

use std::fmt::Write as _;

use super::super::{MARGIN, RenderOptions, dot_at, inner_width, line_at, num_text, px, text_at};
use crate::visual::{
    MarkedInterval, MarkedPoint, MarkedRay, NumberLineFigure, RayDirection, VisualError,
};

const LABELED_TICKS: i64 = 21;

pub(in crate::visual::render) fn number_line_body(
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
    draw_ticks(&mut out, min, step, ticks, baseline, &at);
    draw_intervals(&mut out, &figure.intervals, baseline, &at)?;
    draw_points(&mut out, &figure.points, baseline, &at)?;
    draw_rays(&mut out, &figure.rays, baseline, width, &at)?;
    Ok(out)
}

fn draw_ticks(
    out: &mut String,
    min: f64,
    step: f64,
    ticks: i64,
    baseline: f64,
    at: &impl Fn(f64) -> f64,
) {
    for index in 0..ticks {
        let value = step.mul_add(index as f64, min);
        let x = at(value);
        line_at(
            out,
            (x, baseline - 6.0),
            (x, baseline + 6.0),
            "cadus-visual-tick",
        );
        if ticks <= LABELED_TICKS {
            text_at(out, x, baseline + 22.0, "middle", &num_text(value));
        }
    }
}

fn draw_intervals(
    out: &mut String,
    intervals: &[MarkedInterval],
    baseline: f64,
    at: &impl Fn(f64) -> f64,
) -> Result<(), VisualError> {
    for interval in intervals {
        let (from, to) = (interval.from.to_f64()?, interval.to.to_f64()?);
        line_at(
            out,
            (at(from), baseline - 14.0),
            (at(to), baseline - 14.0),
            "cadus-visual-interval",
        );
        end_cap(out, at(from), baseline - 14.0, interval.closed_start);
        end_cap(out, at(to), baseline - 14.0, interval.closed_end);
        if let Some(label) = interval.label.as_deref() {
            text_at(
                out,
                at(from).midpoint(at(to)),
                baseline - 22.0,
                "middle",
                label.trim(),
            );
        }
    }
    Ok(())
}

fn draw_points(
    out: &mut String,
    points: &[MarkedPoint],
    baseline: f64,
    at: &impl Fn(f64) -> f64,
) -> Result<(), VisualError> {
    for point in points {
        let x = at(point.at.to_f64()?);
        let class = if point.filled {
            "cadus-visual-point"
        } else {
            "cadus-visual-point-open"
        };
        dot_at(out, (x, baseline), 5.0, class);
        if let Some(label) = point
            .label
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
        {
            text_at(out, x, baseline - 24.0, "middle", label);
        }
    }
    Ok(())
}

fn draw_rays(
    out: &mut String,
    rays: &[MarkedRay],
    baseline: f64,
    width: f64,
    at: &impl Fn(f64) -> f64,
) -> Result<(), VisualError> {
    for ray in rays {
        let origin = at(ray.from.to_f64()?);
        let edge = match ray.direction {
            RayDirection::Left => MARGIN,
            RayDirection::Right => MARGIN + width,
        };
        let track = baseline - 14.0;
        line_at(out, (origin, track), (edge, track), "cadus-visual-ray");
        arrowhead(out, edge, track, ray.direction);
        end_cap(out, origin, track, ray.filled);
        if let Some(label) = label_of(ray.label.as_deref()) {
            text_at(out, origin.midpoint(edge), track - 8.0, "middle", label);
        }
    }
    Ok(())
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
