//! Part of `tests/auth_routes.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

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
