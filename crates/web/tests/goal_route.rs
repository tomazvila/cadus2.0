//! `POST /api/goal/xp` (D-XP1): the learner sets the daily XP goal.
//!
//! The route appends one `goal_set` event per call. It never updates an earlier row.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::sessions::*;
use common::{events_of_type, parse};

/// One POST of `body` to the goal route as `user`.
async fn post_goal(app: &Router, user: Uuid, body: Value) -> (StatusCode, Value) {
    let (status, _, text) = call(app, Method::POST, "/api/goal/xp", Some(user), Some(body)).await;
    (status, parse(&text))
}

#[tokio::test]
async fn a_goal_outside_10_to_100_or_missing_answers_422() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "goal-bad@example.com").await;
        let app = app(&db);
        for body in [
            json!({"goal": 5}),
            json!({"goal": 150}),
            json!({}),
            json!({"goal": "50"}),
            json!({"goal": 50.5}),
        ] {
            let (status, answer) = post_goal(&app, user, body.clone()).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(answer["error"]["code"], "invalid_request", "{body}");
        }
        assert_eq!(events_of_type(&db, user, "goal_set").await.len(), 0);
    })
    .await;
}

#[tokio::test]
async fn a_good_goal_is_saved_reported_and_appended_never_updated() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "goal-good@example.com").await;
        let app = app(&db);

        let (status, answer) = post_goal(&app, user, json!({"goal": 50})).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["goal"], 50);
        assert_eq!(answer["xp"]["goal"], 50);

        let (status, _, text) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        assert_eq!(parse(&text)["xp"]["goal"], 50);

        let first = events_of_type(&db, user, "goal_set").await;
        assert_eq!(first.len(), 1);
        assert_eq!(first[0]["goal"], 50);

        let (status, answer) = post_goal(&app, user, json!({"goal": 20})).await;
        assert_eq!(status, StatusCode::OK, "{answer}");
        assert_eq!(answer["xp"]["goal"], 20);
        let both = events_of_type(&db, user, "goal_set").await;
        assert_eq!(both.len(), 2);
        assert_eq!(both[0], first[0], "the first row stands unchanged");
        assert_eq!(both[1]["goal"], 20);

        let (_, _, text) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        assert_eq!(parse(&text)["xp"]["goal"], 20);
    })
    .await;
}
