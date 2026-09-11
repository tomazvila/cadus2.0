//! Terminal lesson failure preserves one result while fresh practice remains available.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use common::{
    LESSON, answer_lesson_ok, answer_task, app_with_content, call, events_of_type, exemplar, kp,
    learner_at_the_fifth_miss, one_unit_curriculum, parse, serve_task, stored_state, topic,
};
use serde_json::json;

#[tokio::test]
async fn a_failed_lesson_finishes_fresh_practice_without_a_second_result() {
    TestDb::with(|db| async move {
        let app = app_with_content(
            &db,
            one_unit_curriculum(vec![topic(
                "addition",
                vec![kp(
                    "kp1",
                    vec![
                        exemplar(common::PROBLEM_TEXT, "13.5"),
                        exemplar("Compute 70 + 4.", "74"),
                    ],
                )],
            )]),
        );
        let user = learner_at_the_fifth_miss(&db, "terminal-practice@example.com").await;
        let failed = answer_lesson_ok(&app, user, "999").await;
        assert_eq!(failed["task_status"], "task_failed");
        assert_eq!(failed["feedback_practice"], true);
        let original = events_of_type(&db, user, "lesson_result").await;
        let remediation = events_of_type(&db, user, "remediation_triggered").await;
        assert_eq!(original.len(), 1);
        assert_eq!(original[0]["passed"], false);
        assert!(!stored_state(&db, user).await.tasks[LESSON].done);
        // A replan/reload retains the pending task after the failed lesson close.
        let (status, raw) = call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert!(
            parse(&raw)["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .any(|task| task["task_id"] == LESSON)
        );
        let (status, fresh) = serve_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{fresh}");
        assert_eq!(fresh["text"], "Compute 70 + 4.");
        let (status, completed) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id":fresh["problem_id"],"answer":"74"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{completed}");
        assert_eq!(completed["correct"], true);
        assert_eq!(completed["task_status"], "task_failed");
        assert!(completed.get("xp").is_none());
        assert!(stored_state(&db, user).await.tasks[LESSON].done);
        assert_eq!(events_of_type(&db, user, "lesson_result").await, original);
        assert_eq!(
            events_of_type(&db, user, "remediation_triggered").await,
            remediation
        );
        let attempts = events_of_type(&db, user, "attempt").await;
        assert_eq!(attempts.last().unwrap()["independent_after_feedback"], true);
    })
    .await;
}
