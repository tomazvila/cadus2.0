//! Part of `tests/auth_routes.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_web::auth::rate::{RateRule, UNKNOWN_CLIENT_IP, client_ip};
use common::*;

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
