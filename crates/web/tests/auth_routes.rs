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

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use axum::body::Body;
use axum::http::Request;
use cadus_store::test_support::TestDb;
use cadus_web::auth::rate::{RateRule, UNKNOWN_CLIENT_IP, client_ip};
use common::{
    Answer, GOOD_PASSWORD, OTHER_PASSWORD, SESSION_TOKEN_ONE, SESSION_TOKEN_TWO, SHORT_PASSWORD,
    app_of, disable, get, get_bearer, in_one_window, last_seen_at, login, mark_verified, post,
    post_bearer, seed_session, send, session_count, shift, signup, user_count, user_id,
    verified_login,
};
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

// ---------------------------------------------------------------------------
// Login (spec section 3.3 "Login", section 10 rows "Signup / unverified login")
// ---------------------------------------------------------------------------

/// (10) An unverified account cannot sign in, and the refusal is the SAME
/// `401 invalid_credentials` as a wrong password.
#[tokio::test]
async fn an_unverified_login_and_a_wrong_password_answer_the_same_401() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "quiet@example.com", GOOD_PASSWORD).await;

        let unverified = login(&app, "quiet@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "quiet@example.com").await;
        let wrong = login(&app, "quiet@example.com", "not the password").await;
        let unknown = login(&app, "ghost@example.com", GOOD_PASSWORD).await;

        for answer in [&unverified, &wrong, &unknown] {
            assert_eq!(answer.status.as_u16(), 401);
            assert_eq!(answer.code(), "invalid_credentials");
            assert_eq!(answer.cookie(), None);
        }
        assert_eq!(unverified.body, wrong.body);
        assert_eq!(wrong.body, unknown.body);
    })
    .await;
}

/// (11) A disabled account gets the same refusal.
#[tokio::test]
async fn a_disabled_account_answers_401_invalid_credentials() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "gone@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "gone@example.com").await;
        disable(&db, "gone@example.com").await;

        let answer = login(&app, "gone@example.com", GOOD_PASSWORD).await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "invalid_credentials");
    })
    .await;
}

/// (11a) An account with no password hash gets the same refusal, and it opens
/// no session.
///
/// A NULL `password_hash` is the OAuth-only shape, and the specification refuses
/// it at login step 1 (section 3.3, "Login"). The account below is verified and
/// live, so the refusal comes from the missing password alone. A `200` here is
/// a sign-in with a password that the account never had.
#[tokio::test]
async fn an_account_with_no_password_answers_401_invalid_credentials() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "oauth@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "oauth@example.com").await;
        common::clear_password_hash(&db, "oauth@example.com").await;
        let user = user_id(&db, "oauth@example.com").await;

        let answer = login(&app, "oauth@example.com", GOOD_PASSWORD).await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "invalid_credentials");
        assert_eq!(answer.body.get("session_token"), None);
        assert_eq!(answer.cookie(), None);
        assert_eq!(session_count(&db, user).await, 0);
    })
    .await;
}

/// (12) A verified login sets the `__Host-` cookie, character for character.
///
/// The literal is the specification string of section 3.1: the `__Host-` name,
/// the 30-day `Max-Age` in seconds, `Path=/`, `HttpOnly`, `Secure`, and
/// `SameSite=Lax`, in that order. The cookie and the body carry the same token.
#[tokio::test]
async fn a_verified_login_sets_the_host_cookie_and_returns_the_user() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "live@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "live@example.com").await;

        let answer = login(&app, "live@example.com", GOOD_PASSWORD).await;

        assert_eq!(answer.status.as_u16(), 200);
        let token = answer
            .body
            .get("session_token")
            .and_then(Value::as_str)
            .expect("Accept-Session-Token: true must put the raw token in the body")
            .to_string();
        assert_eq!(token.len(), 43, "a 32-byte token is 43 characters");
        assert_eq!(
            answer.cookie(),
            Some(format!(
                "__Host-cadus_session={token}; Max-Age=2592000; Path=/; HttpOnly; Secure; \
                 SameSite=Lax"
            ))
        );

        let user = answer.body.get("user").unwrap();
        assert_eq!(
            user.get("email").and_then(Value::as_str),
            Some("live@example.com")
        );
        assert_eq!(
            user.get("email_verified").and_then(Value::as_bool),
            Some(true)
        );
        assert!(user.get("id").and_then(Value::as_str).is_some());
        assert!(user.get("created_at").and_then(Value::as_str).is_some());
    })
    .await;
}

