//! `/api/auth/*`, part 1: sign-up, login, sign-out, `me`, the session guard, and
//! the four paired rate rules.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 11, row U4:
//! "every section 10 auth literal, including the independent email/IP counters
//! (4th and 6th requests) and the <=3x timing bound on an unknown email".
//!
//! Part 2 lives in `auth_recovery.rs`: password change, forgot, reset,
//! verify-email, and verify-email/resend.
//!
//! Every expected value below is a LITERAL of this file: a status code, an error
//! code, a body field, a cookie string, a count, or a number of seconds. Nothing
//! re-reads a constant of the code under test (HANDOVER section 3).
//!
//! Each test builds its own throwaway database, so two tests never share a rate
//! counter, an account, or a session.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use common::*;
use serde_json::{Value, json};

// ---------------------------------------------------------------------------
// Sign-up (spec section 10, row "Signup / unverified login")
// ---------------------------------------------------------------------------

/// (1) Sign-up answers the generic object, and it sets NO cookie.
#[tokio::test]
async fn signup_answers_verification_required_and_opens_no_session() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let answer = signup(&app, "new@example.com", GOOD_PASSWORD).await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(
            answer.body.get("status").and_then(Value::as_str),
            Some("verification_required")
        );
        assert_eq!(answer.cookie(), None, "sign-up must set no session cookie");
    })
    .await;
}

/// (2) A second sign-up for one address answers the SAME body, never `409`.
///
/// The 1.0 contract calls this non-enumerable: a caller must not learn which
/// addresses are registered.
#[tokio::test]
async fn a_repeat_signup_answers_the_same_body_and_never_email_taken() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let first = signup(&app, "twice@example.com", GOOD_PASSWORD).await;
        let second = signup(&app, "twice@example.com", OTHER_PASSWORD).await;

        assert_eq!(first.status.as_u16(), 200);
        assert_eq!(second.status.as_u16(), 200);
        assert_eq!(first.body, second.body);
        assert_eq!(
            second.body.get("status").and_then(Value::as_str),
            Some("verification_required")
        );
        assert_eq!(second.cookie(), None);

        // Exactly one account exists, and the second password never landed.
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

/// (3) A password below the floor is `422 weak_password`.
#[tokio::test]
async fn a_short_password_is_422_weak_password() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let answer = signup(&app, "weak@example.com", SHORT_PASSWORD).await;

        assert_eq!(answer.status.as_u16(), 422);
        assert_eq!(answer.code(), "weak_password");
    })
    .await;
}

/// (4) Sign-up writes one verification token for a new address and none for a
/// repeat.
#[tokio::test]
async fn signup_mints_one_verification_token_for_a_new_address_only() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "tok@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "tok@example.com").await;

        assert_eq!(common::token_count(&db, user, "verify").await, 1);

        signup(&app, "tok@example.com", GOOD_PASSWORD).await;

        assert_eq!(
            common::token_count(&db, user, "verify").await,
            1,
            "a repeat sign-up must not mint a second link"
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// The paired rate rules (spec section 3.2 and section 10, row "Rate limit")
// ---------------------------------------------------------------------------

/// (5) The sign-up counters are independent: the 4th call for one address and
/// the 6th call from one host are the first two refused.
///
/// The numbers are the 1.0 literals: 3 per address and 5 per host, in one hour.
/// `in_one_window` holds the six calls inside one counter window.
#[tokio::test]
async fn the_signup_counters_refuse_the_fourth_address_call_and_the_sixth_host_call() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let (fourth, sixth) = in_one_window(&db, 3_600, || async {
            for round in 1..=3 {
                let answer = signup(&app, "victim@example.com", GOOD_PASSWORD).await;
                assert_eq!(answer.status.as_u16(), 200, "call {round} for one address");
            }

            let fourth = signup(&app, "victim@example.com", GOOD_PASSWORD).await;

            // The host tally is still at 3, so two more addresses go through.
            assert_eq!(
                signup(&app, "one@example.com", GOOD_PASSWORD)
                    .await
                    .status
                    .as_u16(),
                200
            );
            assert_eq!(
                signup(&app, "two@example.com", GOOD_PASSWORD)
                    .await
                    .status
                    .as_u16(),
                200
            );

            let sixth = signup(&app, "three@example.com", GOOD_PASSWORD).await;
            (fourth, sixth)
        })
        .await;

        assert_eq!(fourth.status.as_u16(), 429);
        assert_eq!(fourth.code(), "rate_limited");
        assert_eq!(sixth.status.as_u16(), 429);
        assert_eq!(sixth.code(), "rate_limited");
    })
    .await;
}

