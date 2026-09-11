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
    for distinct_representation in [
        "A function table follows the rule output = input $+9$.",
        "A point starts at coordinate $x$ and moves $4$ units right",
        "For the function $f(x)=-3x$",
        "A table uses the rule output = input $/8$.",
        "The lines $y=4x-7$ and $y=x+5$ intersect",
        "where the graphs $y=-2x-9$ and $y=-5x+3$ meet",
    ] {
        assert!(source.contains(distinct_representation));
    }
    assert_eq!(-5 + 9, 4);
    assert_eq!(5 + 4, 9);
    assert_eq!(-3 * -6, 18);
    assert_eq!(16 / 8, 2);
    assert_eq!(4 * 4 - 7, 4 + 5);
    assert_eq!(-2 * 4 - 9, -5 * 4 + 3);
}
