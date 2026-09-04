//! The requests of the task routes (`serve`, `teach`, `hint`, `answer`), and
//! the reads of the log and the plan that the serve route tests check.

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use serde_json::{Value, json};
use sqlx::types::Uuid;

use super::{call, parse, put_state, serve_raw, stored_state};

/// Serve `task_id` and read the `200` body.
pub async fn serve_ok(app: &Router, user: Uuid, task_id: &str) -> Value {
    let (status, body) = serve_raw(app, user, task_id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

/// `POST /api/task/{task_id}/hint` as `user` with `body`, and the RAW reply.
pub async fn hint_raw(
    app: &Router,
    user: Uuid,
    task_id: &str,
    body: Value,
) -> (StatusCode, String) {
    call(
        app,
        Method::POST,
        &format!("/api/task/{task_id}/hint"),
        Some(user),
        Some(body),
    )
    .await
}

/// Ask for a hint on `problem_id` of `task_id`, and read the parsed reply.
pub async fn hint_task(
    app: &Router,
    user: Uuid,
    task_id: &str,
    problem_id: &str,
) -> (StatusCode, Value) {
    let (status, body) = hint_raw(app, user, task_id, json!({"problem_id": problem_id})).await;
    (status, parse(&body))
}

/// Ask for a hint on `problem_id` of `task_id`, and read the `200` body.
pub async fn hint_ok(app: &Router, user: Uuid, task_id: &str, problem_id: &str) -> Value {
    let (status, body) = hint_task(app, user, task_id, problem_id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// `POST /api/task/{task_id}/teach` as `user`, and the parsed reply.
pub async fn teach_task(app: &Router, user: Uuid, task_id: &str) -> (StatusCode, Value) {
    let (status, body) = call(
        app,
        Method::POST,
        &format!("/api/task/{task_id}/teach"),
        Some(user),
        Some(json!({})),
    )
    .await;
    (status, parse(&body))
}

/// Answer `task_id` with `body`, and read the `200` body.
pub async fn answer_task_ok(app: &Router, user: Uuid, task_id: &str, body: Value) -> Value {
    let (status, reply) = super::answer_task(app, user, task_id, body).await;
    assert_eq!(status, StatusCode::OK, "{reply}");
    reply
}

/// Assert that a reply is the refusal `code` under `expected`.
pub fn assert_refused(reply: &(StatusCode, Value), expected: StatusCode, code: &str) {
    assert_eq!(reply.0, expected, "{}", reply.1);
    assert_eq!(reply.1["error"]["code"], code, "{}", reply.1);
}

/// The `problem_id` of a serve body.
pub fn problem_id_of(served: &Value) -> String {
    served["problem_id"].as_str().unwrap().to_string()
}

/// Close the live problem of `task_id` the way an answer does, so the next
/// serve deals a new one.
pub async fn close_live(db: &TestDb, user: Uuid, task_id: &str) {
    let mut scratch = stored_state(db, user).await;
    scratch.served.remove(task_id);
    put_state(db, user, &scratch).await;
}

/// The count of `model_call_log` rows, of every purpose (T6).
pub async fn model_calls(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM model_call_log")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many `task_served` rows the log holds for one task id.
pub async fn task_served_rows(db: &TestDb, user: Uuid, task_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM events
         WHERE user_id = $1 AND type = 'task_served' AND payload->>'task_id' = $2",
    )
    .bind(user)
    .bind(task_id)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The highest `seq` the log of `user` holds. 0 means an empty log.
pub async fn log_head(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COALESCE(MAX(seq), 0) FROM events WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The fold cursor of the cached learner model of `user`.
pub async fn fold_cursor(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT through_seq FROM learner_models WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many pool rows of `user` are claimed.
pub async fn claimed_rows(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM serving_pool WHERE user_id = $1 AND claimed_at IS NOT NULL",
    )
    .bind(user)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The parsed body of one `GET` route as `user`.
async fn get_ok(app: &Router, user: Uuid, uri: &str) -> Value {
    let (status, body) = call(app, Method::GET, uri, Some(user), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

/// The task ids `GET /api/session/plan` lists, in order.
pub async fn plan_task_ids(app: &Router, user: Uuid) -> Vec<String> {
    get_ok(app, user, "/api/session/plan").await["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["task_id"].as_str().unwrap().to_string())
        .collect()
}

/// The parsed body of `GET /api/status`.
pub async fn status_body(app: &Router, user: Uuid) -> Value {
    get_ok(app, user, "/api/status").await
}
