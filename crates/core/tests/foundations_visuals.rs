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
    // 2026-10-07, courses rewrite: the 97 knowledge-point figures became 95 (42
    // knowledge-point figures and 53 item-level figures) in 51 knowledge points
    // (was 153 entries and 164 figures). The manifest is the dump of the tree.
    assert_eq!(entries.len(), 51);
    assert_eq!(
        cadus_core::curriculum::canonical_dump(&curriculum)
            .matches("\"visuals\"")
            .count(),
        51
    );
    let mut count = 0;
    let mut broken: Vec<String> = Vec::new();
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
        assert_eq!(
            report.get(&entry.kp_id).unwrap().broken_visuals,
            0,
            "{}",
            entry.kp_id
        );
        for visual in &kp.visuals {
            if let Err(error) = visual.validate() {
                broken.push(format!("{}: {error:?}", entry.kp_id));
                continue;
            }
            let text = visual.text_equivalent();
            assert!(!text.is_empty());
            // Superseded by the courses rewrite (2026-10-07): figures belong to
            // items now, and the caption no longer opens with "Reference example:".
            assert!(!visual.caption().unwrap().trim().is_empty());
            let options = RenderOptions::with_prefix(&entry.kp_id);
            let svg = render(visual, &options).unwrap();
            assert_eq!(svg, render(visual, &options).unwrap());
            assert!(svg.contains("role=\"img\""));
            assert!(svg.contains("<desc"));
            families.insert(visual.kind());
            count += 1;
        }
    }
    assert_eq!(broken, Vec::<String>::new(), "figures that fail validation");
    assert_eq!(count, 95);
    assert_eq!(families.len(), 5);
}

#[test]
fn every_reference_segment_joins_two_points_inside_its_axes() {
    // Superseded by the courses rewrite (2026-10-07): the old test matched each
    // caption to a stated equation, and no caption states one now. Each segment
    // must join two whole-number points that the axes contain.
    let mut checked = 0;
    for entry in entries() {
        for visual in entry.visuals {
            let VisualSpec::Coordinate(figure) = visual else {
                continue;
            };
            let parse = |value: &str| value.parse::<i64>().unwrap();
            let (x_min, x_max) = (parse(figure.x_min.as_str()), parse(figure.x_max.as_str()));
            let (y_min, y_max) = (parse(figure.y_min.as_str()), parse(figure.y_max.as_str()));
            for segment in figure.segments {
                for point in [&segment.from, &segment.to] {
                    let (x, y) = (parse(point.x.as_str()), parse(point.y.as_str()));
                    assert!(
                        (x_min..=x_max).contains(&x) && (y_min..=y_max).contains(&y),
                        "{}: a segment end lies outside the axes",
                        entry.kp_id
                    );
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 0);
}

#[test]
fn every_shaded_half_plane_has_a_boundary_and_a_shaded_side() {
    // Superseded by the courses rewrite (2026-10-07): the one reference figure
    // `graphing-linear-inequalities/kp3` is gone, and no figure of the tree
    // shades a half plane. A figure that does must name two boundary points and
    // a shade point that is not on the boundary.
    for entry in entries() {
        for visual in entry.visuals {
            let VisualSpec::Coordinate(figure) = visual else {
                continue;
            };
            for half_plane in &figure.shaded_half_planes {
                let parse = |value: &str| value.parse::<i64>().unwrap();
                let (ax, ay) = (
                    parse(half_plane.through_a.x.as_str()),
                    parse(half_plane.through_a.y.as_str()),
                );
                let (bx, by) = (
                    parse(half_plane.through_b.x.as_str()),
                    parse(half_plane.through_b.y.as_str()),
                );
                let (sx, sy) = (
                    parse(half_plane.shade_toward.x.as_str()),
                    parse(half_plane.shade_toward.y.as_str()),
                );
                assert_ne!((ax, ay), (bx, by), "{}", entry.kp_id);
                let side = (bx - ax) * (sy - ay) - (by - ay) * (sx - ax);
                assert_ne!(
                    side, 0,
                    "{}: the shade point is on the boundary",
                    entry.kp_id
                );
            }
        }
    }
}
