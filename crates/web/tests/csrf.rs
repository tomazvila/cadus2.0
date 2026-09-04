//! Proof tests for the CSRF origin layer of M5 U1, in BOTH polarities.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1 (rows "CSRF" and
//! "Same-origin test"), section 10 (rows "CSRF" and "Login-CSRF"), and the
//! acceptance check of section 11, unit U1:
//!
//! > a cookie-authed cross-site POST to `/api/*` is `403 cross_origin_rejected`
//! > and a bearer one is not; a cross-origin POST to `/api/auth/login` is 403;
//! > a header-less curl POST is not
//!
//! Every assertion names a literal: a literal status code, the literal error
//! code `cross_origin_rejected`, a literal header value.
//!
//! **These tests never depend on a route existing.** The layer runs before the
//! router picks a handler, so the paths below (`/api/task/{id}/answer`,
//! `/api/auth/login`) reach it whether or not units U4 and U8 have added their
//! routes yet. What an ALLOWED request lands on depends on that:
//!
//! - `/api/task/{id}/answer` exists since unit U8, and these requests carry no
//!   credential, so an allowed one is `401 unauthorized` from the tenant guard;
//! - `/api/auth/*` waits for unit U4, so an allowed one is the `404 not_found`
//!   fallback.
//!
//! Both are literals, and asserting the literal is stronger than asserting
//! "not 403": a layer that answered `500` would pass the weaker check. No test
//! below reaches a handler that touches the database, because the tenant guard
//! answers first.
//!
//! The pool below is lazy and points at an address with no server, so a connect
//! never starts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::csrf::*;

/// (1) U1 acceptance: a cookie-authed cross-site POST to `/api/*` is
/// `403 cross_origin_rejected`.
///
/// `Sec-Fetch-Site: cross-site` is the signal a browser sends when a page on
/// another site posts a form here. The session cookie rides along, because it
/// is ambient. That pair is the whole shape of a CSRF write.
#[tokio::test]
async fn a_cookie_authed_cross_site_post_is_403_cross_origin_rejected() {
    let app = app();

    let (status, body) = cross_site_answer(&app, &[]).await;

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(status.as_u16(), 403);
    assert_eq!(body, REJECTION_BODY);
}

/// (2) U1 acceptance: the same request with a bearer token is NOT refused.
///
/// A browser cannot attach an `Authorization` header cross-site, so a bearer
/// write cannot be forged. D-M5-5 keeps the channel for scripts and tests, and
/// the exemption is the reason it works.
#[tokio::test]
async fn the_same_cross_site_post_with_a_bearer_token_is_not_refused() {
    let app = app();

    let (status, body) =
        cross_site_answer(&app, &[("authorization", "Bearer a-session-token")]).await;

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(status.as_u16(), 401);
    assert_eq!(body, UNAUTHORIZED_BODY);
}

/// (3) The scheme of the bearer header is case-insensitive (RFC 9110), so
/// `bearer` earns the same exemption as `Bearer`.
#[tokio::test]
async fn a_lowercase_bearer_scheme_earns_the_exemption() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("sec-fetch-site", "cross-site"),
                ("authorization", "bearer a-session-token"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 401);
}

/// (4) A bare `Authorization: Bearer` earns NO exemption.
///
/// It authenticates nothing, so the request goes on to authenticate by cookie.
/// In 1.0 the CSRF layer read the header without reading its value and exempted
/// exactly the request it exists to refuse.
#[tokio::test]
async fn a_bearer_header_with_an_empty_value_earns_no_exemption() {
    let app = app();

    for header in ["Bearer", "Bearer ", "Bearer    "] {
        let (status, body) = send(
            &app,
            post(
                "/api/task/t-1/answer",
                &[
                    ("host", "tutor.example"),
                    ("cookie", SECURE_COOKIE),
                    ("sec-fetch-site", "cross-site"),
                    ("authorization", header),
                ],
            ),
        )
        .await;

        assert_eq!(
            status.as_u16(),
            403,
            "Authorization: {header:?} must earn no exemption; the body was {body}"
        );
        assert_eq!(body, REJECTION_BODY);
    }
}

