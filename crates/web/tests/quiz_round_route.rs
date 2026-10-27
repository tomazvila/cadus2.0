//! `POST /api/quiz/round` (D-QZ1): a quiz round of a chosen length and scope.
//!
//! The route appends no event. The task stands in the scratch document and in the
//! plan. The quiz keeps its rules: no feedback before the reveal, the exact checker,
//! and the retake wait. Every expected value is a literal.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_core::config::Config;
use cadus_core::event::{Event, QuizResult, SchemaVersion, Timestamp};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::WebState;
use cadus_web::{AppState, create_app};
use common::{
    BASE_US, SESSION, answer_raw, assert_reveals_nothing, call, enroll_course, exemplar, kp,
    now_secs, one_unit_curriculum, parse, passed_lesson, put_state, seed_event, seed_learner,
    seed_open_session, serve_task, stored_state, topic,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

const TOPICS: usize = 12;

fn app(db: &TestDb) -> Router {
    let topics = (0..TOPICS)
        .map(|index| {
            topic(
                &format!("t{index:02}"),
                vec![kp(
                    "kp1",
                    vec![exemplar(
                        &format!("Compute {index} + 1.5."),
                        &format!("{}.5", index + 1),
                    )],
                )],
            )
        })
        .collect();
    let content = common::open_content_with(one_unit_curriculum(topics), Config::default());
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(content)),
    )
}

/// A learner with an open session and `learned` topics with review history.
async fn learner(db: &TestDb, email: &str, learned: usize) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    for index in 0..learned {
        let seq = i64::try_from(index).unwrap() + 2;
        seed_event(
            db,
            user,
            seq,
            BASE_US,
            SESSION,
            passed_lesson(&format!("t{index:02}")),
        )
        .await;
    }
    put_state(db, user, &WebState::for_session(SESSION)).await;
    user
}

async fn start(app: &Router, user: Uuid, body: Value) -> (StatusCode, Value) {
    let (status, raw) = call(app, Method::POST, "/api/quiz/round", Some(user), Some(body)).await;
    (status, parse(&raw))
}

async fn event_rows(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM events WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Serve and answer every question of `task`; the first `right` answers are right.
async fn play(app: &Router, db: &TestDb, user: Uuid, task: &str, n: usize, right: usize) {
    for index in 0..n {
        let (status, _) = serve_task(app, user, task).await;
        assert_eq!(status, StatusCode::OK);
        let scratch = stored_state(db, user).await;
        let live = scratch.served.get(task).unwrap();
        let given = if index < right {
            live.expected.answer.clone()
        } else {
            "999".to_owned()
        };
        let (status, raw) = answer_raw(
            app,
            user,
            task,
            json!({"problem_id": live.problem_id, "answer": given}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        assert_reveals_nothing(&raw);
    }
}

#[tokio::test]
async fn a_bad_length_or_scope_answers_422() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "round-bad@example.com", TOPICS).await;
        let (status, body) = start(&app, user, json!({"length": 7, "scope": "all"})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "invalid_request");
        let (status, _) = start(&app, user, json!({"length": 7})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, body) = start(&app, user, json!({"length": 5, "scope": "x"})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["code"], "invalid_request");
    })
    .await;
}

#[tokio::test]
async fn a_learner_with_no_answered_topic_gets_a_plain_refusal() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "round-none@example.com", 0).await;
        let (status, body) = start(&app, user, json!({"length": 5, "scope": "all"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "no_topics_for_round");
        let (status, body) = start(&app, user, json!({"length": 5, "scope": "course"})).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "no_course");
    })
    .await;
}

#[tokio::test]
async fn a_round_is_recorded_in_the_plan_and_appends_no_event() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "round-start@example.com", TOPICS).await;
        let before = event_rows(&db, user).await;
        let (status, body) = start(&app, user, json!({"length": 10, "scope": "all"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["n"], 10);
        assert_eq!(body["round"]["length"], 10);
        assert_eq!(body["round"]["scope"], "all");
        assert_eq!(body["task"]["n_problems"], 10);
        assert_eq!(body["task"]["task_id"], format!("{SESSION}-round-1"));
        assert_eq!(
            body["task"]["time_budget_secs"],
            body["round"]["target_secs"]
        );
        assert_eq!(event_rows(&db, user).await, before);

        let (status, raw) = call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::OK);
        let ids: Vec<String> = parse(&raw)["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|task| task["task_id"].as_str().unwrap().to_owned())
            .collect();
        assert!(ids.contains(&format!("{SESSION}-round-1")), "{ids:?}");

        let (status, served) = serve_task(&app, user, &format!("{SESSION}-round-1")).await;
        assert_eq!(status, StatusCode::OK, "{served}");
        assert_eq!(served["total"], 10);
    })
    .await;
}

