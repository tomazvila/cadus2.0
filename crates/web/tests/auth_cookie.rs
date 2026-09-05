//! M5 U2 — the session-cookie writer, the clearer, the lifetimes, and the
//! bearer-then-cookie selector.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 3.1, rows "Cookie",
//! "Guards", "Second channel", "Idle / absolute window"; section 10, rows
//! "Cookie string" and "Lifetimes". Acceptance checks of unit U2: "a `__Host-`
//! cookie at `/api` raises" and "`Authorization: Bearer` with an empty value
//! selects nothing".
//!
//! Every header string below is a LITERAL, written out in full.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axum::http::{HeaderMap, HeaderValue};
use cadus_web::auth::session::{
    CookieWriteError, LAST_SEEN_TOUCH_SECS, OAUTH_HANDSHAKE_TTL_SECS, RESET_TOKEN_TTL_SECS,
    SESSION_ABSOLUTE_SECS, SESSION_IDLE_SECS, VERIFY_TOKEN_TTL_SECS, clear_auth_cookie,
    clear_session_cookie, read_session_credential, set_auth_cookie, set_session_cookie,
};
use cadus_web::cookie::CookiePosture;

/// A token of the right shape. The value never changes, so the header strings
/// below are literals.
const TOKEN: &str = "Wr0N0Yj8kQ2tGmV6pLxZ4bC7dEfHiJkLmNoPqRsTuVw";

/// The production cookie name.
const SECURE_NAME: &str = "__Host-cadus_session";

/// Build a header map with one `cookie` header.
fn with_cookie(pair: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", HeaderValue::from_str(pair).unwrap());
    headers
}

/// The text of a `Set-Cookie` value.
fn text(value: &HeaderValue) -> String {
    value.to_str().unwrap().to_string()
}

// --------------------------------------------------------------------------
// The cookie string of spec section 10.
// --------------------------------------------------------------------------

/// The session cookie, character for character, under the secure posture.
///
/// `Max-Age=2592000` is the 30-day idle window of spec section 3.1.
#[test]
fn the_session_cookie_is_the_pinned_string() {
    let header = set_session_cookie(CookiePosture::SECURE, TOKEN).unwrap();

    assert_eq!(
        text(&header),
        "__Host-cadus_session=Wr0N0Yj8kQ2tGmV6pLxZ4bC7dEfHiJkLmNoPqRsTuVw; Max-Age=2592000; \
         Path=/; HttpOnly; Secure; SameSite=Lax"
    );
}

/// The dev posture writes the plain name and no `Secure`.
///
/// Local `http://` cannot carry `Secure`, and a `__Host-` name without it is
/// discarded by the browser, so the two move together.
#[test]
fn the_insecure_posture_writes_the_plain_name_without_secure() {
    let header = set_session_cookie(CookiePosture::INSECURE, TOKEN).unwrap();

    assert_eq!(
        text(&header),
        "cadus_session=Wr0N0Yj8kQ2tGmV6pLxZ4bC7dEfHiJkLmNoPqRsTuVw; Max-Age=2592000; Path=/; \
         HttpOnly; SameSite=Lax"
    );
}

/// The deletion, character for character.
#[test]
fn the_session_deletion_is_the_pinned_string() {
    let header = clear_session_cookie(CookiePosture::SECURE).unwrap();

    assert_eq!(
        text(&header),
        "__Host-cadus_session=; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/; \
         HttpOnly; Secure; SameSite=Lax"
    );
}

/// The deletion matches the creation attribute for attribute.
///
/// A browser drops a cookie only when the deletion carries the same name, path,
/// and flags. A mismatch leaves a live session in the jar after a logout that
/// answered `200`.
#[test]
fn the_deletion_matches_the_creation_attribute_for_attribute() {
    for posture in [CookiePosture::SECURE, CookiePosture::INSECURE] {
        let created = text(&set_session_cookie(posture, TOKEN).unwrap());
        let deleted = text(&clear_session_cookie(posture).unwrap());

        let created_attributes: Vec<&str> = created.split("; ").skip(1).collect();
        let deleted_attributes: Vec<&str> = deleted.split("; ").skip(1).collect();

        // Same name, and the deletion carries no token.
        assert_eq!(
            created.split('=').next(),
            deleted.split('=').next(),
            "the names differ: {created} against {deleted}"
        );
        assert!(deleted.starts_with(&format!("{}=;", posture.name)));

        for attribute in ["Path=/", "HttpOnly", "SameSite=Lax"] {
            assert!(
                created_attributes.contains(&attribute),
                "the creation lost {attribute}: {created}"
            );
            assert!(
                deleted_attributes.contains(&attribute),
                "the deletion lost {attribute}: {deleted}"
            );
        }
        assert_eq!(
            created_attributes.contains(&"Secure"),
            deleted_attributes.contains(&"Secure"),
            "the Secure flags differ: {created} against {deleted}"
        );
    }
}

