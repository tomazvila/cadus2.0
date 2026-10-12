//! Exhaustive production-worker verification of the first Unit05 residual checkpoint.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use common::template_batch::AnswerPolicy;

const PATHS: &[&str] = &["docs/content-foundations/unit05-residual/templates.json"];

crate::template_batch_tests!(
    PATHS,
    // 8 rows and 96 problems before the Foundations merge (2026-10-12), which
    // dropped elimination-with-addition/kp3.
    7,
    84,
    |_| 12,
    AnswerPolicy::Unique,
    &["(0,0)", "(a-a,b-b)", "(a,0)", "(-a,-b)"],
    "(999,999)"
);
