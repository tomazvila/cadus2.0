//! C5: an Undecidable answer teaches the format and is never a wrong verdict.
//!
//! H-1 taught the unit refusals to name the unit; C5 extends that pattern to
//! every other reason the checker can refuse a learner answer with. This file
//! proves the two invariants at the route (D-F2): the reply carries the
//! actionable format prompt, and the attempt lands in the ungraded path — no
//! `correct = false` verdict event, no mastery damage, no XP change.

#![allow(clippy::unwrap_used)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_store::test_support::TestDb;
use common::{
    LESSON, PROBLEM_ID, answer_task, events_of_type, lesson_app, lesson_learner, lesson_problem,
};
use serde_json::json;

/// A learner whose live problem is a contract-KP with an Exact policy on a
/// fraction, the shape a reviewed Foundations item carries.
async fn contract_learner(db: &TestDb, email: &str) -> sqlx::types::Uuid {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.expected.answer = "1/3".to_owned();
    live.expected.answer_contract = Some(AnswerContract::Exact);
    lesson_learner(db, email, live).await
}

/// One unparseable answer on a contract-KP: the reply is the format prompt and
/// the attempt event is ungraded, never a wrong verdict.
#[tokio::test]
async fn an_unparseable_answer_teaches_the_format_and_never_grades_wrong() {
    TestDb::with(|db| async move {
        let user = contract_learner(&db, "undecidable@example.com").await;
        let app = lesson_app(&db);

        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "1/3 ???"}),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");

        // The reply is the ungraded path with the actionable format prompt.
        assert_eq!(body["outcome"], "ungraded");
        let reason = body["reason"].as_str().unwrap().to_string();
        assert!(
            reason.contains("Write the fraction as a/b"),
            "the prompt must name the expected format: {reason}"
        );
        // No correctness claim and no answer reveal (Hard Rule 1, D-F2).
        assert!(body.get("correct").is_none(), "{body}");
        assert!(body.get("solution").is_none(), "{body}");

        // The attempt event carries the SAME ungraded outcome, never a wrong
        // verdict: `correct` stays derivable-false but the outcome is the
        // refusal, and no `incorrect` outcome exists anywhere in the log.
        let events = events_of_type(&db, user, "attempt").await;
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]["outcome"]["ungraded"]["reason"].as_str().unwrap(),
            reason
        );
        assert!(
            events_of_type(&db, user, "lesson_result").await.is_empty(),
            "an ungraded attempt closes no lesson"
        );
    })
    .await;
}

/// A unitless answer to a unit-shaped authored answer still names its unit
/// (H-1, unchanged), and a plain miss on the contract-KP still grades wrong —
/// the C5 prompts never loosened the decided verdicts.
#[tokio::test]
async fn the_decided_verdicts_are_untouched() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);

        // H-1 unchanged: `30` against `30°` (no contract) is the ungraded
        // refusal that names the unit to write.
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.expected.answer = "30°".to_owned();
        let user = lesson_learner(&db, "unit@example.com", live).await;
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "30"}),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");
        assert_eq!(body["outcome"], "ungraded");
        assert!(
            body["reason"]
                .as_str()
                .unwrap()
                .contains("unit °, for example 42°"),
            "{body}"
        );

        // A decided miss is still a miss: the contract-KP answers `1/4` against
        // `1/3` with `incorrect`.
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.expected.answer = "1/3".to_owned();
        live.expected.answer_contract = Some(AnswerContract::Exact);
        let user = lesson_learner(&db, "wrong@example.com", live).await;
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "1/4"}),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");
        assert_eq!(body["outcome"], "incorrect");
    })
    .await;
}
