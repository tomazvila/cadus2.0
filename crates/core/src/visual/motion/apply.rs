//! What each drive does to a figure: the value it draws now, the figure at
//! another value, and the readout text.
//!
//! Every function works on exact rationals. The static figure at a value is
//! an ordinary [`VisualSpec`] with no `motion`, so the existing checks and the
//! existing render apply to it without a change.

use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use super::super::plane::exact_text;
use super::super::{
    CoordinateFigure, GeometryShape, LabeledPoint, Scalar, Segment, VisualError, VisualSpec,
};
use super::{Drive, Motion, MotionError, Parameter, Readout, Steps};

fn mismatch(reason: impl Into<String>) -> VisualError {
    MotionError::DriveMismatch {
        reason: reason.into(),
    }
    .into()
}

fn scalar(value: &BigRational) -> Scalar {
    Scalar::from(exact_text(value).as_str())
}

fn blank(label: Option<&str>) -> bool {
    label.map(str::trim).is_none_or(str::is_empty)
}

fn same(a: &LabeledPoint, b: &LabeledPoint) -> Result<bool, VisualError> {
    Ok(a.value()? == b.value()?)
}

/// The indexes of the plotted points that sit exactly on `end`.
pub(in crate::visual) fn coincident_points(
    figure: &CoordinateFigure,
    end: &LabeledPoint,
) -> Result<Vec<usize>, VisualError> {
    let mut found = Vec::new();
    for (index, point) in figure.points.iter().enumerate() {
        if same(point, end)? {
            found.push(index);
        }
    }
    Ok(found)
}

fn segment_of(figure: &CoordinateFigure, at: usize) -> Result<&Segment, VisualError> {
    figure
        .segments
        .get(at)
        .ok_or_else(|| mismatch(format!("the figure has no segment {at}")))
}

/// The slope of a segment, or the reason it has none.
fn slope_of(segment: &Segment) -> Result<BigRational, VisualError> {
    let ((x0, y0), (x1, y1)) = (segment.from.value()?, segment.to.value()?);
    if x1 == x0 {
        return Err(mismatch("a vertical segment has no slope"));
    }
    Ok((y1 - y0) / (x1 - x0))
}

/// No other segment may touch an end that moves.
fn free_of_others(
    figure: &CoordinateFigure,
    own: usize,
    ends: &[&LabeledPoint],
) -> Result<(), VisualError> {
    for (index, segment) in figure.segments.iter().enumerate() {
        if index == own {
            continue;
        }
        for end in ends {
            if same(&segment.from, end)? || same(&segment.to, end)? {
                return Err(mismatch(format!(
                    "segment {index} shares an end that moves, so it would stay behind"
                )));
            }
        }
    }
    Ok(())
}

/// The value the bare figure draws now for one drive, after the structure checks.
fn drawn(bare: &VisualSpec, drive: &Drive) -> Result<BigRational, VisualError> {
    match (bare, drive) {
        (VisualSpec::Coordinate(figure), drive) => coordinate_drawn(figure, drive),
        (VisualSpec::NumberLine(figure), Drive::Point { index }) => {
            let point = figure
                .points
                .get(*index)
                .ok_or_else(|| mismatch(format!("the figure has no point {index}")))?;
            if blank(point.label.as_deref()) {
                return Err(mismatch(
                    "a moving point needs a label, because an unlabeled point prints its value",
                ));
            }
            point.at.value()
        }
        (VisualSpec::Fraction(figure), Drive::Shaded) => {
            if figure.shaded > figure.parts {
                return Err(mismatch("a shaded count above one whole cannot move"));
            }
            Ok(BigRational::from_integer(figure.shaded.into()))
        }
        (VisualSpec::Geometry(figure), Drive::Angle) => match &figure.figure {
            GeometryShape::Angle {
                terminal_degrees,
                label,
                ..
            } if blank(label.as_deref()) => terminal_degrees.value(),
            GeometryShape::Angle { .. } => Err(mismatch("a turning angle carries no label")),
            _ => Err(mismatch("the angle drive needs an angle diagram")),
        },
        _ => Err(mismatch("the drive does not fit the figure")),
    }
}