// --------------------------------------------------------------------------
// The `__Host-` guard on the writer and on the clearer.
// --------------------------------------------------------------------------

/// A `__Host-` cookie at `/api` is refused. This is the acceptance check.
///
/// The browser discards such a cookie with no error anywhere, so the login
/// appears to work and the session never persists. The writer must make that
/// header impossible to build.
#[test]
fn a_host_prefixed_cookie_at_api_is_refused() {
    let answer = set_auth_cookie(CookiePosture::SECURE, SECURE_NAME, TOKEN, 600, "/api");

    assert_eq!(
        answer,
        Err(CookieWriteError::HostPrefixAtPath {
            name: "__Host-cadus_session".to_string(),
            path: "/api".to_string(),
        })
    );
}

/// Every path but `/` is refused, the OAuth handshake path included.
#[test]
fn a_host_prefixed_cookie_is_refused_at_every_path_but_root() {
    for path in ["/api", "/api/auth/oauth", "/a", "//"] {
        let answer = set_auth_cookie(CookiePosture::SECURE, SECURE_NAME, TOKEN, 600, path);
        assert!(
            matches!(answer, Err(CookieWriteError::HostPrefixAtPath { .. })),
            "path {path:?} must be refused, it gave {answer:?}"
        );
    }
    // The control: the one legal path builds a header.
    assert!(set_auth_cookie(CookiePosture::SECURE, SECURE_NAME, TOKEN, 600, "/").is_ok());
}

/// The clearer carries the same guard.
///
/// A deletion aimed at a path the writer could never have used is a no-op, and
/// the session cookie stays live after the logout.
#[test]
fn a_host_prefixed_deletion_at_api_is_refused() {
    let answer = clear_auth_cookie(CookiePosture::SECURE, SECURE_NAME, "/api");

    assert_eq!(
        answer,
        Err(CookieWriteError::HostPrefixAtPath {
            name: "__Host-cadus_session".to_string(),
            path: "/api".to_string(),
        })
    );
    assert!(clear_auth_cookie(CookiePosture::SECURE, SECURE_NAME, "/").is_ok());
}

/// A name without the prefix may take a narrower path.
///
/// This is the shape unit U5 needs for the OAuth handshake cookie: 600 seconds,
/// scoped to the OAuth routes.
#[test]
fn a_plain_name_may_take_a_scoped_path() {
    let header = set_auth_cookie(
        CookiePosture::SECURE,
        "cadus_oauth_handshake",
        "state-value",
        600,
        "/api/auth/oauth",
    )
    .unwrap();

    assert_eq!(
        text(&header),
        "cadus_oauth_handshake=state-value; Max-Age=600; Path=/api/auth/oauth; HttpOnly; Secure; \
         SameSite=Lax"
    );
}

/// The refusal text names the cookie and the path.
#[test]
fn the_guard_text_names_the_cookie_and_the_path() {
    let error = CookieWriteError::HostPrefixAtPath {
        name: "__Host-cadus_session".to_string(),
        path: "/api".to_string(),
    };

    assert_eq!(
        error.to_string(),
        "refusing to write cookie \"__Host-cadus_session\" at path \"/api\": a browser discards a \
         __Host- cookie at any path but \"/\"; give the cookie a name without the prefix if it \
         needs a narrower scope"
    );
}

// --------------------------------------------------------------------------
// Header injection.
// --------------------------------------------------------------------------

/// A value that would end the cookie early is refused.
///
/// Without this, a caller that put a `;` in a token would append attributes of
/// its own — a `Path=/` on a handshake cookie, or a second cookie entirely.
#[test]
fn a_value_that_would_end_the_cookie_early_is_refused() {
    for value in [
        "tok; Path=/",
        "tok, other=1",
        "tok value",
        "tok\r\nSet-Cookie: evil=1",
        "tok\"",
        "tok\\",
        "tok\u{7f}",
        "tök",
    ] {
        assert_eq!(
            set_auth_cookie(CookiePosture::SECURE, SECURE_NAME, value, 600, "/"),
            Err(CookieWriteError::BadValue),
            "the value {value:?} must be refused"
        );
    }
    // The control: the token shape passes.
    assert!(set_auth_cookie(CookiePosture::SECURE, SECURE_NAME, TOKEN, 600, "/").is_ok());
}

/// A name that is not an HTTP token is refused.
#[test]
fn a_name_that_is_not_a_token_is_refused() {
    for name in ["", "bad name", "bad;name", "bad=name", "b\u{e4}d"] {
        assert!(
            matches!(
                set_auth_cookie(CookiePosture::SECURE, name, TOKEN, 600, "/"),
                Err(CookieWriteError::BadName { .. })
            ),
            "the name {name:?} must be refused"
        );
    }
}

