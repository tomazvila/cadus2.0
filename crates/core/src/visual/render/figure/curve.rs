//! Curve SVG body.

use std::fmt::Write as _;

use super::super::{RenderOptions, dot_at, line_at, point_label, text_at};
use super::plane::{GridFrame, point_list};
use crate::visual::{Asymptote, CurveFigure, VisualError};

const CURVE_SAMPLES: usize = 121;

/// The body of a curve figure: the grid, the asymptotes, the sampled curve,
/// and the exact key points.
pub(in crate::visual::render) fn curve_body(
    figure: &CurveFigure,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    let frame = GridFrame::new(figure, options)?;
    let (x_min, x_max, y_min, y_max) = frame.bounds;
    let mut out = String::new();
    frame.draw(&mut out);
    for asymptote in &figure.asymptotes {
        let (from, to) = match asymptote {
            Asymptote::Horizontal { at: value } => {
                let y = value.to_f64()?;
                (frame.at(x_min, y), frame.at(x_max, y))
            }
            Asymptote::Vertical { at: value } => {
                let x = value.to_f64()?;
                (frame.at(x, y_min), frame.at(x, y_max))
            }
        };
        line_at(&mut out, from, to, "cadus-visual-asymptote");
    }
    for run in curve_runs(&figure.curve, x_min, x_max, y_min, y_max) {
        let _ = write!(
            out,
            "<polyline points=\"{}\" class=\"cadus-visual-curve\"/>",
            point_list(&run, &|x, y| frame.at(x, y))
        );
    }
    for point in &figure.key_points {
        let pair = point.to_pair()?;
        let spot = frame.at(pair.0, pair.1);
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
