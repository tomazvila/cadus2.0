//! Step 5a content: the authored active worked examples of the tree.
//!
//! The loader already refuses a malformed block. This file pins what the
//! loader cannot see: each try-first answer grades correct under its own
//! contract, a plausible miss grades incorrect, and the ten authored topics
//! carry their blocks.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::answer::{Outcome, check_contract};
use common::paths::tree;

/// The owner's next ten Foundations lessons, each authored with a step check
/// on every knowledge point.
const AUTHORED: [&str; 10] = [
    "divisibility-rules",
    "prime-composite-numbers",
    "prime-factorization",
    "translating-phrases-to-expressions",
    "translating-sentences-to-equations",
    "basic-absolute-value-equations",
    "equation-word-problems",
    "absolute-value-equations",
    "consecutive-integer-problems",
    "equations-special-cases",
];

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
    assert_eq!(seen, 5, "the authored try-first count moved");
}

#[test]
fn the_ten_authored_topics_carry_a_step_check_on_every_point() {
    let graph = tree();
    for id in AUTHORED {
        let idx = graph
            .idx_of(id)
            .unwrap_or_else(|| panic!("{id} is in the tree"));
        for point in graph.knowledge_points(idx) {
            assert!(
                point.step_check.is_some(),
                "{id}/{} has no step check",
                point.id
            );
        }
    }
}
