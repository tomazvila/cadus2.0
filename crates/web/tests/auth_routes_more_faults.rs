//! The account routes under the faults their first fault file left open: a
//! tenant bind that fails, a commit that fails, a session lookup that fails,
//! a cookie the posture cannot write, an Argon2 profile that cannot hash, a
//! stored hash that needs a rehash, and a bound account read that sees no row.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::HeaderValue;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::auth::password::{Argon2Profile, hash_password};
use cadus_web::cookie::CookiePosture;
use cadus_web::{AppState, create_app};
use common::*;

/// The login body of `email`.
fn login_body(email: &str) -> Value {
    json!({ "email": email, "password": GOOD_PASSWORD })
}

/// Read `500` back from the sign-out everywhere and then from the plain
/// sign-out, both with `token`.
///
/// The order matters once: a cookie build that fails comes AFTER the delete
/// commits, so the sign-out everywhere goes first and the plain sign-out then
/// builds the same cookie with no session left.
async fn both_signouts_are_500(app: &Router, token: &str) {
    assert_internal_answer(app, post_bearer("/api/auth/logout-all", token, &json!({}))).await;
    assert_internal_answer(app, post_bearer("/api/auth/logout", token, &json!({}))).await;
}

/// The password-change body every test here sends.
fn change_body() -> Value {
    json!({ "current_password": GOOD_PASSWORD, "new_password": OTHER_PASSWORD })
}

/// The application on `db` under `profile` and `posture`, so a test drives
/// the hash paths and the cookie writer with values a deployment could set.
fn app_under(db: &TestDb, profile: Argon2Profile, posture: CookiePosture) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_argon2(profile)
            .with_posture(posture),
    )
}

/// A cookie name with a space, which no `Set-Cookie` can carry.
const BAD_POSTURE: CookiePosture = CookiePosture {
    name: "bad name",
    secure: true,
};

/// A parameter set Argon2 refuses: one kibibyte of memory over four lanes.
const ILLEGAL_PROFILE: Argon2Profile = Argon2Profile {
    name: "illegal",
    time_cost: 0,
    memory_cost_kib: 1,
    parallelism: 4,
};

/// Replace the stored hash of `email` with the admin pool.
async fn set_password_hash(db: &TestDb, email: &str, hash: &str) {
    sqlx::query("UPDATE users SET password_hash = $1 WHERE email = $2::text::citext")
        .bind(hash)
        .bind(email)
        .execute(&db.admin)
        .await
        .unwrap();
}

/// A tenant bind that fails stops every bound account route at its bind, and
/// the sign-up at the token it mints after the account row.
#[tokio::test]
async fn a_tenant_bind_that_fails_is_500_on_the_bound_account_routes() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "bind@example.com", GOOD_PASSWORD).await;
        fail_tenant_bind(&db).await;
        both_signouts_are_500(&app, &token).await;
        assert_internal_answer(&app, get_bearer("/api/auth/me", &token)).await;
        assert_internal_answer(
            &app,
            post_bearer("/api/auth/password/change", &token, &change_body()),
        )
        .await;
        assert_internal_answer(
            &app,
            post("/api/auth/signup", &signup_body("minted@example.com")),
        )
        .await;
    })
    .await;
}

/// A commit that fails rolls each account write back: the two sign-outs, the
/// password change, the token the sign-up mints, and the session the login
/// opens.
#[tokio::test]
async fn a_commit_that_fails_is_500_on_the_account_writes() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "commit-out@example.com", GOOD_PASSWORD).await;
        // Each write rolls back at its commit, so the one session stays for
        // the next call; the change sweeps no other session, so only its
        // password write meets a trigger.
        fail_commit_after(&db, "auth_sessions", "DELETE", "true").await;
        fail_commit_after(
            &db,
            "users",
            "UPDATE",
            "NEW.password_hash IS DISTINCT FROM OLD.password_hash",
        )
        .await;
        both_signouts_are_500(&app, &token).await;
        assert_internal_answer(
            &app,
            post_bearer("/api/auth/password/change", &token, &change_body()),
        )
        .await;
    })
    .await;
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "commit-in@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "commit-in@example.com").await;
        fail_commit_after(&db, "auth_tokens", "INSERT", "true").await;
        assert_internal_answer(
            &app,
            post("/api/auth/signup", &signup_body("commit-mint@example.com")),
        )
        .await;
        fail_commit_after(&db, "auth_sessions", "INSERT", "true").await;
        assert_internal_answer(
            &app,
            post("/api/auth/login", &login_body("commit-in@example.com")),
        )
        .await;
    })
    .await;
}

