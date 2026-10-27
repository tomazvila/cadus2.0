//! A figure with one learner-set parameter (D-FA1): the law of every group at
//! every slider index, the refusals, and the bytes of a still figure.
//!
//! The expected figure at each index comes from an exact `BigRational`
//! computation in this file. It never calls the production law code.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use cadus_core::visual::{
    Law, MAX_TABLE_BYTES, MotionTable, RenderOptions, VisualError, VisualSpec, render, render_all,
    render_with_motion,
};
use num_bigint::BigInt;
use num_rational::BigRational;
use serde_json::{Value, json};

fn spec(value: Value) -> VisualSpec {
    serde_json::from_value(value).expect("a visual spec")
}

fn rat(num: i64, den: i64) -> BigRational {
    BigRational::new(BigInt::from(num), BigInt::from(den))
}

fn text(value: &BigRational) -> String {
    if value.denom() == &BigInt::from(1) {
        value.numer().to_string()
    } else {
        format!("{}/{}", value.numer(), value.denom())
    }
}

/// All attribute pairs of every tag `<name ... class="class" .../>`.
fn tags(svg: &str, name: &str, class: &str) -> Vec<Vec<(String, String)>> {
    let mut out = Vec::new();
    let open = format!("<{name} ");
    let mut rest = svg;
    while let Some(at) = rest.find(&open) {
        let tail = &rest[at..];
        let end = tail.find('>').expect("tag end");
        let tag = &tail[..end];
        if tag.contains(&format!("class=\"{class}\"")) {
            let mut attrs = Vec::new();
            let mut cursor = &tag[open.len()..];
            while let Some(eq) = cursor.find("=\"") {
                let key = cursor[..eq].trim().to_owned();
                let after = &cursor[eq + 2..];
                let close = after.find('"').expect("quote");
                attrs.push((key, after[..close].to_owned()));
                cursor = &after[close + 1..];
            }
            out.push(attrs);
        }
        rest = &tail[end..];
    }
    out
}

fn num(tag: &[(String, String)], key: &str) -> f64 {
    tag.iter()
        .find(|(name, _)| name == key)
        .and_then(|(_, value)| value.parse().ok())
        .expect("a number attribute")
}

/// The `matrix` of a law at one index, as the six CSS entries.
fn matrix_at(law: &Law, index: usize) -> [f64; 6] {
    let m = match law {
        Law::Matrix { m } => Some(m),
        Law::Rotate { .. } | Law::Reveal { .. } => None,
    }
    .expect("a matrix law");
    let mut out = [0.0; 6];
    for (slot, pair) in out.iter_mut().zip(m) {
        *slot = pair[0] * index as f64 + pair[1];
    }
    out
}

fn rotate(law: &Law) -> ([f64; 2], f64, f64) {
    match law {
        Law::Rotate { deg, cx, cy } => Some((*deg, *cx, *cy)),
        Law::Matrix { .. } | Law::Reveal { .. } => None,
    }
    .expect("a rotate law")
}

fn apply(m: [f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 0.02, "{a} differs from {b}");
}

fn table_of(figure: &VisualSpec) -> (String, MotionTable) {
    let (svg, table) = render_with_motion(figure, &RenderOptions::default()).expect("render");
    (svg, table.expect("a motion table"))
}

fn plane_json(slope: &BigRational, motion: bool) -> Value {
    let y = slope * rat(4, 1);
    let mut figure = json!({
        "kind": "coordinate", "x_min": "0", "x_max": "8", "y_min": "-4", "y_max": "8",
        "x_tick": "1", "y_tick": "1",
        "points": [{"x": "4", "y": text(&y), "label": "T"}],
        "segments": [{"from": {"x": "0", "y": "0"}, "to": {"x": "4", "y": text(&y)}}],
    });
    if motion {
        figure["motion"] = json!({
            "parameter": {"name": "slope", "min": "-1", "max": "2", "default": "1/2", "step": "1/2"},
            "drives": [{"kind": "line_slope", "segment": 0}],
        });
    }
    figure
}

