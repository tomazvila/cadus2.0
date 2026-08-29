//! OAuth, part 1: the mechanics and the start route.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 11, row U5:
//! "unconfigured provider → `404 not_found`; a mismatched `state` → `400
//! oauth_error` with no session; an unverified provider email never links".
//! Part 2 lives in `auth_oauth_callback.rs` and holds the last two.
//!
//! The section 3.1 rows this file pins: PKCE S256, the `cadus_oauth_handshake`
//! cookie with `HttpOnly`, `SameSite=Lax`, a path scoped to the OAuth routes and
//! `Max-Age=600`, the Google scope `openid email profile`, the GitHub scope
//! `read:user user:email`, and the `404` of an unconfigured provider.
//!
//! Every expected value below is a LITERAL of this file: a status code, an error
//! code, a URL, a cookie attribute, a digest, or a length. Nothing re-reads a
//! constant of the code under test (HANDOVER section 3).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use cadus_store::test_support::TestDb;
use cadus_web::auth::oauth::{
    Credentials, Handshake, OAuthConfig, code_challenge_s256, decode_handshake, encode_handshake,
    safe_next,
};
use common::{
    FakeProvider, cookies_of, get, github_config, google_config, location_of, oauth_app, send,
};

/// The base64url of the SHA-256 of `u5-pkce-verifier`, with no padding.
///
/// The value comes from `printf %s u5-pkce-verifier | sha256sum` piped through a
/// base64url encoder, NOT from the function under test.
const PINNED_CHALLENGE: &str = "yMX3MGbSS42o98kk95nVakkIY8_y97e5ZshCbIQQQmU";

/// The base64url of `{"next":"/dashboard","p":"google","s":"u5-state-value","v":"u5-pkce-verifier"}`.
const PINNED_COOKIE_VALUE: &str = "eyJuZXh0IjoiL2Rhc2hib2FyZCIsInAiOiJnb29nbGUiLCJzIjoidTUtc3RhdGUtdmFsdWUiLCJ2IjoidTUtcGtjZS12ZXJpZmllciJ9";

