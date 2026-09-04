//! Part of `tests/auth_recovery.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_store::test_support::TestDb;
use common::*;
use serde_json::json;

// Password reset
// ---------------------------------------------------------------------------

/// (8) An unknown token, an expired token, and a token of the other purpose are
/// all `400 invalid_token`.
#[tokio::test]
async fn an_unusable_reset_token_is_400_invalid_token() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "r@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "r@example.com").await;
        // Expired one second ago.
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(-1)).await;
        // A live token of the OTHER purpose.
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;

        for raw in ["no-such-token", RESET_TOKEN_ONE.0, VERIFY_TOKEN_ONE.0] {
            let answer = send(
                &app,
                post(
                    "/api/auth/password/reset",
                    &json!({ "token": raw, "new_password": OTHER_PASSWORD }),
                ),
            )
            .await;

            assert_eq!(answer.status.as_u16(), 400, "token {raw}");
            assert_eq!(answer.code(), "invalid_token", "token {raw}");
        }
    })
    .await;
}

/// (9) A reset spends the link once: the new password works, every session ends,
/// the address becomes verified, and a second use is `400 invalid_token`.
#[tokio::test]
async fn a_reset_spends_the_link_once_and_ends_every_session() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "r@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "r@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        assert_eq!(session_count(&db, user).await, 1);

        let first = send(
            &app,
            post(
                "/api/auth/password/reset",
                &json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD }),
            ),
        )
        .await;
        let second = send(
            &app,
            post(
                "/api/auth/password/reset",
                &json!({ "token": RESET_TOKEN_ONE.0, "new_password": "yet another passphrase" }),
            ),
        )
        .await;

        assert_eq!(first.status.as_u16(), 200);
        assert_eq!(first.body, json!({ "ok": true }));
        assert_eq!(second.status.as_u16(), 400);
        assert_eq!(second.code(), "invalid_token");
        assert_eq!(session_count(&db, user).await, 0);
        assert_eq!(
            send(&app, get_bearer("/api/auth/me", &token))
                .await
                .status
                .as_u16(),
            401
        );
        assert_eq!(
            login(&app, "r@example.com", OTHER_PASSWORD)
                .await
                .status
                .as_u16(),
            200
        );
        assert_eq!(
            login(&app, "r@example.com", GOOD_PASSWORD)
                .await
                .status
                .as_u16(),
            401
        );
    })
    .await;
}

/// (10) A reset verifies an unverified address, so an account that never opened
/// its first link is not stranded.
#[tokio::test]
async fn a_reset_verifies_an_unverified_address() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "r@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "r@example.com").await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        assert!(!is_verified(&db, "r@example.com").await);

        let answer = send(
            &app,
            post(
                "/api/auth/password/reset",
                &json!({ "token": RESET_TOKEN_ONE.0, "new_password": OTHER_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 200);
        assert!(is_verified(&db, "r@example.com").await);
        assert_eq!(
            login(&app, "r@example.com", OTHER_PASSWORD)
                .await
                .status
                .as_u16(),
            200
        );
    })
    .await;
}

/// (11) A password the policy refuses is `422 weak_password` and LEAVES the link
/// usable.
///
/// 1.0 spent the token first, so a typo cost the learner a second email. 2.0
/// runs the policy test before the token is spent.
#[tokio::test]
async fn a_weak_reset_password_is_422_and_leaves_the_link_usable() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "r@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "r@example.com").await;
        seed_token(&db, user, RESET_TOKEN_TWO.1, "reset", shift(1_800)).await;

        let weak = send(
            &app,
            post(
                "/api/auth/password/reset",
                &json!({ "token": RESET_TOKEN_TWO.0, "new_password": SHORT_PASSWORD }),
            ),
        )
        .await;
        let retry = send(
            &app,
            post(
                "/api/auth/password/reset",
                &json!({ "token": RESET_TOKEN_TWO.0, "new_password": OTHER_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(weak.status.as_u16(), 422);
        assert_eq!(weak.code(), "weak_password");
        assert_eq!(retry.status.as_u16(), 200);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Verify email
// ---------------------------------------------------------------------------

/// (12) A verification link marks the address verified AND signs the person in.
#[tokio::test]
async fn a_verification_link_verifies_the_address_and_opens_a_session() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "v@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "v@example.com").await;
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;

        let answer = send(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_ONE.0 }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(answer.body["user"]["email"].as_str(), Some("v@example.com"));
        assert_eq!(answer.body["user"]["email_verified"].as_bool(), Some(true));
        assert!(
            answer
                .cookie()
                .is_some_and(|value| value.starts_with("__Host-cadus_session=")),
            "the verification link must set the session cookie"
        );
        assert_eq!(session_count(&db, user).await, 1);
        assert!(is_verified(&db, "v@example.com").await);
        assert_eq!(
            login(&app, "v@example.com", GOOD_PASSWORD)
                .await
                .status
                .as_u16(),
            200
        );
    })
    .await;
}

/// (13) A link works exactly once, and a leftover link for an address that is
/// already verified is `400 invalid_token`.
///
/// Without that rule a leftover link is a password-free login for as long as it
/// lives.
#[tokio::test]
async fn a_verification_link_works_once_and_never_after_verification() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "v@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "v@example.com").await;
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;
        seed_token(&db, user, VERIFY_TOKEN_TWO.1, "verify", shift(86_400)).await;

        let first = send(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_ONE.0 }),
            ),
        )
        .await;
        let again = send(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_ONE.0 }),
            ),
        )
        .await;
        let leftover = send(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_TWO.0 }),
            ),
        )
        .await;

        assert_eq!(first.status.as_u16(), 200);
        assert_eq!(again.status.as_u16(), 400);
        assert_eq!(again.code(), "invalid_token");
        assert_eq!(leftover.status.as_u16(), 400);
        assert_eq!(leftover.code(), "invalid_token");
        assert_eq!(session_count(&db, user).await, 1);
    })
    .await;
}

/// (14) An unknown token, an expired token, a token of the other purpose, and a
/// token of a disabled account are all `400 invalid_token`.
#[tokio::test]
async fn an_unusable_verification_token_is_400_invalid_token() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "v@example.com", GOOD_PASSWORD).await;
        signup(&app, "off@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "v@example.com").await;
        let off = user_id(&db, "off@example.com").await;
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(-1)).await;
        seed_token(&db, user, RESET_TOKEN_ONE.1, "reset", shift(1_800)).await;
        seed_token(&db, off, VERIFY_TOKEN_TWO.1, "verify", shift(86_400)).await;
        disable(&db, "off@example.com").await;

        for raw in [
            "no-such-token",
            VERIFY_TOKEN_ONE.0,
            RESET_TOKEN_ONE.0,
            VERIFY_TOKEN_TWO.0,
        ] {
            let answer = send(
                &app,
                post("/api/auth/verify-email", &json!({ "token": raw })),
            )
            .await;

            assert_eq!(answer.status.as_u16(), 400, "token {raw}");
            assert_eq!(answer.code(), "invalid_token", "token {raw}");
            assert_eq!(answer.cookie(), None, "token {raw} must open no session");
        }
    })
    .await;
}

// ---------------------------------------------------------------------------
