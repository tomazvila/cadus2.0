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
        // Re-pinned 2026-10-07 to the courses rewrite wording.
        "For $f(x) = 5x - 7$, which input gives the output $18$?",
        "A machine divides its input by $-4$",
        "Three identical boxes each hold $2x-1$ counters",
        "The function $f(x)=7x-3x-5$ has the output $15$.",
        "At which $x$-coordinates does the graph of $y = |x|$ meet the line $y = 5$?",
        "At which $x$-coordinates does the graph of $y = |x + 4|$ meet the line $y = 2$?",
        "At which $x$-coordinates does the graph of $y = |3x - 6|$ meet the line $y = 12$?",
    ] {
        assert!(source.contains(representation), "{representation}");
    }
    assert_eq!(5 * 5 - 7, 18);
    assert_eq!(-20 / -4 - 3, 2);
    assert_eq!(3 * (2 * 3 - 1), 15);
    assert_eq!(7 * 5 - 3 * 5 - 5, 15);
    assert_eq!([(-5_i32).abs(), 5_i32.abs()], [5, 5]);
    assert_eq!([(-6_i32 + 4).abs(), (-2_i32 + 4).abs()], [2, 2]);
    assert_eq!([(3 * -2_i32 - 6).abs(), (3 * 6_i32 - 6).abs()], [12, 12]);
}
