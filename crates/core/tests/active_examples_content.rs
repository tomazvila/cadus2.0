//! Step 5a content: the authored active worked examples of the tree.
//!
//! The loader already refuses a malformed block. This file pins what the
//! loader cannot see: each try-first answer grades correct under its own
//! contract, and a plausible miss grades incorrect.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::{Outcome, check_contract};
use common::paths::tree;

/// A wrong answer of the same shape as `answer`.
fn plausible_miss(answer: &str) -> String {
    match answer {
        "yes" => "no".to_owned(),
        "0" => "1".to_owned(),
        "{-23,23}" => "{23}".to_owned(),
        other => format!("{other}+1"),
    }
}

#[test]
fn every_try_first_grades_its_own_answer_and_refuses_a_miss() {
    let graph = tree();
    let mut seen = 0;
    for topic in graph.topics() {
        for point in &topic.knowledge_points {
            let Some(first) = &point.try_first else {
                continue;
            };
            seen += 1;
            let key = format!("{}/{}", topic.id, point.id);
            assert!(
                matches!(
                    check_contract(&first.answer, &first.answer, first.answer_contract.clone()),
                    Outcome::Decided(verdict) if verdict.correct
                ),
                "{key}: the try-first answer does not grade correct"
            );
            let miss = plausible_miss(&first.answer);
            assert!(
                matches!(
                    check_contract(&first.answer, &miss, first.answer_contract.clone()),
                    Outcome::Decided(verdict) if !verdict.correct
                ),
                "{key}: the miss {miss:?} does not grade incorrect"
            );
        }
    }
    // 627 on 2026-10-07 after the courses rewrite (was 44).
    assert_eq!(seen, 627, "the authored try-first count moved");
}
