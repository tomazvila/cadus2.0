//! Exhaustive production-worker verification of the Unit05 inequality checkpoint.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use common::template_batch::AnswerPolicy;

const PATHS: &[&str] = &["docs/content-foundations/unit05-inequalities/templates.json"];

crate::template_batch_tests!(
    PATHS,
    19,
    228,
    |_| 12,
    AnswerPolicy::LabelOrUnique,
    &["0", "a-a", "b-b", "-999"],
    "x <= 999"
);
