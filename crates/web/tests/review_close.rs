//! Reviews persist their actual close and their uncertain skills.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::{
    REVIEW, answer_task, events_of_type, learner_at_review_index, lesson_app, serve_task,
    stored_state,
};
use serde_json::json;

#[tokio::test]
async fn completed_review_has_one_close_and_persists_done() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner_at_review_index(&db, "review-close@example.com", 0).await;
        let (status, _) = serve_task(&app, user, REVIEW).await;
        assert_eq!(status, StatusCode::OK);
        let mut last = json!({});
        for _ in 0..4 {
            let problem = stored_state(&db, user).await.served[REVIEW].clone();
            let (status, reply) = answer_task(
                &app,
                user,
                REVIEW,
                json!({"problem_id":problem.problem_id,"answer":problem.expected.answer}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{reply}");
            last = reply;
        }
        assert_eq!(last["task_status"], "task_passed");
        let closes = events_of_type(&db, user, "review_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], true);
        assert_eq!(closes[0]["task_id"], REVIEW);
        assert!(closes[0].get("inconclusive").is_none());
        assert!(stored_state(&db, user).await.tasks[REVIEW].done);
    })
    .await;
}

#[tokio::test]
async fn a_last_answer_that_disagrees_with_score_queues_targeted_confirmation() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner_at_review_index(&db, "review-inconclusive@example.com", 0).await;
        assert_eq!(serve_task(&app, user, REVIEW).await.0, StatusCode::OK);
        let mut last = json!({});
        for correct in [false, false, false, true] {
            let problem = stored_state(&db, user).await.served[REVIEW].clone();
            let given = if correct {
                problem.expected.answer
            } else {
                "99999".to_owned()
            };
            let (status, reply) = answer_task(
                &app,
                user,
                REVIEW,
                json!({"problem_id":problem.problem_id,"answer":given}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{reply}");
            last = reply;
        }
        assert_eq!(last["task_status"], "task_inconclusive");
        assert_eq!(last["xp"], 0.0);
        let closes = events_of_type(&db, user, "review_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["inconclusive"], true);
        assert!(
            !closes[0]["confirmation_skills"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let followups = events_of_type(&db, user, "remediation_triggered").await;
        assert!(!followups.is_empty());
        assert!(followups.iter().all(|row| {
            row["kind"]
                .as_str()
                .unwrap()
                .starts_with("review_confirmation:")
        }));
    })
    .await;
}
