//! Part of `tests/auth_recovery.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;
use sqlx::types::chrono::Utc;

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
        assert_paired_rate_rule(&db, "/api/auth/verify-email/resend").await
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
