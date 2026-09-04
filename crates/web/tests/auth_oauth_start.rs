//! Part of `tests/auth_oauth.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::sync::Arc;

use cadus_store::test_support::TestDb;
use cadus_web::auth::oauth::{Credentials, OAuthConfig};
use common::*;

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
