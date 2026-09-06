//! The recovery pair and the body readers under the refusals their first
//! files left open: a field that is absent, an account with no password, a
//! token whose account read fails or finds nothing, a spend that touches no
//! row, and a commit that fails.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::future::Future;
use std::pin::Pin;

use axum::body::Body;
use common::*;

/// Send `request` and read the `422 invalid_request` envelope back.
async fn unprocessable(app: &Router, request: Request<Body>) {
    let answer = send(app, request).await;
    assert_eq!(answer.status.as_u16(), 422, "{}", answer.body);
    assert_eq!(answer.code(), "invalid_request");
}

/// Send `request` and read the status and the error code back.
async fn refused(app: &Router, request: Request<Body>, status: u16, code: &str) {
    let answer = send(app, request).await;
    assert_eq!(answer.status.as_u16(), status, "{}", answer.body);
    assert_eq!(answer.code(), code);
}

/// The reset body of the first reset token.
fn reset_body() -> Value {
    json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD })
}

/// The verification body of the first verification token.
fn verify_body() -> Value {
    json!({ "token": VERIFY_TOKEN_ONE.0 })
}

/// Register `email` and seed a reset token and a verification token on it.
async fn learner_with_both_tokens(db: &TestDb, app: &Router, email: &str) -> Uuid {
    signup(app, email, GOOD_PASSWORD).await;
    let user = user_id(db, email).await;
    seed_token(db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
    seed_token(db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;
    user
}

/// A body with a string field absent is `422` on every route that reads it.
#[tokio::test]
async fn a_body_with_a_field_absent_is_422_on_every_reader() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "fields@example.com", GOOD_PASSWORD).await;
        let email = json!({ "email": "fields@example.com" });
        unprocessable(&app, post("/api/auth/signup", &email)).await;
        unprocessable(&app, post("/api/auth/login", &email)).await;
        unprocessable(
            &app,
            post_bearer("/api/auth/password/change", &token, &json!({})),
        )
        .await;
        unprocessable(
            &app,
            post_bearer(
                "/api/auth/password/change",
                &token,
                &json!({ "current_password": GOOD_PASSWORD }),
            ),
        )
        .await;
        unprocessable(
            &app,
            post(
                "/api/auth/password/reset",
                &json!({ "new_password": OTHER_PASSWORD }),
            ),
        )
        .await;
    })
    .await;
}

/// An account with no password cannot change one: the current password
/// matches nothing, so the answer is the uniform `401`.
#[tokio::test]
async fn an_account_with_no_password_cannot_change_it() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "federated@example.com", GOOD_PASSWORD).await;
        clear_password_hash(&db, "federated@example.com").await;
        refused(
            &app,
            post_bearer(
                "/api/auth/password/change",
                &token,
                &json!({ "current_password": GOOD_PASSWORD, "new_password": OTHER_PASSWORD }),
            ),
            401,
            "invalid_credentials",
        )
        .await;
    })
    .await;
}

/// One fault a test applies to the database before its request.
type BoxFault<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Register `email` with both tokens, apply `fault`, and read `status` and
/// `code` back from the reset (`reset` true) or the verification.
async fn token_route_refused(
    email: &'static str,
    fault: fn(&TestDb) -> BoxFault<'_>,
    reset: bool,
    status: u16,
    code: &'static str,
) {
    TestDb::with(move |db| async move {
        let app = app_of(&db);
        learner_with_both_tokens(&db, &app, email).await;
        fault(&db).await;
        let request = if reset {
            post("/api/auth/password/reset", &reset_body())
        } else {
            post("/api/auth/verify-email", &verify_body())
        };
        refused(&app, request, status, code).await;
    })
    .await;
}

/// A reset or a verification whose account read fails is `500`; a spend that
/// touches no row, a verification whose account read finds nothing, a disabled
/// account, and an address already verified are each `400 invalid_token`.
#[tokio::test]
async fn the_token_routes_refuse_the_reads_and_spends_that_go_wrong() {
    let dropped: fn(&TestDb) -> BoxFault<'_> =
        |db| Box::pin(drop_function(db, "auth_user_by_id(uuid)"));
    token_route_refused(
        "read-fault@example.com",
        dropped,
        true,
        500,
        "internal_error",
    )
    .await;
    token_route_refused(
        "verify-fault@example.com",
        dropped,
        false,
        500,
        "internal_error",
    )
    .await;
    token_route_refused(
        "hidden@example.com",
        |db| Box::pin(hide_user_by_id(db)),
        false,
        400,
        "invalid_token",
    )
    .await;
    token_route_refused(
        "skipped@example.com",
        |db| Box::pin(skip_updates(db, "auth_tokens", "OLD.consumed_at IS NULL")),
        true,
        400,
        "invalid_token",
    )
    .await;
    token_route_refused(
        "disabled@example.com",
        |db| Box::pin(disable(db, "disabled@example.com")),
        false,
        400,
        "invalid_token",
    )
    .await;
    token_route_refused(
        "verified@example.com",
        |db| Box::pin(mark_verified(db, "verified@example.com")),
        false,
        400,
        "invalid_token",
    )
    .await;
}

/// A reset whose session sweep does not commit, and one whose bind fails after
/// the spend, are both `500`.
#[tokio::test]
async fn a_reset_whose_sweep_fails_is_500() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        verified_login(&db, &app, "sweep@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "sweep@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        fail_commit_after(&db, "auth_sessions", "DELETE", "true").await;
        refused(
            &app,
            post("/api/auth/password/reset", &reset_body()),
            500,
            "internal_error",
        )
        .await;
    })
    .await;
    TestDb::with(|db| async move {
        let app = app_of(&db);
        learner_with_both_tokens(&db, &app, "bind@example.com").await;
        fail_tenant_bind(&db).await;
        refused(
            &app,
            post("/api/auth/password/reset", &reset_body()),
            500,
            "internal_error",
        )
        .await;
    })
    .await;
}