/// One query parameter of a URL, or `None`.
fn param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        let (name, value) = pair.split_once('=')?;
        if name == key {
            return Some(value.to_string());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// PKCE, the handshake record, and the redirect target
// ---------------------------------------------------------------------------

/// S256 is the SHA-256 of the verifier in base64url with no padding.
#[test]
fn the_pkce_challenge_is_the_pinned_digest() {
    assert_eq!(code_challenge_s256("u5-pkce-verifier"), PINNED_CHALLENGE);
    assert_eq!(
        code_challenge_s256("").len(),
        43,
        "a 32-byte digest is 43 base64url characters"
    );
}

/// A fresh handshake draws a 32-character `state` and a 64-character verifier,
/// and two draws differ.
///
/// 24 bytes of entropy occupy 32 base64 characters and 48 bytes occupy 64. RFC
/// 7636 wants a verifier of 43 to 128 characters, so 64 is inside the range.
#[test]
fn a_fresh_handshake_draws_a_state_and_a_verifier() {
    let first = Handshake::fresh("google", "/dashboard").unwrap();
    let second = Handshake::fresh("google", "/dashboard").unwrap();

    assert_eq!(first.provider, "google");
    assert_eq!(first.next_url, "/dashboard");
    assert_eq!(first.state.len(), 32);
    assert_eq!(first.code_verifier.len(), 64);
    assert_ne!(first.state, second.state, "two draws gave the same state");
    assert_ne!(
        first.code_verifier, second.code_verifier,
        "two draws gave the same verifier"
    );
}

/// A `Debug` of the handshake prints neither secret.
///
/// A failing assertion in any test prints this value, and a `state` in a log
/// line is a live CSRF token.
#[test]
fn the_handshake_debug_redacts_both_secrets() {
    let handshake = Handshake {
        provider: "google".to_string(),
        state: "u5-state-value".to_string(),
        code_verifier: "u5-pkce-verifier".to_string(),
        next_url: "/dashboard".to_string(),
    };
    let shown = format!("{handshake:?}");

    assert!(!shown.contains("u5-state-value"), "{shown}");
    assert!(!shown.contains("u5-pkce-verifier"), "{shown}");
    assert!(shown.contains("google"), "{shown}");
}

/// The cookie value carries the four fields and reads back unchanged.
#[test]
fn the_handshake_cookie_value_round_trips() {
    let handshake = Handshake {
        provider: "google".to_string(),
        state: "u5-state-value".to_string(),
        code_verifier: "u5-pkce-verifier".to_string(),
        next_url: "/dashboard".to_string(),
    };
    assert_eq!(encode_handshake(&handshake), PINNED_COOKIE_VALUE);

    let read = decode_handshake(Some(PINNED_COOKIE_VALUE)).unwrap();
    assert_eq!(read.provider, "google");
    assert_eq!(read.state, "u5-state-value");
    assert_eq!(read.code_verifier, "u5-pkce-verifier");
    assert_eq!(read.next_url, "/dashboard");
}

/// Every unusable cookie value reads back as nothing.
///
/// The callback answers all of them with one `400 oauth_error`, so a caller
/// cannot tell a tampered cookie from an absent one.
#[test]
fn an_unusable_handshake_cookie_reads_back_as_nothing() {
    // base64url of `[]`, of `{"p":"google"}`, and of
    // `{"p":"google","s":"x","v":""}`.
    for value in [
        None,
        Some(""),
        Some("not base64 at all!!"),
        Some("W10"),
        Some("eyJwIjoiZ29vZ2xlIn0"),
        Some("eyJwIjoiZ29vZ2xlIiwicyI6IngiLCJ2IjoiIn0"),
    ] {
        assert!(
            decode_handshake(value).is_none(),
            "{value:?} decoded to a handshake"
        );
    }
}

/// The redirect target is a same-site path, or `/`.
///
/// A browser reads `//host` and `/\host` as scheme-relative, so both would leave
/// the site. That is the open redirect this function closes.
#[test]
fn the_redirect_target_is_a_same_site_path_or_the_root() {
    assert_eq!(safe_next(Some("/dashboard")), "/dashboard");
    assert_eq!(safe_next(Some("/a/b?c=d")), "/a/b?c=d");
    assert_eq!(safe_next(Some("/")), "/");
    assert_eq!(safe_next(None), "/");
    assert_eq!(safe_next(Some("")), "/");
    assert_eq!(safe_next(Some("//evil.example")), "/");
    assert_eq!(safe_next(Some("/\\evil.example")), "/");
    assert_eq!(safe_next(Some("https://evil.example")), "/");
    assert_eq!(safe_next(Some("dashboard")), "/");
}

/// A byte below `0x21`, and a backslash anywhere, are refused.
///
/// A browser removes every ASCII tab, LF, and CR from a URL before it parses the
/// URL (WHATWG URL, "remove all ASCII tab or newline"), so a target of `/`, one
/// tab, `/evil.example` becomes the scheme-relative `//evil.example`. A
/// backslash is a path separator to a browser. The accepted set is therefore a
/// leading `/`, a second byte that is not `/` or a backslash, and no other byte
/// below `0x21` and no other backslash.
#[test]
fn a_control_byte_or_a_backslash_in_the_target_is_refused() {
    assert_eq!(safe_next(Some("/\t/evil.example")), "/");
    assert_eq!(safe_next(Some("/\n/evil.example")), "/");
    assert_eq!(safe_next(Some("/\r/evil.example")), "/");
    assert_eq!(safe_next(Some("/ /evil.example")), "/");
    assert_eq!(safe_next(Some("/dashboard\u{0}")), "/");
    assert_eq!(safe_next(Some("/a\\evil.example")), "/");
    assert_eq!(safe_next(Some("/dashboard\t")), "/");
}

/// A cookie value whose `next` is not same-site reads back as `/`.
///
/// The base64url is of `{"next":"//evil.example","p":"google","s":"x","v":"y"}`.
#[test]
fn a_tampered_target_in_the_cookie_reads_back_as_the_root() {
    let read = decode_handshake(Some(
        "eyJuZXh0IjoiLy9ldmlsLmV4YW1wbGUiLCJwIjoiZ29vZ2xlIiwicyI6IngiLCJ2IjoieSJ9",
    ))
    .unwrap();
    assert_eq!(read.next_url, "/");
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// A provider needs BOTH credential halves. One half alone configures nothing.
#[test]
fn a_provider_needs_both_credential_halves() {
    let table: HashMap<&str, &str> = HashMap::from([
        ("OAUTH_GOOGLE_CLIENT_ID", "id"),
        ("OAUTH_GOOGLE_CLIENT_SECRET", "secret"),
        ("OAUTH_GITHUB_CLIENT_ID", "id-only"),
        ("OAUTH_GITHUB_CLIENT_SECRET", "   "),
        ("OAUTH_REDIRECT_BASE_URL", "https://tutor.example"),
    ]);
    let config = OAuthConfig::from_env(|name| table.get(name).map(|value| (*value).to_string()));

    assert!(config.google.is_some(), "both Google halves are set");
    assert!(
        config.github.is_none(),
        "a blank secret must not configure a provider"
    );
    assert_eq!(
        config.redirect_base.as_deref(),
        Some("https://tutor.example")
    );
    assert!(
        config.transport.is_none(),
        "from_env installs no transport by itself"
    );
}

/// A `Debug` of the configuration prints no client secret.
#[test]
fn the_configuration_debug_redacts_the_client_secret() {
    let config = OAuthConfig {
        google: Some(Credentials {
            client_id: "u5-google-client-id".to_string(),
            client_secret: "u5-google-client-secret".to_string(),
        }),
        github: None,
        redirect_base: None,
        transport: None,
    };
    let shown = format!("{config:?}");

    assert!(!shown.contains("u5-google-client-secret"), "{shown}");
    assert!(shown.contains("u5-google-client-id"), "{shown}");
}

// ---------------------------------------------------------------------------
// The start route
// ---------------------------------------------------------------------------

/// The start route sends the browser to Google with PKCE S256 and a `state`.
#[tokio::test]
async fn the_start_route_redirects_to_google_with_pkce_and_state() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(FakeProvider::new())));

        let answer = send(&app, get("/api/auth/oauth/google/start?next=/dashboard")).await;

        assert_eq!(answer.status.as_u16(), 302);
        let target = location_of(&answer);
        assert!(
            target.starts_with(
                "https://accounts.google.com/o/oauth2/v2/auth?response_type=code\
                 &client_id=u5-google-client-id\
                 &redirect_uri=https%3A%2F%2Ftutor.example%2Fapi%2Fauth%2Foauth%2Fgoogle%2Fcallback\
                 &scope=openid%20email%20profile\
                 &state="
            ),
            "the authorize URL is {target}"
        );
        assert_eq!(
            param(&target, "code_challenge_method").as_deref(),
            Some("S256")
        );
        assert_eq!(param(&target, "state").unwrap().len(), 32);
        assert_eq!(param(&target, "code_challenge").unwrap().len(), 43);
        assert!(
            !target.contains("code_verifier"),
            "the authorize URL must never carry the verifier: {target}"
        );
    })
    .await;
}

