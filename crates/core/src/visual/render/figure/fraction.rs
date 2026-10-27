//! Fraction SVG body.

use std::fmt::Write as _;

use super::super::{MARGIN, RenderOptions, inner_height, inner_width, px};
use crate::visual::{FractionFigure, FractionShape};

/// The size of one whole and the gap between two wholes.
struct Layout {
    wholes: i64,
    each: f64,
    height: f64,
}

impl Layout {
    fn of(figure: &FractionFigure, options: &RenderOptions) -> Self {
        let wholes = figure.wholes().max(1);
        let gap = 14.0;
        let each = ((inner_width(options) - gap * (wholes - 1) as f64) / wholes as f64).max(1.0);
        Self {
            wholes,
            each,
            height: inner_height(options),
        }
    }

    fn origin(&self, whole: i64) -> f64 {
        MARGIN + (self.each + 14.0) * whole as f64
    }
}

pub(in crate::visual::render) fn fraction_body(
    figure: &FractionFigure,
    options: &RenderOptions,
) -> String {
    let layout = Layout::of(figure, options);
    let mut out = String::new();
    let mut left = figure.shaded;
    for whole in 0..layout.wholes {
        let shaded = left.clamp(0, figure.parts);
        left -= shaded;
        for index in 0..figure.parts {
            let class = if index < shaded {
                "cadus-visual-part-shaded"
            } else {
                "cadus-visual-part"
            };
            out.push_str(&part(figure, &layout, whole, index, class));
        }
    }
    out
}

/// The drawn elements of every part, in order, with the marker `@CLASS@` where
/// the class goes. A figure with motion draws the shaded copy of each part as a
/// group of its own, so the browser fades the group in or out.
pub(super) fn part_templates(figure: &FractionFigure, options: &RenderOptions) -> Vec<String> {
    let layout = Layout::of(figure, options);
    (0..layout.wholes)
        .flat_map(|whole| (0..figure.parts).map(move |index| (whole, index)))
        .map(|(whole, index)| part(figure, &layout, whole, index, "@CLASS@"))
        .collect()
}

/// One part of one whole: a bar segment or a circle sector.
fn part(figure: &FractionFigure, layout: &Layout, whole: i64, index: i64, class: &str) -> String {
    let origin = layout.origin(whole);
    let mut out = String::new();
    match figure.shape {
        FractionShape::Bar => {
            let each = layout.each / figure.parts as f64;
            let _ = write!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" class=\"{class}\"/>",
                px(each.mul_add(index as f64, origin)),
                px(MARGIN),
                px(each),
                px(layout.height.min(90.0))
            );
        }
        FractionShape::Circle => {
            let radius = (layout.each / 2.0).min(layout.height / 2.0).max(1.0);
            let center = (origin + layout.each / 2.0, MARGIN + radius);
            let sweep = 360.0 / figure.parts as f64;
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
    out
}