#[test]
fn a_slope_law_matches_the_still_figure_at_every_index() {
    let model = spec(plane_json(&rat(1, 2), true));
    let (svg, table) = table_of(&model);
    assert_eq!(table.parameter.n, 6);
    assert_eq!(table.parameter.default_index, 3);
    assert_eq!(table.parameter.labels.len(), 7);
    assert_eq!(table.parameter.labels[0], "-1");
    assert_eq!(table.parameter.labels[3], "1/2");
    assert_eq!(table.groups.len(), 2);
    let end = tags(&svg, "line", "cadus-visual-segment")[0].clone();
    let (x2, y2) = (num(&end, "x2"), num(&end, "y2"));
    let dot = tags(&svg, "circle", "cadus-visual-point")[0].clone();
    let (cx, cy) = (num(&dot, "cx"), num(&dot, "cy"));
    for index in 0..=table.parameter.n {
        let slope = rat(-2, 2) + rat(1, 2) * BigRational::from_integer(BigInt::from(index));
        let still = render(&spec(plane_json(&slope, false)), &RenderOptions::default()).unwrap();
        let wanted = tags(&still, "line", "cadus-visual-segment")[0].clone();
        let got = apply(matrix_at(&table.groups[0].law, index), x2, y2);
        close(got.0, num(&wanted, "x2"));
        close(got.1, num(&wanted, "y2"));
        let wanted_dot = tags(&still, "circle", "cadus-visual-point")[0].clone();
        let got_dot = apply(matrix_at(&table.groups[1].law, index), cx, cy);
        close(got_dot.0, num(&wanted_dot, "cx"));
        close(got_dot.1, num(&wanted_dot, "cy"));
        let expected = match index {
            0 => "y = -x",
            1 => "y = -1/2 x",
            2 => "y = 0",
            3 => "y = 1/2 x",
            4 => "y = x",
            5 => "y = 3/2 x",
            _ => "y = 2x",
        };
        assert_eq!(table.readouts[0].texts[index], expected);
        assert!(table.summary[index].contains("segment"));
    }
}

#[test]
fn the_law_at_the_default_is_the_identity() {
    let (_, table) = table_of(&spec(plane_json(&rat(1, 2), true)));
    for group in &table.groups {
        let m = matrix_at(&group.law, table.parameter.default_index);
        for (got, want) in m.iter().zip([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]) {
            close(*got, want);
        }
    }
}

#[test]
fn an_intercept_and_a_point_slide_exactly() {
    let base = |intercept: i64, point_x: i64, point_y: i64, motion: Option<Value>| {
        let mut figure = json!({
            "kind": "coordinate", "x_min": "-5", "x_max": "5", "y_min": "-5", "y_max": "5",
            "x_tick": "1", "y_tick": "1",
            "points": [{"x": point_x.to_string(), "y": point_y.to_string(), "label": "P"}],
            "segments": [{"from": {"x": "-2", "y": (intercept - 2).to_string()},
                          "to": {"x": "2", "y": (intercept + 2).to_string()}}],
        });
        if let Some(motion) = motion {
            figure["motion"] = motion;
        }
        spec(figure)
    };
    let intercept_motion = json!({
        "parameter": {"name": "intercept", "min": "-2", "max": "2", "default": "0", "step": "1"},
        "drives": [{"kind": "line_intercept", "segment": 0}],
    });
    let (svg, table) = table_of(&base(0, 4, 4, Some(intercept_motion)));
    let line = tags(&svg, "line", "cadus-visual-segment")[0].clone();
    for index in 0..=4 {
        let still = render(
            &base(index as i64 - 2, 4, 4, None),
            &RenderOptions::default(),
        )
        .unwrap();
        let wanted = tags(&still, "line", "cadus-visual-segment")[0].clone();
        let got = apply(
            matrix_at(&table.groups[0].law, index),
            num(&line, "x1"),
            num(&line, "y1"),
        );
        close(got.0, num(&wanted, "x1"));
        close(got.1, num(&wanted, "y1"));
        assert!(!table.readouts[0].texts[index].is_empty());
    }
    let x_motion = json!({
        "parameter": {"name": "x", "min": "-4", "max": "4", "default": "0", "step": "2"},
        "drives": [{"kind": "point_x", "point": 0}],
    });
    let (svg, table) = table_of(&base(0, 0, 3, Some(x_motion)).without_segments());
    for index in 0..=4 {
        let still = render(
            &base(0, index as i64 * 2 - 4, 3, None).without_segments(),
            &RenderOptions::default(),
        )
        .unwrap();
        dot_slides(&table.groups[0].law, index, &svg, &still);
    }
}

