//! The SVG render of the four visual families (unit f9).
//!
//! Every test reads the rendered bytes and the accessible equivalent together.
//! A picture that a screen reader cannot read is not an accessible visual, and a
//! picture the check refuses must never reach the bytes at all.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::visual::{
    Asymptote, CoordinateFigure, CurveFigure, CurveKind, FractionFigure, GeometryFigure,
    LabeledCurve, LabeledPoint, MarkedRay, NumberLineFigure, RayDirection, RenderOptions, Scalar,
    Segment, ShadedHalfPlane, VisualError, VisualSpec, render,
};

#[path = "visual_render/geometry.rs"]
mod geometry;

/// The count of one substring in one text.
fn count(text: &str, needle: &str) -> usize {
    text.matches(needle).count()
}

/// A number line from 0 to 5 with a point at 3.
fn line_spec() -> VisualSpec {
    let mut figure = NumberLineFigure::new(0_i64, 5_i64, 1_i64).with_point(3_i64, Some("x"));
    figure.caption = Some("Plot 3".to_owned());
    VisualSpec::NumberLine(figure)
}

#[test]
fn every_render_carries_the_accessible_frame_the_screen_reader_reads() {
    let options = RenderOptions::with_prefix("q1");
    let svg = render(&line_spec(), &options).unwrap();
    assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 480 200\""));
    assert!(svg.contains("role=\"img\""));
    assert!(svg.contains("aria-labelledby=\"q1-title q1-desc\""));
    assert!(svg.contains("<title id=\"q1-title\">Plot 3</title>"));
    assert!(svg.contains(
        "<desc id=\"q1-desc\">Plot 3. A number line from 0 to 5 with a tick every 1. \
         A filled point at 3, labeled x.</desc>"
    ));
    assert!(svg.ends_with("</svg>"));
    assert!(svg.contains("class=\"cadus-visual cadus-visual-number_line\""));
}

#[test]
fn the_title_falls_back_to_the_family_name_when_no_author_wrote_a_caption() {
    let spec = VisualSpec::NumberLine(NumberLineFigure::new(0_i64, 2_i64, 1_i64));
    let svg = render(&spec, &RenderOptions::default()).unwrap();
    assert!(svg.contains("<title id=\"visual-title\">Number line</title>"));
    let plane = VisualSpec::Coordinate(CoordinateFigure::square(2));
    assert!(
        render(&plane, &RenderOptions::default())
            .unwrap()
            .contains("<title id=\"visual-title\">Coordinate plane</title>")
    );
    let fraction = VisualSpec::Fraction(FractionFigure::bar(1, 2));
    assert!(
        render(&fraction, &RenderOptions::default())
            .unwrap()
            .contains("<title id=\"visual-title\">Fraction figure</title>")
    );
    let shape = VisualSpec::Geometry(GeometryFigure::circle(
        LabeledPoint::new(0_i64, 0_i64),
        1_i64,
    ));
    assert!(
        render(&shape, &RenderOptions::default())
            .unwrap()
            .contains("<title id=\"visual-title\">Geometry diagram</title>")
    );
}

#[test]
fn the_render_is_deterministic_and_the_prefix_is_the_only_difference() {
    let first = render(&line_spec(), &RenderOptions::with_prefix("a")).unwrap();
    let again = render(&line_spec(), &RenderOptions::with_prefix("a")).unwrap();
    assert_eq!(first, again);
    let other = render(&line_spec(), &RenderOptions::with_prefix("b")).unwrap();
    assert_ne!(first, other);
    assert_eq!(
        first
            .replace("a-title", "b-title")
            .replace("a-desc", "b-desc"),
        other
    );
}

#[test]
fn a_figure_that_fails_the_check_renders_no_bytes() {
    let bad = VisualSpec::NumberLine(NumberLineFigure::new(5_i64, 0_i64, 1_i64));
    assert!(matches!(
        render(&bad, &RenderOptions::default()),
        Err(VisualError::RangeNotAscending { axis: "line" })
    ));
    let outside =
        VisualSpec::NumberLine(NumberLineFigure::new(0_i64, 5_i64, 1_i64).with_point(9_i64, None));
    assert!(matches!(
        render(&outside, &RenderOptions::default()),
        Err(VisualError::OutOfRange { .. })
    ));
}