/// (6) The login rule refuses the 31st attempt from one host across distinct
/// addresses. 30 per host in five minutes is the 1.0 literal.
#[tokio::test]
async fn the_login_rule_refuses_the_thirty_first_attempt_from_one_host() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let refused = in_one_window(&db, 300, || async {
            for index in 0..30 {
                let answer = login(
                    &app,
                    &format!("spray{index}@example.com"),
                    "wrong wrong wrong",
                )
                .await;
                assert_eq!(answer.status.as_u16(), 401, "attempt {index}");
            }
            login(&app, "spray30@example.com", "wrong wrong wrong").await
        })
        .await;

        assert_eq!(refused.status.as_u16(), 429);
        assert_eq!(refused.code(), "rate_limited");
    })
    .await;
}

/// (7) The login rule refuses the 11th attempt for ONE address, well below the
/// per-host ceiling. 10 per address in five minutes is the 1.0 literal.
#[tokio::test]
async fn the_login_rule_refuses_the_eleventh_attempt_for_one_address() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let refused = in_one_window(&db, 300, || async {
            for index in 1..=10 {
                let answer = login(&app, "target@example.com", "wrong wrong wrong").await;
                assert_eq!(answer.status.as_u16(), 401, "attempt {index}");
            }
            login(&app, "target@example.com", "wrong wrong wrong").await
        })
        .await;

        assert_eq!(refused.status.as_u16(), 429);
        assert_eq!(refused.code(), "rate_limited");
    })
    .await;
}

/// (8) A refused address counter leaves the host counter where it was.
///
/// This is what makes rule (5) hold. The four sign-ups below spend three host
/// calls, not four, so the host budget of 5 still has two left.
#[tokio::test]
async fn an_address_refusal_does_not_spend_a_host_call() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        in_one_window(&db, 3_600, || async {
            for _ in 0..4 {
                signup(&app, "victim@example.com", GOOD_PASSWORD).await;
            }
        })
        .await;

        let host_count: i32 =
            sqlx::query_scalar("SELECT count FROM auth_rate_counters WHERE scope = 'signup_ip'")
                .fetch_one(&db.admin)
                .await
                .unwrap();
        let address_count: i32 =
            sqlx::query_scalar("SELECT count FROM auth_rate_counters WHERE scope = 'signup_email'")
                .fetch_one(&db.admin)
                .await
                .unwrap();

        assert_eq!(host_count, 3);
        assert_eq!(address_count, 4);
    })
    .await;
}

/// (9) The counters key on the normalized address, so case and padding share one
/// bucket.
#[tokio::test]
async fn the_address_counter_keys_on_the_normalized_address() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let fourth = in_one_window(&db, 3_600, || async {
            signup(&app, "Mixed@Example.COM", GOOD_PASSWORD).await;
            signup(&app, "  mixed@example.com  ", GOOD_PASSWORD).await;
            signup(&app, "MIXED@EXAMPLE.COM", GOOD_PASSWORD).await;
            signup(&app, "mixed@example.com", GOOD_PASSWORD).await
        })
        .await;

        assert_eq!(fourth.status.as_u16(), 429);
        assert_eq!(fourth.code(), "rate_limited");
    })
    .await;
}

/// (9a) The refusal of a registered address is byte for byte the refusal of an
/// unregistered one.
///
/// Both counters run BEFORE any account lookup (specification section 3.2), so
/// nothing in a `429` tells the two apart. The rule below is the forgot one: 3
/// per address and 10 per host in one hour. Eight calls spend six host calls, so
/// the host ceiling stays clear and BOTH refusals come from the address counter.
#[tokio::test]
async fn the_rate_refusal_of_a_known_address_matches_an_unknown_one() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "known@example.com", GOOD_PASSWORD).await;

        let forgot = |address: &'static str| {
            let app = app.clone();
            async move {
                send(
                    &app,
                    post("/api/auth/password/forgot", &json!({ "email": address })),
                )
                .await
            }
        };

        let (known, unknown) = in_one_window(&db, 3_600, || async {
            for round in 1..=3 {
                assert_eq!(
                    forgot("known@example.com").await.status.as_u16(),
                    200,
                    "known call {round}"
                );
            }
            let known = forgot("known@example.com").await;

            for round in 1..=3 {
                assert_eq!(
                    forgot("never@example.com").await.status.as_u16(),
                    200,
                    "unknown call {round}"
                );
            }
            (known, forgot("never@example.com").await)
        })
        .await;

        assert_eq!(known.status.as_u16(), 429);
        assert_eq!(known.code(), "rate_limited");
        assert_eq!(unknown.status, known.status);
        assert_eq!(unknown.body, known.body);
    })
    .await;
}