/// The dot of `svg`, moved by `law` at `index`, must sit where the still figure draws it.
fn dot_slides(law: &Law, index: usize, svg: &str, still: &str) {
    let dot = tags(svg, "circle", "cadus-visual-point")[0].clone();
    let wanted = tags(still, "circle", "cadus-visual-point")[0].clone();
    let got = apply(matrix_at(law, index), num(&dot, "cx"), num(&dot, "cy"));
    close(got.0, num(&wanted, "cx"));
    close(got.1, num(&wanted, "cy"));
}

trait WithoutSegments {
    fn without_segments(self) -> Self;
}

impl WithoutSegments for VisualSpec {
    fn without_segments(self) -> Self {
        let mut value = serde_json::to_value(&self).unwrap();
        value["segments"] = json!([]);
        spec(value)
    }
}

#[test]
fn a_number_line_point_slides_exactly() {
    let build = |at: i64, motion: bool| {
        let mut figure = json!({
            "kind": "number_line", "min": "0", "max": "10", "tick": "1",
            "points": [{"at": at.to_string(), "label": "A"}],
        });
        if motion {
            figure["motion"] = json!({
                "parameter": {"name": "place", "min": "0", "max": "10", "default": "2", "step": "1"},
                "drives": [{"kind": "point", "index": 0}],
            });
        }
        spec(figure)
    };
    let (svg, table) = table_of(&build(2, true));
    for index in 0..=10 {
        let still = render(&build(index as i64, false), &RenderOptions::default()).unwrap();
        dot_slides(&table.groups[0].law, index, &svg, &still);
    }
}

#[test]
fn a_shaded_count_reveals_one_part_per_index() {
    let build = |shaded: i64, motion: bool| {
        let mut figure = json!({"kind": "fraction", "parts": 5, "shaded": shaded});
        if motion {
            figure["motion"] = json!({
                "parameter": {"name": "shaded parts", "min": "0", "max": "5", "default": "2", "step": "1"},
                "drives": [{"kind": "shaded"}],
            });
        }
        spec(figure)
    };
    let (svg, table) = table_of(&build(2, true));
    assert_eq!(table.groups.len(), 5);
    assert_eq!(svg.matches("cadus-visual-on").count(), 2);
    assert_eq!(svg.matches("cadus-visual-off").count(), 3);
    for index in 0..=5_u32 {
        let still = render(&build(i64::from(index), false), &RenderOptions::default()).unwrap();
        let shown = table
            .groups
            .iter()
            .filter(|group| matches!(group.law, Law::Reveal { from } if from <= index))
            .count();
        assert_eq!(shown, still.matches("cadus-visual-part-shaded").count());
    }
    assert_eq!(table.readouts[0].texts[3], "3 of 5 equal parts");
}

#[test]
fn an_angle_turns_exactly() {
    let build = |degrees: i64, motion: bool| {
        let mut figure = json!({
            "kind": "geometry",
            "figure": {"shape": "angle", "vertex": {"x": "0", "y": "0"}, "initial_degrees": "0",
                       "terminal_degrees": degrees.to_string(), "ray_length": "3"},
        });
        if motion {
            figure["motion"] = json!({
                "parameter": {"name": "angle", "unit": "°", "min": "30", "max": "150", "default": "90", "step": "30"},
                "drives": [{"kind": "angle"}],
            });
        }
        spec(figure)
    };
    let (svg, table) = table_of(&build(90, true));
    assert_eq!(table.parameter.labels[2], "90°");
    assert_eq!(table.readouts[0].texts[0], "30°");
    let (deg, cx, cy) = rotate(&table.groups[0].law);
    let rays = tags(&svg, "line", "cadus-visual-segment");
    let (ex, ey) = (num(&rays[1], "x2"), num(&rays[1], "y2"));
    for index in 0..=4 {
        let still = render(
            &build(30 + 30 * index as i64, false),
            &RenderOptions::default(),
        )
        .unwrap();
        let wanted = tags(&still, "line", "cadus-visual-segment");
        let turn = (deg[0] * index as f64 + deg[1]).to_radians();
        let (dx, dy) = (ex - cx, ey - cy);
        let (rx, ry) = (
            dx.mul_add(turn.cos(), -dy * turn.sin()) + cx,
            dx.mul_add(turn.sin(), dy * turn.cos()) + cy,
        );
        // The still figure fits its own bounds, so only the angle between the rays is compared.
        let want = (
            num(&wanted[1], "x2") - num(&wanted[1], "x1"),
            num(&wanted[1], "y2") - num(&wanted[1], "y1"),
        );
        let got = (rx - cx, ry - cy);
        let (wl, gl) = (want.0.hypot(want.1), got.0.hypot(got.1));
        close(want.0 / wl, got.0 / gl);
        close(want.1 / wl, got.1 / gl);
    }
}