/// A path that is not a legal `Set-Cookie` path is refused.
#[test]
fn a_path_that_is_not_legal_is_refused() {
    for path in ["", "api", "/api;x", "/api\r\n", "/\u{e4}"] {
        assert!(
            matches!(
                set_auth_cookie(CookiePosture::SECURE, "cadus_x", TOKEN, 600, path),
                Err(CookieWriteError::BadPath { .. })
            ),
            "the path {path:?} must be refused"
        );
    }
}

// --------------------------------------------------------------------------
// The lifetimes of spec section 10.
// --------------------------------------------------------------------------

/// 30 days, 90 days, 1 hour, 30 minutes, 24 hours, 600 seconds.
#[test]
fn the_lifetimes_are_the_pinned_seconds() {
    assert_eq!(SESSION_IDLE_SECS, 2_592_000);
    assert_eq!(SESSION_ABSOLUTE_SECS, 7_776_000);
    assert_eq!(LAST_SEEN_TOUCH_SECS, 3_600);
    assert_eq!(RESET_TOKEN_TTL_SECS, 1_800);
    assert_eq!(VERIFY_TOKEN_TTL_SECS, 86_400);
    assert_eq!(OAUTH_HANDSHAKE_TTL_SECS, 600);
}

// --------------------------------------------------------------------------
// The bearer-then-cookie selector.
// --------------------------------------------------------------------------

/// The bearer header wins over the cookie.
///
/// An explicit credential must never be shadowed by the ambient one a browser
/// attached by itself.
#[test]
fn the_bearer_header_wins_over_the_cookie() {
    let mut headers = with_cookie("__Host-cadus_session=from-cookie");
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer from-header"),
    );

    assert_eq!(
        read_session_credential(&headers, SECURE_NAME),
        Some("from-header")
    );
}

/// The scheme is case-insensitive (RFC 9110).
#[test]
fn the_bearer_scheme_is_case_insensitive() {
    for raw in ["Bearer tok", "bearer tok", "BEARER tok", "bEaReR tok"] {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_str(raw).unwrap());
        assert_eq!(
            read_session_credential(&headers, SECURE_NAME),
            Some("tok"),
            "the header {raw:?} must select the token"
        );
    }
}

/// `Authorization: Bearer` with an empty value selects nothing. This is the
/// acceptance check.
///
/// Fail the test when one of the `Authorization` values in `raws` selects a
/// credential.
fn assert_selects_nothing(raws: &[&str]) {
    for raw in raws {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_str(raw).unwrap());
        assert_eq!(
            read_session_credential(&headers, SECURE_NAME),
            None,
            "the header {raw:?} must select nothing"
        );
    }
}

/// A bare `Bearer` authenticates nothing. If it read as "this request is
/// bearer-authed", it would earn the CSRF exemption of spec section 3.1 and then
/// go on to authenticate by cookie — the exact request the layer exists to
/// refuse.
#[test]
fn an_empty_bearer_value_selects_nothing() {
    assert_selects_nothing(&["Bearer", "Bearer ", "Bearer   ", "Bearer \t"]);
}

/// An empty bearer value falls through to the cookie; it does not shadow it.
#[test]
fn an_empty_bearer_value_falls_through_to_the_cookie() {
    let mut headers = with_cookie("__Host-cadus_session=from-cookie");
    headers.insert("authorization", HeaderValue::from_static("Bearer "));

    assert_eq!(
        read_session_credential(&headers, SECURE_NAME),
        Some("from-cookie")
    );
}

/// Another scheme selects nothing from the header.
#[test]
fn another_authorization_scheme_selects_nothing() {
    assert_selects_nothing(&["Basic YWRhOnNlY3JldA==", "Token tok", "Bearertok", "tok"]);
}

/// The cookie alone is selected when no bearer header is present.
#[test]
fn the_cookie_is_selected_when_no_bearer_header_arrives() {
    let headers = with_cookie("other=1; __Host-cadus_session=from-cookie; third=3");

    assert_eq!(
        read_session_credential(&headers, SECURE_NAME),
        Some("from-cookie")
    );
}

/// A cookie under the OTHER posture's name is ignored, never trusted.
#[test]
fn a_cookie_under_the_other_name_is_ignored() {
    let headers = with_cookie("cadus_session=from-dev-cookie");

    assert_eq!(read_session_credential(&headers, SECURE_NAME), None);
    assert_eq!(
        read_session_credential(&headers, "cadus_session"),
        Some("from-dev-cookie")
    );
}

/// A request with neither channel selects nothing.
#[test]
fn a_request_with_no_credential_selects_nothing() {
    assert_eq!(
        read_session_credential(&HeaderMap::new(), SECURE_NAME),
        None
    );
}