/// (13) Without `Accept-Session-Token: true` the raw token stays out of the
/// body.
#[tokio::test]
async fn the_raw_token_stays_out_of_the_body_without_the_header() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "plain@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "plain@example.com").await;

        let answer = send(
            &app,
            post(
                "/api/auth/login",
                &json!({ "email": "plain@example.com", "password": GOOD_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(answer.body.get("session_token"), None);
        assert!(answer.cookie().is_some());
    })
    .await;
}

/// (14) Each login mints a FRESH session; it never hands the old token back.
#[tokio::test]
async fn each_login_mints_a_fresh_session() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let first = verified_login(&db, &app, "two@example.com", GOOD_PASSWORD).await;
        let second = login(&app, "two@example.com", GOOD_PASSWORD).await;
        let second = second.body["session_token"].as_str().unwrap().to_string();

        assert_ne!(first, second);
        let user = user_id(&db, "two@example.com").await;
        assert_eq!(session_count(&db, user).await, 2);
    })
    .await;
}

/// (15) The unknown-address path spends a real Argon2 verify, so its answer time
/// stays inside 3x of the known-address path in both directions.
///
/// The bound is the 1.0 one (`test_authn_endpoints.py:409-428`). Best of five
/// runs on each side damps the scheduler noise of a loaded box.
#[tokio::test]
async fn an_unknown_address_login_stays_inside_three_times_the_known_one() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "known@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "known@example.com").await;

        // Warm the per-profile dummy hash, so its one-time cost stays out of the
        // measurement below.
        login(&app, "warmup@example.com", "x wrong y").await;

        async fn best(app: &axum::Router, email: &str) -> std::time::Duration {
            let mut best = std::time::Duration::MAX;
            for _ in 0..5 {
                let start = std::time::Instant::now();
                let answer = login(app, email, "x wrong y").await;
                assert_eq!(answer.status.as_u16(), 401);
                best = best.min(start.elapsed());
            }
            best
        }

        let known = best(&app, "known@example.com").await;
        let unknown = best(&app, "ghost@example.com").await;

        assert!(
            unknown * 3 >= known,
            "the unknown-address path skipped the verify: known {known:?}, unknown {unknown:?}"
        );
        assert!(
            unknown <= known * 3,
            "the unknown-address path is far slower: known {known:?}, unknown {unknown:?}"
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// The session guard (spec section 3.3, "Cookie check")
// ---------------------------------------------------------------------------

/// (16) `me` without a credential, with the wrong scheme, and with `Basic` are
/// all `401 unauthorized`.
#[tokio::test]
async fn me_without_a_usable_credential_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let bare = send(&app, get("/api/auth/me")).await;
        let basic = send(
            &app,
            Request::builder()
                .method("GET")
                .uri("/api/auth/me")
                .header("authorization", "Basic dXNlcjpwYXNz")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let empty_bearer = send(
            &app,
            Request::builder()
                .method("GET")
                .uri("/api/auth/me")
                .header("authorization", "Bearer")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let unknown = send(&app, get_bearer("/api/auth/me", "no-such-token")).await;

        for answer in [&bare, &basic, &empty_bearer, &unknown] {
            assert_eq!(answer.status.as_u16(), 401);
            assert_eq!(answer.code(), "unauthorized");
        }
    })
    .await;
}

/// (17) `me` with a live session answers the account.
#[tokio::test]
async fn me_with_a_live_session_answers_the_account() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "me@example.com", GOOD_PASSWORD).await;

        let answer = send(&app, get_bearer("/api/auth/me", &token)).await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(
            answer.body["user"]["email"].as_str(),
            Some("me@example.com")
        );
        assert_eq!(answer.body["user"]["email_verified"].as_bool(), Some(true));
    })
    .await;
}

/// (18) A session past its idle window is refused, and `now == expires_at`
/// counts as past it.
#[tokio::test]
async fn an_expired_session_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "stale@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "stale@example.com").await;
        let user = user_id(&db, "stale@example.com").await;
        seed_session(
            &db,
            user,
            SESSION_TOKEN_ONE.1,
            shift(-3_600),
            shift(-3_600),
            shift(-1),
        )
        .await;

        let answer = send(&app, get_bearer("/api/auth/me", SESSION_TOKEN_ONE.0)).await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "unauthorized");
    })
    .await;
}