fn refused(figure: Value) -> VisualError {
    spec(figure).validate().expect_err("a refusal")
}

#[test]
fn the_motion_refusals() {
    let with = |motion: Value| {
        let mut figure = plane_json(&rat(1, 2), false);
        figure["motion"] = motion;
        figure
    };
    let slope = |min: &str, max: &str, default: &str, step: &str| {
        with(json!({
            "parameter": {"name": "slope", "min": min, "max": max, "default": default, "step": step},
            "drives": [{"kind": "line_slope", "segment": 0}],
        }))
    };
    assert!(matches!(
        refused(slope("-1", "2", "1/2", "2/3")),
        VisualError::Motion(_)
    ));
    assert!(matches!(
        refused(slope("-1", "2", "1/4", "1/2")),
        VisualError::Motion(_)
    ));
    assert!(matches!(
        refused(slope("0", "61", "0", "1")),
        VisualError::Motion(_)
    ));
    assert!(matches!(
        refused(slope("0", "1", "1/3", "1/3")),
        VisualError::Motion(_)
    ));
    // The default slope of the segment is 1/2, not 1.
    assert!(matches!(
        refused(slope("0", "2", "1", "1")),
        VisualError::Motion(_)
    ));
    // At the greatest value the end leaves the plane.
    assert!(matches!(
        refused(slope("0", "4", "1/2", "1/2")),
        VisualError::OutOfRange { .. }
    ));
    // A rectangle drive on a triangle, a drive of another family, and no drive.
    assert!(matches!(
        refused(with(json!({
            "parameter": {"name": "x", "min": "0", "max": "2", "default": "1", "step": "1"},
            "drives": [{"kind": "shaded"}],
        }))),
        VisualError::Motion(_)
    ));
    assert!(matches!(
        refused(with(json!({
            "parameter": {"name": "x", "min": "0", "max": "2", "default": "1", "step": "1"},
            "drives": [],
        }))),
        VisualError::Motion(_)
    ));
    // A second parameter is an unknown field.
    let mut two = slope("-1", "2", "1/2", "1/2");
    two["motion"]["second"] = json!({});
    assert!(serde_json::from_value::<VisualSpec>(two).is_err());
    // A reveal count above the part count.
    assert!(matches!(
        refused(
            json!({"kind": "fraction", "parts": 5, "shaded": 2, "motion": {
            "parameter": {"name": "shaded", "min": "0", "max": "6", "default": "2", "step": "1"},
            "drives": [{"kind": "shaded"}]}})
        ),
        VisualError::Motion(_)
    ));
    // More than twelve groups.
    assert!(matches!(
        refused(
            json!({"kind": "fraction", "parts": 13, "shaded": 2, "motion": {
            "parameter": {"name": "shaded", "min": "0", "max": "13", "default": "2", "step": "1"},
            "drives": [{"kind": "shaded"}]}})
        ),
        VisualError::Motion(_)
    ));
}

#[test]
fn a_still_figure_keeps_its_json_and_its_bytes() {
    let figure = spec(plane_json(&rat(1, 2), false));
    let all = render_all(std::slice::from_ref(&figure), "p");
    let json = serde_json::to_value(&all[0]).unwrap();
    assert!(json.get("motion").is_none());
    assert!(!all[0].svg.contains("cadus-visual-move"));
    assert!(figure.motion().is_none());
    let moving = spec(plane_json(&rat(1, 2), true));
    assert_eq!(moving.item_terms(), figure.item_terms());
    let a = render_all(std::slice::from_ref(&moving), "p");
    let b = render_all(std::slice::from_ref(&moving), "p");
    assert_eq!(a, b);
    assert!(a[0].motion.is_some());
}

#[test]
fn the_desc_names_the_slider_and_the_table_stays_small() {
    let (svg, table) = table_of(&spec(plane_json(&rat(1, 2), true)));
    assert!(svg.contains("The slider is named slope. It runs from -1 to 2."));
    assert!(serde_json::to_string(&table).unwrap().len() < MAX_TABLE_BYTES);
    for group in &table.groups {
        assert!(svg.contains(&format!("id=\"{}\"", group.id)));
    }
}
