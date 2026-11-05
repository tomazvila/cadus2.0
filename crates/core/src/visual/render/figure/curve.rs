//! Curve SVG body.

use std::fmt::Write as _;

use super::super::{MARGIN, RenderOptions, dot_at, line_at, point_label, text_at};
use super::plane::{GridFrame, point_list};
use crate::visual::{Asymptote, CurveFigure, LabeledPoint, VisualError};

const CURVE_SAMPLES: usize = 121;

/// The stroke classes of the labelled curves, in order: solid, dashed, dotted,
/// dash-dot, long dash, and dash-dot-dot. `web/src/styles/app.css` draws them.
/// Each reads without colour.
const STROKES: [&str; 6] = [
    "cadus-visual-curve cadus-visual-curve-s1",
    "cadus-visual-curve cadus-visual-curve-s2",
    "cadus-visual-curve cadus-visual-curve-s3",
    "cadus-visual-curve cadus-visual-curve-s4",
    "cadus-visual-curve cadus-visual-curve-s5",
    "cadus-visual-curve cadus-visual-curve-s6",
];

/// The pixel width of one label character, and the pixel height of one label.
const LABEL_CHAR: f64 = 8.0;
const LABEL_HEIGHT: f64 = 14.0;

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
    if let Some(curve) = &figure.curve {
        for run in curve_runs(curve, x_min, x_max, y_min, y_max) {
            let _ = write!(
                out,
                "<polyline points=\"{}\" class=\"cadus-visual-curve\"/>",
                point_list(&run, &|x, y| frame.at(x, y))
            );
        }
        draw_key_points(&mut out, &figure.key_points, &frame)?;
    }
    let tick = figure.y_tick.to_f64()? / (y_max - y_min) * frame.height;
    let mut placed: Vec<Rect> = Vec::new();
    let mut labels = String::new();
    for (index, item) in figure.curves.iter().enumerate() {
        let mut last = None;
        for run in curve_runs(&item.curve, x_min, x_max, y_min, y_max) {
            let _ = write!(
                out,
                "<polyline points=\"{}\" class=\"{}\"/>",
                point_list(&run, &|x, y| frame.at(x, y)),
                STROKES[index % STROKES.len()]
            );
            last = run.last().copied();
        }
        if let Some((x, y)) = last {
            let spot = frame.at(x, y);
            let width = LABEL_CHAR * item.label.chars().count() as f64;
            let (x, y) = place_label(spot, width, tick, &frame, &placed);
            placed.push(Rect::new(x, y, width));
            text_at(&mut labels, x, y, "end", &item.label);
        }
        draw_key_points(&mut out, &item.key_points, &frame)?;
    }
    out.push_str(&labels);
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
    let mut xs: Vec<f64> = (0..CURVE_SAMPLES)
        .map(|index| step.mul_add(index as f64, x_min))
        .collect();
    // A root curve starts at its domain edge, so the drawn line meets it.
    if let Some(edge) = kind.domain_edge().filter(|e| *e > x_min && *e < x_max) {
        let at = xs.partition_point(|x| *x < edge);
        xs.insert(at, edge);
    }
    for x in xs {
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

/// The dot and the label of each key point.
fn draw_key_points(
    out: &mut String,
    points: &[LabeledPoint],
    frame: &GridFrame,
) -> Result<(), VisualError> {
    for point in points {
        let pair = point.to_pair()?;
        let spot = frame.at(pair.0, pair.1);
        dot_at(out, spot, 4.0, "cadus-visual-point");
        let label = point_label(point);
        if !label.is_empty() {
            text_at(out, spot.0 + 8.0, spot.1 - 8.0, "start", label);
        }
    }
    Ok(())
}

/// The box of one placed label, in pixels. `y` is the text baseline.
#[derive(Clone, Copy)]
struct Rect {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
}

impl Rect {
    /// The box of a label whose end anchor sits at `(x, y)`.
    fn new(x: f64, y: f64, width: f64) -> Self {
        Self {
            left: x - width,
            right: x,
            top: y - LABEL_HEIGHT + 2.0,
            bottom: y + 2.0,
        }
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.left < other.right
            && other.left < self.right
            && self.top < other.bottom
            && other.top < self.bottom
    }
}

/// The text anchor of the label of one curve.
///
/// The rule: the label sits at the last drawn point of its curve, left of the
/// point and above it, and always inside the plane. If its box meets the box
/// of a label placed before, the label moves up by one y tick. If that fails
/// too, it moves down by one tick from the first place, then up by two ticks,
/// then down by two, and so on, up to six ticks. If no place is free, the label
/// keeps the first place. Curves are placed in the order of `curves`, so the
/// result is stable.
fn place_label(
    spot: (f64, f64),
    width: f64,
    tick: f64,
    frame: &GridFrame,
    placed: &[Rect],
) -> (f64, f64) {
    let x = (spot.0 - 4.0).clamp(MARGIN + width, MARGIN + frame.width);
    let low = MARGIN + LABEL_HEIGHT - 2.0;
    let high = MARGIN + frame.height - 2.0;
    let first = (spot.1 - 6.0).clamp(low, high);
    let step = tick.max(LABEL_HEIGHT);
    for shift in 0..=12_u32 {
        let turns = f64::from(shift.div_ceil(2));
        let sign = if shift % 2 == 1 { -1.0 } else { 1.0 };
        let y = (first + sign * turns * step).clamp(low, high);
        let candidate = Rect::new(x, y, width);
        if !placed.iter().any(|other| candidate.overlaps(other)) {
            return (x, y);
        }
    }
    (x, first)
}