#[test]
fn the_number_line_draws_one_tick_per_step_and_labels_them_below_the_line() {
    let spec = line_spec();
    let svg = render(&spec, &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "cadus-visual-tick"), 6);
    for label in ["0", "1", "2", "3", "4", "5"] {
        assert!(
            svg.contains(&format!(">{label}</text>")),
            "the tick label {label} is missing"
        );
    }
    assert_eq!(count(&svg, "cadus-visual-point\""), 1);
}

#[test]
fn a_crowded_number_line_drops_the_tick_labels_and_keeps_the_ticks() {
    let spec = VisualSpec::NumberLine(NumberLineFigure::new(0_i64, 100_i64, 1_i64));
    let svg = render(&spec, &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "cadus-visual-tick"), 101);
    assert_eq!(count(&svg, "cadus-visual-label"), 0);
}

#[test]
fn an_open_interval_end_draws_an_open_cap_and_a_closed_end_draws_a_filled_cap() {
    let mut figure = NumberLineFigure::new(0_i64, 4_i64, 1_i64);
    figure.intervals = vec![cadus_core::visual::MarkedInterval {
        from: Scalar::from("1"),
        to: Scalar::from("3"),
        closed_start: true,
        closed_end: false,
        label: Some("A".to_owned()),
    }];
    let svg = render(&VisualSpec::NumberLine(figure), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "cadus-visual-point\""), 1);
    assert_eq!(count(&svg, "cadus-visual-point-open\""), 1);
    assert_eq!(count(&svg, "cadus-visual-interval"), 1);
    assert!(svg.contains(">A</text>"));
}

#[test]
fn a_ray_draws_one_end_cap_an_arrowhead_at_the_frame_edge_and_no_second_end_cap() {
    let mut figure = NumberLineFigure::new(-5_i64, 5_i64, 1_i64);
    figure.rays = vec![MarkedRay {
        from: Scalar::from("2"),
        direction: RayDirection::Right,
        filled: true,
        label: Some("x ≥ 2".to_owned()),
    }];
    let svg = render(&VisualSpec::NumberLine(figure), &RenderOptions::default()).unwrap();
    assert_eq!(
        count(&svg, "cadus-visual-ray"),
        2,
        "a line and an arrowhead"
    );
    assert_eq!(count(&svg, "cadus-visual-point\""), 1, "one filled origin");
    assert_eq!(count(&svg, "cadus-visual-point-open\""), 0);
    assert!(svg.contains("x ≥ 2"));

    let mut open = NumberLineFigure::new(-5_i64, 5_i64, 1_i64);
    open.rays = vec![MarkedRay {
        from: Scalar::from("-2"),
        direction: RayDirection::Left,
        filled: false,
        label: None,
    }];
    let svg = render(&VisualSpec::NumberLine(open), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "cadus-visual-point-open\""), 1);
}

#[test]
fn a_ray_origin_outside_the_drawn_range_fails_the_check() {
    let mut figure = NumberLineFigure::new(-5_i64, 5_i64, 1_i64);
    figure.rays = vec![MarkedRay {
        from: Scalar::from("9"),
        direction: RayDirection::Right,
        filled: true,
        label: None,
    }];
    assert!(matches!(
        render(&VisualSpec::NumberLine(figure), &RenderOptions::default()),
        Err(VisualError::OutOfRange { .. })
    ));
}

#[test]
fn the_fraction_bar_shades_the_numerator_and_leaves_the_rest_plain() {
    let spec = VisualSpec::Fraction(FractionFigure::bar(3, 4));
    let svg = render(&spec, &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "<rect"), 4);
    assert_eq!(count(&svg, "cadus-visual-part-shaded"), 3);
    assert_eq!(count(&svg, "class=\"cadus-visual-part\""), 1);
    assert!(svg.contains("The figure shows 3/4."));
}

#[test]
fn an_improper_fraction_draws_two_wholes_and_shades_across_both() {
    let spec = VisualSpec::Fraction(FractionFigure::bar(5, 3));
    let svg = render(&spec, &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "<rect"), 6);
    assert_eq!(count(&svg, "cadus-visual-part-shaded"), 5);
    assert_eq!(count(&svg, "class=\"cadus-visual-part\""), 1);
}

