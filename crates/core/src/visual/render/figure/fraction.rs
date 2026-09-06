//! Fraction SVG body.

use std::fmt::Write as _;

use super::super::{MARGIN, RenderOptions, inner_height, inner_width, px};
use crate::visual::{FractionFigure, FractionShape};

pub(in crate::visual::render) fn fraction_body(
    figure: &FractionFigure,
    options: &RenderOptions,
) -> String {
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
