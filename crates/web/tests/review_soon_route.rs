//! `POST /api/topics/{id}/review-soon`, and the `last_topic`, `last_active_at`,
//! `session_open` and `plan_preview` fields of `GET /api/status`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::sessions::*;
use common::{BASE_US, SESSION, parse, seed_open_session};

use std::collections::BTreeMap;

use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};

/// A learner with `addition` practiced one day ago, on a 400-day interval: not due.
async fn fresh_learner(db: &TestDb, email: &str) -> Uuid {
    let user = common::seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_event(db, user, 2, &enrolled_c1()).await;
    let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
    topics.insert(
        "addition".to_string(),
        TopicState {
            status: TopicStatus::Learning,
            rep_num: 5.0,
            memory_base: 1.0,
            t0: Some(Timestamp::from_micros(BASE_US - 86_400_000_000)),
            interval_days: 400.0,
            ability: 0.6,
            ..TopicState::default()
        },
    );
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    common::seed_cached_model(db, user, &model, 2).await;
    user
}

async fn post_review(app: &Router, user: Uuid, topic: &str) -> (StatusCode, Value) {
    let uri = format!("/api/topics/{topic}/review-soon");
    let (status, _, body) = call(app, Method::POST, &uri, Some(user), Some(json!({}))).await;
    (status, parse(&body))
}

/// A practiced topic that is not due becomes due after the request.
#[tokio::test]
async fn a_practiced_topic_becomes_a_due_review() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = fresh_learner(&db, "soon@example.com").await;
        let before = parse(
            &call(&app, Method::GET, "/api/status", Some(user), None)
                .await
                .2,
        );
        assert_eq!(before["due_reviews"], 0);

        let (status, body) = post_review(&app, user, "addition").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["review_soon"], true);

        let after = parse(
            &call(&app, Method::GET, "/api/status", Some(user), None)
                .await
                .2,
        );
        assert_eq!(after["due_reviews"], 1);
        assert_eq!(after["session_open"], true);
    })
    .await;
}

/// A topic without state answers 409, and a topic outside the curriculum answers 404.
#[tokio::test]
async fn an_unlearned_topic_is_refused() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = fresh_learner(&db, "unlearned@example.com").await;

        let (status, body) = post_review(&app, user, "subtraction").await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["message"], "Learn this topic first.");

        let (status, body) = post_review(&app, user, "no-such-topic").await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    })
    .await;
}

/// With no graded answer the last-studied fields are null.
#[tokio::test]
async fn status_has_no_last_topic_before_the_first_answer() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = fresh_learner(&db, "nolast@example.com").await;
        let value = parse(
            &call(&app, Method::GET, "/api/status", Some(user), None)
                .await
                .2,
        );
        assert_eq!(value["last_topic"], Value::Null);
        assert_eq!(value["last_active_at"], Value::Null);
        assert!(value["plan_preview"].is_object());
    })
    .await;
}

/// The latest graded answer names the topic and the instant. An ungraded one does not count.
#[tokio::test]
async fn status_names_the_topic_of_the_latest_graded_answer() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = fresh_learner(&db, "last@example.com").await;
        let attempt = |id: &str, topic: &str, ts: &str, extra: Value| {
            let mut doc = json!({
                "type": "attempt", "ts": ts, "session": SESSION, "attempt_id": id,
                "task_id": "t", "topic": topic, "task_type": "lesson",
                "problem": {"text": "p", "expected": "1"}, "given_answer": "1",
                "correct": true, "secs": 3, "work_quality": "perfect", "v": 1,
            });
            if let Value::Object(map) = extra {
                doc.as_object_mut().unwrap().extend(map);
            }
            doc
        };
        sqlx::query(
            "INSERT INTO events (user_id, seq, ts, type, session_id, v, attempt_id, payload) \
             VALUES ($1, 3, '2026-01-02T10:00:00Z', 'attempt', $2, 1, 'a1', $3), \
                    ($1, 4, '2026-01-03T10:00:00Z', 'attempt', $2, 1, 'a2', $4)",
        )
        .bind(user)
        .bind(SESSION)
        .bind(attempt("a1", "addition", "2026-01-02T10:00:00Z", json!({})))
        .bind(attempt(
            "a2",
            "subtraction",
            "2026-01-03T10:00:00Z",
            json!({"correct": false, "outcome": {"ungraded": {"reason": "no checker"}}}),
        ))
        .execute(&db.admin)
        .await
        .unwrap();

        let value = parse(
            &call(&app, Method::GET, "/api/status", Some(user), None)
                .await
                .2,
        );
        assert_eq!(value["last_topic"]["id"], "addition");
        assert_eq!(value["last_active_at"], "2026-01-02T10:00:00Z");
    })
    .await;
}
