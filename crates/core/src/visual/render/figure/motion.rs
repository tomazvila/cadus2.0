//! The bodies of a figure with motion: the fixed part, then one `<g id>` per
//! moving part, and the law of each group in the slider index.
//!
//! A law is affine in the index `i`. A drive moves a group by `(i - i0) * step`
//! units, where `i0` is the default index, so each entry is `k * i + c` with
//! `c = -k * i0` and the group stands still at the default.
//!
//! The server draws the moving part where the default puts it. The browser sets
//! `transform` and `opacity` from the table. It does no geometry.

use std::collections::BTreeSet;

use num_traits::ToPrimitive;

use super::super::{MARGIN, RenderOptions, dot_at, inner_width, line_at};
use super::fraction::part_templates;
use super::number_line::draw_points as number_line_points;
use super::plane::{GridFrame, draw_points, draw_segments};
use super::{Fit, coordinate_body, fraction_body, number_line_body};
use crate::visual::motion::{GroupLaw, Law, Motion, Steps, affine, coincident_points, round4};
use crate::visual::{
    CoordinateFigure, Drive, FractionFigure, GeometryShape, LabeledPoint, MotionError,
    NumberLineFigure, Scalar, VisualError, VisualSpec,
};

/// The groups of one figure, in order.
struct Groups<'a> {
    prefix: &'a str,
    steps: &'a Steps,
    body: String,
    laws: Vec<GroupLaw>,
}

impl<'a> Groups<'a> {
    fn new(prefix: &'a str, steps: &'a Steps) -> Self {
        Self {
            prefix,
            steps,
            body: String::new(),
            laws: Vec::new(),
        }
    }

    /// The step of the parameter in figure units, and the default index.
    fn unit(&self) -> Result<(f64, f64), VisualError> {
        let step = self.steps.step.to_f64().ok_or_else(|| {
            VisualError::Motion(MotionError::ParameterStep {
                reason: "the step is not a number".to_owned(),
            })
        })?;
        Ok((step, self.steps.default_index as f64))
    }

    fn open(&mut self, extra: &str) -> String {
        let id = format!("{}-m{}", self.prefix, self.laws.len());
        let class = if extra.is_empty() {
            "cadus-visual-move".to_owned()
        } else {
            format!("cadus-visual-move {extra}")
        };
        self.body
            .push_str(&format!("<g id=\"{id}\" class=\"{class}\">"));
        id
    }

    fn add(&mut self, inner: &str, law: Law) {
        let id = self.open("");
        self.body.push_str(inner);
        self.body.push_str("</g>");
        self.laws.push(GroupLaw { id, law });
    }

    /// A move of `ke` px in x and `kf` px in y per unit of the parameter.
    fn translate(&mut self, inner: &str, ke: f64, kf: f64) -> Result<(), VisualError> {
        let (step, i0) = self.unit()?;
        let (ke, kf) = (ke * step, kf * step);
        self.add(
            inner,
            Law::Matrix {
                m: [
                    [0.0, 1.0],
                    [0.0, 0.0],
                    [0.0, 0.0],
                    [0.0, 1.0],
                    affine(ke, -ke * i0),
                    affine(kf, -kf * i0),
                ],
            },
        );
        Ok(())
    }
}

/// The fixed body and the moving groups of a figure with motion.
pub(in crate::visual::render) fn motion_body(
    spec: &VisualSpec,
    options: &RenderOptions,
    motion: &Motion,
    steps: &Steps,
) -> Result<(String, Vec<GroupLaw>), VisualError> {
    let mut groups = Groups::new(&options.id_prefix, steps);
    let fixed = match spec {
        VisualSpec::Coordinate(figure) => coordinate(figure, options, motion, &mut groups)?,
        VisualSpec::NumberLine(figure) => number_line(figure, options, motion, &mut groups)?,
        VisualSpec::Fraction(figure) => fraction(figure, options, &mut groups),
        VisualSpec::Geometry(figure) => match &figure.figure {
            GeometryShape::Angle {
                vertex,
                initial_degrees,
                terminal_degrees,
                ray_length,
                ..
            } => angle(
                options,
                (vertex, ray_length),
                (initial_degrees, terminal_degrees),
                &mut groups,
            )?,
            _ => return Err(not_fit()),
        },
        _ => return Err(not_fit()),
    };
    Ok((format!("{fixed}{}", groups.body), groups.laws))
}