/// The start route sends the browser to GitHub with the GitHub scope.
#[tokio::test]
async fn the_start_route_redirects_to_github_with_its_own_scope() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, github_config(Arc::new(FakeProvider::new())));

        let answer = send(&app, get("/api/auth/oauth/github/start")).await;

        assert_eq!(answer.status.as_u16(), 302);
        let target = location_of(&answer);
        assert!(
            target.starts_with(
                "https://github.com/login/oauth/authorize?response_type=code\
                 &client_id=u5-github-client-id\
                 &redirect_uri=https%3A%2F%2Ftutor.example%2Fapi%2Fauth%2Foauth%2Fgithub%2Fcallback\
                 &scope=read%3Auser%20user%3Aemail\
                 &state="
            ),
            "the authorize URL is {target}"
        );
    })
    .await;
}

/// The handshake cookie is scoped to the OAuth routes and lives 600 seconds.
///
/// The name carries no `__Host-` prefix on purpose: that prefix demands
/// `Path=/`, and a cookie at `/` would ride on every API request.
#[tokio::test]
async fn the_start_route_scopes_the_handshake_cookie_to_the_oauth_path() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(FakeProvider::new())));

        let answer = send(&app, get("/api/auth/oauth/google/start")).await;
        let cookies = cookies_of(&answer);

        assert_eq!(cookies.len(), 1, "the start route sets one cookie");
        let cookie = cookies[0].to_lowercase();
        assert!(cookie.starts_with("cadus_oauth_handshake="), "{cookie}");
        assert!(cookie.contains("max-age=600"), "{cookie}");
        assert!(cookie.contains("path=/api/auth/oauth"), "{cookie}");
        assert!(cookie.contains("httponly"), "{cookie}");
        assert!(cookie.contains("secure"), "{cookie}");
        assert!(cookie.contains("samesite=lax"), "{cookie}");
        assert!(
            !cookie.contains("__host-"),
            "a path-scoped cookie must not carry the __Host- prefix: {cookie}"
        );
    })
    .await;
}

