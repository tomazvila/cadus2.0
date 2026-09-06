//! Geometry and special-triangle renderer tests.

use cadus_core::visual::{
    AngleMark, GeometryFigure, GeometryShape, LabeledPoint, RenderOptions, Scalar,
    SpecialTriangleFigure, SpecialTriangleShape, VisualError, VisualSpec, render,
};

use super::count;

#[test]
fn the_geometry_polygon_draws_its_outline_its_labels_and_its_right_angle() {
    let vertices = vec![
        LabeledPoint::labeled(0_i64, 0_i64, "A"),
        LabeledPoint::labeled(4_i64, 0_i64, "B"),
        LabeledPoint::labeled(0_i64, 3_i64, "C"),
    ];
    let figure = GeometryFigure {
        figure: GeometryShape::Polygon {
            vertices,
            right_angles: vec![0],
            angle_marks: vec![AngleMark {
                at: 1,
                label: Some("θ".to_owned()),
            }],
        },
        caption: Some("Right triangle".to_owned()),
    };
    let svg = render(&VisualSpec::Geometry(figure), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "<polygon"), 1);
    assert_eq!(count(&svg, "cadus-visual-right-angle"), 1);
    assert_eq!(count(&svg, "cadus-visual-angle"), 1);
    for label in ["A", "B", "C"] {
        assert!(svg.contains(&format!(">{label}</text>")));
    }
    assert!(svg.contains(">θ</text>"));
    assert!(svg.contains("<title id=\"visual-title\">Right triangle</title>"));
    assert!(svg.contains("A right angle at (0, 0) labeled A."));
}

#[test]
fn an_angle_mark_naming_a_missing_vertex_renders_no_bytes() {
    let figure = GeometryFigure {
        figure: GeometryShape::Polygon {
            vertices: vec![
                LabeledPoint::new(0_i64, 0_i64),
                LabeledPoint::new(4_i64, 0_i64),
                LabeledPoint::new(0_i64, 3_i64),
            ],
            right_angles: vec![],
            angle_marks: vec![AngleMark { at: 9, label: None }],
        },
        caption: None,
    };
    assert!(matches!(
        render(&VisualSpec::Geometry(figure), &RenderOptions::default()),
        Err(VisualError::NoSuchVertex { at: 9, count: 3 })
    ));
}

#[test]
fn an_angle_diagram_draws_two_rays_an_arc_and_its_label() {
    let figure = GeometryFigure {
        figure: GeometryShape::Angle {
            vertex: LabeledPoint::new(0_i64, 0_i64),
            initial_degrees: Scalar::from("0"),
            terminal_degrees: Scalar::from("135"),
            ray_length: Scalar::from("4"),
            label: Some("135°".to_owned()),
        },
        caption: Some("Reference angle".to_owned()),
    };
    let svg = render(&VisualSpec::Geometry(figure), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "cadus-visual-segment"), 2, "two rays");
    assert_eq!(count(&svg, "cadus-visual-angle"), 1);
    assert!(svg.contains(">135°</text>"));
    assert!(
        svg.contains("opening from 0° to 135°, measured counterclockwise from the positive x-axis")
    );
}

#[test]
fn the_geometry_circle_draws_the_radius_beside_the_center() {
    let figure = GeometryFigure::circle(LabeledPoint::new(1_i64, 1_i64), "2.5");
    let svg = render(&VisualSpec::Geometry(figure), &RenderOptions::default()).unwrap();
    assert_eq!(count(&svg, "cadus-visual-shape"), 1);
    assert_eq!(count(&svg, "cadus-visual-segment"), 1);
    assert!(svg.contains(">r = 2.5</text>"));
    assert!(svg.contains("A circle with center at (1, 1) and radius 2.5."));
}

#[test]
fn a_forty_five_special_triangle_draws_its_computed_hypotenuse_label() {
    let figure = SpecialTriangleFigure {
        figure: SpecialTriangleShape::FortyFiveFortyFiveNinety {
            leg: Scalar::from("4"),
        },
        caption: Some("Reference: 45-45-90".to_owned()),
    };
    let svg = render(
        &VisualSpec::SpecialTriangle(figure),
        &RenderOptions::default(),
    )
    .unwrap();
    assert_eq!(count(&svg, "<polygon"), 1);
    assert_eq!(count(&svg, "cadus-visual-right-angle"), 1);
    assert!(svg.contains(">4√2</text>"));
    assert!(svg.contains("Reference: 45-45-90. A 45-45-90 right triangle"));
}

#[test]
fn a_thirty_sixty_special_triangle_marks_both_acute_angles() {
    let figure = SpecialTriangleFigure {
        figure: SpecialTriangleShape::ThirtySixtyNinety {
            short_leg: Scalar::from("3"),
        },
        caption: None,
    };
    let svg = render(
        &VisualSpec::SpecialTriangle(figure),
        &RenderOptions::default(),
    )
    .unwrap();
    assert_eq!(count(&svg, "cadus-visual-angle"), 2);
    assert!(svg.contains(">30°</text>"));
    assert!(svg.contains(">60°</text>"));
    assert!(svg.contains(">3√3</text>"));
    assert!(svg.contains(">6</text>"));
}

#[test]
fn a_sas_area_special_triangle_refuses_an_angle_with_no_exact_sine() {
    let bad = SpecialTriangleFigure {
        figure: SpecialTriangleShape::SasArea {
            side_a: Scalar::from("4"),
            side_b: Scalar::from("5"),
            included_angle_degrees: Scalar::from("40"),
        },
        caption: None,
    };
    assert!(matches!(
        render(&VisualSpec::SpecialTriangle(bad), &RenderOptions::default()),
        Err(VisualError::Degenerate { .. })
    ));

    let good = SpecialTriangleFigure {
        figure: SpecialTriangleShape::SasArea {
            side_a: Scalar::from("4"),
            side_b: Scalar::from("5"),
            included_angle_degrees: Scalar::from("60"),
        },
        caption: None,
    };
    let svg = render(
        &VisualSpec::SpecialTriangle(good),
        &RenderOptions::default(),
    )
    .unwrap();
    assert!(svg.contains("The exact area is 5√3 square units."));
}