/// A number line: each moving point slides with its label.
fn number_line(
    figure: &NumberLineFigure,
    options: &RenderOptions,
    motion: &Motion,
    groups: &mut Groups<'_>,
) -> Result<String, VisualError> {
    let mut base = figure.clone();
    base.motion = None;
    let (min, max) = (base.min.to_f64()?, base.max.to_f64()?);
    let width = inner_width(options);
    let baseline = options.height as f64 / 2.0;
    let at = |value: f64| MARGIN + (value - min) / (max - min) * width;
    let moving: BTreeSet<usize> = motion
        .drives
        .iter()
        .filter_map(|drive| match drive {
            Drive::Point { index } => Some(*index),
            _ => None,
        })
        .collect();
    for index in &moving {
        let mut inner = String::new();
        number_line_points(&mut inner, &[base.points[*index].clone()], baseline, &at)?;
        groups.translate(&inner, width / (max - min), 0.0)?;
    }
    base.points = keep(&base.points, &moving);
    number_line_body(&base, options)
}

/// A geometry angle: the terminal ray turns about the vertex.
fn angle(
    options: &RenderOptions,
    (vertex, ray_length): (&LabeledPoint, &Scalar),
    (initial, terminal): (&Scalar, &Scalar),
    groups: &mut Groups<'_>,
) -> Result<String, VisualError> {
    let (vx, vy) = vertex.to_pair()?;
    let length = ray_length.to_f64()?;
    let fit = Fit::new(
        (vx - length, vx + length, vy - length, vy + length),
        options,
    );
    let corner = fit.at(vx, vy);
    let end = |degrees: f64| {
        let turn = degrees.to_radians();
        fit.at(
            length.mul_add(turn.cos(), vx),
            length.mul_add(turn.sin(), vy),
        )
    };
    let mut fixed = String::new();
    line_at(
        &mut fixed,
        corner,
        end(initial.to_f64()?),
        "cadus-visual-segment",
    );
    dot_at(&mut fixed, corner, 3.0, "cadus-visual-point");
    let mut inner = String::new();
    line_at(
        &mut inner,
        corner,
        end(terminal.to_f64()?),
        "cadus-visual-segment",
    );
    let (step, i0) = groups.unit()?;
    groups.add(
        &inner,
        Law::Rotate {
            deg: affine(-step, step * i0),
            cx: round4(corner.0),
            cy: round4(corner.1),
        },
    );
    Ok(fixed)
}

fn not_fit() -> VisualError {
    MotionError::DriveMismatch {
        reason: "no drive fits this figure".to_owned(),
    }
    .into()
}

/// The coordinate plane: lines turn and slide, points slide.
fn coordinate(
    figure: &CoordinateFigure,
    options: &RenderOptions,
    motion: &Motion,
    groups: &mut Groups<'_>,
) -> Result<String, VisualError> {
    let mut base = figure.clone();
    base.motion = None;
    let frame = GridFrame::new(&base, options)?;
    let (x_min, x_max, y_min, y_max) = frame.bounds;
    let mut plane = Plane {
        scale: (
            frame.width / (x_max - x_min),
            frame.height / (y_max - y_min),
        ),
        frame,
        gone_segments: BTreeSet::new(),
        gone_points: BTreeSet::new(),
        base: &base,
    };
    for drive in &motion.drives {
        match drive {
            Drive::LineSlope { segment } => plane.slope(groups, *segment)?,
            Drive::LineIntercept { segment } => plane.intercept(groups, *segment)?,
            Drive::PointX { point } => plane.point(groups, *point, true)?,
            Drive::PointY { point } => plane.point(groups, *point, false)?,
            _ => return Err(not_fit()),
        }
    }
    let (gone_segments, gone_points) = (plane.gone_segments, plane.gone_points);
    base.segments = keep(&base.segments, &gone_segments);
    base.points = keep(&base.points, &gone_points);
    coordinate_body(&base, options)
}

/// The moving parts of a plane, and the pixels per unit of each axis.
struct Plane<'a> {
    base: &'a CoordinateFigure,
    frame: GridFrame,
    scale: (f64, f64),
    gone_segments: BTreeSet<usize>,
    gone_points: BTreeSet<usize>,
}

