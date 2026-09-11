//! Actual drill close emits one replayable completion after supplemental practice.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::{
    DRILL, answer_task, answer_task_ok, drill_app, events_of_type, seed_drill_due, seed_learner,
    seed_open_session, serve_task, stored_state,
};
use serde_json::json;

#[tokio::test]
async fn a_drill_records_one_completion_after_its_final_independent_answer() {
    TestDb::with(|db| async move {
        let app = drill_app(&db);
        let user = seed_learner(&db, "drill-close@example.com").await;
        seed_open_session(&db, user).await;
        seed_drill_due(&db, user).await;
        for index in 0..20 {
            let (status, served) = serve_task(&app, user, DRILL).await;
            assert_eq!(status, StatusCode::OK, "{served}");
            let live = stored_state(&db, user).await.served[DRILL].clone();
            let answer = if index == 19 {
                "999999".to_owned()
            } else {
                live.expected.answer
            };
            let (status, reply) = answer_task(
                &app,
                user,
                DRILL,
                json!({"problem_id":live.problem_id,"answer":answer}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{reply}");
            assert!(events_of_type(&db, user, "drill_result").await.is_empty());
        }
        let fresh = stored_state(&db, user).await.served[DRILL].clone();
        let done = answer_task_ok(
            &app,
            user,
            DRILL,
            json!({"problem_id":fresh.problem_id,"answer":fresh.expected.answer}),
        )
        .await;
        assert_eq!(done["correct"], true);
        let results = events_of_type(&db, user, "drill_result").await;
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["task_id"], DRILL);
        assert!(results[0].get("xp").is_none());
        assert!(stored_state(&db, user).await.tasks[DRILL].done);
        let (status, _) = answer_task(
            &app,
            user,
            DRILL,
            json!({"problem_id":fresh.problem_id,"answer":"1"}),
        )
        .await;
        assert_ne!(status, StatusCode::OK);
        assert_eq!(events_of_type(&db, user, "drill_result").await, results);
    })
    .await;
}