/// The value a coordinate drive reads from the figure.
fn coordinate_drawn(figure: &CoordinateFigure, drive: &Drive) -> Result<BigRational, VisualError> {
    match drive {
        Drive::LineSlope { segment } => {
            let seg = segment_of(figure, *segment)?;
            if !blank(seg.label.as_deref()) {
                return Err(mismatch("a segment that turns carries no label"));
            }
            free_of_others(figure, *segment, &[&seg.to])?;
            slope_of(seg)
        }
        Drive::LineIntercept { segment } => {
            let seg = segment_of(figure, *segment)?;
            free_of_others(figure, *segment, &[&seg.from, &seg.to])?;
            let (x0, y0) = seg.from.value()?;
            Ok(y0 - slope_of(seg)? * x0)
        }
        Drive::PointX { point } | Drive::PointY { point } => {
            let at = figure
                .points
                .get(*point)
                .ok_or_else(|| mismatch(format!("the figure has no point {point}")))?;
            for segment in &figure.segments {
                if same(&segment.from, at)? || same(&segment.to, at)? {
                    return Err(mismatch("a segment ends at the moving point"));
                }
            }
            let (x, y) = at.value()?;
            Ok(if matches!(drive, Drive::PointX { .. }) {
                x
            } else {
                y
            })
        }
        _ => Err(mismatch("the drive does not fit the figure")),
    }
}

/// The default of the parameter must equal what the figure draws.
pub(super) fn check_default(
    bare: &VisualSpec,
    drive: &Drive,
    parameter: &Parameter,
    steps: &Steps,
) -> Result<(), VisualError> {
    let now = drawn(bare, drive)?;
    if now != steps.value(steps.default_index) {
        return Err(mismatch(format!(
            "the figure draws {} and the parameter default is {}",
            exact_text(&now),
            parameter.default
        )));
    }
    if matches!(drive, Drive::Shaded) {
        let VisualSpec::Fraction(figure) = bare else {
            return Err(mismatch("the shaded drive needs a fraction figure"));
        };
        let whole = BigRational::from_integer(figure.parts.into());
        let fits = steps.min.is_zero()
            && steps.step.is_one()
            && steps.value(steps.n) == whole
            && figure.parts <= super::MAX_GROUPS as i64;
        if !fits {
            return Err(MotionError::ParameterRange {
                reason: "a shaded slider runs from 0 to the part count in steps of 1, with \
                         at most 12 parts"
                    .to_owned(),
            }
            .into());
        }
    }
    Ok(())
}

/// Move the figure to one value, for one drive.
fn apply(spec: &mut VisualSpec, drive: &Drive, value: &BigRational) -> Result<(), VisualError> {
    match (spec, drive) {
        (VisualSpec::Coordinate(figure), drive) => apply_coordinate(figure, drive, value)?,
        (VisualSpec::NumberLine(figure), Drive::Point { index }) => {
            figure.points[*index].at = scalar(value);
        }
        (VisualSpec::Fraction(figure), Drive::Shaded) => {
            figure.shaded = value.numer().try_into().unwrap_or(0);
        }
        (VisualSpec::Geometry(figure), Drive::Angle) => {
            if let GeometryShape::Angle {
                terminal_degrees, ..
            } = &mut figure.figure
            {
                *terminal_degrees = scalar(value);
            }
        }
        _ => return Err(mismatch("the drive does not fit the figure")),
    }
    Ok(())
}

fn apply_coordinate(
    figure: &mut CoordinateFigure,
    drive: &Drive,
    value: &BigRational,
) -> Result<(), VisualError> {
    match drive {
        Drive::LineSlope { segment } => {
            let (from, to) = ends_of(figure, *segment)?;
            let (x0, y0) = from.value()?;
            let (x1, _) = to.value()?;
            let moved = scalar(&(y0 + value.clone() * (x1 - x0)));
            for index in coincident_points(figure, &to)? {
                figure.points[index].y = moved.clone();
            }
            figure.segments[*segment].to.y = moved;
        }
        Drive::LineIntercept { segment } => {
            let (from, to) = ends_of(figure, *segment)?;
            let now = drawn_intercept(figure, *segment)?;
            let shift = value.clone() - now;
            for end in [&from, &to] {
                for index in coincident_points(figure, end)? {
                    figure.points[index].y = scalar(&(figure.points[index].y.value()? + &shift));
                }
            }
            let seg = &mut figure.segments[*segment];
            seg.from.y = scalar(&(from.y.value()? + &shift));
            seg.to.y = scalar(&(to.y.value()? + shift));
        }
        Drive::PointX { point } => figure.points[*point].x = scalar(value),
        Drive::PointY { point } => figure.points[*point].y = scalar(value),
        _ => return Err(mismatch("the drive does not fit the figure")),
    }
    Ok(())
}

fn ends_of(
    figure: &CoordinateFigure,
    segment: usize,
) -> Result<(LabeledPoint, LabeledPoint), VisualError> {
    let seg = segment_of(figure, segment)?;
    Ok((seg.from.clone(), seg.to.clone()))
}

fn drawn_intercept(figure: &CoordinateFigure, segment: usize) -> Result<BigRational, VisualError> {
    coordinate_drawn(figure, &Drive::LineIntercept { segment })
}