impl Plane<'_> {
    fn dots(&self, tied: &[usize]) -> Vec<LabeledPoint> {
        tied.iter()
            .map(|index| self.base.points[*index].clone())
            .collect()
    }

    /// A shear about the `from` end: `b = -step * sy / sx` per index, and `f = -b * X0`,
    /// so the pivot stands still. The dots on the `to` end slide in y.
    fn slope(&mut self, groups: &mut Groups<'_>, segment: usize) -> Result<(), VisualError> {
        let at = |x: f64, y: f64| self.frame.at(x, y);
        let (sx, sy) = self.scale;
        let (step, i0) = groups.unit()?;
        let seg = self.base.segments[segment].clone();
        let (from, to) = (seg.from.to_pair()?, seg.to.to_pair()?);
        let start = at(from.0, from.1);
        let mut inner = String::new();
        draw_segments(&mut inner, std::slice::from_ref(&seg), &at)?;
        let kb = -step * sy / sx;
        groups.add(
            &inner,
            Law::Matrix {
                m: [
                    [0.0, 1.0],
                    affine(kb, -kb * i0),
                    [0.0, 0.0],
                    [0.0, 1.0],
                    [0.0, 0.0],
                    affine(-kb * start.0, kb * i0 * start.0),
                ],
            },
        );
        let tied = coincident_points(self.base, &seg.to)?;
        if !tied.is_empty() {
            let mut dots = String::new();
            draw_points(&mut dots, &self.dots(&tied), &at)?;
            groups.translate(&dots, 0.0, -(to.0 - from.0) * sy)?;
        }
        self.gone_segments.insert(segment);
        self.gone_points.extend(tied);
        Ok(())
    }

    /// The whole segment, with the dots on its ends, slides in y.
    fn intercept(&mut self, groups: &mut Groups<'_>, segment: usize) -> Result<(), VisualError> {
        let at = |x: f64, y: f64| self.frame.at(x, y);
        let seg = self.base.segments[segment].clone();
        let mut inner = String::new();
        draw_segments(&mut inner, std::slice::from_ref(&seg), &at)?;
        let mut tied = coincident_points(self.base, &seg.from)?;
        tied.extend(coincident_points(self.base, &seg.to)?);
        draw_points(&mut inner, &self.dots(&tied), &at)?;
        groups.translate(&inner, 0.0, -self.scale.1)?;
        self.gone_segments.insert(segment);
        self.gone_points.extend(tied);
        Ok(())
    }

    /// One point and its label slide in x or in y.
    fn point(
        &mut self,
        groups: &mut Groups<'_>,
        point: usize,
        along_x: bool,
    ) -> Result<(), VisualError> {
        let at = |x: f64, y: f64| self.frame.at(x, y);
        let mut inner = String::new();
        draw_points(&mut inner, &self.dots(&[point]), &at)?;
        if along_x {
            groups.translate(&inner, self.scale.0, 0.0)?;
        } else {
            groups.translate(&inner, 0.0, -self.scale.1)?;
        }
        self.gone_points.insert(point);
        Ok(())
    }
}

fn keep<T: Clone>(all: &[T], gone: &BTreeSet<usize>) -> Vec<T> {
    all.iter()
        .enumerate()
        .filter(|(index, _)| !gone.contains(index))
        .map(|(_, item)| item.clone())
        .collect()
}

/// A fraction bar or circle: one overlay group per part.
fn fraction(figure: &FractionFigure, options: &RenderOptions, groups: &mut Groups<'_>) -> String {
    let mut base = figure.clone();
    base.motion = None;
    let shown = base.shaded;
    base.shaded = 0;
    let fixed = fraction_body(&base, options);
    for (index, template) in part_templates(&base, options).iter().enumerate() {
        let state = if (index as i64) < shown {
            "cadus-visual-on"
        } else {
            "cadus-visual-off"
        };
        let id = groups.open(state);
        groups
            .body
            .push_str(&template.replace("@CLASS@", "cadus-visual-part-shaded"));
        groups.body.push_str("</g>");
        groups.laws.push(GroupLaw {
            id,
            law: Law::Reveal {
                from: u32::try_from(index + 1).unwrap_or(u32::MAX),
            },
        });
    }
    fixed
}
