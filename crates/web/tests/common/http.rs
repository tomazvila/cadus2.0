//! The application under test, the request builders, and the token literals.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::auth::password::Argon2Profile;
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::mark_verified;

/// A raw verification token and its SHA-256 hex digest.
pub const VERIFY_TOKEN_ONE: (&str, &str) = (
    "u4-verify-token-one",
    "d2572740393549b1a105785255f3734f10abc89285e4506602dd0c9434921c61",
);

/// A second raw verification token and its digest.
pub const VERIFY_TOKEN_TWO: (&str, &str) = (
    "u4-verify-token-two",
    "2b767c70e89525a359c8e91522a3d7bcab50ff22e8179215e08b1aa1b2a17cfe",
);

/// A raw reset token and its digest.
pub const RESET_TOKEN_ONE: (&str, &str) = (
    "u4-reset-token-one",
    "c1b440edf5da1656f44be97523cf6adba5372fd1dab4f0199dc5d0206fb82ff2",
);

/// A second raw reset token and its digest.
pub const RESET_TOKEN_TWO: (&str, &str) = (
    "u4-reset-token-two",
    "126be2068748fcd74ea66493be1d3b9f5297bbb6da5cad7465cf399308c82567",
);

/// A raw session token and its digest.
pub const SESSION_TOKEN_ONE: (&str, &str) = (
    "u4-session-token-one",
    "9dcf1165562b66dc6d96f1663e671d0af94f8f0f5c1c5b3bd6e94f1dccbde85c",
);

/// A second raw session token and its digest.
pub const SESSION_TOKEN_TWO: (&str, &str) = (
    "u4-session-token-two",
    "032a673797f3a2fd3fa3120834067e598fd15eff6a44443e7a8bf94f580f695e",
);

/// A password that the 8-to-256-character policy accepts.
pub const GOOD_PASSWORD: &str = "correct horse battery staple";

/// A second acceptable password.
pub const OTHER_PASSWORD: &str = "another perfectly fine passphrase";

/// Seven characters: one below the floor of the policy.
pub const SHORT_PASSWORD: &str = "7chars!";

/// The application under test, on the `cadus_app` pool of `db`.
///
/// The Argon2 profile is `test`, so a suite of dozens of hashes finishes in
/// seconds. The cookie posture is the production one, so every cookie the tests
/// read carries the `__Host-` name.
pub fn app_of(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_argon2(Argon2Profile::TEST),
    )
}

/// One finished answer: the status, the headers, and the parsed body.
pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Answer {
    /// The `error.code` of an error envelope, or a message that names the miss.
    pub fn code(&self) -> String {
        match self.body.get("error").and_then(|error| error.get("code")) {
            Some(Value::String(code)) => code.clone(),
            _ => format!("the body carries no error.code: {}", self.body),
        }
    }

    /// The first `Set-Cookie` header, or `None`.
    pub fn cookie(&self) -> Option<String> {
        self.headers
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string)
    }
}

/// Send one request and read the whole answer.
pub async fn send(app: &Router, request: Request<Body>) -> Answer {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Answer {
        status,
        headers,
        body,
    }
}

/// A `POST` of `body` to `path`, with no credential and no origin header.
pub fn post(path: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// A `POST` of `body` to `path` that carries `token` as a bearer credential.
///
/// The bearer channel is exempt from the CSRF origin rule (D-M5-5), so a test
/// that writes with a credential and no `Origin` header uses this one.
pub fn post_bearer(path: &str, token: &str, body: &Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::from(body.to_string()))
        .unwrap()
}

/// A `GET` of `path` that carries `token` as a bearer credential.
pub fn get_bearer(path: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// A `GET` of `path` with no credential.
pub fn get(path: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

/// Register `email` through the real sign-up route.
pub async fn signup(app: &Router, email: &str, password: &str) -> Answer {
    send(
        app,
        post(
            "/api/auth/signup",
            &json!({ "email": email, "password": password }),
        ),
    )
    .await
}

/// Sign in and return the answer. The caller asks for the raw token in the body.
pub async fn login(app: &Router, email: &str, password: &str) -> Answer {
    let request = Request::builder()
        .method("POST")
        .uri("/api/auth/login")
        .header("content-type", "application/json")
        .header("accept-session-token", "true")
        .body(Body::from(
            json!({ "email": email, "password": password }).to_string(),
        ))
        .unwrap();
    send(app, request).await
}

/// Register `email`, mark the address verified with the admin pool, and sign in.
///
/// The answer is the raw session token. The verification stamp goes through the
/// admin pool on purpose: this fixture serves the tests that are about the
/// session, not about the verification link.
pub async fn verified_login(db: &TestDb, app: &Router, email: &str, password: &str) -> String {
    let answer = signup(app, email, password).await;
    assert_eq!(
        answer.status.as_u16(),
        200,
        "sign-up failed: {}",
        answer.body
    );
    mark_verified(db, email).await;
    let answer = login(app, email, password).await;
    assert_eq!(answer.status.as_u16(), 200, "login failed: {}", answer.body);
    answer
        .body
        .get("session_token")
        .and_then(Value::as_str)
        .expect("the login answer carries no session_token")
        .to_string()
}
