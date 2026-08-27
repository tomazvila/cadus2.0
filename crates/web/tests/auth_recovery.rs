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

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_store::test_support::TestDb;
use common::{
    GOOD_PASSWORD, OTHER_PASSWORD, RESET_TOKEN_ONE, RESET_TOKEN_TWO, SHORT_PASSWORD,
    VERIFY_TOKEN_ONE, VERIFY_TOKEN_TWO, app_of, disable, get_bearer, is_verified, login,
    mark_verified, password_hash, post, post_bearer, seed_token, send, session_count, shift,
    signup, token_count, user_id, verified_login,
};
use serde_json::{Value, json};
use sqlx::types::chrono::{DateTime, Utc};

/// The `expires_at` of the one live token of `purpose`.
async fn token_expiry(db: &TestDb, purpose: &str) -> DateTime<Utc> {
    sqlx::query_scalar("SELECT expires_at FROM auth_tokens WHERE purpose = $1")
        .bind(purpose)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
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
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let body = json!({ "email": "one@example.com" });

        for round in 1..=3 {
            let answer = send(&app, post("/api/auth/password/forgot", &body)).await;
            assert_eq!(answer.status.as_u16(), 200, "call {round}");
        }
        let fourth = send(&app, post("/api/auth/password/forgot", &body)).await;
        assert_eq!(fourth.status.as_u16(), 429);
        assert_eq!(fourth.code(), "rate_limited");

        // The host tally stands at 3. Seven more addresses fill it to 10.
        for index in 0..7 {
            let answer = send(
                &app,
                post(
                    "/api/auth/password/forgot",
                    &json!({ "email": format!("host{index}@example.com") }),
                ),
            )
            .await;
            assert_eq!(answer.status.as_u16(), 200, "host call {index}");
        }
        let eleventh = send(
            &app,
            post(
                "/api/auth/password/forgot",
                &json!({ "email": "last@example.com" }),
            ),
        )
        .await;

        assert_eq!(eleventh.status.as_u16(), 429);
        assert_eq!(eleventh.code(), "rate_limited");
    })
    .await;
}

// ---------------------------------------------------------------------------
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
// Verify email, resend
// ---------------------------------------------------------------------------

/// (15) Resend answers the same body for an unknown address, an unverified one,
/// and a verified one, and it mints a link for the unverified one only.
///
/// The link lives 24 hours: 86,400 seconds is the section 10 literal.
#[tokio::test]
async fn resend_answers_one_body_and_mints_for_an_unverified_address_only() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "waiting@example.com", GOOD_PASSWORD).await;
        signup(&app, "done@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "done@example.com").await;
        let waiting = user_id(&db, "waiting@example.com").await;
        let done = user_id(&db, "done@example.com").await;
        // Sign-up already minted one link each. Clear them, so this test counts
        // what resend writes and nothing else.
        sqlx::query("DELETE FROM auth_tokens")
            .execute(&db.admin)
            .await
            .unwrap();

        let mut answers = Vec::new();
        for address in [
            "waiting@example.com",
            "done@example.com",
            "ghost@example.com",
        ] {
            answers.push(
                send(
                    &app,
                    post(
                        "/api/auth/verify-email/resend",
                        &json!({ "email": address }),
                    ),
                )
                .await,
            );
        }

        for answer in &answers {
            assert_eq!(answer.status.as_u16(), 200);
            assert_eq!(answer.body, json!({ "ok": true }));
        }
        assert_eq!(token_count(&db, waiting, "verify").await, 1);
        assert_eq!(token_count(&db, done, "verify").await, 0);
        let window = token_expiry(&db, "verify").await.timestamp() - Utc::now().timestamp();
        assert!(
            (86_395..=86_400).contains(&window),
            "the verification window must be 86400 seconds, it is {window}"
        );
    })
    .await;
}

/// (16) A resent link supersedes the older one, so a leaked older link stops
/// working.
#[tokio::test]
async fn a_resent_link_supersedes_the_older_one() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "again@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "again@example.com").await;
        sqlx::query("DELETE FROM auth_tokens")
            .execute(&db.admin)
            .await
            .unwrap();
        seed_token(&db, user, VERIFY_TOKEN_ONE.1, "verify", shift(86_400)).await;

        send(
            &app,
            post(
                "/api/auth/verify-email/resend",
                &json!({ "email": "again@example.com" }),
            ),
        )
        .await;
        let old_link = send(
            &app,
            post(
                "/api/auth/verify-email",
                &json!({ "token": VERIFY_TOKEN_ONE.0 }),
            ),
        )
        .await;

        assert_eq!(token_count(&db, user, "verify").await, 1);
        assert_eq!(old_link.status.as_u16(), 400);
        assert_eq!(old_link.code(), "invalid_token");
    })
    .await;
}

/// (17) The resend rule refuses the 4th call for one address and the 11th call
/// from one host. 3 and 10 in one hour are the section 3.2 literals.
#[tokio::test]
async fn the_resend_rule_refuses_the_fourth_address_call_and_the_eleventh_host_call() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let body = json!({ "email": "one@example.com" });

        for round in 1..=3 {
            let answer = send(&app, post("/api/auth/verify-email/resend", &body)).await;
            assert_eq!(answer.status.as_u16(), 200, "call {round}");
        }
        let fourth = send(&app, post("/api/auth/verify-email/resend", &body)).await;
        assert_eq!(fourth.status.as_u16(), 429);
        assert_eq!(fourth.code(), "rate_limited");

        for index in 0..7 {
            let answer = send(
                &app,
                post(
                    "/api/auth/verify-email/resend",
                    &json!({ "email": format!("host{index}@example.com") }),
                ),
            )
            .await;
            assert_eq!(answer.status.as_u16(), 200, "host call {index}");
        }
        let eleventh = send(
            &app,
            post(
                "/api/auth/verify-email/resend",
                &json!({ "email": "last@example.com" }),
            ),
        )
        .await;

        assert_eq!(eleventh.status.as_u16(), 429);
        assert_eq!(eleventh.code(), "rate_limited");
    })
    .await;
}

/// (18) A body without the field the route reads is `422 invalid_request` on
/// every route of this file, and no handler panics.
#[tokio::test]
async fn a_body_without_the_field_is_422_invalid_request() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let cases: [(&str, Value); 4] = [
            ("/api/auth/password/forgot", json!({})),
            ("/api/auth/password/reset", json!({ "token": "t" })),
            ("/api/auth/verify-email", json!({ "nope": 1 })),
            ("/api/auth/verify-email/resend", json!({ "email": 12 })),
        ];

        for (path, body) in cases {
            let answer = send(&app, post(path, &body)).await;

            assert_eq!(answer.status.as_u16(), 422, "path {path}");
            assert_eq!(answer.code(), "invalid_request", "path {path}");
        }
    })
    .await;
}
