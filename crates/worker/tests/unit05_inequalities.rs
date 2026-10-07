//! Exhaustive production-worker verification of the Unit05 inequality checkpoint.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use common::template_batch::AnswerPolicy;

const PATHS: &[&str] = &["docs/content-foundations/unit05-inequalities/templates.json"];

// 19 reviewed rows of 12 instances each; two were retired on 2026-10-05
// and eight more on 2026-10-07 (courses rewrite)
// (`docs/reports/unit05-inequalities-retired-pending-templates.json`).
crate::template_batch_tests!(
    PATHS,
    9,
    108,
    |_| 12,
    AnswerPolicy::LabelOrUnique,
    &["0", "a-a", "b-b", "-999"],
    "x <= 999"
);

#[test]
fn the_retired_rows_left_the_checkpoint_and_keep_their_gate_refusal() {
    let pending = common::template_batch::read_rows(PATHS)
        .iter()
        .map(|row| row["kp_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        common::retired::assert_retired("unit05-inequalities", PATHS, &pending),
        10
    );
}
