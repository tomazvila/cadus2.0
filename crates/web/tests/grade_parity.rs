//! M5 U8 oracle parity: the deterministic tier over the whole 1.0 corpus.
//!
//! Requirements: A3, C4, R5, V3. `docs/plans/M5.md`, "Oracle parity": U8's
//! verdicts and tiers run against the 1.0 corpus through the M2 harness.
//!
//! # What this file adds, and what it does not repeat
//!
//! The VERDICT parity — does 2.0 answer `correct` the way 1.0's checker does —
//! is already proven by `crates/core/tests/answer_oracle.rs`, which compares
//! every generated learner spelling against verdicts recorded from the live 1.0
//! checker. U8 adds the layer above it: the TIER and the TAGS that 1.0's
//! `cadus_web/deterministic_grade.py` stamps on the same three classes of
//! submission. This file runs that layer over all 3,492 corpus answers.
//!
//! The three classes and the 1.0 rule each one is compared with:
//!
//! | Class | 1.0 | 2.0 |
//! |---|---|---|
//! | the authored answer, given back | `CORRECT_TIER = nearly_perfect`, no tag (`deterministic_grade.py:72`, `:143-149`) | the same |
//! | a blank submission | `blank_answer_grade()`: `poor`, tag `blank_answer` (`:103-104`) | `poor`, tag `blank-answer` (D-M5-7, trap W1) |
//! | a decided miss | `None` — 1.0 sends it to the model | `nearly_passable`, no tag (D-M5-2, D-M5-4) |
//!
//! The corpus file is the M2 fixture, read from its own crate, so one file
//! serves both milestones and the two cannot drift apart.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::event::WorkQuality;
use cadus_web::grade::deterministic_grade;
use common::{CORPUS_ROWS, corpus, kind_of};

/// A learner answer no authored answer of the corpus is. Every pair built with
/// it is a miss.
const SENTINEL_MISS: &str = "-987654321.125";

/// Class 1. The authored answer, given back by the learner, is a pass at the
/// NEUTRAL tier with no tag — 1.0's `CORRECT_TIER` and its empty tag list.
///
/// The string rung decides every one of these, so the count is the whole corpus
/// and not the 3,227 answers the grammar reads.
#[test]
fn every_authored_answer_grades_nearly_perfect_with_no_tag() {
    let corpus = corpus();
    assert_eq!(corpus.len(), CORPUS_ROWS, "the corpus is 3,492 answers");
    let mut passes = 0_usize;
    let mut wrong: Vec<&str> = Vec::new();
    for row in &corpus {
        let grade = deterministic_grade(&row.answer, &row.answer, kind_of(row));
        if grade.correct
            && grade.work_quality == WorkQuality::NearlyPerfect
            && grade.error_tags.is_empty()
        {
            passes += 1;
        } else {
            wrong.push(&row.answer);
        }
    }
    assert_eq!(
        passes,
        CORPUS_ROWS,
        "the first divergences are {:?}",
        wrong.iter().take(5).collect::<Vec<_>>()
    );
}

/// Class 2. A blank submission against any authored answer is `poor` with the
/// one tag, whatever the kind is. 1.0 spells the tag `blank_answer`; D-M5-7
/// ships the hyphenated spelling, because the 1.0 spelling is outside 1.0's own
/// vocabulary (trap W1).
#[test]
fn every_blank_submission_grades_poor_with_the_blank_answer_tag() {
    let corpus = corpus();
    let mut blanks = 0_usize;
    for row in &corpus {
        for submitted in ["", "   ", "\t\n"] {
            let grade = deterministic_grade(&row.answer, submitted, kind_of(row));
            assert!(!grade.correct, "{:?} vs {submitted:?}", row.answer);
            assert_eq!(grade.work_quality, WorkQuality::Poor, "{:?}", row.answer);
            assert_eq!(
                grade.error_tags,
                vec!["blank-answer".to_string()],
                "{:?}",
                row.answer
            );
            blanks += 1;
        }
    }
    assert_eq!(blanks, CORPUS_ROWS * 3);
}

/// Class 3. A miss is `nearly_passable` with NO tag (D-M5-2, D-M5-4).
///
/// The rule covers both misses 1.0 leaves to the model: the DECIDED miss, and
/// the answer that leaves the grammar, which 2.0 records as a miss and never as
/// a model verdict (spec section 5.1). No answer of the corpus matches the
/// sentinel, so the pass count is 0.
#[test]
fn every_missed_answer_grades_nearly_passable_with_no_tag() {
    let corpus = corpus();
    let mut passes = 0_usize;
    for row in &corpus {
        let grade = deterministic_grade(&row.answer, SENTINEL_MISS, kind_of(row));
        if grade.correct {
            passes += 1;
            continue;
        }
        assert_eq!(
            grade.work_quality,
            WorkQuality::NearlyPassable,
            "{:?}",
            row.answer
        );
        assert!(grade.error_tags.is_empty(), "{:?}", row.answer);
    }
    assert_eq!(passes, 0, "no authored answer is the sentinel");
}

/// The period-grouped reading is the ONE tag a correct verdict may carry, and it
/// is a claim about form, not about the mathematics (`deterministic_grade.py:146-149`).
#[test]
fn the_period_grouped_reading_is_the_only_tag_a_pass_carries() {
    let grade = deterministic_grade("7329", "7.329", cadus_core::curriculum::AnswerKind::Numeric);
    assert!(grade.correct);
    assert_eq!(grade.work_quality, WorkQuality::NearlyPerfect);
    assert_eq!(grade.error_tags, vec!["notation".to_string()]);
}