/// (19) A session 90 days old is refused even while `expires_at` is in the
/// future.
///
/// 7,776,000 seconds is the absolute ceiling of section 10, and it never slides.
/// The seeded row below is one second past it and still has a live idle window,
/// so only the absolute test can refuse it.
#[tokio::test]
async fn a_session_past_the_absolute_ceiling_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "ancient@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "ancient@example.com").await;
        let user = user_id(&db, "ancient@example.com").await;
        seed_session(
            &db,
            user,
            SESSION_TOKEN_ONE.1,
            shift(-7_776_001),
            shift(-60),
            shift(2_592_000),
        )
        .await;
        seed_session(
            &db,
            user,
            SESSION_TOKEN_TWO.1,
            shift(-7_775_000),
            shift(-60),
            shift(2_592_000),
        )
        .await;

        let old = send(&app, get_bearer("/api/auth/me", SESSION_TOKEN_ONE.0)).await;
        let young = send(&app, get_bearer("/api/auth/me", SESSION_TOKEN_TWO.0)).await;

        assert_eq!(old.status.as_u16(), 401);
        assert_eq!(old.code(), "unauthorized");
        assert_eq!(
            young.status.as_u16(),
            200,
            "a session inside 90 days must still work: {}",
            young.body
        );
    })
    .await;
}

/// (20) A session of a disabled account is refused.
#[tokio::test]
async fn a_session_of_a_disabled_account_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "shut@example.com", GOOD_PASSWORD).await;
        disable(&db, "shut@example.com").await;

        let answer = send(&app, get_bearer("/api/auth/me", &token)).await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "unauthorized");
    })
    .await;
}

/// (21) `last_seen_at` moves at most once per hour.
///
/// The seeded row was last seen two hours ago, so the first request writes it.
/// The second request is inside the hour that follows, so it writes nothing.
#[tokio::test]
async fn last_seen_at_is_written_at_most_once_per_hour() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        signup(&app, "touch@example.com", GOOD_PASSWORD).await;
        mark_verified(&db, "touch@example.com").await;
        let user = user_id(&db, "touch@example.com").await;
        seed_session(
            &db,
            user,
            SESSION_TOKEN_ONE.1,
            shift(-7_200),
            shift(-7_200),
            shift(2_592_000),
        )
        .await;
        let seeded = last_seen_at(&db, SESSION_TOKEN_ONE.1).await;

        let first = send(&app, get_bearer("/api/auth/me", SESSION_TOKEN_ONE.0)).await;
        assert_eq!(first.status.as_u16(), 200);
        let after_first = last_seen_at(&db, SESSION_TOKEN_ONE.1).await;

        let second = send(&app, get_bearer("/api/auth/me", SESSION_TOKEN_ONE.0)).await;
        assert_eq!(second.status.as_u16(), 200);
        let after_second = last_seen_at(&db, SESSION_TOKEN_ONE.1).await;

        assert!(
            after_first > seeded,
            "a session last seen two hours ago must be touched"
        );
        assert_eq!(
            after_first, after_second,
            "a second request inside the hour must write nothing"
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// Sign-out
// ---------------------------------------------------------------------------

/// (22) Sign-out ends the presented session and clears the cookie
/// attribute for attribute.
#[tokio::test]
async fn logout_ends_the_session_and_clears_the_cookie() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let token = verified_login(&db, &app, "bye@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "bye@example.com").await;

        let answer = send(&app, post_bearer("/api/auth/logout", &token, &json!({}))).await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(answer.body, json!({ "ok": true }));
        assert_eq!(
            answer.cookie(),
            Some(
                "__Host-cadus_session=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/; \
                 HttpOnly; Secure; SameSite=Lax"
                    .to_string()
            )
        );
        assert_eq!(session_count(&db, user).await, 0);

        let after: Answer = send(&app, get_bearer("/api/auth/me", &token)).await;
        assert_eq!(after.status.as_u16(), 401);
    })
    .await;
}

