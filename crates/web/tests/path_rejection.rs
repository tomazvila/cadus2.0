//! FIX2-M5-C, finding V4: the path extractor answers the section 2 envelope.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 2 pins
//! `{"error":{"code","message"}}` on every 4xx and 5xx answer, with no
//! exception. Seven routes read a path segment, and each one took the axum
//! `Path` extractor, whose rejection answers `400 text/plain` with the sentence
//! ``Invalid URL: Invalid UTF-8 in `provider` ``. `ApiPath`
//! (`crates/web/src/path.rs`) maps that rejection onto the envelope.
//!
//! The trigger is one path segment that holds an invalid UTF-8 percent escape.
//! `%ff` is such a segment: the URI is syntactically valid, so hyper accepts it
//! and matchit matches the route, and the failure lands inside the handler's own
//! extractor.
//!
//! Every expected value here is a LITERAL: a literal status code, a literal
//! error code, a literal content type, a literal body.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_store::test_support::TestDb;
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::types::Uuid;
use tower::ServiceExt;

/// The seven routes that read a path segment, with the method each one serves
/// and whether the route sits behind the tenant layer.
///
/// The segment is `%ff` in every one. The list is the whole set of `ApiPath`
/// call sites.
const ROUTES: [(Method, &str, bool); 7] = [
    (Method::GET, "/api/auth/oauth/%ff/start", false),
    (Method::GET, "/api/auth/oauth/%ff/callback", false),
    (Method::POST, "/api/task/%ff/serve", true),
    (Method::POST, "/api/task/%ff/teach", true),
    (Method::POST, "/api/task/%ff/hint", true),
    (Method::POST, "/api/task/%ff/answer", true),
    (Method::GET, "/api/diagnosis/%ff", true),
];

/// The whole body that every one of the seven bad-segment answers carries.
const BAD_PATH_BODY: &str = "{\"error\":{\"code\":\"invalid_request\",\"message\":\"The request \
                             path is not valid for this route.\"}}";

/// One request against the router. `tenant` is the bound learner.
async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    tenant: Option<Uuid>,
) -> (StatusCode, Option<String>, String) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap();
    if let Some(user) = tenant {
        common::present_session(request.headers_mut(), user);
    }
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, content_type, String::from_utf8_lossy(&bytes).into())
}

/// The `error.code` of a body, or a sentence that names the miss.
fn code(body: &str) -> String {
    let value: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(err) => return format!("the body is not JSON ({err}): {body}"),
    };
    match value.get("error").and_then(|error| error.get("code")) {
        Some(Value::String(code)) => code.clone(),
        _ => format!("the body carries no error.code: {body}"),
    }
}

/// An invalid UTF-8 path segment answers `422 invalid_request` in the envelope.
///
/// The learner is signed in on the five guarded routes, so the tenant layer
/// passes and the path extractor is the step that refuses. The two OAuth routes
/// carry no credential, because neither one is guarded.
#[tokio::test]
async fn an_invalid_utf8_path_segment_is_422_invalid_request_on_all_seven_routes() {
    TestDb::with(|db| async move {
        let app = common::app_of(&db);
        let user = common::seed_learner(&db, "path-rejection@example.com").await;

        for (method, uri, guarded) in ROUTES {
            let tenant = if guarded { Some(user) } else { None };
            let (status, content_type, body) = call(&app, method.clone(), uri, tenant).await;
            assert_eq!(
                status,
                StatusCode::UNPROCESSABLE_ENTITY,
                "{method} {uri} answered {status} with {body}"
            );
            assert_eq!(code(&body), "invalid_request", "{method} {uri}");
            assert_eq!(
                content_type.as_deref(),
                Some("application/json"),
                "{method} {uri}"
            );
            assert_eq!(body, BAD_PATH_BODY, "{method} {uri}");
        }
    })
    .await;
}

/// A readable path segment still reaches the handler.
///
/// The extractor holds no policy of its own, so each route keeps the answer it
/// gave before: an OAuth provider this deployment does not serve is
/// `404 not_found`, and a diagnosis id that is not a UUID is
/// `404 unknown_diagnosis`. Neither one is `422`, so the mapping refuses the
/// unreadable segment alone.
#[tokio::test]
async fn a_readable_path_segment_still_reaches_the_handler() {
    TestDb::with(|db| async move {
        let app = common::app_of(&db);
        let user = common::seed_learner(&db, "path-readable@example.com").await;

        let (status, _, body) = call(&app, Method::GET, "/api/auth/oauth/google/start", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(code(&body), "not_found");

        let (status, _, body) =
            call(&app, Method::GET, "/api/auth/oauth/google/callback", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(code(&body), "not_found");

        let (status, _, body) =
            call(&app, Method::GET, "/api/diagnosis/not-a-uuid", Some(user)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
        assert_eq!(code(&body), "unknown_diagnosis");
    })
    .await;
}
