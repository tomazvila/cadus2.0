//! Exhaustive validation of the independently authored Foundations reference figures.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use cadus_core::{
    curriculum::load_curriculum,
    readiness::{EmptyContent, ReadinessIndex},
    visual::{RenderOptions, VisualSpec, render},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Entry {
    kp_id: String,
    visuals: Vec<VisualSpec>,
}

fn entries() -> Vec<Entry> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_str(
        &std::fs::read_to_string(
            root.join("docs/content-visuals/foundations-reference-manifest.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn every_manifest_figure_is_installed_valid_accessible_and_deterministic() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let entries = entries();
    let (curriculum, findings) = load_curriculum(&root.join("curriculum")).unwrap();
    assert!(findings.is_empty(), "{findings:?}");
    let report = ReadinessIndex::build(&curriculum).resolve(&EmptyContent);
    assert_eq!(entries.len(), 153);
    assert_eq!(
        cadus_core::curriculum::canonical_dump(&curriculum)
            .matches("\"visuals\"")
            .count(),
        153
    );
    let mut count = 0;
    let mut families = std::collections::BTreeSet::new();
    for entry in entries {
        let (topic, kp) = entry.kp_id.split_once('/').unwrap();
        let topic = curriculum.topic(curriculum.idx_of(topic).unwrap()).unwrap();
        let kp = topic
            .knowledge_points
            .iter()
            .find(|point| point.id.as_str() == kp)
            .unwrap();
        assert_eq!(kp.visuals, entry.visuals, "{}", entry.kp_id);
        let readiness = report.get(&entry.kp_id).unwrap();
        assert!(readiness.visual_present);
        assert_eq!(readiness.broken_visuals, 0);
        for visual in &kp.visuals {
            visual.validate().unwrap();
            let text = visual.text_equivalent();
            assert!(!text.is_empty());
            assert!(visual.caption().unwrap().starts_with("Reference example:"));
            let options = RenderOptions::with_prefix(&entry.kp_id);
            let svg = render(visual, &options).unwrap();
            assert_eq!(svg, render(visual, &options).unwrap());
            assert!(svg.contains("role=\"img\""));
            assert!(svg.contains("<desc"));
            families.insert(visual.kind());
            count += 1;
        }
    }
    assert_eq!(count, 164);
    assert_eq!(families.len(), 6);
    let mut blocked = 0;
    for topic in curriculum.topics() {
        if curriculum.course_of(curriculum.idx_of(topic.id.as_str()).unwrap()) != "foundations" {
            continue;
        }
        for kp in &topic.knowledge_points {
            let state = report.get(&format!("{}/{}", topic.id, kp.id)).unwrap();
            blocked += usize::from(state.visual_needed && !state.visual_present);
        }
    }
    assert_eq!(blocked, 0);
    let money_setup = report.get("money-geometry-problems/kp1").unwrap();
    assert!(!money_setup.visual_needed);
    assert!(!money_setup.visual_present);
}

#[test]
fn every_reference_segment_satisfies_its_stated_equation() {
    let entries = entries();
    let mut checked = 0;
    for entry in entries {
        for visual in entry.visuals {
            let VisualSpec::Coordinate(figure) = visual else {
                continue;
            };
            let caption = figure.caption.as_deref().unwrap();
            let equations: Vec<(i64, i64, i64)> = if caption.contains("slope product -1") {
                vec![(-3, 2, 2), (2, 3, 3)]
            } else if caption.contains("y = (3/2)x - 2") {
                vec![(-3, 2, 2), (-3, 2, -4)]
            } else if caption.contains("intersect at I") {
                vec![(-1, 1, 2), (1, 1, 4)]
            } else if caption.contains("y = -(3/2)x + 2") {
                vec![(3, 2, 4)]
            } else if caption.contains("y = (3/2)x + 1") {
                vec![(-3, 2, 2)]
            } else if caption.contains("y = 3x") {
                vec![(-3, 1, 0)]
            } else if caption.contains("equation y = 2") {
                vec![(0, 1, 2)]
            } else if caption.contains("equation x = 2") {
                vec![(1, 0, 2)]
            } else if caption.contains("share x = 1") {
                vec![(1, 0, 1)]
            } else if caption.contains("closed segment") {
                vec![(-1, 1, 1)]
            } else {
                assert!(figure.segments.is_empty(), "unverified equation: {caption}");
                vec![]
            };
            for segment in figure.segments {
                let endpoints = [&segment.from, &segment.to].map(|point| {
                    (
                        point.x.as_str().parse::<i64>().unwrap(),
                        point.y.as_str().parse::<i64>().unwrap(),
                    )
                });
                assert!(
                    equations
                        .iter()
                        .any(|(a, b, c)| endpoints.iter().all(|(x, y)| a * x + b * y == *c)),
                    "{}: {caption}",
                    entry.kp_id
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 60);
}

#[test]
fn linear_inequality_reference_shades_the_satisfying_side() {
    let entry = entries()
        .into_iter()
        .find(|entry| entry.kp_id == "graphing-linear-inequalities/kp3")
        .unwrap();
    let VisualSpec::Coordinate(figure) = &entry.visuals[0] else {
        panic!("graphing-linear-inequalities/kp3 must use a coordinate figure");
    };
    let half_plane = &figure.shaded_half_planes[0];

    let parse = |value: &str| value.parse::<i64>().unwrap();
    for point in [&half_plane.through_a, &half_plane.through_b] {
        let x = parse(point.x.as_str());
        let y = parse(point.y.as_str());
        assert_eq!(y, x - 1, "boundary point must satisfy y = x - 1");
    }
    let shade_x = parse(half_plane.shade_toward.x.as_str());
    let shade_y = parse(half_plane.shade_toward.y.as_str());
    let satisfies = |x: i64, y: i64| {
        let boundary_y = x.checked_sub(1).unwrap();
        y <= boundary_y
    };
    assert!(
        satisfies(shade_x, shade_y),
        "shade point must satisfy y ≤ x - 1"
    );
    assert!(!satisfies(0, 0), "the origin must fail y ≤ x - 1");
    assert!(half_plane.solid, "the inclusive boundary must be solid");
    assert_eq!(half_plane.label.as_deref(), Some("y ≤ x - 1"));
    assert_eq!(
        figure.caption.as_deref(),
        Some(
            "Reference example: the origin fails y ≤ x - 1. Shade the side containing (0,-2), which satisfies the inequality."
        )
    );
}
