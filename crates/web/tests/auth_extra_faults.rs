//! The store-fault and shape edges of the auth routes past the earlier sets:
//! the OAuth exchange and account resolution, the session bind of a login, the
//! token lookup of a reset, the second verification of an account, and the
//! account read of `GET /api/account`.
//!
//! Every fault is `500 internal_error`, or the `400`/`401` the shape names.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::auth::password::Argon2Profile;
use cadus_web::{AppState, create_app};

/// Install a db-scoped tenant-bind fault: the `set_config` the tenant bind
/// runs succeeds `passes` times and then raises. The shadow lives in a schema
/// ahead of `pg_catalog` on this database's `cadus_app` role, so a connection
/// opened after this picks it up. Returns a one-connection app whose single
/// session carries the shadow.
async fn app_whose_bind_fails_after(db: &TestDb, passes: i64) -> Router {
    for statement in [
        "CREATE SCHEMA fault".to_string(),
        "CREATE SEQUENCE fault.bind_calls".to_string(),
        format!(
            "CREATE FUNCTION fault.set_config(text, text, boolean) RETURNS text \
             LANGUAGE plpgsql AS $$ BEGIN \
             IF nextval('fault.bind_calls') > {passes} THEN \
             RAISE EXCEPTION 'injected tenant bind fault'; END IF; \
             RETURN pg_catalog.set_config($1, $2, $3); END $$"
        ),
        "GRANT USAGE ON SCHEMA fault TO cadus_app".to_string(),
        "GRANT EXECUTE ON FUNCTION fault.set_config(text, text, boolean) TO cadus_app".to_string(),
        "GRANT USAGE, UPDATE ON SEQUENCE fault.bind_calls TO cadus_app".to_string(),
        format!(
            "ALTER ROLE cadus_app IN DATABASE \"{}\" \
             SET search_path = fault, public, pg_catalog",
            db.name
        ),
    ] {
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&db.admin)
            .await
            .unwrap();
    }
    let pool = db.pool_as("cadus_app", 1).await;
    create_app(
        AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS)).with_argon2(Argon2Profile::TEST),
    )
}

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

/// The tenant bind of a reset fails after the token-spend bind succeeded: the
/// reset is `500`. The reset binds twice — once to spend the token, once to
/// sweep the sessions — so a fault after the first bind hits the second.
#[tokio::test]
async fn a_session_sweep_bind_that_fails_is_500_on_the_reset() {
    TestDb::with(|db| async move {
        let setup = app_of(&db);
        signup(&setup, "sweepbind@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "sweepbind@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;

        let app = app_whose_bind_fails_after(&db, 1).await;
        let body = json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD });
        assert_internal_answer(&app, post("/api/auth/password/reset", &body)).await;
    })
    .await;
}