#[test]
fn the_fraction_circle_draws_one_path_per_sector() {
    let spec = VisualSpec::Fraction(FractionFigure::circle(1, 4));
    let svg = render(&spec, &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "<path"), 4);
    assert_eq!(count(&svg, "cadus-visual-part-shaded"), 1);
    assert!(svg.contains("4 equal sectors"));
}

#[test]
fn the_coordinate_plane_draws_both_axes_only_when_it_holds_zero() {
    let mut figure = CoordinateFigure::square(2);
    figure.points = vec![LabeledPoint::labeled(1_i64, 1_i64, "P")];
    figure.segments = vec![Segment {
        from: LabeledPoint::new(-2_i64, -2_i64),
        to: LabeledPoint::new(2_i64, 2_i64),
        label: None,
    }];
    let svg = render(&VisualSpec::Coordinate(figure), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "cadus-visual-axis"), 2);
    assert_eq!(count(&svg, "cadus-visual-grid"), 10);
    assert_eq!(count(&svg, "cadus-visual-segment"), 1);
    assert!(svg.contains(">P</text>"));

    let mut shifted = CoordinateFigure::square(2);
    shifted.x_min = Scalar::from("1");
    shifted.x_max = Scalar::from("5");
    shifted.y_min = Scalar::from("1");
    shifted.y_max = Scalar::from("5");
    let away = render(&VisualSpec::Coordinate(shifted), &RenderOptions::default()).unwrap();
    assert_eq!(count(&away, "cadus-visual-axis"), 0);
}

#[test]
fn a_shaded_half_plane_draws_a_filled_region_and_a_solid_or_dashed_boundary() {
    let mut solid = CoordinateFigure::square(5);
    solid.shaded_half_planes = vec![ShadedHalfPlane {
        through_a: LabeledPoint::new(0_i64, 0_i64),
        through_b: LabeledPoint::new(1_i64, 1_i64),
        solid: true,
        shade_toward: LabeledPoint::new(4_i64, 0_i64),
        label: Some("y ≤ x".to_owned()),
    }];
    let svg = render(&VisualSpec::Coordinate(solid), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "<polygon"), 1);
    assert_eq!(count(&svg, "cadus-visual-boundary-solid"), 1);
    assert_eq!(count(&svg, "cadus-visual-boundary-dashed"), 0);
    assert!(svg.contains("y ≤ x"));

    let mut dashed = CoordinateFigure::square(5);
    dashed.shaded_half_planes = vec![ShadedHalfPlane {
        through_a: LabeledPoint::new(0_i64, 0_i64),
        through_b: LabeledPoint::new(1_i64, 1_i64),
        solid: false,
        shade_toward: LabeledPoint::new(4_i64, 0_i64),
        label: None,
    }];
    let dashed_svg = render(&VisualSpec::Coordinate(dashed), &RenderOptions::default()).unwrap();
    assert_eq!(count(&dashed_svg, "cadus-visual-boundary-dashed"), 1);
}

#[test]
fn a_shaded_half_plane_that_sits_on_the_boundary_or_repeats_a_point_renders_no_bytes() {
    let mut degenerate = CoordinateFigure::square(5);
    degenerate.shaded_half_planes = vec![ShadedHalfPlane::new((0, 0), (2, 2), true, (1, 1))];
    assert!(matches!(
        render(
            &VisualSpec::Coordinate(degenerate),
            &RenderOptions::default()
        ),
        Err(VisualError::Degenerate { .. })
    ));
}