/// A session lookup that fails stops the sign-out, and a token no row carries
/// still ends with a cleared cookie.
#[tokio::test]
async fn a_session_lookup_that_fails_or_finds_nothing_on_the_logout() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let answer = send(
            &app,
            post_bearer("/api/auth/logout", "no-such-token", &json!({})),
        )
        .await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert!(answer.cookie().is_some());

        let token = verified_login(&db, &app, "lookup@example.com", GOOD_PASSWORD).await;
        drop_function(&db, "auth_session_by_token_hash(text)").await;
        assert_internal_answer(&app, post_bearer("/api/auth/logout", &token, &json!({}))).await;
    })
    .await;
}

/// A cookie the posture cannot write is `500` on the login and on both
/// sign-outs, and the login writes no session before it.
#[tokio::test]
async fn a_cookie_the_posture_cannot_write_is_500_on_the_cookie_routes() {
    TestDb::with(|db| async move {
        let app = app_under(&db, Argon2Profile::TEST, BAD_POSTURE);
        let registered = signup(&app, "posture@example.com", GOOD_PASSWORD).await;
        assert_eq!(registered.status.as_u16(), 200, "{}", registered.body);
        mark_verified(&db, "posture@example.com").await;
        assert_internal_answer(
            &app,
            post("/api/auth/login", &login_body("posture@example.com")),
        )
        .await;

        let user = user_id(&db, "posture@example.com").await;
        seed_live_session(&db, user, SESSION_TOKEN_ONE.1).await;
        both_signouts_are_500(&app, SESSION_TOKEN_ONE.0).await;
    })
    .await;
}

/// An Argon2 profile that cannot hash is `500` on the sign-up and on the
/// rehash of a login, and an unknown address still takes the uniform `401`
/// with no dummy hash to verify against.
#[tokio::test]
async fn an_argon2_profile_that_cannot_hash_is_500_on_the_hash_paths() {
    TestDb::with(|db| async move {
        verified_login(&db, &app_of(&db), "illegal@example.com", GOOD_PASSWORD).await;
        let app = app_under(&db, ILLEGAL_PROFILE, CookiePosture::SECURE);
        assert_internal_answer(
            &app,
            post("/api/auth/login", &login_body("illegal@example.com")),
        )
        .await;
        assert_internal_answer(
            &app,
            post("/api/auth/signup", &signup_body("fresh@example.com")),
        )
        .await;
        let answer = send(
            &app,
            post("/api/auth/login", &login_body("nobody@example.com")),
        )
        .await;
        assert_eq!(answer.status.as_u16(), 401, "{}", answer.body);
        assert_eq!(answer.code(), "invalid_credentials");
    })
    .await;
}

/// A stored hash of another profile is written again at the login, and a
/// rehash write that fails stops that login.
#[tokio::test]
async fn a_login_rehashes_a_stored_hash_of_another_profile() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let other = Argon2Profile {
            name: "other",
            time_cost: 1,
            memory_cost_kib: 16 * 1024,
            parallelism: 1,
        };
        let stale = hash_password(other, GOOD_PASSWORD).unwrap();
        for email in ["rehash@example.com", "rehash-fault@example.com"] {
            signup(&app, email, GOOD_PASSWORD).await;
            mark_verified(&db, email).await;
            set_password_hash(&db, email, &stale).await;
        }

        let answer = send(
            &app,
            post("/api/auth/login", &login_body("rehash@example.com")),
        )
        .await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);
        assert_ne!(
            password_hash(&db, "rehash@example.com").await.as_deref(),
            Some(stale.as_str()),
            "the login writes a hash of the active profile"
        );

        fail_updates(
            &db,
            "users",
            "NEW.password_hash IS DISTINCT FROM OLD.password_hash",
        )
        .await;
        assert_internal_answer(
            &app,
            post("/api/auth/login", &login_body("rehash-fault@example.com")),
        )
        .await;
    })
    .await;
}

/// A bound account read that sees no row after the session insert is `500`,
/// and a `User-Agent` reaches the session row of a login that succeeds.
#[tokio::test]
async fn a_bound_account_read_that_sees_no_row_is_500_on_the_sign_in() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "unseen@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "unseen@example.com").await;

        let mut request = post("/api/auth/login", &login_body("unseen@example.com"));
        request
            .headers_mut()
            .insert("user-agent", HeaderValue::from_static("cadus-test/1"));
        let answer = send(&app, request).await;
        assert_eq!(answer.status.as_u16(), 200, "{}", answer.body);

        hide_rows(&db, "users", "email::text AS").await;
        assert_internal_answer(
            &app,
            post("/api/auth/login", &login_body("unseen@example.com")),
        )
        .await;
    })
    .await;
}