/// (23) Sign-out with no credential still answers `200` and still clears the
/// cookie.
#[tokio::test]
async fn logout_without_a_session_still_answers_200() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let answer = send(&app, post("/api/auth/logout", &json!({}))).await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(answer.body, json!({ "ok": true }));
        assert!(answer.cookie().is_some());
    })
    .await;
}

/// (24) Sign-out everywhere ends every session of the account and reports the
/// count.
#[tokio::test]
async fn logout_all_ends_every_session_of_the_account() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let first = verified_login(&db, &app, "all@example.com", GOOD_PASSWORD).await;
        let second = login(&app, "all@example.com", GOOD_PASSWORD).await;
        let second = second.body["session_token"].as_str().unwrap().to_string();
        let user = user_id(&db, "all@example.com").await;
        assert_eq!(session_count(&db, user).await, 2);

        let answer = send(
            &app,
            post_bearer("/api/auth/logout-all", &first, &json!({})),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(answer.body, json!({ "ok": true, "revoked": 2 }));
        assert_eq!(session_count(&db, user).await, 0);
        assert_eq!(
            send(&app, get_bearer("/api/auth/me", &second))
                .await
                .status
                .as_u16(),
            401
        );
    })
    .await;
}

/// (24a) Sign-out-everywhere is guarded. Sign-out is public and idempotent.
/// This route is neither: with no credential it is `401 unauthorized`, and it
/// deletes nothing.
#[tokio::test]
async fn logout_all_without_a_session_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        verified_login(&db, &app, "kept@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "kept@example.com").await;

        let answer = send(&app, post("/api/auth/logout-all", &json!({}))).await;

        assert_eq!(answer.status.as_u16(), 401);
        assert_eq!(answer.code(), "unauthorized");
        assert_eq!(session_count(&db, user).await, 1);
    })
    .await;
}

/// (25) One tenant's sign-out-everywhere never touches another tenant's
/// sessions. The `tenant_isolation` policy is what holds the DELETE (C3).
#[tokio::test]
async fn logout_all_never_reaches_another_tenant() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let mine = verified_login(&db, &app, "mine@example.com", GOOD_PASSWORD).await;
        verified_login(&db, &app, "yours@example.com", GOOD_PASSWORD).await;
        let other = user_id(&db, "yours@example.com").await;

        send(&app, post_bearer("/api/auth/logout-all", &mine, &json!({}))).await;

        assert_eq!(session_count(&db, other).await, 1);
    })
    .await;
}

// ---------------------------------------------------------------------------
// The envelope, the CSRF polarity, and the bodies a handler must survive
// ---------------------------------------------------------------------------

/// (26) An explicitly cross-origin login is `403 cross_origin_rejected`, and a
/// header-less one is not.
#[tokio::test]
async fn a_cross_origin_login_is_403_and_a_header_less_one_is_not() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let cross = send(
            &app,
            Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .header("host", "tutor.example")
                .header("origin", "https://evil.example")
                .body(Body::from(
                    json!({ "email": "a@example.com", "password": GOOD_PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await;
        let plain = login(&app, "a@example.com", GOOD_PASSWORD).await;

        assert_eq!(cross.status.as_u16(), 403);
        assert_eq!(cross.code(), "cross_origin_rejected");
        assert_eq!(plain.status.as_u16(), 401);
    })
    .await;
}

/// (27) A body that is not the JSON object the route reads is
/// `422 invalid_request`, and the handler answers instead of panicking.
#[tokio::test]
async fn a_body_the_route_cannot_read_is_422_invalid_request() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        let mut answers = Vec::new();
        for body in ["", "not json at all", "[]", "\"text\"", "null", "{}"] {
            answers.push(
                send(
                    &app,
                    Request::builder()
                        .method("POST")
                        .uri("/api/auth/login")
                        .header("content-type", "application/json")
                        .body(Body::from(body))
                        .unwrap(),
                )
                .await,
            );
        }
        // A field of the wrong type is the same refusal.
        answers.push(
            send(
                &app,
                post("/api/auth/login", &json!({ "email": 7, "password": true })),
            )
            .await,
        );

        for answer in &answers {
            assert_eq!(answer.status.as_u16(), 422, "body gave {}", answer.body);
            assert_eq!(answer.code(), "invalid_request");
        }
    })
    .await;
}

