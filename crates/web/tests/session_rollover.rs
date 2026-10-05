//! The day rollover (D-F11 step 6): a session left open on an earlier day
//! closes at the learner's next request, and today's session opens.
//!
//! Every fixture seeds the open session `s_2026-01-01a` at 2026-01-01, so any
//! real clock reads a later day. The router turns the rollover ON; the shared
//! test router keeps it off.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use common::sessions::call;
use common::{
    LESSON, PROBLEM_ID, SESSION, addition_curriculum, answer_task, events_of_type,
    learner_with_kp1, parse, seed_learner, seed_open_session, state_with_content,
};
use serde_json::json;
use sqlx::types::Uuid;

/// The lesson router with the day rollover on.
fn rollover_app(db: &TestDb) -> Router {
    cadus_web::create_app(
        state_with_content(db, addition_curriculum(Vec::new())).with_day_rollover(true),
    )
}

/// The `session` field of every event of `kind`, oldest first.
async fn sessions_of(db: &TestDb, user: Uuid, kind: &str) -> Vec<String> {
    events_of_type(db, user, kind)
        .await
        .iter()
        .map(|payload| payload["session"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[tokio::test]
async fn the_first_start_of_a_new_day_ends_the_stale_session_and_opens_todays() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "rollover-start@example.com").await;
        seed_open_session(&db, user).await;
        let app = rollover_app(&db);

        let (status, _, body) =
            call(&app, Method::POST, "/api/session/start", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let started = parse(&body);
        assert_eq!(started["reopened"], false);
        let today = started["session"].as_str().unwrap().to_owned();
        assert_ne!(today, SESSION);

        assert_eq!(sessions_of(&db, user, "session_end").await, [SESSION]);
        assert_eq!(
            sessions_of(&db, user, "session_start").await,
            [SESSION.to_owned(), today.clone()]
        );

        // A second start the same day resumes today's session and writes nothing.
        let (status, _, body) =
            call(&app, Method::POST, "/api/session/start", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["reopened"], true);
        assert_eq!(parse(&body)["session"], today.as_str());
        assert_eq!(sessions_of(&db, user, "session_end").await.len(), 1);
    })
    .await;
}

#[tokio::test]
async fn an_answer_to_yesterdays_problem_is_set_aside_and_today_opens() {
    TestDb::with(|db| async move {
        // The live problem went on screen a day before the answer.
        let user = learner_with_kp1(&db, "rollover-answer@example.com", 86_400.0).await;
        let app = rollover_app(&db);

        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "session_rolled_over");

        // No attempt: no day-long solve time reaches the log.
        assert!(events_of_type(&db, user, "attempt").await.is_empty());
        let ended = events_of_type(&db, user, "session_end").await;
        assert_eq!(ended.len(), 1);
        assert_eq!(ended[0]["session"], SESSION);
        let starts = sessions_of(&db, user, "session_start").await;
        assert_eq!(starts.len(), 2);
        let today = starts[1].clone();
        assert_ne!(today, SESSION);

        // The plan the client loads next is today's.
        let (status, _, body) =
            call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["session"], today.as_str());

        // The old problem id names nothing any more.
        let (status, body) = answer_task(
            &app,
            user,
            LESSON,
            json!({"problem_id": PROBLEM_ID, "answer": "13.5"}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert!(events_of_type(&db, user, "attempt").await.is_empty());
        assert_eq!(sessions_of(&db, user, "session_end").await.len(), 1);
    })
    .await;
}

#[tokio::test]
async fn the_shared_router_keeps_the_rollover_off() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "rollover-off@example.com").await;
        seed_open_session(&db, user).await;
        let app = common::sessions::app(&db);
        let (status, _, body) =
            call(&app, Method::POST, "/api/session/start", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["reopened"], true);
        assert_eq!(parse(&body)["session"], SESSION);
        assert!(events_of_type(&db, user, "session_end").await.is_empty());
    })
    .await;
}