/// The static figure at one slider index.
pub(super) fn at_index(
    bare: &VisualSpec,
    motion: &Motion,
    steps: &Steps,
    index: usize,
) -> Result<VisualSpec, VisualError> {
    let value = steps.value(index);
    let mut spec = bare.clone();
    for drive in &motion.drives {
        apply(&mut spec, drive, &value)?;
    }
    Ok(spec)
}

/// The static figure at every slider index.
pub(in crate::visual) fn step_specs(
    spec: &VisualSpec,
    motion: &Motion,
    steps: &Steps,
) -> Result<Vec<VisualSpec>, VisualError> {
    let bare = spec.without_motion();
    (0..=steps.n)
        .map(|index| at_index(&bare, motion, steps, index))
        .collect()
}

/// How many groups the drives make.
pub(in crate::visual) fn group_count(
    bare: &VisualSpec,
    motion: &Motion,
) -> Result<usize, VisualError> {
    let mut total = 0;
    for drive in &motion.drives {
        total += match (bare, drive) {
            (VisualSpec::Coordinate(figure), Drive::LineSlope { segment }) => {
                let to = &segment_of(figure, *segment)?.to;
                1 + usize::from(!coincident_points(figure, to)?.is_empty())
            }
            (VisualSpec::Fraction(figure), Drive::Shaded) => {
                usize::try_from(figure.parts).unwrap_or(usize::MAX)
            }
            _ => 1,
        };
    }
    Ok(total)
}

/// The text of a line `y = m x + b`.
fn line_text(slope: &BigRational, intercept: &BigRational) -> String {
    let minus_one = -BigRational::one();
    let term = if slope.is_zero() {
        String::new()
    } else if slope.is_one() {
        "x".to_owned()
    } else if *slope == minus_one {
        "-x".to_owned()
    } else if slope.denom().is_one() {
        format!("{}x", slope.numer())
    } else {
        format!("{} x", exact_text(slope))
    };
    if term.is_empty() {
        return format!("y = {}", exact_text(intercept));
    }
    if intercept.is_zero() {
        return format!("y = {term}");
    }
    let sign = if intercept.is_negative() { "-" } else { "+" };
    format!("y = {term} {sign} {}", exact_text(&intercept.abs()))
}

/// The readout lines: one per drive, with the exact text at every index.
pub(in crate::visual) fn readouts(
    motion: &Motion,
    shown: &[VisualSpec],
) -> Result<Vec<Readout>, VisualError> {
    let mut out = Vec::new();
    for drive in &motion.drives {
        let mut texts = Vec::with_capacity(shown.len());
        for spec in shown {
            texts.push(readout_text(spec, drive)?);
        }
        let label = match drive {
            Drive::LineSlope { .. } | Drive::LineIntercept { .. } => "Line",
            Drive::PointX { .. } | Drive::PointY { .. } | Drive::Point { .. } => "Point",
            Drive::Shaded => "Shaded",
            Drive::Angle => "Angle",
        };
        out.push(Readout {
            label: label.to_owned(),
            texts,
        });
    }
    Ok(out)
}

fn readout_text(spec: &VisualSpec, drive: &Drive) -> Result<String, VisualError> {
    Ok(match (spec, drive) {
        (VisualSpec::Coordinate(figure), drive) => coordinate_readout(figure, drive)?,
        (VisualSpec::NumberLine(figure), Drive::Point { index }) => figure
            .points
            .get(*index)
            .ok_or_else(|| mismatch("no such point"))?
            .at
            .value()
            .map(|value| exact_text(&value))?,
        (VisualSpec::Fraction(figure), Drive::Shaded) => {
            format!("{} of {} equal parts", figure.shaded, figure.parts)
        }
        (VisualSpec::Geometry(figure), Drive::Angle) => match &figure.figure {
            GeometryShape::Angle {
                terminal_degrees, ..
            } => format!("{}°", exact_text(&terminal_degrees.value()?)),
            _ => return Err(mismatch("the angle drive needs an angle diagram")),
        },
        _ => return Err(mismatch("the drive does not fit the figure")),
    })
}

fn coordinate_readout(figure: &CoordinateFigure, drive: &Drive) -> Result<String, VisualError> {
    match drive {
        Drive::LineSlope { segment } | Drive::LineIntercept { segment } => {
            let seg = segment_of(figure, *segment)?;
            let slope = slope_of(seg)?;
            let (x0, y0) = seg.from.value()?;
            Ok(line_text(&slope, &(y0 - slope.clone() * x0)))
        }
        Drive::PointX { point } | Drive::PointY { point } => {
            let (x, y) = figure
                .points
                .get(*point)
                .ok_or_else(|| mismatch("no such point"))?
                .value()?;
            Ok(format!("({}, {})", exact_text(&x), exact_text(&y)))
        }
        _ => Err(mismatch("the drive does not fit the figure")),
    }
}