#[test]
fn a_curve_draws_a_broken_polyline_around_its_asymptote_and_its_key_points() {
    let figure = CurveFigure {
        x_min: Scalar::from("-6"),
        x_max: Scalar::from("6"),
        y_min: Scalar::from("-6"),
        y_max: Scalar::from("6"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: Some(CurveKind::Reciprocal {
            a: Scalar::from("4"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        }),
        curves: Vec::new(),
        key_points: vec![
            LabeledPoint::labeled(2_i64, 2_i64, "A"),
            LabeledPoint::new(-2_i64, -2_i64),
        ],
        asymptotes: vec![
            Asymptote::Horizontal {
                at: Scalar::from("0"),
            },
            Asymptote::Vertical {
                at: Scalar::from("0"),
            },
        ],
        caption: Some("Reference: y = 4/x".to_owned()),
        motion: None,
    };
    let svg = render(&VisualSpec::Curve(figure), &RenderOptions::default()).unwrap();
    assert!(svg.contains("class=\"cadus-visual cadus-visual-curve\""));
    assert_eq!(count(&svg, "<polyline"), 2, "one run on each side of x = 0");
    assert_eq!(count(&svg, "cadus-visual-asymptote"), 2);
    assert_eq!(count(&svg, "cadus-visual-point\""), 2);
    assert!(svg.contains(">A</text>"));
}

#[test]
fn a_curve_with_a_key_point_off_the_curve_renders_no_bytes() {
    let mut figure = CurveFigure::square(
        5,
        CurveKind::Polynomial {
            coefficients: vec![Scalar::from("0"), Scalar::from("0"), Scalar::from("1")],
        },
    );
    figure.key_points = vec![LabeledPoint::new(2_i64, 5_i64)];
    assert!(matches!(
        render(&VisualSpec::Curve(figure), &RenderOptions::default()),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
}

#[test]
fn the_render_escapes_the_text_an_author_wrote() {
    let mut figure = NumberLineFigure::new(0_i64, 2_i64, 1_i64).with_point(1_i64, Some("<a & b>"));
    figure.caption = Some("\"quoted\" & 'marked'".to_owned());
    let svg = render(&VisualSpec::NumberLine(figure), &RenderOptions::default()).unwrap();
    assert!(svg.contains("&quot;quoted&quot; &amp; &apos;marked&apos;"));
    assert!(svg.contains("&lt;a &amp; b&gt;"));
    assert!(!svg.contains("<a &"));
}

#[test]
fn the_payload_drops_a_refused_figure_and_gives_each_kept_one_its_own_ids() {
    let specs = vec![
        line_spec(),
        VisualSpec::NumberLine(NumberLineFigure::new(5_i64, 0_i64, 1_i64)),
        VisualSpec::Fraction(FractionFigure::bar(3, 4)),
    ];
    let drawn = cadus_core::visual::render_all(&specs, "p7");
    assert_eq!(drawn.len(), 2);
    assert_eq!(drawn[0].kind, "number_line");
    assert_eq!(drawn[1].kind, "fraction");
    assert!(
        drawn[0]
            .svg
            .contains("aria-labelledby=\"p7-0-title p7-0-desc\"")
    );
    assert!(
        drawn[1]
            .svg
            .contains("aria-labelledby=\"p7-2-title p7-2-desc\"")
    );
    assert_eq!(
        drawn[1].text,
        "A fraction bar divided into 4 equal parts, with 3 of them shaded. \
         The figure shows 3/4."
    );
    assert!(cadus_core::visual::render_all(&[], "p7").is_empty());
}

fn line(slope: &str, offset: &str) -> CurveKind {
    CurveKind::Polynomial {
        coefficients: vec![Scalar::from(offset), Scalar::from(slope)],
    }
}

fn labeled(label: &str, curve: CurveKind) -> LabeledCurve {
    LabeledCurve {
        label: label.to_owned(),
        curve,
        key_points: Vec::new(),
    }
}

/// Four curves on one plane. A and D both end at (5, 5), so their labels collide.
fn which_graph() -> CurveFigure {
    let mut figure = CurveFigure::square(5, line("1", "0"));
    figure.curve = None;
    figure.curves = vec![
        labeled("A", line("1", "0")),
        labeled("B", line("-1", "0")),
        labeled("C", line("0", "1")),
        labeled(
            "D",
            CurveKind::Polynomial {
                coefficients: vec![Scalar::from("0"), Scalar::from("0"), Scalar::from("1/5")],
            },
        ),
    ];
    figure.curves[0].key_points = vec![LabeledPoint::new(2_i64, 2_i64)];
    figure.caption = Some("Which graph is y = x?".to_owned());
    figure
}

/// The `(x, y)` of the `<text>` element that holds `label`.
fn label_at(svg: &str, label: &str) -> (f64, f64) {
    let end = svg.find(&format!(">{label}</text>")).unwrap();
    let start = svg[..end].rfind("<text").unwrap();
    let tag = &svg[start..end];
    let number = |name: &str| -> f64 {
        let at = tag.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
        tag[at..].split('"').next().unwrap().parse().unwrap()
    };
    (number("x"), number("y"))
}

#[test]
fn four_curves_draw_four_strokes_and_four_labels() {
    let spec = VisualSpec::Curve(which_graph());
    let svg = render(&spec, &RenderOptions::default()).unwrap();
    for index in 1..=4 {
        assert_eq!(
            count(
                &svg,
                &format!("class=\"cadus-visual-curve cadus-visual-curve-s{index}\"")
            ),
            1,
            "stroke {index}"
        );
    }
    assert_eq!(count(&svg, "<polyline"), 4);
    for label in ["A", "B", "C", "D"] {
        assert_eq!(count(&svg, &format!(">{label}</text>")), 1, "label {label}");
    }
    assert_eq!(count(&svg, "cadus-visual-point\""), 1, "the key point of A");
    assert!(svg.contains("Curve A is y = x."));
    assert!(svg.contains("Curve D is y ="));
    assert!(svg.contains("A key point on curve A at (2, 2)."));
    assert_eq!(svg, render(&spec, &RenderOptions::default()).unwrap());
}

#[test]
fn two_labels_at_one_end_never_overlap_and_stay_inside_the_plane() {
    let svg = render(&VisualSpec::Curve(which_graph()), &RenderOptions::default()).unwrap();
    let a = label_at(&svg, "A");
    let d = label_at(&svg, "D");
    assert!((a.1 - d.1).abs() >= 14.0, "A at {a:?} and D at {d:?}");
    for (x, y) in [a, label_at(&svg, "B"), label_at(&svg, "C"), d] {
        assert!(
            (36.0..=444.0).contains(&x) && (36.0..=164.0).contains(&y),
            "({x}, {y})"
        );
    }
}

#[test]
fn a_curve_figure_with_one_curve_renders_exactly_as_before_d_fg1() {
    let mut figure = CurveFigure::square(
        5,
        CurveKind::Polynomial {
            coefficients: vec![Scalar::from("-4"), Scalar::from("0"), Scalar::from("1")],
        },
    );
    figure.key_points = vec![
        LabeledPoint::labeled(0_i64, -4_i64, "vertex"),
        LabeledPoint::new(2_i64, 0_i64),
    ];
    figure.caption = Some("Parabola".to_owned());
    let svg = render(&VisualSpec::Curve(figure), &RenderOptions::default()).unwrap();
    // The fixture holds the bytes the render gave before `curves` existed.
    assert_eq!(svg, include_str!("fixtures/visual/curve_one_parabola.svg"));
}

#[test]
fn a_figure_with_curves_takes_no_slider() {
    let mut figure = which_graph();
    figure.motion = Some(
        serde_norway::from_str(
            "parameter: {name: m, min: '1', max: '3', default: '1', step: '1'}\n\
             drives: [{kind: line_slope, segment: 0}]\n",
        )
        .unwrap(),
    );
    assert!(matches!(
        render(&VisualSpec::Curve(figure), &RenderOptions::default()),
        Err(VisualError::CurveSet { .. })
    ));
}

#[test]
fn a_root_curve_draws_among_the_labelled_curves() {
    let root = |a: &str, b: &str, c: &str, d: &str| CurveKind::Sqrt {
        a: Scalar::from(a),
        b: Scalar::from(b),
        c: Scalar::from(c),
        d: Scalar::from(d),
    };
    let mut figure = CurveFigure::square(5, line("1", "0"));
    figure.curve = None;
    figure.curves = vec![
        labeled("A", root("1", "1", "0", "0")),
        labeled("B", root("1", "1", "-3", "0")),
        labeled("C", root("-1", "1", "0", "2")),
    ];
    figure.curves[1].key_points = vec![LabeledPoint::new(4_i64, 1_i64)];
    let svg = render(&VisualSpec::Curve(figure), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "<polyline"), 3);
    for label in ["A", "B", "C"] {
        assert_eq!(count(&svg, &format!(">{label}</text>")), 1, "label {label}");
    }
    assert!(svg.contains("Curve B is y = 1 · sqrt(1 · x + -3) + 0"));
}
