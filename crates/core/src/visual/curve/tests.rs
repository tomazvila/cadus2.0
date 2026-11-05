//! Curve validation tests.

use super::{Asymptote, CurveFigure, CurveKind, LabeledCurve};
use crate::visual::{LabeledPoint, Scalar, VisualError};

fn parabola() -> CurveFigure {
    CurveFigure {
        x_min: Scalar::from("-5"),
        x_max: Scalar::from("5"),
        y_min: Scalar::from("-6"),
        y_max: Scalar::from("10"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("2"),
        curve: Some(CurveKind::Polynomial {
            coefficients: vec![Scalar::from("-4"), Scalar::from("0"), Scalar::from("1")],
        }),
        curves: Vec::new(),
        key_points: vec![
            LabeledPoint::labeled(0_i64, -4_i64, "vertex"),
            LabeledPoint::new(2_i64, 0_i64),
            LabeledPoint::new(-2_i64, 0_i64),
        ],
        asymptotes: Vec::new(),
        caption: None,
        motion: None,
    }
}

#[test]
fn a_parabola_validates_its_exact_key_points_and_reads_its_formula() {
    let figure = parabola();
    assert!(figure.validate().is_ok());
    let text = figure.text_equivalent();
    assert!(text.contains("The curve y = -4 + x^2."));
    assert!(text.contains("A key point at (0, -4) labeled vertex."));
}

#[test]
fn a_key_point_off_the_curve_is_refused() {
    let mut figure = parabola();
    figure.key_points = vec![LabeledPoint::new(1_i64, 1_i64)];
    assert!(matches!(
        figure.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
}

#[test]
fn natural_exponential_names_e_and_only_asserts_its_exact_intercept() {
    let figure = CurveFigure {
        x_min: Scalar::from("-2"),
        x_max: Scalar::from("2"),
        y_min: Scalar::from("0"),
        y_max: Scalar::from("8"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: Some(CurveKind::NaturalExponential {
            a: Scalar::from("1"),
            rate: Scalar::from("1"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        }),
        curves: Vec::new(),
        key_points: vec![LabeledPoint::labeled(0_i64, 1_i64, "exact intercept")],
        asymptotes: vec![Asymptote::Horizontal {
            at: Scalar::from("0"),
        }],
        caption: None,
        motion: None,
    };
    assert!(figure.validate().is_ok());
    assert!(figure.text_equivalent().contains("e^(1 · (x - 0))"));

    let mut approximate_e_is_not_an_exact_point = figure;
    approximate_e_is_not_an_exact_point.key_points = vec![LabeledPoint::new(1_i64, "2.71828")];
    assert!(matches!(
        approximate_e_is_not_an_exact_point.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
}

#[test]
fn an_exponential_checks_only_integer_steps_from_its_shift() {
    let figure = CurveFigure {
        x_min: Scalar::from("-3"),
        x_max: Scalar::from("3"),
        y_min: Scalar::from("0"),
        y_max: Scalar::from("10"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: Some(CurveKind::Exponential {
            a: Scalar::from("1"),
            b: Scalar::from("2"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        }),
        curves: Vec::new(),
        key_points: vec![
            LabeledPoint::new(0_i64, 1_i64),
            LabeledPoint::new(1_i64, 2_i64),
            LabeledPoint::new(3_i64, 8_i64),
        ],
        asymptotes: vec![Asymptote::Horizontal {
            at: Scalar::from("0"),
        }],
        caption: None,
        motion: None,
    };
    assert!(figure.validate().is_ok());

    let mut half_step = figure.clone();
    half_step.key_points = vec![LabeledPoint::new("0.5", "1.414")];
    assert!(matches!(
        half_step.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
}

#[test]
fn a_logarithm_checks_exact_powers_of_its_base() {
    let figure = CurveFigure {
        x_min: Scalar::from("0.5"),
        x_max: Scalar::from("10"),
        y_min: Scalar::from("-3"),
        y_max: Scalar::from("3"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: Some(CurveKind::Logarithm {
            a: Scalar::from("1"),
            base: Scalar::from("2"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        }),
        curves: Vec::new(),
        key_points: vec![
            LabeledPoint::new(1_i64, 0_i64),
            LabeledPoint::new(2_i64, 1_i64),
            LabeledPoint::new(4_i64, 2_i64),
        ],
        asymptotes: vec![],
        caption: None,
        motion: None,
    };
    assert!(figure.validate().is_ok());

    let mut off_grid = figure.clone();
    off_grid.key_points = vec![LabeledPoint::new("3", "1.585")];
    assert!(matches!(
        off_grid.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
}

#[test]
fn a_reciprocal_refuses_a_key_point_at_its_own_asymptote() {
    let mut figure = CurveFigure::square(
        5,
        CurveKind::Reciprocal {
            a: Scalar::from("2"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        },
    );
    figure.key_points = vec![LabeledPoint::new(0_i64, 0_i64)];
    assert!(matches!(
        figure.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
}

#[test]
fn a_sine_curve_checks_exact_quarter_period_points_only() {
    let figure = CurveFigure {
        x_min: Scalar::from("-1"),
        x_max: Scalar::from("5"),
        y_min: Scalar::from("-2"),
        y_max: Scalar::from("2"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: Some(CurveKind::Sine {
            amplitude: Scalar::from("1"),
            period: Scalar::from("4"),
            phase: Scalar::from("0"),
            midline: Scalar::from("0"),
        }),
        curves: Vec::new(),
        key_points: vec![
            LabeledPoint::new(0_i64, 0_i64),
            LabeledPoint::new(1_i64, 1_i64),
            LabeledPoint::new(2_i64, 0_i64),
            LabeledPoint::new(3_i64, -1_i64),
            LabeledPoint::new(4_i64, 0_i64),
        ],
        asymptotes: vec![],
        caption: None,
        motion: None,
    };
    assert!(figure.validate().is_ok());

    let mut wrong = figure.clone();
    wrong.key_points = vec![LabeledPoint::new(1_i64, 0_i64)];
    assert!(matches!(
        wrong.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
}

#[test]
fn degenerate_parameters_are_refused_before_any_key_point_check() {
    let mut zero_amplitude = CurveFigure {
        x_min: Scalar::from("-1"),
        x_max: Scalar::from("1"),
        y_min: Scalar::from("-1"),
        y_max: Scalar::from("1"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: Some(CurveKind::Sine {
            amplitude: Scalar::from("0"),
            period: Scalar::from("4"),
            phase: Scalar::from("0"),
            midline: Scalar::from("0"),
        }),
        curves: Vec::new(),
        key_points: vec![],
        asymptotes: vec![],
        caption: None,
        motion: None,
    };
    assert!(matches!(
        zero_amplitude.validate(),
        Err(VisualError::Degenerate { .. })
    ));

    zero_amplitude.curve = Some(CurveKind::Exponential {
        a: Scalar::from("1"),
        b: Scalar::from("1"),
        h: Scalar::from("0"),
        k: Scalar::from("0"),
    });
    assert!(matches!(
        zero_amplitude.validate(),
        Err(VisualError::Degenerate { .. })
    ));
}

#[test]
fn an_asymptote_outside_the_drawn_range_is_refused() {
    let mut figure = CurveFigure::square(
        5,
        CurveKind::Reciprocal {
            a: Scalar::from("1"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        },
    );
    figure.asymptotes = vec![Asymptote::Horizontal {
        at: Scalar::from("50"),
    }];
    assert!(matches!(
        figure.validate(),
        Err(VisualError::OutOfRange { .. })
    ));
    figure.asymptotes = vec![Asymptote::Vertical {
        at: Scalar::from("0"),
    }];
    assert!(figure.validate().is_ok());
}

fn line(slope: &str) -> CurveKind {
    CurveKind::Polynomial {
        coefficients: vec![Scalar::from("0"), Scalar::from(slope)],
    }
}

fn entry(label: &str, slope: &str) -> LabeledCurve {
    LabeledCurve {
        label: label.to_owned(),
        curve: line(slope),
        key_points: Vec::new(),
    }
}

fn several(labels: &[&str]) -> CurveFigure {
    let mut figure = CurveFigure::square(5, line("1"));
    figure.curve = None;
    figure.curves = labels
        .iter()
        .enumerate()
        .map(|(at, label)| entry(label, &(at + 1).to_string()))
        .collect();
    figure
}

fn reason(figure: &CurveFigure) -> String {
    match figure.validate() {
        Err(VisualError::CurveSet { reason }) => reason,
        other => panic!("expected CurveSet, got {other:?}"),
    }
}

#[test]
fn a_figure_gives_curve_or_curves_never_both_and_never_neither() {
    assert!(several(&["A", "B"]).validate().is_ok());
    let mut both = several(&["A", "B"]);
    both.curve = Some(line("1"));
    assert!(reason(&both).contains("both"));
    let mut neither = several(&["A", "B"]);
    neither.curves.clear();
    assert!(reason(&neither).contains("neither"));
}

#[test]
fn the_curves_count_is_two_to_six() {
    assert!(reason(&several(&["A"])).contains("1 entries"));
    assert!(several(&["A", "B", "C", "D", "E", "F"]).validate().is_ok());
    assert!(reason(&several(&["A", "B", "C", "D", "E", "F", "G"])).contains("7 entries"));
}

#[test]
fn a_curve_label_is_unique_and_one_or_two_characters() {
    assert!(reason(&several(&["A", "A"])).contains("repeats"));
    assert!(reason(&several(&["A", ""])).contains("one or two"));
    assert!(reason(&several(&["A", "ABC"])).contains("one or two"));
    assert!(several(&["A1", "B"]).validate().is_ok());
}

#[test]
fn curves_take_their_key_points_on_each_curve_and_no_slider() {
    let mut figure = several(&["A", "B"]);
    figure.key_points = vec![LabeledPoint::new(1_i64, 1_i64)];
    assert!(reason(&figure).contains("key_points"));

    let mut off = several(&["A", "B"]);
    off.curves[1].key_points = vec![LabeledPoint::new(1_i64, 1_i64)];
    match off.validate() {
        Err(VisualError::KeyPointOffCurve { point }) => assert!(point.ends_with("on curve B")),
        other => panic!("expected KeyPointOffCurve, got {other:?}"),
    }
    off.curves[1].key_points = vec![LabeledPoint::new(1_i64, 2_i64)];
    assert!(off.validate().is_ok());
}

#[test]
fn the_text_names_every_label_with_its_function() {
    let text = several(&["A", "B"]).text_equivalent();
    assert!(text.contains("The plane holds 2 curves."));
    assert!(text.contains("Curve A is y = x."));
    assert!(text.contains("Curve B is y = 2 x."), "{text}");
}

#[test]
fn a_one_curve_spec_serializes_with_no_curves_key() {
    let json = serde_json::to_string(&parabola()).unwrap();
    assert!(json.contains("\"curve\"") && !json.contains("curves"));
    let text = "x_min: '-5'\nx_max: '5'\ny_min: '-5'\ny_max: '5'\nx_tick: '1'\ny_tick: '1'\n\
                curves:\n- {label: A, curve: {family: polynomial, coefficients: ['0', '1']}}\n\
                - {label: B, curve: {family: polynomial, coefficients: ['0', '2']}}\n";
    let figure: CurveFigure = serde_norway::from_str(text).unwrap();
    assert!(figure.validate().is_ok());
}

fn root(a: &str, b: &str, c: &str, d: &str) -> CurveKind {
    CurveKind::Sqrt {
        a: Scalar::from(a),
        b: Scalar::from(b),
        c: Scalar::from(c),
        d: Scalar::from(d),
    }
}

#[test]
fn a_root_curve_checks_key_points_exactly_and_refuses_a_zero_factor() {
    // y = -sqrt(x) + 2: (4, 0) and (0, 2) sit on it, (2, 1) does not (sqrt 2 is irrational).
    let mut figure = CurveFigure::square(5, root("-1", "1", "0", "2"));
    figure.key_points = vec![
        LabeledPoint::new(4_i64, 0_i64),
        LabeledPoint::new(0_i64, 2_i64),
    ];
    assert!(figure.validate().is_ok());
    figure.key_points = vec![LabeledPoint::new(4_i64, 1_i64)];
    assert!(matches!(
        figure.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
    figure.key_points = vec![LabeledPoint::new(2_i64, 1_i64)];
    assert!(matches!(
        figure.validate(),
        Err(VisualError::KeyPointOffCurve { .. })
    ));
    // Left of the domain edge there is no point at all.
    let mut shifted = CurveFigure::square(5, root("1", "1", "-3", "0"));
    shifted.key_points = vec![LabeledPoint::new(-1_i64, 2_i64)];
    assert!(shifted.validate().is_err());
    shifted.key_points = vec![
        LabeledPoint::new(3_i64, 0_i64),
        LabeledPoint::new(4_i64, 1_i64),
    ];
    assert!(shifted.validate().is_ok());
    assert!(
        CurveFigure::square(5, root("1", "0", "1", "0"))
            .validate()
            .is_err()
    );
    assert!(
        CurveFigure::square(5, root("0", "1", "0", "0"))
            .validate()
            .is_err()
    );
    assert!(
        CurveFigure::square(5, root("1", "1", "-3", "0"))
            .text_equivalent()
            .contains("the square-root curve shifted")
    );
}

#[test]
fn a_root_curve_starts_at_its_domain_edge() {
    use crate::visual::{RenderOptions, VisualSpec, render};
    // y = sqrt(x - 3) in a plane from -5 to 5: nothing is drawn left of x = 3.
    let figure = CurveFigure::square(5, root("1", "1", "-3", "0"));
    let svg = render(&VisualSpec::Curve(figure), &RenderOptions::default()).unwrap();
    let start = svg.find("<polyline points=\"").unwrap() + 18;
    let first = svg[start..].split(' ').next().unwrap();
    let x: f64 = first.split(',').next().unwrap().parse().unwrap();
    // x = 3 sits at 36 + (3 + 5) / 10 * 408 = 362.4 px.
    assert!((x - 362.4).abs() < 0.01, "the line starts at {x}");
}