#[tokio::test]
async fn a_small_scope_gives_a_shorter_round() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "round-small@example.com", 7).await;
        let (status, body) = start(&app, user, json!({"length": 10, "scope": "all"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["length"], 10);
        assert_eq!(body["round"]["n"], 7);
        let (status, _) = enroll_course(&app, user, "c1").await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = start(&app, user, json!({"length": 20, "scope": "course"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["n"], 7);
        assert_eq!(body["round"]["course_name"], "Foundations");
    })
    .await;
}

#[tokio::test]
async fn a_started_round_resumes_and_an_unstarted_one_is_replaced() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "round-resume@example.com", TOPICS).await;
        let (_, first) = start(&app, user, json!({"length": 10, "scope": "all"})).await;
        let (status, second) = start(&app, user, json!({"length": 5, "scope": "all"})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(second["round"]["n"], 5);
        assert_eq!(second["task"]["task_id"], first["task"]["task_id"]);
        assert!(second.get("resumed").is_none());

        let task = format!("{SESSION}-round-1");
        play(&app, &db, user, &task, 1, 1).await;
        let (status, third) = start(&app, user, json!({"length": 20, "scope": "all"})).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(third["resumed"], true);
        assert_eq!(third["round"]["n"], 5);
    })
    .await;
}

#[tokio::test]
async fn a_start_during_the_retake_wait_answers_409() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "round-wait@example.com", TOPICS).await;
        let now_us = (now_secs() * 1_000_000.0) as i64;
        let result = Event::QuizResult(QuizResult {
            inconclusive: false,
            ts: Timestamp::from_micros(now_us),
            session: Some(SESSION.to_owned()),
            v: SchemaVersion::current(),
            quiz_id: "earlier".to_owned(),
            score: 0.5,
            per_topic: Vec::new(),
            xp: 0.0,
        });
        seed_event(
            &db,
            user,
            100,
            now_us,
            SESSION,
            serde_json::to_value(result).unwrap(),
        )
        .await;
        let (status, body) = start(&app, user, json!({"length": 5, "scope": "all"})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "quiz_retake_wait");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("waits one day")
        );
    })
    .await;
}

/// Play a round to its end and give the `quiz_result` event back.
async fn finished(app: &Router, db: &TestDb, user: Uuid, length: u64, right: usize) -> Value {
    let (status, body) = start(app, user, json!({"length": length, "scope": "all"})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let task = body["task"]["task_id"].as_str().unwrap().to_owned();
    let n = usize::try_from(length).unwrap();
    play(app, db, user, &task, n, right).await;
    let results = common::events_of_type(db, user, "quiz_result").await;
    assert_eq!(results.len(), 1);
    results[0].clone()
}

async fn retake_pending(app: &Router, user: Uuid) -> bool {
    let (_, raw) = call(app, Method::GET, "/api/status", Some(user), None).await;
    parse(&raw)["quiz"]["retake_pending"].as_bool().unwrap()
}

#[tokio::test]
async fn a_round_closes_with_the_exact_score_and_scaled_points() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let five = learner(&db, "round-five@example.com", TOPICS).await;
        let short = finished(&app, &db, five, 5, 4).await;
        assert_eq!(short["per_topic"].as_array().unwrap().len(), 5);
        assert_eq!(short["score"], 0.8);
        assert!(!retake_pending(&app, five).await);

        let ten = learner(&db, "round-ten@example.com", TOPICS).await;
        let long = finished(&app, &db, ten, 10, 8).await;
        assert_eq!(long["score"], 0.8);
        let scaled = short["xp"].as_f64().unwrap() * 8.0 / 5.0;
        assert!(
            (scaled - long["xp"].as_f64().unwrap()).abs() < 1e-9,
            "{short} {long}"
        );

        let (status, raw) = call(
            &app,
            Method::POST,
            &format!("/api/task/{SESSION}-round-1/quiz-result"),
            Some(five),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let reply = parse(&raw);
        assert_eq!(reply["round"]["n"], 5);
        assert_eq!(reply["round"]["scope"], "all");

        let weak = learner(&db, "round-weak@example.com", TOPICS).await;
        let missed = finished(&app, &db, weak, 5, 3).await;
        assert_eq!(missed["score"], 0.6);
        assert!(retake_pending(&app, weak).await);
    })
    .await;
}
