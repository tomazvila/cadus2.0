//! `/api/auth/*` under a store fault.
//!
//! Each test makes ONE statement of a route fail on purpose, and reads the
//! `500 internal_error` envelope back. The faults are triggers and row
//! policies on the throwaway database; no test double stands between the
//! handler and Postgres.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::body::Body;
use common::*;

/// Send `request` and read the `500 internal_error` envelope back.
async fn assert_internal_answer(app: &Router, request: Request<Body>) -> Answer {
    let answer = send(app, request).await;
    assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
    assert_eq!(answer.code(), "internal_error");
    answer
}

/// The sign-up body of `email`.
fn signup_body(email: &str) -> Value {
    json!({ "email": email, "password": GOOD_PASSWORD })
}

/// A user write that fails stops the sign-up at the account insert.
#[tokio::test]
async fn a_user_write_that_fails_is_500_on_the_signup() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        fail_writes(&db, "users", "true").await;
        assert_internal_answer(
            &app,
            post("/api/auth/signup", &signup_body("a@example.com")),
        )
        .await;
        assert_eq!(user_count(&db).await, 0);
    })
    .await;
}

/// A token write that fails stops the sign-up, the forgot and the resend at
/// the token insert.
#[tokio::test]
async fn a_token_write_that_fails_is_500_on_the_token_minters() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "minted@example.com", GOOD_PASSWORD).await;
        fail_writes(&db, "auth_tokens", "true").await;
        assert_internal_answer(
            &app,
            post("/api/auth/signup", &signup_body("b@example.com")),
        )
        .await;
        let body = json!({ "email": "minted@example.com" });
        assert_internal_answer(&app, post("/api/auth/password/forgot", &body)).await;
        assert_internal_answer(&app, post("/api/auth/verify-email/resend", &body)).await;
    })
    .await;
}

/// A token sweep that fails stops the forgot before the insert.
#[tokio::test]
async fn a_token_sweep_that_fails_is_500_on_the_forgot() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "swept@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "swept@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        fail_deletes(&db, "auth_tokens", "true").await;
        let body = json!({ "email": "swept@example.com" });
        assert_internal_answer(&app, post("/api/auth/password/forgot", &body)).await;
    })
    .await;
}

/// A user read that fails stops the login and `me` at the bound account read.
#[tokio::test]
async fn a_user_read_that_fails_is_500_on_the_account_lookups() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "read@example.com", GOOD_PASSWORD).await;
        fail_reads(&db, "users", "FROM users").await;
        let body = json!({ "email": "read@example.com", "password": GOOD_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/login", &body)).await;
        assert_internal_answer(&app, get_bearer("/api/auth/me", &token)).await;
    })
    .await;
}

/// A session insert that fails stops the login and the verification sign-in.
#[tokio::test]
async fn a_session_insert_that_fails_is_500_on_the_sign_ins() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "insert@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "insert@example.com").await;
        signup(&app, "verify@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "verify@example.com").await;
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;
        fail_writes(&db, "auth_sessions", "true").await;
        let body = json!({ "email": "insert@example.com", "password": GOOD_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/login", &body)).await;
        assert_internal_answer(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_ONE.0 }),
            ),
        )
        .await;
    })
    .await;
}

/// A session delete that fails stops the logout, the logout-all, the
/// password change and the reset.
#[tokio::test]
async fn a_session_delete_that_fails_is_500_on_the_session_sweeps() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "sweep@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "sweep@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        fail_deletes(&db, "auth_sessions", "true").await;
        // A second session, so the sweep of the password change deletes one.
        seed_session(
            &db,
            user,
            SESSION_TOKEN_TWO.1,
            shift(0),
            shift(0),
            shift(3600),
        )
        .await;
        assert_internal_answer(&app, post_bearer("/api/auth/logout", &token, &json!({}))).await;
        assert_internal_answer(
            &app,
            post_bearer("/api/auth/logout-all", &token, &json!({})),
        )
        .await;
        let change = json!({ "current_password": GOOD_PASSWORD, "new_password": OTHER_PASSWORD });
        assert_internal_answer(
            &app,
            post_bearer("/api/auth/password/change", &token, &change),
        )
        .await;
        let reset = json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/password/reset", &reset)).await;
    })
    .await;
}

/// A password write that fails stops the password change.
#[tokio::test]
async fn a_password_write_that_fails_is_500_on_the_change() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "write@example.com", GOOD_PASSWORD).await;
        fail_writes(&db, "users", "true").await;
        let change = json!({ "current_password": GOOD_PASSWORD, "new_password": OTHER_PASSWORD });
        assert_internal_answer(
            &app,
            post_bearer("/api/auth/password/change", &token, &change),
        )
        .await;
    })
    .await;
}

/// A token spend that fails stops the reset and the verification.
#[tokio::test]
async fn a_token_spend_that_fails_is_500_on_the_reset_and_the_verification() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "spend@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "spend@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;
        fail_writes(&db, "auth_tokens", "NEW.consumed_at IS NOT NULL").await;
        let reset = json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/password/reset", &reset)).await;
        assert_internal_answer(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_ONE.0 }),
            ),
        )
        .await;
    })
    .await;
}

/// A verification stamp that fails stops the reset of an unverified address.
#[tokio::test]
async fn a_verification_stamp_that_fails_is_500_on_the_reset() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "stamp@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "stamp@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        fail_writes(&db, "users", "NEW.email_verified_at IS NOT NULL").await;
        let reset = json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/password/reset", &reset)).await;
    })
    .await;
}

/// A rate-counter write that fails stops the sign-up at its rate rule, before
/// any account lookup.
#[tokio::test]
async fn a_rate_counter_write_that_fails_is_500_on_the_signup() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        fail_writes(&db, "auth_rate_counters", "true").await;
        assert_internal_answer(
            &app,
            post("/api/auth/signup", &signup_body("rate@example.com")),
        )
        .await;
        assert_eq!(user_count(&db).await, 0);
    })
    .await;
}