/// A target that is not same-site never reaches the handshake.
#[tokio::test]
async fn the_start_route_refuses_a_target_that_leaves_the_site() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(FakeProvider::new())));

        let answer = send(
            &app,
            get("/api/auth/oauth/google/start?next=https%3A%2F%2Fevil.example"),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 302);
        let cookie = cookies_of(&answer).remove(0);
        assert!(
            !cookie.contains("evil.example"),
            "the handshake cookie carries the tampered target: {cookie}"
        );
    })
    .await;
}

// ---------------------------------------------------------------------------
// Unconfigured means absent
// ---------------------------------------------------------------------------

/// A deployment with no OAuth answers `404 not_found` on both routes.
#[tokio::test]
async fn an_unconfigured_provider_is_404_on_both_routes() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, OAuthConfig::default());

        for path in [
            "/api/auth/oauth/google/start",
            "/api/auth/oauth/google/callback?code=c&state=s",
            "/api/auth/oauth/github/start",
            "/api/auth/oauth/github/callback?code=c&state=s",
        ] {
            let answer = send(&app, get(path)).await;
            assert_eq!(answer.status.as_u16(), 404, "{path}");
            assert_eq!(answer.code(), "not_found", "{path}");
            assert_eq!(
                answer.body["error"]["message"], "This OAuth provider is not enabled.",
                "{path}"
            );
        }
    })
    .await;
}

/// A provider name this service does not know is `404` too.
#[tokio::test]
async fn an_unknown_provider_name_is_404() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(FakeProvider::new())));

        let answer = send(&app, get("/api/auth/oauth/facebook/start")).await;

        assert_eq!(answer.status.as_u16(), 404);
        assert_eq!(answer.code(), "not_found");
    })
    .await;
}

/// Credentials with no installed transport serve no provider.
///
/// The callback cannot reach the provider without one, so a start route that
/// redirected would strand every sign-in. `404` on both is the honest answer.
#[tokio::test]
async fn credentials_without_a_transport_serve_no_provider() {
    TestDb::with(|db| async move {
        let config = OAuthConfig {
            google: Some(Credentials {
                client_id: "u5-google-client-id".to_string(),
                client_secret: "u5-google-client-secret".to_string(),
            }),
            github: None,
            redirect_base: Some("https://tutor.example".to_string()),
            transport: None,
        };
        let app = oauth_app(&db, config);

        let answer = send(&app, get("/api/auth/oauth/google/start")).await;

        assert_eq!(answer.status.as_u16(), 404);
        assert_eq!(answer.code(), "not_found");
    })
    .await;
}

/// The list names the providers this deployment serves, and nothing else.
#[tokio::test]
async fn the_providers_list_names_the_served_providers() {
    TestDb::with(|db| async move {
        let served = oauth_app(&db, google_config(Arc::new(FakeProvider::new())));
        let answer = send(&served, get("/api/auth/oauth/providers")).await;
        assert_eq!(answer.status.as_u16(), 200);
        assert_eq!(answer.body, serde_json::json!({ "providers": ["google"] }));

        let bare = oauth_app(&db, OAuthConfig::default());
        let answer = send(&bare, get("/api/auth/oauth/providers")).await;
        assert_eq!(
            answer.body,
            serde_json::json!({ "providers": [] }),
            "an unconfigured deployment names no provider"
        );
    })
    .await;
}
