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
#[ignore]
async fn a_last_answer_that_disagrees_with_score_queues_targeted_confirmation() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner_at_review_index(&db, "review-inconclusive@example.com", 0).await;
        seed_variety(&db, user).await;
        assert_eq!(serve_task(&app, user, REVIEW).await.0, StatusCode::OK);
        let mut last = json!({});
        for correct in [false, false, false, true] {
            last = original_then_practice(&app, &db, user, correct).await;
        }
        assert_eq!(last["task_status"], "task_inconclusive");
        assert_eq!(last["xp"], 0.0);
        let closes = events_of_type(&db, user, "review_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["inconclusive"], true);
        assert_eq!(closes[0]["weighted_score"], 0.4);
        let attempts = events_of_type(&db, user, "attempt").await;
        assert_eq!(attempts.len(), 7);
        assert_eq!(
            attempts
                .iter()
                .filter(|event| event["feedback_practice"] == true)
                .count(),
            3
        );
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

async fn seed_variety(db: &TestDb, user: sqlx::types::Uuid) {
    for kp in ["kp1", "kp2"] {
        for index in 0..8 {
            common::seed_pool_row(
                db,
                user,
                &format!("addition/{kp}"),
                &format!("Give {kp} value {index}."),
                &index.to_string(),
                &format!("fresh-{kp}-{index}"),
            )
            .await;
        }
    }
}

async fn original_then_practice(
    app: &axum::Router,
    db: &TestDb,
    user: sqlx::types::Uuid,
    correct: bool,
) -> serde_json::Value {
    let problem = stored_state(db, user).await.served[REVIEW].clone();
    let given = if correct {
        problem.expected.answer
    } else {
        "99999".to_owned()
    };
    let (status, reply) = answer_task(
        app,
        user,
        REVIEW,
        json!({"problem_id":problem.problem_id,"answer":given}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    let mut last = reply;
    if last["feedback_practice"] == true {
        let before = stored_state(db, user).await.tasks[REVIEW].answered;
        let fresh = stored_state(db, user).await.served[REVIEW].clone();
        assert_eq!(fresh.kp, problem.kp);
        assert_ne!(fresh.text, problem.text);
        let (status, practice) = answer_task(
            app,
            user,
            REVIEW,
            json!({"problem_id":fresh.problem_id,"answer":fresh.expected.answer}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{practice}");
        assert_eq!(stored_state(db, user).await.tasks[REVIEW].answered, before);
        last = practice;
    }
    last
}

#[tokio::test]
#[ignore]
async fn a_final_review_miss_closes_only_after_practice_without_changing_its_score() {
    TestDb::with(|db| async move {
        let user = learner_at_review_index(&db, "review-last-miss@example.com", 0).await;
        let app = lesson_app(&db);
        seed_variety(&db, user).await;
        assert_eq!(serve_task(&app, user, REVIEW).await.0, StatusCode::OK);
        let mut last = json!({});
        for correct in [true, true, true, false] {
            last = original_then_practice(&app, &db, user, correct).await;
        }
        assert_eq!(last["correct"], true);
        assert_eq!(last["task_status"], "task_failed");
        let closes = events_of_type(&db, user, "review_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], false);
        assert_eq!(closes[0]["weighted_score"], 0.6);
        assert_eq!(stored_state(&db, user).await.tasks[REVIEW].answered, 4);
    })
    .await;
}
