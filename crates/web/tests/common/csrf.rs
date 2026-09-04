//! The fixtures of `tests/csrf.rs` and its parts.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::cookie::CookiePosture;
use cadus_web::origin::OriginPolicy;
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use super::*;

/// A session cookie of the production posture, as a browser would send it.
pub const SECURE_COOKIE: &str = "__Host-cadus_session=s3cr3t";

/// A session cookie of the dev posture.
pub const DEV_COOKIE: &str = "cadus_session=s3cr3t";

/// Build the application with the production cookie posture and the fallback
/// origin policy.
pub fn app() -> Router {
    app_with(CookiePosture::SECURE, OriginPolicy::default())
}

/// Build the application with a chosen posture and origin policy.
pub fn app_with(posture: CookiePosture, origin: OriginPolicy) -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    create_app(
        AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS))
            .with_posture(posture)
            .with_origin(origin),
    )
}

/// Send one request and return the status code and the body as text.
pub async fn send(app: &Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

/// A request builder for a POST to `path` with the headers of `headers`.
pub fn post(path: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut builder = Request::builder().method("POST").uri(path);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    builder.body(Body::empty()).unwrap()
}

/// The refusal body of the layer, character for character.
pub const REJECTION_BODY: &str = concat!(
    r#"{"error":{"code":"cross_origin_rejected","message":"The server refuses a cross-origin "#,
    r#"write that carries a session cookie. Send the request same-origin, or send a bearer "#,
    r#"token."}}"#
);

/// The `422` body of a pre-auth route that ran and read an empty body.
///
/// U1 wrote tests (9) and (10) before the `/api/auth/*` routes existed, so "the
/// layer let this through" showed as the `404` of an unmatched path. M5 U4
/// mounted the three pre-auth routes, so the same request now reaches the
/// handler and the handler refuses the empty body. The MARKER changed; the rule
/// under test did not. A `403` on any of them still fails the test.
pub const EMPTY_BODY_REJECTION: &str =
    r#"{"error":{"code":"invalid_request","message":"The body is not JSON."}}"#;

/// The body of the `401` the `Tenant` extractor answers, character for
/// character.
///
/// Unit U8 added `POST /api/task/{task_id}/answer`, so a request the CSRF layer
/// ALLOWS now reaches that route and its tenant guard. These tests carry no
/// credential, so "not refused by the CSRF layer" is this `401` on the task
/// paths. It is a literal, and it is stronger than "not 403": a layer that
/// answered `500` would pass the weaker check. The `/api/auth/*` paths that
/// unit U4 added answer their own refusal, so they carry their own marker.
pub const UNAUTHORIZED_BODY: &str = concat!(
    r#"{"error":{"code":"unauthorized","message":"This route needs a session. Send the "#,
    r#"session cookie or a bearer token."}}"#
);

/// The headers of a cookie-authed cross-site write: the host, the production
/// session cookie, the cross-site fetch signal, and a foreign origin.
pub const CROSS_SITE: [(&str, &str); 4] = [
    ("host", "tutor.example"),
    ("cookie", SECURE_COOKIE),
    ("sec-fetch-site", "cross-site"),
    ("origin", "https://evil.example"),
];

/// Send a cross-site `POST` to the answer route, with `extra` headers on top
/// of [`CROSS_SITE`].
pub async fn cross_site_answer(app: &Router, extra: &[(&str, &str)]) -> (StatusCode, String) {
    let mut headers: Vec<(&str, &str)> = CROSS_SITE.to_vec();
    headers.extend_from_slice(extra);
    send(app, post("/api/task/t-review-fractions/answer", &headers)).await
}
