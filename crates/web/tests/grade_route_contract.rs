//! The persisted item policy owns its grade and event evidence (D-F1, C2).

#![allow(clippy::unwrap_used)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_store::test_support::TestDb;
use common::{
    LESSON, PROBLEM_ID, answer_task, events_of_type, lesson_app, lesson_learner, lesson_problem,
};
use serde_json::json;

#[tokio::test]
async fn a_multi_step_numeric_contract_is_graded_and_recorded() {
    TestDb::with(|db| async move {
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.answer_kind = Some("multi-step".to_owned());
        live.expected.answer = "12".to_owned();
        live.expected.answer_contract = Some(AnswerContract::Exact);
        let user = lesson_learner(&db, "contract-multistep@example.com", live).await;
        let app = lesson_app(&db);
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "24/2"}),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");
        assert_eq!(body["outcome"], "correct");
        let events = events_of_type(&db, user, "attempt").await;
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0]["problem"]["answer_contract"],
            json!({"kind": "exact"})
        );
    })
    .await;
}

#[tokio::test]
async fn the_captured_exact_policy_refuses_a_rounded_third() {
    TestDb::with(|db| async move {
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.expected.answer = "1/3".to_owned();
        live.expected.answer_contract = Some(AnswerContract::Exact);
        let user = lesson_learner(&db, "contract-exact@example.com", live).await;
        let app = lesson_app(&db);
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "0.3"}),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");
        assert_eq!(body["outcome"], "incorrect");
    })
    .await;
}

#[tokio::test]
async fn the_authored_decimal_count_controls_the_grade() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        for (index, answer, outcome) in [(0, "0.33", "correct"), (1, "0.339", "incorrect")] {
            let mut live = lesson_problem(5.0, "kp1", Vec::new());
            live.expected.answer = "1/3".to_owned();
            live.expected.answer_contract = Some(AnswerContract::Approx { decimals: 2 });
            let user =
                lesson_learner(&db, &format!("contract-approx-{index}@example.com"), live).await;
            let (status, body) = answer_task(
                &app,
                user,
                LESSON,
                json!({"problem_id": PROBLEM_ID, "answer": answer}),
            )
            .await;
            assert_eq!(status.as_u16(), 200, "{body}");
            assert_eq!(body["outcome"], outcome);
        }
    })
    .await;
}
