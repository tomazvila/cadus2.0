//! Exhaustive production-worker verification of the second Unit05 residual checkpoint.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use common::template_batch::AnswerPolicy;

const PATHS: &[&str] = &["docs/content-foundations/unit05-residual/templates-second.json"];

crate::template_batch_tests!(
    PATHS,
    9,
    108,
    |_| 12,
    AnswerPolicy::Unique,
    &[
        "(0,0)",
        "(a-a,b-b)",
        "(a,0)",
        "(-a,-b)",
        "0",
        "a-a+b-b",
        "a"
    ],
    "999"
);
