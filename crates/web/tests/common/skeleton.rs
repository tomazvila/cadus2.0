//! The fixtures of `tests/skeleton.rs` and its parts.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::cookie::{
    CookiePosture, CookiePostureError, read_bearer_token, read_session_cookie,
};
use cadus_web::origin::{OriginPolicy, OriginPolicyError};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

use super::*;

/// The Content-Security-Policy of 1.0, character for character.
pub const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
                   font-src 'self'; base-uri 'none'; frame-ancestors 'none'; connect-src 'self' \
                   http://localhost:* http://127.0.0.1:*";

/// The five security headers of spec section 3.1, as literal pairs.
pub const SECURITY_HEADERS: [(&str, &str); 5] = [
    ("content-security-policy", CSP),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    ("referrer-policy", "no-referrer"),
    ("cache-control", "no-cache"),
];

/// An application on a lazy pool that points at an address with no server.
pub fn offline_app() -> Router {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    app_on(pool)
}

/// An application on `pool`, with the production posture.
pub fn app_on(pool: PgPool) -> Router {
    create_app(state_with(pool))
}

/// Send one request and return the status, the headers, and the body as text.
pub async fn send(app: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, String) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, headers, String::from_utf8(body.to_vec()).unwrap())
}

/// A `GET` request for `path`.
pub fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

/// Read `/metrics` and return the exposition text.
pub async fn scrape(app: &Router) -> String {
    let (status, headers, body) = send(app, get("/metrics")).await;
    assert_eq!(status.as_u16(), 200);
    assert_eq!(
        headers.get("content-type").unwrap().to_str().unwrap(),
        "text/plain; version=0.0.4; charset=utf-8"
    );
    body
}

/// Read one header as text.
pub fn header_of(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .unwrap_or_else(|| panic!("the answer carries no {name} header"))
        .to_str()
        .unwrap()
        .to_string()
}
