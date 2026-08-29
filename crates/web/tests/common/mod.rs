//! Fixtures the `/api/auth/*` test files and the guarded route test files share.
//!
//! The module holds no assertion. Every expected value is a literal of the test
//! file that reads it (HANDOVER section 3).
//!
//! **The token digests below are literals, not computed values.** A fixture that
//! called `hash_token` to seed a row would agree with a broken `hash_token`, so
//! each pair here is a raw token and the SHA-256 hex that `sha256sum` gives for
//! it. Seeding with the literal digest and presenting the raw token proves the
//! production digest matches.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, Request, StatusCode};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::auth::oauth::{
    Credentials, OAuthConfig, ProviderRequest, ProviderResponse, ProviderTransport, TransportError,
};
use cadus_web::auth::password::Argon2Profile;
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;
use sqlx::types::chrono::{DateTime, Utc};
use tower::ServiceExt;

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

/// Stamp `email_verified_at` with the admin pool.
pub async fn mark_verified(db: &TestDb, email: &str) {
    sqlx::query("UPDATE users SET email_verified_at = now() WHERE email = $1::text::citext")
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Stamp `disabled_at` with the admin pool.
pub async fn disable(db: &TestDb, email: &str) {
    sqlx::query("UPDATE users SET disabled_at = now() WHERE email = $1::text::citext")
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// The id of the account that `email` names.
pub async fn user_id(db: &TestDb, email: &str) -> Uuid {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1::text::citext")
        .bind(email)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Write one `auth_tokens` row with the admin pool.
pub async fn seed_token(
    db: &TestDb,
    user: Uuid,
    token_hash: &str,
    purpose: &str,
    expires_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at) VALUES ($1, $2, $3, $4)",
    )
    .bind(token_hash)
    .bind(user)
    .bind(purpose)
    .bind(expires_at)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// Write one `auth_sessions` row with the admin pool.
pub async fn seed_session(
    db: &TestDb,
    user: Uuid,
    token_hash: &str,
    created_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
) {
    sqlx::query(
        "INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at, expires_at)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(token_hash)
    .bind(user)
    .bind(created_at)
    .bind(last_seen_at)
    .bind(expires_at)
    .execute(&db.admin)
    .await
    .unwrap();
}

/// How many session rows one account has.
pub async fn session_count(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_sessions WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many live tokens of `purpose` one account has.
pub async fn token_count(db: &TestDb, user: Uuid, purpose: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_tokens WHERE user_id = $1 AND purpose = $2")
        .bind(user)
        .bind(purpose)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The `last_seen_at` of one session row.
pub async fn last_seen_at(db: &TestDb, token_hash: &str) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT last_seen_at FROM auth_sessions WHERE token_hash = $1")
        .bind(token_hash)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The stored password hash of one account.
pub async fn password_hash(db: &TestDb, email: &str) -> Option<String> {
    sqlx::query_scalar("SELECT password_hash FROM users WHERE email = $1::text::citext")
        .bind(email)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Drop the password hash of `email` with the admin pool.
///
/// The row that stays behind has the shape of an OAuth-only account: an address
/// and no password. The M5 call order refuses a password login against it
/// (specification section 3.3, "Login").
pub async fn clear_password_hash(db: &TestDb, email: &str) {
    sqlx::query("UPDATE users SET password_hash = NULL WHERE email = $1::text::citext")
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Whether `email` is a verified address.
pub async fn is_verified(db: &TestDb, email: &str) -> bool {
    let stamp: Option<DateTime<Utc>> =
        sqlx::query_scalar("SELECT email_verified_at FROM users WHERE email = $1::text::citext")
            .bind(email)
            .fetch_one(&db.admin)
            .await
            .unwrap();
    stamp.is_some()
}

/// `now` plus `secs`, for a seeded window.
pub fn shift(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(Utc::now().timestamp() + secs, 0).unwrap()
}

/// The fixed-window index of the cluster clock, for a window of `window_secs`.
///
/// A rate rule floors `now` to a window that starts at the 1970 epoch
/// (specification section 3.2). The expression below writes that formula in SQL,
/// so the guard never calls the function it guards.
pub async fn window_index(db: &TestDb, window_secs: i64) -> i64 {
    sqlx::query_scalar("SELECT floor(extract(epoch FROM now()) / $1)::bigint")
        .bind(window_secs)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// Delete every rate counter of this throwaway database.
pub async fn clear_rate_counters(db: &TestDb) {
    sqlx::query("DELETE FROM auth_rate_counters")
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Run `burst` inside ONE fixed window, and give the answer of that run.
///
/// **Why the guard exists.** The windows are fixed, not sliding: a burst that
/// crosses a boundary starts a second counter, the tally falls back to 1, and
/// the refusal the test asks for never comes. The shortest window is 300
/// seconds and a burst of 31 logins takes seconds, so the crossing is rare and
/// it is not impossible. A test that fails once a month is a test nobody trusts.
///
/// The guard reads the window index before and after the burst. Two equal
/// indices mean one window held the whole burst, and the answer stands. Two
/// different indices mean the window rolled: the guard clears the counters and
/// runs the burst again. A second crossing fails the test instead of looping.
pub async fn in_one_window<F, Fut, T>(db: &TestDb, window_secs: i64, mut burst: F) -> T
where
    F: FnMut() -> Fut,
    Fut: Future<Output = T>,
{
    let opened = window_index(db, window_secs).await;
    let answer = burst().await;
    if window_index(db, window_secs).await == opened {
        return answer;
    }

    clear_rate_counters(db).await;
    let opened = window_index(db, window_secs).await;
    let answer = burst().await;
    assert_eq!(
        window_index(db, window_secs).await,
        opened,
        "the burst crossed a window boundary twice"
    );
    answer
}

// ---------------------------------------------------------------------------
// M5 U5: the fake OAuth provider
// ---------------------------------------------------------------------------

/// The external origin the OAuth tests build the `redirect_uri` on.
pub const TEST_ORIGIN: &str = "https://tutor.example";

/// The Google client id the OAuth tests configure.
pub const GOOGLE_CLIENT_ID: &str = "u5-google-client-id";

/// The Google client secret the OAuth tests configure.
pub const GOOGLE_CLIENT_SECRET: &str = "u5-google-client-secret";

/// The GitHub client id the OAuth tests configure.
pub const GITHUB_CLIENT_ID: &str = "u5-github-client-id";

/// The GitHub client secret the OAuth tests configure.
pub const GITHUB_CLIENT_SECRET: &str = "u5-github-client-secret";

/// A provider that answers from a table instead of from a socket.
///
/// `cadus-web` opens no socket (R4, `tests/purity.rs`), so the production code
/// states each provider call as a `ProviderRequest` and hands it to an installed
/// transport. This fake IS that transport: it answers the URL of the request
/// from a table of canned pairs, and it records every request it saw. A test
/// then asserts on the exact bytes the service sent, which a socket server would
/// only hide behind one more parse.
///
/// An unknown URL is a transport error, so a test that forgets to stub an
/// endpoint fails instead of passing on a default.
pub struct FakeProvider {
    answers: Mutex<HashMap<String, (u16, Vec<u8>)>>,
    seen: Mutex<Vec<ProviderRequest>>,
}

impl FakeProvider {
    /// A provider with no canned answer.
    pub fn new() -> Self {
        Self {
            answers: Mutex::new(HashMap::new()),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Answer `url` with `status` and the bytes of `body`.
    pub fn answer(self, url: &str, status: u16, body: &str) -> Self {
        self.answers
            .lock()
            .unwrap()
            .insert(url.to_string(), (status, body.as_bytes().to_vec()));
        self
    }

    /// Every request the service made, in order.
    pub fn seen(&self) -> Vec<ProviderRequest> {
        self.seen.lock().unwrap().clone()
    }
}

impl ProviderTransport for FakeProvider {
    fn fetch<'a>(
        &'a self,
        request: ProviderRequest,
    ) -> Pin<Box<dyn Future<Output = Result<ProviderResponse, TransportError>> + Send + 'a>> {
        let found = self.answers.lock().unwrap().get(&request.url).cloned();
        let url = request.url.clone();
        self.seen.lock().unwrap().push(request);
        Box::pin(async move {
            match found {
                Some((status, body)) => Ok(ProviderResponse { status, body }),
                None => Err(TransportError {
                    reason: format!("the fake provider has no answer for {url}"),
                }),
            }
        })
    }
}

/// A Google-only configuration on `transport`.
pub fn google_config(transport: Arc<FakeProvider>) -> OAuthConfig {
    OAuthConfig {
        google: Some(Credentials {
            client_id: GOOGLE_CLIENT_ID.to_string(),
            client_secret: GOOGLE_CLIENT_SECRET.to_string(),
        }),
        github: None,
        redirect_base: Some(TEST_ORIGIN.to_string()),
        transport: Some(transport),
    }
}

/// A GitHub-only configuration on `transport`.
pub fn github_config(transport: Arc<FakeProvider>) -> OAuthConfig {
    OAuthConfig {
        google: None,
        github: Some(Credentials {
            client_id: GITHUB_CLIENT_ID.to_string(),
            client_secret: GITHUB_CLIENT_SECRET.to_string(),
        }),
        redirect_base: Some(TEST_ORIGIN.to_string()),
        transport: Some(transport),
    }
}

/// The application under test with `oauth` installed.
pub fn oauth_app(db: &TestDb, oauth: OAuthConfig) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_argon2(Argon2Profile::TEST)
            .with_oauth(oauth),
    )
}

/// A `GET` of `path` that carries `cookie` as the whole `Cookie` header.
pub fn get_with_cookie(path: &str, cookie: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(path)
        .header("cookie", cookie)
        .body(Body::empty())
        .unwrap()
}

/// The `Set-Cookie` headers of an answer, in order.
pub fn cookies_of(answer: &Answer) -> Vec<String> {
    answer
        .headers
        .get_all("set-cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_string)
        .collect()
}

/// The `Location` header of an answer.
pub fn location_of(answer: &Answer) -> String {
    answer
        .headers
        .get("location")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// How many `oauth_accounts` rows one account has.
pub async fn oauth_link_count(db: &TestDb, user: Uuid) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM oauth_accounts WHERE user_id = $1")
        .bind(user)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many accounts this database holds.
pub async fn user_count(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
// FIX-M5-C: the live session that the guarded route tests present
// ---------------------------------------------------------------------------

/// The session-cookie name of the production posture, written out.
pub const SESSION_COOKIE_NAME: &str = "__Host-cadus_session";

/// One raw token and the SHA-256 hex that `sha256sum` gives for it.
///
/// [`seed_learner`] checks [`sha256_hex`] against this pair before it writes a
/// row. A fixture that computes the wrong digest then fails HERE, and not later
/// with a `401` that names no cause.
const DIGEST_PROBE: (&str, &str) = (
    "fixc-probe",
    "156399f8f5f614d31f31688de9858f27ed32160a043f7bc9829d855b2696fe8b",
);

/// The SHA-256 digest of `text`, in lowercase hex.
///
/// The fixture hashes with `sha2` itself. It never calls the production
/// `hash_token`, so a broken `hash_token` fails the route tests instead of
/// agreeing with them.
fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The raw session token of `user`.
pub fn session_token_of(user: Uuid) -> String {
    format!("fixc-{user}")
}

/// Seed one learner AND one live session row on it.
///
/// The route tests present that session with [`present_session`], so the tenant
/// layer binds the learner exactly as it binds a browser. `created_at` and
/// `last_seen_at` are `now`, so the session is inside both windows of section
/// 10 and the layer writes no `last_seen_at` touch.
pub async fn seed_learner(db: &TestDb, email: &str) -> Uuid {
    assert_eq!(
        sha256_hex(DIGEST_PROBE.0),
        DIGEST_PROBE.1,
        "the fixture digest does not match the literal digest of {:?}",
        DIGEST_PROBE.0
    );
    let user = db.seed_user(email).await;
    let now = Utc::now();
    seed_session(
        db,
        user,
        &sha256_hex(&session_token_of(user)),
        now,
        now,
        shift(3600),
    )
    .await;
    user
}

/// Present the live session of `user` on `headers`.
///
/// The cookie is the ambient credential, so the request also carries the
/// same-origin signal a browser sends. Without it the CSRF layer refuses every
/// write and no test reaches a handler.
pub fn present_session(headers: &mut HeaderMap, user: Uuid) {
    let cookie = format!("{SESSION_COOKIE_NAME}={}", session_token_of(user));
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        headers.insert("cookie", value);
    }
    headers.insert("sec-fetch-site", HeaderValue::from_static("same-origin"));
}

/// The id of the account that `email` names, or `None`.
pub async fn maybe_user_id(db: &TestDb, email: &str) -> Option<Uuid> {
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1::text::citext")
        .bind(email)
        .fetch_optional(&db.admin)
        .await
        .unwrap()
}