/// (28) A body far past the password ceiling is refused and never hashed.
///
/// The policy reads the length before Argon2 runs, so a megabyte of text costs
/// one length test.
#[tokio::test]
async fn a_huge_password_is_422_weak_password() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let huge = "a".repeat(200_000);

        let answer = signup(&app, "huge@example.com", &huge).await;

        assert_eq!(answer.status.as_u16(), 422);
        assert_eq!(answer.code(), "weak_password");
    })
    .await;
}

// ---------------------------------------------------------------------------
// The rule table and the client-address key (spec section 3.2)
// ---------------------------------------------------------------------------

/// (29) The four rules carry the specification numbers, and the scope strings
/// are the stored keys.
///
/// The numbers are the section 3.2 table: sign-up 3 + 5 per hour, login 10 + 30
/// per five minutes, forgot 3 + 10 per hour, resend 3 + 10 per hour. A scope
/// string is part of the persisted `auth_rate_counters` key, so a change to one
/// resets that rule's live windows.
#[test]
fn the_four_rate_rules_are_the_specification_numbers() {
    let table = [
        (RateRule::SIGNUP, "signup", 3, 5, 3_600),
        (RateRule::LOGIN, "login", 10, 30, 300),
        (RateRule::FORGOT, "forgot", 3, 10, 3_600),
        (RateRule::VERIFY_RESEND, "verify_resend", 3, 10, 3_600),
    ];

    for (rule, prefix, per_email, per_ip, window_secs) in table {
        assert_eq!(rule.prefix, prefix);
        assert_eq!(rule.per_email, per_email, "rule {prefix}");
        assert_eq!(rule.per_ip, per_ip, "rule {prefix}");
        assert_eq!(rule.window_secs, window_secs, "rule {prefix}");
        assert_eq!(rule.email_scope(), format!("{prefix}_email"));
        assert_eq!(rule.ip_scope(), format!("{prefix}_ip"));
    }
    assert_eq!(RateRule::ALL.len(), 4);
}

/// (30) The client-address key reads `X-Forwarded-For` first, then the peer
/// address, then the fixed fallback.
///
/// `deploy/Caddyfile` overwrites `X-Forwarded-For` with the real remote address,
/// so the first value of it is the one this deployment trusts.
#[test]
fn the_client_address_key_reads_the_forwarded_header_first() {
    use axum::http::HeaderMap;

    let peer: std::net::SocketAddr = "198.51.100.9:4444".parse().unwrap();

    let mut forwarded = HeaderMap::new();
    forwarded.insert("x-forwarded-for", "203.0.113.7, 10.0.0.1".parse().unwrap());
    assert_eq!(client_ip(&forwarded, Some(peer)), "203.0.113.7");
    assert_eq!(client_ip(&forwarded, None), "203.0.113.7");

    let mut empty_header = HeaderMap::new();
    empty_header.insert("x-forwarded-for", "   ".parse().unwrap());
    assert_eq!(client_ip(&empty_header, Some(peer)), "198.51.100.9");

    assert_eq!(client_ip(&HeaderMap::new(), Some(peer)), "198.51.100.9");
    assert_eq!(client_ip(&HeaderMap::new(), None), "unknown");
    assert_eq!(UNKNOWN_CLIENT_IP, "unknown");
}

// ---------------------------------------------------------------------------
// The request-body limit of the write routes (spec section 2, F6)
// ---------------------------------------------------------------------------

/// The seven `/api/auth/*` routes that read a request body.
///
/// The other three routes read none: `logout` and `logout-all` take the session
/// alone, and `me` is a `GET`. No body rejection reaches those three.
const BODY_ROUTES: [&str; 7] = [
    "/api/auth/signup",
    "/api/auth/login",
    "/api/auth/password/change",
    "/api/auth/password/forgot",
    "/api/auth/password/reset",
    "/api/auth/verify-email",
    "/api/auth/verify-email/resend",
];

/// A `POST` of a raw body to `path`, with no credential and no origin header.
///
/// The shared `post` helper renders a `Value`. These two tests send a body that
/// no `Value` can carry: one past the limit, and one that fails mid-read.
fn post_raw(path: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header("content-type", "application/json")
        .body(body)
        .unwrap()
}

