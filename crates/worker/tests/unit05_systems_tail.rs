//! Pending Unit05 source content passes the production authoring gate exhaustively.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use common::template_batch::AnswerPolicy;

const PATHS: &[&str] = &[
    "docs/content-foundations/unit05-systems-tail/mixtures.json",
    "docs/content-foundations/unit05-systems-tail/checking.json",
    "docs/content-foundations/unit05-systems-tail/special.json",
    "docs/content-foundations/unit05-systems-tail/regions.json",
];

fn tuple_count(key: &str) -> usize {
    match key {
        "checking-systems-solutions/kp2" => 22,
        "systems-special-cases/kp2" | "systems-special-cases/kp3" => 24,
        _ => 12,
    }
}

crate::template_batch_tests!(
    PATHS,
    13,
    190,
    tuple_count,
    AnswerPolicy::Adversarial,
    &[
        "(0,0)",
        "(a-a,b-b)",
        "(a,0)",
        "(-a,-b)",
        "a-a+b-b",
        "multipart((a-a,b-b),equalitylabel(a,a))",
        "multipart(equalitylabel(a,a),equalitylabel(b,b))",
        "multipart((a-a,b-b,1),(1,1,-1))",
    ],
    "999"
);
