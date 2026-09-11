//! Supplemental practice preserves the original drill count and server-owned evidence.
#![allow(clippy::unwrap_used)]
mod common;
use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use cadus_web::state::{TaskProgress, WebState};
use common::{
    DRILL, SESSION, answer_task, drill_app, events_of_type, put_state, seed_drill_due,
    seed_learner, seed_open_session, serve_task, stored_state,
};
use serde_json::json;

#[tokio::test]
async fn a_final_drill_miss_requires_fresh_untimed_practice_without_an_extra_count() {
    TestDb::with(|db| async move {
        let app = drill_app(&db);
        let user = seed_learner(&db, "drill-practice@example.com").await;
        seed_open_session(&db, user).await;
        seed_drill_due(&db, user).await;
        let mut scratch = WebState::for_session(SESSION);
        scratch.tasks.insert(
            DRILL.to_owned(),
            TaskProgress {
                task_id: DRILL.to_owned(),
                task_type: "drill".to_owned(),
                total: 20,
                served: 19,
                answered: 19,
                ..TaskProgress::default()
            },
        );
        put_state(&db, user, &scratch).await;
        let (status, original) = serve_task(&app, user, DRILL).await;
        assert_eq!(status, StatusCode::OK, "{original}");
        let (status, feedback) = answer_task(
            &app,
            user,
            DRILL,
            json!({
                "problem_id":original["problem_id"],"answer":"99999","feedback_practice":true,
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{feedback}");
        assert_eq!(feedback["feedback_practice"], true);
        assert_eq!(feedback["next"]["feedback_practice"], true);
        assert_eq!(feedback["next"]["countdown"], false);
        let pending = stored_state(&db, user).await;
        assert_eq!(pending.tasks[DRILL].answered, 20);
        assert!(!pending.tasks[DRILL].done);
        let fresh = pending.served[DRILL].clone();
        assert_ne!(fresh.text, original["text"]);
        let (status, done) = answer_task(
            &app,
            user,
            DRILL,
            json!({"problem_id":fresh.problem_id,"answer":fresh.expected.answer}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{done}");
        let progress = stored_state(&db, user).await.tasks[DRILL].clone();
        assert_eq!(progress.answered, 20);
        assert_eq!(progress.served, 20);
        assert!(progress.done);
        let attempts = events_of_type(&db, user, "attempt").await;
        assert_eq!(attempts.len(), 2);
        assert!(attempts[0].get("feedback_practice").is_none());
        assert_eq!(attempts[1]["feedback_practice"], true);
        assert_eq!(attempts[1]["independent_after_feedback"], true);
        assert_eq!(attempts[1]["assisted"], false);
    })
    .await;
}