/// (31) A body over the limit answers `413 payload_too_large` in the envelope.
///
/// The section 2 envelope has no exception, so the answer carries
/// `{"error":{"code","message"}}` as JSON, not the axum plain-text sentence.
#[tokio::test]
async fn a_body_over_the_limit_is_413_payload_too_large_on_every_write_route() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        // 3 MiB of JSON. It is past the 2 MiB that the server buffers, so the
        // read stops before any handler sees a field.
        let huge = format!(
            "{{\"email\":\"big@example.com\",\"password\":\"{}\"}}",
            "a".repeat(3 * 1024 * 1024)
        );

        for path in BODY_ROUTES {
            let answer = send(&app, post_raw(path, Body::from(huge.clone()))).await;

            assert_eq!(
                answer.status.as_u16(),
                413,
                "route {path} answered {}",
                answer.body
            );
            assert_eq!(answer.code(), "payload_too_large", "route {path}");
            assert_eq!(
                answer
                    .headers
                    .get("content-type")
                    .and_then(|value| value.to_str().ok()),
                Some("application/json"),
                "route {path}"
            );
            assert_eq!(
                answer
                    .body
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str),
                Some("The request body is over the size limit."),
                "route {path}"
            );
        }
    })
    .await;
}

/// (32) A body that the server cannot read answers `422 invalid_request`.
///
/// The stream below fails on its first frame, which is what a client that hangs
/// up mid-body gives. The answer is the envelope, never a plain-text `400`.
#[tokio::test]
async fn a_body_the_server_cannot_buffer_is_422_invalid_request() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let broken = Body::from_stream(tokio_stream::once(Err::<&'static [u8], std::io::Error>(
            std::io::Error::other("the client hung up"),
        )));

        let answer = send(&app, post_raw("/api/auth/signup", broken)).await;

        assert_eq!(answer.status.as_u16(), 422, "body gave {}", answer.body);
        assert_eq!(answer.code(), "invalid_request");
        assert_eq!(
            answer
                .headers
                .get("content-type")
                .and_then(|value| value.to_str().ok()),
            Some("application/json")
        );
        assert_eq!(
            answer
                .body
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str),
            Some("The server could not read the request body.")
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// FIX2-M5-C, finding V5: the email field has a byte cap
// ---------------------------------------------------------------------------

/// How many rate-counter rows this database holds.
///
/// The four public credential routes bump two counters each, so a route that
/// reached its rate rule leaves at least one row behind.
async fn rate_counter_rows(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_rate_counters")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// One body of `{"email": <address>}` plus the fields that `extra` names.
fn email_body(address: &str, extra: &[(&str, &str)]) -> Value {
    let mut body = json!({ "email": address });
    for (name, value) in extra {
        body[*name] = json!(value);
    }
    body
}

/// The four public routes that read an email field, with the extra fields each
/// one needs to reach its email step.
const EMAIL_ROUTES: [(&str, &[(&str, &str)]); 4] = [
    ("/api/auth/signup", &[("password", GOOD_PASSWORD)]),
    ("/api/auth/login", &[("password", GOOD_PASSWORD)]),
    ("/api/auth/password/forgot", &[]),
    ("/api/auth/verify-email/resend", &[]),
];

/// (33) A 3 KB address is `422 invalid_request` on all four public routes, and
/// it writes NOTHING.
///
/// Without the cap the normalized address becomes the `key` of the
/// `auth_rate_counters` primary key and the `email` of the `users` unique
/// index. A btree index entry has a hard limit of about 2704 bytes, so a long
/// address of low compressibility makes the rate limiter itself throw, and the
/// very call the limiter must refuse answers `500` uncounted.
///
/// This test pins the CAP, not that index limit: it asserts the `422` and then
/// asserts that both tables stayed empty, so the refusal came before the
/// counter and before the account write. The address here is one repeated
/// character, so Postgres compresses it and the index takes it — which is
/// exactly why the earlier code accepted a 3012-byte address instead of
/// refusing it.
#[tokio::test]
async fn a_three_kilobyte_email_is_422_invalid_request_and_writes_nothing() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let address = format!("{}@example.com", "a".repeat(3000));
        assert_eq!(address.len(), 3012);

        for (path, extra) in EMAIL_ROUTES {
            let answer = send(&app, post(path, &email_body(&address, extra))).await;

            assert_eq!(
                answer.status.as_u16(),
                422,
                "{path} answered {} with {}",
                answer.status,
                answer.body
            );
            assert_eq!(answer.code(), "invalid_request", "{path}");
            assert_eq!(
                answer
                    .body
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str),
                Some("The email address is too long."),
                "{path}"
            );
        }

        assert_eq!(
            rate_counter_rows(&db).await,
            0,
            "an over-cap address must reach no rate counter"
        );
        assert_eq!(
            user_count(&db).await,
            0,
            "an over-cap address must write no account"
        );
    })
    .await;
}

