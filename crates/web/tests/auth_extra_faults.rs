//! The store-fault and shape edges of the auth routes past the earlier sets:
//! the OAuth exchange and account resolution, the session bind of a login, the
//! token lookup of a reset, the second verification of an account, and the
//! account read of `GET /api/account`.
//!
//! Every fault is `500 internal_error`, or the `400`/`401` the shape names.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

/// The userinfo fetch of the OAuth exchange fails: the provider answers the
/// token endpoint and nothing else, so the exchange gives up and the callback
/// is `400 oauth_error`.
#[tokio::test]
async fn a_userinfo_fetch_that_fails_refuses_the_callback() {
    TestDb::with(|db| async move {
        let provider = FakeProvider::new().answer(
            GOOGLE_TOKEN_URL,
            200,
            r#"{"access_token":"u5-google-access","token_type":"Bearer"}"#,
        );
        let app = google_app(&db, Arc::new(provider));

        let answer = google_callback(&app, GOOGLE_QUERY).await;
        assert_eq!(answer.status.as_u16(), 400, "{}", answer.body);
        assert_eq!(answer.code(), "oauth_error");
    })
    .await;
}

/// The session bind of a login fails: the login is `500`. The bind is the
/// first tenant transaction of the login path.
#[tokio::test]
async fn a_session_bind_that_fails_is_500_on_the_login() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        verified_login(&db, &app, "bind-login@example.com", GOOD_PASSWORD).await;
        fail_tenant_bind(&db).await;

        let body = json!({ "email": "bind-login@example.com", "password": GOOD_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/login", &body)).await;
    })
    .await;
}

/// The token lookup of a reset fails: the reset is `500`. The lookup is the
/// first read of the token routes.
#[tokio::test]
async fn a_token_lookup_that_fails_is_500_on_the_reset() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "lookup@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "lookup@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        drop_function(&db, "auth_token_by_hash(text)").await;

        let body = json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/password/reset", &body)).await;
    })
    .await;
}

/// A verification of an already-verified account is `400 invalid_token`: the
/// link both activates and signs in, so a second use finds nothing to do.
#[tokio::test]
async fn a_second_verification_is_invalid_token() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "reverify@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "reverify@example.com").await;
        // The account is verified, and a fresh verification token still stands.
        sqlx::query("UPDATE users SET email_verified_at = now() WHERE id = $1")
            .bind(user)
            .execute(&db.admin)
            .await
            .unwrap();
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;

        let answer = send(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_ONE.0 }),
            ),
        )
        .await;
        assert_eq!(answer.status.as_u16(), 400, "{}", answer.body);
        assert_eq!(answer.code(), "invalid_token");
    })
    .await;
}

/// The account read of `GET /api/account` finds no row: the session is live,
/// but the account row is hidden, so the route is `401 unauthorized`.
#[tokio::test]
async fn an_account_read_that_finds_no_row_is_401() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "gone@example.com", GOOD_PASSWORD).await;
        // The session lookup is a SECURITY DEFINER function, so it still finds
        // the session; the account read runs under row-level security, so this
        // policy hides its row alone.
        hide_rows(&db, "users", "email_verified_at, created_at").await;

        let answer = send(&app, get_bearer("/api/auth/me", &token)).await;
        assert_eq!(answer.status.as_u16(), 401, "{}", answer.body);
        assert_eq!(answer.code(), "unauthorized");
    })
    .await;
}
