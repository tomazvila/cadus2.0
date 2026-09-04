//! The router with a fixture curriculum, and the requests the task route tests
//! send through it.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::curriculum::Curriculum;
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::Content;
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use tower::ServiceExt;

use super::{LESSON, PROBLEM_ID, addition_curriculum, present_session};

/// The router of a test, with `arena` loaded.
pub fn app_with_content(db: &TestDb, arena: Curriculum) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(Content::new(arena))),
    )
}

/// One request against the router. `tenant` is the bound learner.
pub async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    let payload = match body {
        Some(value) => {
            builder = builder.header(header::CONTENT_TYPE, "application/json");
            Body::from(value.to_string())
        }
        None => Body::empty(),
    };
    let mut request = builder.body(payload).unwrap();
    if let Some(user) = tenant {
        present_session(request.headers_mut(), user);
    }
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into())
}

/// The parsed JSON body of a call.
pub fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON: {err}\n{body}"))
}

/// `POST /api/task/{task_id}/answer` as `user`, with `body`.
pub async fn answer_task(
    app: &Router,
    user: Uuid,
    task_id: &str,
    body: Value,
) -> (StatusCode, Value) {
    let (status, raw) = answer_raw(app, user, task_id, body).await;
    (status, parse(&raw))
}

/// `POST /api/task/{task_id}/answer` as `user`, with `body`, and the RAW reply
/// text.
pub async fn answer_raw(
    app: &Router,
    user: Uuid,
    task_id: &str,
    body: Value,
) -> (StatusCode, String) {
    call(
        app,
        Method::POST,
        &format!("/api/task/{task_id}/answer"),
        Some(user),
        Some(body),
    )
    .await
}

/// `POST /api/task/{task_id}/serve` as `user`.
pub async fn serve_task(app: &Router, user: Uuid, task_id: &str) -> (StatusCode, Value) {
    let (status, raw) = serve_raw(app, user, task_id).await;
    (status, parse(&raw))
}

/// `POST /api/task/{task_id}/serve` as `user`, and the RAW reply text.
pub async fn serve_raw(app: &Router, user: Uuid, task_id: &str) -> (StatusCode, String) {
    call(
        app,
        Method::POST,
        &format!("/api/task/{task_id}/serve"),
        Some(user),
        None,
    )
    .await
}

/// `POST /api/enroll` as `user`, for `course`.
pub async fn enroll_course(app: &Router, user: Uuid, course: &str) -> (StatusCode, Value) {
    let (status, raw) = call(
        app,
        Method::POST,
        "/api/enroll",
        Some(user),
        Some(json!({"course": course})),
    )
    .await;
    (status, parse(&raw))
}

/// Read `/metrics` from the same router and return the exposition text.
pub async fn scrape(app: &Router) -> String {
    let (status, body) = call(app, Method::GET, "/metrics", None, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// Assert that `text` holds `line`, and print the whole scrape when it does not.
pub fn holds(text: &str, line: &str) {
    assert!(
        text.contains(&format!("{line}\n")),
        "the scrape carries no line {line:?}:\n{text}"
    );
}

/// The router of a grade test, with the two-topic fixture and no key
/// prerequisite loaded.
pub fn lesson_app(db: &TestDb) -> Router {
    app_with_content(db, addition_curriculum(Vec::new()))
}

/// Answer the live problem of the `addition` lesson with `given`.
pub async fn answer_lesson(app: &Router, user: Uuid, given: &str) -> (StatusCode, Value) {
    answer_task(
        app,
        user,
        LESSON,
        json!({"problem_id": PROBLEM_ID, "answer": given}),
    )
    .await
}

/// Answer the live problem of the `addition` lesson with `given`, and read the
/// `200` body.
pub async fn answer_lesson_ok(app: &Router, user: Uuid, given: &str) -> Value {
    let (status, body) = answer_lesson(app, user, given).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}
