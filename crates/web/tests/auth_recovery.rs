//! `/api/auth/*`, part 2: password change, password forgot, password reset,
//! verify-email, and verify-email/resend.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1 (rows "Reset /
//! verify TTL" and "Password policy"), section 3.2 (the forgot and resend
//! rules), section 3.3 ("Password reset / email verify"), and section 10 (rows
//! "Bad / reused token", "Weak password / wrong current password",
//! "Login / forgot / resend limits", "Lifetimes").
//!
//! Every expected value is a LITERAL of this file. The token digests come from
//! `tests/common/mod.rs`, where each one is the recorded SHA-256 hex of a raw
//! token, so a seeded row and the presented token agree only when the production
//! digest is right.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use common::*;
use serde_json::json;
use sqlx::types::chrono::Utc;

// Password change
// ---------------------------------------------------------------------------

/// (1) A wrong current password is `401 invalid_credentials`, and the stored
/// hash does not move.
#[tokio::test]
async fn a_wrong_current_password_is_401_invalid_credentials() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "pw@example.com", GOOD_PASSWORD).await;
        let before = password_hash(&db, "pw@example.com").await;

        let answer = send(
            &app,
            post_bearer(
                "/api/auth/password/change",
                &token,
                &json!({ "current_password": "wrong one", "new_password": OTHER_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "invalid_credentials");
        assert_eq!(password_hash(&db, "pw@example.com").await, before);
    })
    .await;
}

/// (2) A new password below the floor is `422 weak_password`.
#[tokio::test]
async fn a_weak_new_password_is_422_weak_password() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "pw@example.com", GOOD_PASSWORD).await;

        let answer = send(
            &app,
            post_bearer(
                "/api/auth/password/change",
                &token,
                &json!({ "current_password": GOOD_PASSWORD, "new_password": SHORT_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 422);
        assert_eq!(answer.code(), "weak_password");
    })
    .await;
}

/// (3) A change keeps THIS session and ends every other one.
#[tokio::test]
async fn a_password_change_keeps_this_session_and_ends_the_others() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let keep = verified_login(&db, &app, "pw@example.com", GOOD_PASSWORD).await;
        let other = login(&app, "pw@example.com", GOOD_PASSWORD).await;
        let other = other.body["session_token"].as_str().unwrap().to_string();
        let user = user_id(&db, "pw@example.com").await;
        assert_eq!(session_count(&db, user).await, 2);

        let answer = send(
            &app,
            post_bearer(
                "/api/auth/password/change",
                &keep,
                &json!({ "current_password": GOOD_PASSWORD, "new_password": OTHER_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(answer.body, json!({ "ok": true }));
        assert_eq!(session_count(&db, user).await, 1);
        assert_eq!(
            send(&app, get_bearer("/api/auth/me", &keep))
                .await
                .status
                .as_u16(),
            200
        );
        assert_eq!(
            send(&app, get_bearer("/api/auth/me", &other))
                .await
                .status
                .as_u16(),
            401
        );
        // The new password works and the old one does not.
        assert_eq!(
            login(&app, "pw@example.com", OTHER_PASSWORD)
                .await
                .status
                .as_u16(),
            200
        );
        assert_eq!(
            login(&app, "pw@example.com", GOOD_PASSWORD)
                .await
                .status
                .as_u16(),
            401
        );
    })
    .await;
}

/// (4) A password change without a session is `401 unauthorized`, not
/// `invalid_credentials`.
#[tokio::test]
async fn a_password_change_without_a_session_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let answer = send(
            &app,
            post(
                "/api/auth/password/change",
                &json!({ "current_password": GOOD_PASSWORD, "new_password": OTHER_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "unauthorized");
    })
    .await;
}

// ---------------------------------------------------------------------------
// Password forgot
// ---------------------------------------------------------------------------

/// (5) Forgot answers the same `200 {"ok":true}` for a known and an unknown
/// address, and it writes a token for the known one only.
#[tokio::test]
async fn forgot_answers_the_same_body_for_a_known_and_an_unknown_address() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "known@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "known@example.com").await;

        let known = send(
            &app,
            post(
                "/api/auth/password/forgot",
                &json!({ "email": "known@example.com" }),
            ),
        )
        .await;
        let unknown = send(
            &app,
            post(
                "/api/auth/password/forgot",
                &json!({ "email": "ghost@example.com" }),
            ),
        )
        .await;

        assert_eq!(known.status.as_u16(), 200);
        assert_eq!(known.body, json!({ "ok": true }));
        assert_eq!(unknown.status.as_u16(), 200);
        assert_eq!(unknown.body, json!({ "ok": true }));
        assert_eq!(token_count(&db, user, "reset").await, 1);
        let all: i64 =
            sqlx::query_scalar("SELECT count(*) FROM auth_tokens WHERE purpose = 'reset'")
                .fetch_one(&db.admin)
                .await
                .unwrap();
        assert_eq!(all, 1, "an unknown address must write no token");
    })
    .await;
}

/// (6) A reset token lives 30 minutes, and a fresh request supersedes the older
/// link.
///
/// 1,800 seconds is the section 10 literal.
#[tokio::test]
async fn a_reset_token_lives_thirty_minutes_and_supersedes_the_older_one() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "known@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "known@example.com").await;
        let body = json!({ "email": "known@example.com" });

        send(&app, post("/api/auth/password/forgot", &body)).await;
        send(&app, post("/api/auth/password/forgot", &body)).await;

        assert_eq!(
            token_count(&db, user, "reset").await,
            1,
            "a second request must supersede the first link"
        );
        let window = token_expiry(&db, "reset").await.timestamp() - Utc::now().timestamp();
        assert!(
            (1_795..=1_800).contains(&window),
            "the reset window must be 1800 seconds, it is {window}"
        );
    })
    .await;
}

/// (7) The forgot rule refuses the 4th call for one address and the 11th call
/// from one host. 3 and 10 in one hour are the section 3.2 literals.
#[tokio::test]
async fn the_forgot_rule_refuses_the_fourth_address_call_and_the_eleventh_host_call() {
    TestDb::with(
        |db| async move { assert_paired_rate_rule(&db, "/api/auth/password/forgot").await },
    )
    .await;
}

// ---------------------------------------------------------------------------
