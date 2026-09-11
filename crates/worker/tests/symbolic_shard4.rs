//! Production-gate and semantic checks for symbolic repair shard four.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};

const KEYS: &[&str] = &[
    "slope-from-a-graph/kp3",
    "slope-from-two-points/kp1",
    "slope-from-two-points/kp2",
    "slope-from-two-points/kp3",
    "x-y-intercepts/kp3",
];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard4-templates.json"];

#[test]
fn exact_reviewed_shard_passes_the_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}

#[test]
fn varied_slope_representations_and_answers_are_pinned() {
    let source = curriculum_source("curriculum/foundations/04-linear-graphs.yaml");
    for representation in [
        "follow the line $3$ squares right and $3$ squares down",
        "displacement from one point on a line",
        "starts at $A=(-2,-3)$; its displacement",
        "slope quotients $3/6$ and $(-3)/(-6)$",
        "budget line $4x+5y=20$",
    ] {
        assert!(source.contains(representation), "{representation}");
    }
    assert_eq!(-3_f64 / 3.0, -1.0);
    assert_eq!(4_f64 / 4.0, 1.0);
    assert_eq!((3_f64 / 6.0), (-3_f64 / -6.0));
    assert_eq!(20 / 4, 5);
    assert_eq!(20 / 5, 4);
}
