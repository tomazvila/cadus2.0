//! Curve validation tests.

use super::{Asymptote, CurveFigure, CurveKind};
use crate::visual::{LabeledPoint, Scalar, VisualError};

fn parabola() -> CurveFigure {
    CurveFigure {
        x_min: Scalar::from("-5"),
        x_max: Scalar::from("5"),
        y_min: Scalar::from("-6"),
        y_max: Scalar::from("10"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("2"),
        curve: CurveKind::Polynomial {
            coefficients: vec![Scalar::from("-4"), Scalar::from("0"), Scalar::from("1")],
        },
        key_points: vec![
            LabeledPoint::labeled(0_i64, -4_i64, "vertex"),
            LabeledPoint::new(2_i64, 0_i64),
            LabeledPoint::new(-2_i64, 0_i64),
        ],
        asymptotes: Vec::new(),
        caption: None,
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
        curve: CurveKind::NaturalExponential {
            a: Scalar::from("1"),
            rate: Scalar::from("1"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        },
        key_points: vec![LabeledPoint::labeled(0_i64, 1_i64, "exact intercept")],
        asymptotes: vec![Asymptote::Horizontal {
            at: Scalar::from("0"),
        }],
        caption: None,
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
        curve: CurveKind::Exponential {
            a: Scalar::from("1"),
            b: Scalar::from("2"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        },
        key_points: vec![
            LabeledPoint::new(0_i64, 1_i64),
            LabeledPoint::new(1_i64, 2_i64),
            LabeledPoint::new(3_i64, 8_i64),
        ],
        asymptotes: vec![Asymptote::Horizontal {
            at: Scalar::from("0"),
        }],
        caption: None,
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
        curve: CurveKind::Logarithm {
            a: Scalar::from("1"),
            base: Scalar::from("2"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        },
        key_points: vec![
            LabeledPoint::new(1_i64, 0_i64),
            LabeledPoint::new(2_i64, 1_i64),
            LabeledPoint::new(4_i64, 2_i64),
        ],
        asymptotes: vec![],
        caption: None,
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
    let figure = CurveFigure {
        x_min: Scalar::from("-5"),
        x_max: Scalar::from("5"),
        y_min: Scalar::from("-5"),
        y_max: Scalar::from("5"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: CurveKind::Reciprocal {
            a: Scalar::from("2"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        },
        key_points: vec![LabeledPoint::new(0_i64, 0_i64)],
        asymptotes: vec![],
        caption: None,
    };
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
        curve: CurveKind::Sine {
            amplitude: Scalar::from("1"),
            period: Scalar::from("4"),
            phase: Scalar::from("0"),
            midline: Scalar::from("0"),
        },
        key_points: vec![
            LabeledPoint::new(0_i64, 0_i64),
            LabeledPoint::new(1_i64, 1_i64),
            LabeledPoint::new(2_i64, 0_i64),
            LabeledPoint::new(3_i64, -1_i64),
            LabeledPoint::new(4_i64, 0_i64),
        ],
        asymptotes: vec![],
        caption: None,
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
        curve: CurveKind::Sine {
            amplitude: Scalar::from("0"),
            period: Scalar::from("4"),
            phase: Scalar::from("0"),
            midline: Scalar::from("0"),
        },
        key_points: vec![],
        asymptotes: vec![],
        caption: None,
    };
    assert!(matches!(
        zero_amplitude.validate(),
        Err(VisualError::Degenerate { .. })
    ));

    zero_amplitude.curve = CurveKind::Exponential {
        a: Scalar::from("1"),
        b: Scalar::from("1"),
        h: Scalar::from("0"),
        k: Scalar::from("0"),
    };
    assert!(matches!(
        zero_amplitude.validate(),
        Err(VisualError::Degenerate { .. })
    ));
}

#[test]
fn an_asymptote_outside_the_drawn_range_is_refused() {
    let mut figure = CurveFigure {
        x_min: Scalar::from("-5"),
        x_max: Scalar::from("5"),
        y_min: Scalar::from("-5"),
        y_max: Scalar::from("5"),
        x_tick: Scalar::from("1"),
        y_tick: Scalar::from("1"),
        curve: CurveKind::Reciprocal {
            a: Scalar::from("1"),
            h: Scalar::from("0"),
            k: Scalar::from("0"),
        },
        key_points: vec![],
        asymptotes: vec![Asymptote::Horizontal {
            at: Scalar::from("50"),
        }],
        caption: None,
    };
    assert!(matches!(
        figure.validate(),
        Err(VisualError::OutOfRange { .. })
    ));
    figure.asymptotes = vec![Asymptote::Vertical {
        at: Scalar::from("0"),
    }];
    assert!(figure.validate().is_ok());
}
