//! Special-triangle SVG body.

use super::super::{RenderOptions, px, text_at};
use super::Fit;
use super::geometry::{bounds_of, right_angle_mark, vertex_angle_arc};
use crate::visual::plane::exact_text;
use crate::visual::special_triangle::radical_text;
use crate::visual::{SpecialTriangleFigure, SpecialTriangleShape, VisualError};

pub(in crate::visual::render) fn special_triangle_body(
    figure: &SpecialTriangleFigure,
    options: &RenderOptions,
) -> Result<String, VisualError> {
    match &figure.figure {
        SpecialTriangleShape::FortyFiveFortyFiveNinety { leg } => {
            let length = leg.to_f64()?;
            let value = leg.value()?;
            let vertices = [(0.0, 0.0), (length, 0.0), (0.0, length)];
            let (spots, mut out) = right_triangle(&vertices, options);
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
            let (spots, mut out) = right_triangle(&vertices, options);
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

/// A fitted right triangle and its outline plus right-angle mark.
fn right_triangle(
    vertices: &[(f64, f64); 3],
    options: &RenderOptions,
) -> (Vec<(f64, f64)>, String) {
    let fit = Fit::new(bounds_of(vertices), options);
    let spots: Vec<(f64, f64)> = vertices.iter().map(|(x, y)| fit.at(*x, *y)).collect();
    let mut out = triangle_outline(&spots);
    if let Some(mark) = right_angle_mark(&spots, 0) {
        out.push_str(&mark);
    }
    (spots, out)
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
