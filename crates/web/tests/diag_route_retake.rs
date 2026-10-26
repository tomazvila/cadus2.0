//! A retake of the placement diagnostic over HTTP: `POST /api/diag/finish` sets
//! `refresh` on `diagnostic_placed` when the learner has a placement or a lesson
//! pass on record, and the fold then averages instead of re-seeding.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use common::placement::*;
use common::sessions::enrolled_c1;
use common::{seed_cached_model, seed_typed_event};

use axum::body::Body;
use axum::http::{Method, Request};
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_store::test_support::TestDb;
use cadus_web::state::Tenant;
use http_body_util::BodyExt;
use tower::ServiceExt;

/// One POST as `user`.
async fn call(app: &Router, user: Uuid, path: &str, body: &Value) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(axum::http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    request.extensions_mut().insert(Tenant(user));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let value = serde_json::from_str(&text).unwrap_or_else(|err| panic!("{err}: {text}"));
    (status, value)
}

/// Answer every probe from the first with `given`, until the diagnostic is done.
async fn walk(app: &Router, user: Uuid, given: &str) {
    let (status, start) = call(app, user, "/api/diag/start", &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{start}");
    let mut problem_id = start["probe"]["problem_id"].as_str().unwrap().to_owned();
    for _ in 0..20 {
        let (status, reply) = call(
            app,
            user,
            "/api/diag/answer",
            &json!({ "problem_id": problem_id, "answer": given }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{reply}");
        let next = &reply["next_probe"];
        if next["done"] == Value::Bool(true) {
            return;
        }
        problem_id = next["problem_id"].as_str().unwrap().to_owned();
    }
    panic!("the probe list did not end");
}

/// Close the diagnostic of `user` and read the reply.
async fn finish_ok(app: &Router, user: Uuid) -> Value {
    let (status, body) = call(app, user, "/api/diag/finish", &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Every `diagnostic_placed` payload of `user`, oldest first.
async fn placed_events(db: &TestDb, user: Uuid) -> Vec<Value> {
    sqlx::query_scalar(
        "SELECT payload FROM events WHERE user_id = $1 AND type = 'diagnostic_placed' ORDER BY seq",
    )
    .bind(user)
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// The `topics` object of the saved model of `user`.
async fn saved_topics(db: &TestDb, user: Uuid) -> Value {
    let model: Value = sqlx::query_scalar("SELECT model FROM learner_models WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap();
    model["topics"].clone()
}

/// A learner enrolled in `c1` whose `addition` topic was practiced for a long
/// review interval.
async fn practiced_learner(db: &TestDb, email: &str) -> Uuid {
    let user = db.seed_user(email).await;
    seed_typed_event(db, user, 1, &enrolled_c1()).await;
    let mut topics = BTreeMap::new();
    topics.insert(
        "addition".to_owned(),
        TopicState {
            status: TopicStatus::Learning,
            rep_num: 10.0,
            memory_base: 1.0,
            t0: Some(Timestamp::from_micros(1)),
            interval_days: 30.0,
            ability: 0.8,
            ..TopicState::default()
        },
    );
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    seed_cached_model(db, user, &model, 1).await;
    user
}

#[tokio::test]
async fn a_first_placement_is_not_a_refresh() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = db.seed_user("first@example.test").await;
        walk(&app, user, ANSWER).await;
        let reply = finish_ok(&app, user).await;
        assert_eq!(reply["refresh"], false);
        assert_eq!(placed_events(&db, user).await[0]["refresh"], false);
    })
    .await;
}

#[tokio::test]
async fn a_retake_after_a_lesson_pass_averages_instead_of_reseeding() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = practiced_learner(&db, "practiced@example.test").await;
        walk(&app, user, ANSWER).await;
        let reply = finish_ok(&app, user).await;
        assert_eq!(reply["refresh"], true);
        assert_eq!(placed_events(&db, user).await[0]["refresh"], true);
        let topics = saved_topics(&db, user).await;
        // The initial path caps the repetition number at 4.0. The refresh path
        // averages 10.0 with a credit of at most 4.0.
        assert!(topics["addition"]["repNum"].as_f64().unwrap() > 4.0);
    })
    .await;
}

#[tokio::test]
async fn a_retake_leaves_a_never_learned_missed_topic_untouched() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = practiced_learner(&db, "missed@example.test").await;
        walk(&app, user, "0").await;
        let reply = finish_ok(&app, user).await;
        assert_eq!(reply["refresh"], true);
        let topics = saved_topics(&db, user).await;
        assert!(topics.get("word-problems").is_none(), "{topics}");
        // A miss gives `addition` no balance, so the refresh leaves it as it was.
        assert_eq!(topics["addition"]["repNum"].as_f64().unwrap(), 10.0);
        assert_eq!(topics["addition"]["status"], "learning");
    })
    .await;
}

#[tokio::test]
async fn a_retake_after_a_placement_alone_is_a_refresh() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = db.seed_user("twice@example.test").await;
        walk(&app, user, ANSWER).await;
        finish_ok(&app, user).await;
        walk(&app, user, ANSWER).await;
        let reply = finish_ok(&app, user).await;
        assert_eq!(reply["refresh"], true);
        let events = placed_events(&db, user).await;
        assert_eq!(events[0]["refresh"], false);
        assert_eq!(events[1]["refresh"], true);
    })
    .await;
}
