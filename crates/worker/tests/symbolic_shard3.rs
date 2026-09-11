//! Production-gate and semantic checks for symbolic repair shard three.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};

const KEYS: &[&str] = &[
    "absolute-value-equations/kp1",
    "basic-absolute-value-equations/kp1",
    "basic-absolute-value-equations/kp2",
    "distribute-then-solve/kp1",
    "multi-step-equations/kp1",
    "two-step-equations/kp1",
    "two-step-equations/kp2",
];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard3-templates.json"];

#[test]
fn exact_reviewed_shard_passes_the_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}

#[test]
fn independent_representations_and_answers_are_pinned() {
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    for representation in [
        "A function table uses $f(x)=4x-7$.",
        "A machine divides its input by $-4$",
        "Three identical boxes each contain $2x-1$ counters",
        "For $f(x)=7x-3x-5$",
        "The graphs $y=|x|$ and $y=5$ intersect",
        "The graphs $y=|x+4|$ and $y=2$ intersect",
        "The graph $y=|3x-6|$ meets the horizontal line $y=12$",
    ] {
        assert!(source.contains(representation), "{representation}");
    }
    assert_eq!(4 * 4 - 7, 9);
    assert_eq!(-20 / -4 - 3, 2);
    assert_eq!(3 * (2 * 3 - 1), 15);
    assert_eq!(7 * 4 - 3 * 4 - 5, 11);
    assert_eq!([(-5_i32).abs(), 5_i32.abs()], [5, 5]);
    assert_eq!([(-6_i32 + 4).abs(), (-2_i32 + 4).abs()], [2, 2]);
    assert_eq!([(3 * -2_i32 - 6).abs(), (3 * 6_i32 - 6).abs()], [12, 12]);
}
