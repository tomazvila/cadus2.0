//! Input presentation keeps the captured mathematics and source skill authoritative.
#![allow(clippy::unwrap_used, clippy::panic)]
mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_core::event::AttemptOutcome;
use cadus_store::test_support::TestDb;
use cadus_web::grade::grade_served_item;
use cadus_web::state::ServedProblem;
use common::{
    LESSON, PROBLEM_ID, answer_task, events_of_type, lesson_app, lesson_learner, lesson_problem,
};
use serde_json::json;

fn factors(expected: &str) -> ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.topic = Some("factors-and-multiples".to_owned());
    live.serve_topic = live.topic.clone();
    live.answer_kind = Some("expression".to_owned());
    live.text = "List all positive factors of the given number.".to_owned();
    live.expected.answer = expected.to_owned();
    live
}

#[test]
fn surrounding_whitespace_keeps_a_correct_expression_correct() {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.expected.answer = "-6x".to_owned();
    for answer in ["-6x", "  -6x  ", "\t-6x\n"] {
        let grade = grade_served_item(&live, answer, AnswerKind::Expression);
        assert_eq!(grade.outcome, AttemptOutcome::Correct, "{answer:?}");
    }
}

#[test]
fn an_equation_in_an_expression_field_has_actionable_nonrevealing_feedback() {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.expected.answer = "-6x".to_owned();
    let grade = grade_served_item(&live, "8x-2x=6x", AnswerKind::Expression);
    assert!(!grade.correct);
    let AttemptOutcome::Ungraded { reason } = grade.outcome else {
        panic!("An equation in an expression field must remain ungraded");
    };
    let lower = reason.to_lowercase();
    assert!(lower.contains("expression"), "{reason}");
    assert!(
        lower.contains("equation") || lower.contains("equals") || lower.contains('='),
        "{reason}"
    );
    assert!(!lower.contains("trailing"), "{reason}");
    assert!(!reason.contains("-6x"), "{reason}");
}

#[test]
fn complete_factor_pairs_accept_supported_symbols_and_reversed_order() {
    for contract in [
        None,
        Some(AnswerContract::Exact),
        Some(AnswerContract::List {
            ordered: false,
            member: Box::new(AnswerContract::Exact),
        }),
    ] {
        let mut live = factors("1,2,3,4,6,8,12,24");
        live.expected.answer_contract = contract;
        for answer in [
            "24x1,12x2,8x3,6x4",
            "1*24,2*12,3*8,4*6",
            "6×4,8×3,12×2,24×1",
            " 4 x 6, 3 x 8, 2 x 12, 1 x 24 ",
        ] {
            let grade = grade_served_item(&live, answer, AnswerKind::Expression);
            assert_eq!(grade.outcome, AttemptOutcome::Correct, "{answer:?}");
        }
    }
}

#[test]
fn factor_pair_representations_preserve_completeness_and_pair_products() {
    let live = factors("1,2,3,4,6,8,12,24");
    for answer in [
        "24x1,12x2,8x3",         // Missing both 4 and 6.
        "24x1,12x2,8x3,6x4,5x5", // Extra factors.
        "24x2,12x1,8x3,6x4",     // Complete factor set, wrong individual products.
        "24x1,12x2,8x3,6x4,0x24",
        "24x1,12x2,8x3,-6x-4",
        "24x1,12x2,8x3,6x4,1x24", // Duplicate unordered pair.
    ] {
        assert!(
            !grade_served_item(&live, answer, AnswerKind::Expression).correct,
            "{answer:?}"
        );
    }
}

#[test]
fn a_square_pair_contributes_its_repeated_factor_once() {
    let live = factors("1,2,3,4,6,9,12,18,36");
    assert!(grade_served_item(&live, "36x1,18x2,12x3,9x4,6x6", AnswerKind::Expression).correct);
    assert!(!grade_served_item(&live, "36x1,18x2,12x3,9x4", AnswerKind::Expression).correct);
}

#[test]
fn physical_source_skill_owns_factor_pair_eligibility() {
    let answer = "24x1,12x2,8x3,6x4";
    let mut live = factors("1,2,3,4,6,8,12,24");
    live.topic = Some("addition".to_owned());
    assert!(grade_served_item(&live, answer, AnswerKind::Expression).correct);
    live.topic = Some("factors-and-multiples".to_owned());
    live.serve_topic = Some("addition".to_owned());
    assert!(!grade_served_item(&live, answer, AnswerKind::Expression).correct);
    live.serve_topic = Some("factors-and-multiples".to_owned());
    live.kp = Some("kp2".to_owned());
    assert!(!grade_served_item(&live, answer, AnswerKind::Expression).correct);
}

#[test]
fn explicit_noncompatible_contracts_keep_their_original_policy() {
    for contract in [
        AnswerContract::List {
            ordered: true,
            member: Box::new(AnswerContract::Exact),
        },
        AnswerContract::None,
        AnswerContract::RequiredAssignment,
    ] {
        let mut live = factors("1,2,3,4,6,8,12,24");
        live.expected.answer_contract = Some(contract);
        assert!(!grade_served_item(&live, "24x1,12x2,8x3,6x4", AnswerKind::Expression).correct);
    }
}

#[tokio::test]
async fn grading_factor_pairs_preserves_the_original_submitted_event_answer() {
    TestDb::with(|db| async move {
        let mut live = factors("1,2,3,4,6,8,12,24");
        // The task records addition, while this mixed item physically sources factors/kp1.
        live.topic = Some("addition".to_owned());
        let user = lesson_learner(&db, "factor-pair-evidence@example.com", live).await;
        let app = lesson_app(&db);
        let original = "24x1,12x2,8x3,6x4";
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": original}),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");
        assert_eq!(body["outcome"], "correct");
        let attempts = events_of_type(&db, user, "attempt").await;
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0]["given_answer"], original);
        assert_eq!(attempts[0]["problem"]["expected"], "1,2,3,4,6,8,12,24");
    })
    .await;
}