/// (34) The cap is 254 bytes: 254 passes, 255 is refused.
///
/// 254 octets is the longest address that RFC 5321 carries, so the boundary is
/// where a real address stops.
#[tokio::test]
async fn the_email_cap_admits_254_bytes_and_refuses_255() {
    TestDb::with(|db| async move {
        let app = app_of(&db);

        // 242 + "@example.com" (12 bytes) = 254 bytes.
        let at_cap = format!("{}@example.com", "a".repeat(242));
        assert_eq!(at_cap.len(), 254);
        // One byte more.
        let over_cap = format!("{}@example.com", "a".repeat(243));
        assert_eq!(over_cap.len(), 255);

        let accepted = send(
            &app,
            post(
                "/api/auth/signup",
                &json!({ "email": at_cap, "password": GOOD_PASSWORD }),
            ),
        )
        .await;
        assert_eq!(accepted.status.as_u16(), 200, "body {}", accepted.body);
        assert_eq!(
            accepted.body.get("status").and_then(Value::as_str),
            Some("verification_required")
        );

        let refused = send(
            &app,
            post(
                "/api/auth/signup",
                &json!({ "email": over_cap, "password": GOOD_PASSWORD }),
            ),
        )
        .await;
        assert_eq!(refused.status.as_u16(), 422, "body {}", refused.body);
        assert_eq!(refused.code(), "invalid_request");

        assert_eq!(
            user_count(&db).await,
            1,
            "only the address at the cap opens an account"
        );
    })
    .await;
}

/// (35) The cap counts the bytes AFTER normalization.
///
/// `U+3316` (`㌖`) is 3 UTF-8 bytes, and NFKC replaces it with the six
/// characters `キロメートル`, which are 18 UTF-8 bytes. The address below is 102
/// bytes on the wire and 552 bytes after normalization, so a cap on the raw
/// field would let it through and a cap on the normalized string refuses it.
/// The normalized string is what reaches the counter key, so the cap counts it.
#[tokio::test]
async fn the_email_cap_counts_the_bytes_after_normalization() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let address = format!("{}@example.com", "\u{3316}".repeat(30));
        assert_eq!(address.len(), 102, "the raw address is under the 254 cap");

        let answer = send(
            &app,
            post(
                "/api/auth/signup",
                &json!({ "email": address, "password": GOOD_PASSWORD }),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 422, "body {}", answer.body);
        assert_eq!(answer.code(), "invalid_request");
        assert_eq!(
            rate_counter_rows(&db).await,
            0,
            "the refusal comes before the rate counter"
        );
    })
    .await;
}

/// (36) A registered over-cap address and an unknown one give the SAME answer.
///
/// The cap runs before every account lookup, so it opens no enumeration
/// channel. The seeded address is 300 bytes, which the `users` unique index
/// still holds, so the known half of the pair really exists.
#[tokio::test]
async fn a_known_and_an_unknown_over_cap_address_answer_the_same() {
    TestDb::with(|db| async move {
        let app = app_of(&db);
        let known = format!("{}@example.com", "k".repeat(288));
        let unknown = format!("{}@example.com", "u".repeat(288));
        assert_eq!(known.len(), 300);
        assert_eq!(unknown.len(), 300);
        db.seed_user(&known).await;

        for (path, extra) in EMAIL_ROUTES {
            let on_known = send(&app, post(path, &email_body(&known, extra))).await;
            let on_unknown = send(&app, post(path, &email_body(&unknown, extra))).await;

            assert_eq!(on_known.status.as_u16(), 422, "{path}");
            assert_eq!(on_unknown.status.as_u16(), 422, "{path}");
            assert_eq!(on_known.status, on_unknown.status, "{path}");
            assert_eq!(on_known.body, on_unknown.body, "{path}");
        }

        assert_eq!(
            rate_counter_rows(&db).await,
            0,
            "neither half reaches the rate counter"
        );
    })
    .await;
}
