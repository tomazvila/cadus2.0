//! The fixtures of `tests/auth_oauth_callback.rs` and its parts.

use std::sync::Arc;

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
