//! FIX-M5-C: the live session that the guarded route tests present.

use axum::http::{HeaderMap, HeaderValue};
use cadus_store::test_support::TestDb;
use sha2::{Digest, Sha256};
use sqlx::types::Uuid;
use sqlx::types::chrono::Utc;

use super::{seed_session, shift};

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
