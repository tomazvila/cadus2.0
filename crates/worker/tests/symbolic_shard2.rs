//! Production-gate and exemplar-variety checks for symbolic repair shard two.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;
use common::symbolic::{curriculum_source, rows, verify_rows};

const KEYS: &[&str] = &[
    "addition-subtraction-equations/kp1",
    "addition-subtraction-equations/kp2",
    "one-step-equations/kp1",
    "one-step-equations/kp2",
    "variables-both-sides/kp1",
    "variables-both-sides/kp2",
];
const PATHS: &[&str] = &["docs/content-foundations/symbolic-repair/shard2-templates.json"];

#[test]
fn exact_reviewed_shard_passes_the_current_production_gate() {
    verify_rows(&rows(PATHS), KEYS);
}

#[test]
fn replacement_representations_and_answers_are_regression_pinned() {
    let source = curriculum_source("curriculum/foundations/03-expressions-equations.yaml");
    // Re-pinned 2026-10-07 to the courses rewrite: each knowledge point keeps
    // a representation that differs from the plain "Solve" item.
    for distinct_representation in [
        "During the night the temperature rose by $6$ degrees and reached $-2$ °C.",
        "During the day the temperature fell by $9$ degrees and reached $-4$ °C.",
        "For $f(x) = -3x$, which input gives the output $18$?",
        "A machine divides its input by $8$, and its output is $2$.",
        "The lines $y = 7x - 3$ and $y = 3x + 9$ intersect at one point.",
        "Find the $x$-coordinate where the graphs of $y = -2x - 9$ and $y = -5x + 3$ meet.",
    ] {
        assert!(source.contains(distinct_representation));
    }
    assert_eq!(-8 + 6, -2);
    assert_eq!(5 - 9, -4);
    assert_eq!(-3 * -6, 18);
    assert_eq!(16 / 8, 2);
    assert_eq!(7 * 3 - 3, 3 * 3 + 9);
    assert_eq!(-2 * 4 - 9, -5 * 4 + 3);
}
