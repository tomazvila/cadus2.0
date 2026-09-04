//! The fixtures of `tests/auth_oauth_callback.rs` and its parts.

use std::sync::Arc;

use axum::Router;

use super::*;
use cadus_store::test_support::TestDb;
use sqlx::types::Uuid;

/// The Google token endpoint.
pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// The Google userinfo endpoint.
pub const GOOGLE_USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";

/// The GitHub token endpoint.
pub const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// The GitHub account endpoint.
pub const GITHUB_USER_URL: &str = "https://api.github.com/user";

/// The GitHub address-list endpoint.
pub const GITHUB_EMAILS_URL: &str = "https://api.github.com/user/emails";

/// The base64url of
/// `{"next":"/dashboard","p":"google","s":"u5-state-value","v":"u5-pkce-verifier"}`.
pub const GOOGLE_COOKIE: &str = "eyJuZXh0IjoiL2Rhc2hib2FyZCIsInAiOiJnb29nbGUiLCJzIjoidTUtc3RhdGUtdmFsdWUiLCJ2IjoidTUtcGtjZS12ZXJpZmllciJ9";

/// The base64url of
/// `{"next":"/","p":"github","s":"u5-github-state","v":"u5-github-verifier"}`.
pub const GITHUB_COOKIE: &str = "eyJuZXh0IjoiLyIsInAiOiJnaXRodWIiLCJzIjoidTUtZ2l0aHViLXN0YXRlIiwidiI6InU1LWdpdGh1Yi12ZXJpZmllciJ9";

/// The whole `Cookie` header of a Google callback.
pub fn google_jar() -> String {
    format!("cadus_oauth_handshake={GOOGLE_COOKIE}")
}

/// A Google provider that answers a verified identity.
pub fn google_verified() -> FakeProvider {
    FakeProvider::new()
        .answer(
            GOOGLE_TOKEN_URL,
            200,
            r#"{"access_token":"u5-google-access","token_type":"Bearer"}"#,
        )
        .answer(
            GOOGLE_USERINFO_URL,
            200,
            r#"{"sub":"google-subject-1","email":"Learner@Example.com","email_verified":true}"#,
        )
}

/// How many session rows this database holds.
pub async fn session_rows(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_sessions")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many session rows carry `token_hash` (0 or 1).
pub async fn seeded_session_rows(db: &TestDb, token_hash: &str) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_sessions WHERE token_hash = $1")
        .bind(token_hash)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many link rows this database holds.
pub async fn link_rows(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM oauth_accounts")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The `(provider, provider_account_id, email_at_link)` of one account's link.
pub async fn link_of(db: &TestDb, user: Uuid) -> (String, String, String) {
    sqlx::query_as(
        "SELECT provider, provider_account_id, email_at_link FROM oauth_accounts WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The `Set-Cookie` header whose name is `name`.
pub fn cookie_named(answer: &Answer, name: &str) -> String {
    cookies_of(answer)
        .into_iter()
        .find(|cookie| cookie.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no Set-Cookie named {name} in {:?}", cookies_of(answer)))
}

/// The default Google callback query: the code and the `state` of
/// [`GOOGLE_COOKIE`].
pub const GOOGLE_QUERY: &str = "code=u5-code&state=u5-state-value";

/// A GitHub provider that answers the token, the account `user_doc`, and the
/// address list `emails_doc`.
pub fn github_provider(user_doc: &str, emails_doc: &str) -> FakeProvider {
    FakeProvider::new()
        .answer(
            GITHUB_TOKEN_URL,
            200,
            r#"{"access_token":"u5-github-access"}"#,
        )
        .answer(GITHUB_USER_URL, 200, user_doc)
        .answer(GITHUB_EMAILS_URL, 200, emails_doc)
}

/// A Google provider that answers the token and the identity `userinfo`.
pub fn google_provider(userinfo: &str) -> FakeProvider {
    FakeProvider::new()
        .answer(
            GOOGLE_TOKEN_URL,
            200,
            r#"{"access_token":"u5-google-access"}"#,
        )
        .answer(GOOGLE_USERINFO_URL, 200, userinfo)
}

/// The router of a test that serves Google through `provider`.
pub fn google_app(db: &TestDb, provider: Arc<FakeProvider>) -> Router {
    oauth_app(db, google_config(provider))
}

/// `GET /api/auth/oauth/{provider}/callback?{query}` with the handshake `jar`.
pub async fn callback_with(app: &Router, provider: &str, query: &str, jar: &str) -> Answer {
    send(
        app,
        get_with_cookie(&format!("/api/auth/oauth/{provider}/callback?{query}"), jar),
    )
    .await
}

/// The Google callback with `query` and the Google handshake cookie.
pub async fn google_callback(app: &Router, query: &str) -> Answer {
    callback_with(app, "google", query, &google_jar()).await
}

/// The GitHub callback with the code and the `state` of [`GITHUB_COOKIE`].
pub async fn github_callback(db: &TestDb, provider: FakeProvider) -> Answer {
    let app = oauth_app(db, github_config(Arc::new(provider)));
    callback_with(
        &app,
        "github",
        "code=u5-code&state=u5-github-state",
        &format!("cadus_oauth_handshake={GITHUB_COOKIE}"),
    )
    .await
}

/// Fail the test when `answer` is not `400 oauth_error`.
pub fn assert_oauth_error(answer: &Answer) {
    assert_eq!(answer.status.as_u16(), 400);
    assert_eq!(answer.code(), "oauth_error");
}

/// Fail the test when `answer` is not the `400 oauth_error` of a state
/// mismatch.
pub fn assert_state_mismatch(answer: &Answer) {
    assert_oauth_error(answer);
    assert_eq!(
        answer.body["error"]["message"],
        "OAuth state mismatch; please start sign-in again."
    );
}

/// Run the default Google callback against `provider`, and read the
/// `400 oauth_error` back.
pub async fn refused_google_callback(db: &TestDb, provider: FakeProvider) -> Answer {
    let app = google_app(db, Arc::new(provider));
    let answer = google_callback(&app, GOOGLE_QUERY).await;
    assert_oauth_error(&answer);
    answer
}

/// Give `user` a live session, run the default Google callback on `app`, and
/// read back that the callback linked into that same account: `302`, one
/// account, the same id.
pub async fn link_existing_account(db: &TestDb, app: &Router, user: Uuid) -> Answer {
    seed_session(
        db,
        user,
        SESSION_TOKEN_ONE.1,
        shift(0),
        shift(0),
        shift(3600),
    )
    .await;
    let answer = google_callback(app, GOOGLE_QUERY).await;
    assert_eq!(answer.status.as_u16(), 302);
    assert_eq!(user_count(db).await, 1, "no second account was created");
    assert_eq!(user_id(db, "learner@example.com").await, user);
    answer
}
