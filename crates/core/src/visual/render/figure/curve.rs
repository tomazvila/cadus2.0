//! Curve SVG body.

use std::fmt::Write as _;

use super::super::{
    MARGIN, RenderOptions, dot_at, inner_height, inner_width, line_at, point_label, px, text_at,
};
use super::plane::grid_and_axes;
use crate::visual::{Asymptote, CurveFigure, VisualError};

const CURVE_SAMPLES: usize = 121;

/// The body of a curve figure: the grid, the asymptotes, the sampled curve,
/// and the exact key points.
pub(in crate::visual::render) fn curve_body(
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
