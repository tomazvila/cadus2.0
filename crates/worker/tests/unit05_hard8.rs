//! Exhaustive production-worker verification of the hard Unit05 checkpoint.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use common::template_batch::AnswerPolicy;

const PATHS: &[&str] = &[
    "docs/content-foundations/unit05-hard8/union-templates.json",
    "docs/content-foundations/unit05-hard8/boundary-graph-templates.json",
    "docs/content-foundations/unit05-hard8/degenerate-templates.json",
];

fn tuple_count(key: &str) -> usize {
    if key == "basic-absolute-value-inequalities/kp3" {
        16
    } else {
        12
    }
}

crate::template_batch_tests!(
    PATHS,
    7,
    88,
    tuple_count,
    AnswerPolicy::Unique,
    &["0", "a-a", "b-b", "-999"],
    "x <= 999"
);