/// (5) Another scheme is not the bearer channel and earns no exemption.
#[tokio::test]
async fn a_basic_authorization_header_earns_no_exemption() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("cookie", SECURE_COOKIE),
                ("sec-fetch-site", "cross-site"),
                ("authorization", "Basic dXNlcjpwYXNz"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 403);
}

/// (6) Rule 1 fails CLOSED: a cookie-authed `/api/*` write with NO origin
/// signal at all is refused (trap W11).
///
/// A live session cookie is at stake here, so the absence of a signal is not
/// evidence of innocence. This is the polarity that test (9) below inverts for
/// the pre-auth routes.
#[tokio::test]
async fn a_cookie_authed_post_with_no_origin_signal_is_refused() {
    let app = app();

    let (status, body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[("host", "tutor.example"), ("cookie", SECURE_COOKIE)],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 403);
    assert_eq!(body, REJECTION_BODY);
}

/// (7) A request with no session cookie carries no ambient credential, so the
/// layer lets it through to the auth path, which answers `401`.
#[tokio::test]
async fn a_cross_site_post_without_a_session_cookie_is_not_refused() {
    let app = app();

    let (status, _body) = send(
        &app,
        post(
            "/api/task/t-1/answer",
            &[
                ("host", "tutor.example"),
                ("sec-fetch-site", "cross-site"),
                ("origin", "https://evil.example"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 401);
}

/// (8) U1 acceptance: a cross-origin POST to `/api/auth/login` is `403`, and so
/// is one to `/api/auth/signup` and `/api/auth/verify-email`.
///
/// No session cookie exists yet, so rule 1 cannot see this. It is login-CSRF:
/// the attacker forces a victim into an account the attacker owns.
/// `verify-email` is on the list because it mints a session.
#[tokio::test]
async fn a_cross_origin_post_to_a_pre_auth_route_is_403() {
    let app = app();

    for path in [
        "/api/auth/login",
        "/api/auth/signup",
        "/api/auth/verify-email",
    ] {
        let (status, body) = send(
            &app,
            post(
                path,
                &[
                    ("host", "tutor.example"),
                    ("origin", "https://evil.example"),
                ],
            ),
        )
        .await;

        assert_eq!(
            status.as_u16(),
            403,
            "{path} must refuse a cross-origin POST"
        );
        assert_eq!(body, REJECTION_BODY);
    }

    // `Sec-Fetch-Site` alone is signal enough, with no `Origin` header.
    for site in ["cross-site", "same-site"] {
        let (status, _body) = send(
            &app,
            post(
                "/api/auth/login",
                &[("host", "tutor.example"), ("sec-fetch-site", site)],
            ),
        )
        .await;

        assert_eq!(
            status.as_u16(),
            403,
            "Sec-Fetch-Site: {site} must be refused"
        );
    }
}

/// (9) U1 acceptance: a header-less curl POST to a pre-auth route is NOT
/// refused.
///
/// Rule 2 fails OPEN, which is the exact opposite of rule 1 (trap W11). No
/// ambient credential is at stake before a session exists, and a browser CSRF
/// always carries one of the two signals, so a header-less programmatic client
/// — curl, a test, a native application — must go through.
#[tokio::test]
async fn a_header_less_post_to_a_pre_auth_route_is_not_refused() {
    let app = app();

    for path in [
        "/api/auth/login",
        "/api/auth/signup",
        "/api/auth/verify-email",
    ] {
        let (status, body) = send(&app, post(path, &[("host", "tutor.example")])).await;

        assert_eq!(
            status.as_u16(),
            422,
            "{path} must let a header-less POST through"
        );
        assert_eq!(body, EMPTY_BODY_REJECTION);
    }
}

/// (10) A pre-auth POST with a bearer token is exempt from rule 2 too.
#[tokio::test]
async fn a_bearer_post_to_a_pre_auth_route_is_not_refused() {
    let app = app();

    let (status, body) = send(
        &app,
        post(
            "/api/auth/login",
            &[
                ("host", "tutor.example"),
                ("origin", "https://evil.example"),
                ("authorization", "Bearer a-session-token"),
            ],
        ),
    )
    .await;

    assert_eq!(status.as_u16(), 422);
    assert_eq!(body, EMPTY_BODY_REJECTION);
}
