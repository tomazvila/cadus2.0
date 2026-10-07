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
    // Re-pinned 2026-10-07 to the courses rewrite: a grid graph, a table, a
    // displacement, the opposite subtraction order, and a budget line.
    for representation in [
        "A line passes through the grid points $(1,1)$ and $(3,4)$.",
        "two points listed in a table: $x=-2$, $y=1$",
        "A second point is $6$ units to the right of it and $3$ units higher.",
        "Ben subtracts in the opposite order and gets $\\frac{3-9}{1-4}$",
        "$4x+5y=20$",
    ] {
        assert!(source.contains(representation), "{representation}");
    }
    assert_eq!((4_f64 - 1.0) / (3.0 - 1.0), 1.5);
    assert_eq!((7_f64 - 1.0) / (1.0 - -2.0), 2.0);
    assert_eq!(3_f64 / 6.0, 0.5);
    assert_eq!((9_f64 - 3.0) / (4.0 - 1.0), (3_f64 - 9.0) / (1.0 - 4.0));
    assert_eq!(20 / 4, 5);
    assert_eq!(20 / 5, 4);
}
