//! OAuth, part 2: the callback.
//!
//! Spec `docs/reference/web-service-1.0-spec.md` section 11, row U5, and section
//! 3.3, "OAuth callback". This file holds the two acceptance checks part 1 does
//! not: "a mismatched `state` → `400 oauth_error` with no session" and "an
//! unverified provider email never links".
//!
//! Every expected value below is a LITERAL of this file: a status code, an error
//! code, a message, a redirect target, a cookie attribute, an address, a
//! provider account id, or a row count. Nothing re-reads a constant of the code
//! under test (HANDOVER section 3).
//!
//! Each test builds its own throwaway database, so two tests never share an
//! account or a link row.

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
use common::{
    Answer, FakeProvider, GOOD_PASSWORD, cookies_of, disable, get, get_with_cookie, github_config,
    google_config, is_verified, location_of, maybe_user_id, oauth_app, oauth_link_count,
    password_hash, send, signup, user_count, user_id,
};
use sqlx::types::Uuid;

/// The Google token endpoint.
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// The Google userinfo endpoint.
const GOOGLE_USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";

/// The GitHub token endpoint.
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// The GitHub account endpoint.
const GITHUB_USER_URL: &str = "https://api.github.com/user";

/// The GitHub address-list endpoint.
const GITHUB_EMAILS_URL: &str = "https://api.github.com/user/emails";

/// The base64url of
/// `{"next":"/dashboard","p":"google","s":"u5-state-value","v":"u5-pkce-verifier"}`.
const GOOGLE_COOKIE: &str = "eyJuZXh0IjoiL2Rhc2hib2FyZCIsInAiOiJnb29nbGUiLCJzIjoidTUtc3RhdGUtdmFsdWUiLCJ2IjoidTUtcGtjZS12ZXJpZmllciJ9";

/// The base64url of
/// `{"next":"/","p":"github","s":"u5-github-state","v":"u5-github-verifier"}`.
const GITHUB_COOKIE: &str = "eyJuZXh0IjoiLyIsInAiOiJnaXRodWIiLCJzIjoidTUtZ2l0aHViLXN0YXRlIiwidiI6InU1LWdpdGh1Yi12ZXJpZmllciJ9";

/// The whole `Cookie` header of a Google callback.
fn google_jar() -> String {
    format!("cadus_oauth_handshake={GOOGLE_COOKIE}")
}

/// A Google provider that answers a verified identity.
fn google_verified() -> FakeProvider {
    FakeProvider::new()
        .answer(
            GOOGLE_TOKEN_URL,
            200,
            r#"{"access_token":"u5-google-access","token_type":"Bearer"}"#,
        )
        .answer(
            GOOGLE_USERINFO_URL,
            200,
            r#"{"sub":"google-subject-1","email":"Learner@Example.com","email_verified":true}"#,
        )
}

/// How many session rows this database holds.
async fn session_rows(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM auth_sessions")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// How many link rows this database holds.
async fn link_rows(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM oauth_accounts")
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The `(provider, provider_account_id, email_at_link)` of one account's link.
async fn link_of(db: &TestDb, user: Uuid) -> (String, String, String) {
    sqlx::query_as(
        "SELECT provider, provider_account_id, email_at_link FROM oauth_accounts WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The `Set-Cookie` header whose name is `name`.
fn cookie_named(answer: &Answer, name: &str) -> String {
    cookies_of(answer)
        .into_iter()
        .find(|cookie| cookie.starts_with(&format!("{name}=")))
        .unwrap_or_else(|| panic!("no Set-Cookie named {name} in {:?}", cookies_of(answer)))
}

// ---------------------------------------------------------------------------
// The happy paths
// ---------------------------------------------------------------------------

/// A verified Google identity creates a password-less account and signs in.
#[tokio::test]
async fn a_verified_google_identity_creates_an_account_and_opens_a_session() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 302);
        assert_eq!(location_of(&answer), "/dashboard");

        // The address is normalized, the account carries no password, and the
        // provider's claim stands in for the verification link.
        assert_eq!(user_count(&db).await, 1);
        let user = user_id(&db, "learner@example.com").await;
        assert_eq!(password_hash(&db, "learner@example.com").await, None);
        assert!(is_verified(&db, "learner@example.com").await);

        assert_eq!(oauth_link_count(&db, user).await, 1);
        assert_eq!(
            link_of(&db, user).await,
            (
                "google".to_string(),
                "google-subject-1".to_string(),
                "learner@example.com".to_string()
            )
        );
        assert_eq!(session_rows(&db).await, 1);

        // The session cookie opens the session and the handshake cookie ends.
        let session = cookie_named(&answer, "__Host-cadus_session").to_lowercase();
        assert!(session.contains("httponly"), "{session}");
        assert!(session.contains("secure"), "{session}");
        assert!(session.contains("samesite=lax"), "{session}");
        assert!(session.contains("max-age=2592000"), "{session}");
        let spent = cookie_named(&answer, "cadus_oauth_handshake").to_lowercase();
        assert!(spent.contains("max-age=0"), "{spent}");
        assert!(spent.contains("path=/api/auth/oauth"), "{spent}");
    })
    .await;
}

/// The token exchange carries the PKCE verifier, the code, and the secret, and
/// the identity read carries the access token.
#[tokio::test]
async fn the_token_exchange_carries_the_verifier_and_the_identity_read_the_token() {
    TestDb::with(|db| async move {
        let provider = Arc::new(google_verified());
        let app = oauth_app(&db, google_config(Arc::clone(&provider)));

        send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        let seen = provider.seen();
        assert_eq!(seen.len(), 2, "one exchange and one identity read");

        assert_eq!(seen[0].method, "POST");
        assert_eq!(seen[0].url, GOOGLE_TOKEN_URL);
        assert_eq!(seen[0].bearer, None);
        assert_eq!(
            seen[0].body.as_deref(),
            Some(
                "grant_type=authorization_code&code=u5-code\
                 &redirect_uri=https%3A%2F%2Ftutor.example%2Fapi%2Fauth%2Foauth%2Fgoogle%2Fcallback\
                 &client_id=u5-google-client-id&client_secret=u5-google-client-secret\
                 &code_verifier=u5-pkce-verifier"
            )
        );

        assert_eq!(seen[1].method, "GET");
        assert_eq!(seen[1].url, GOOGLE_USERINFO_URL);
        assert_eq!(seen[1].bearer.as_deref(), Some("u5-google-access"));
        assert_eq!(seen[1].body, None);
    })
    .await;
}

/// GitHub's numeric id is the subject, and the PRIMARY verified address is the
/// account address.
#[tokio::test]
async fn a_verified_github_identity_links_the_primary_address() {
    TestDb::with(|db| async move {
        let provider = FakeProvider::new()
            .answer(
                GITHUB_TOKEN_URL,
                200,
                r#"{"access_token":"u5-github-access"}"#,
            )
            .answer(GITHUB_USER_URL, 200, r#"{"id":424242,"login":"learner"}"#)
            .answer(
                GITHUB_EMAILS_URL,
                200,
                r#"[{"email":"second@example.com","primary":false,"verified":true},
                    {"email":"Primary@Example.com","primary":true,"verified":true}]"#,
            );
        let app = oauth_app(&db, github_config(Arc::new(provider)));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/github/callback?code=u5-code&state=u5-github-state",
                &format!("cadus_oauth_handshake={GITHUB_COOKIE}"),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 302);
        assert_eq!(location_of(&answer), "/");
        assert_eq!(user_count(&db).await, 1);
        let user = user_id(&db, "primary@example.com").await;
        assert_eq!(
            link_of(&db, user).await,
            (
                "github".to_string(),
                "424242".to_string(),
                "primary@example.com".to_string()
            )
        );
        assert_eq!(session_rows(&db).await, 1);
    })
    .await;
}

/// An account that already owns the verified address is linked, not duplicated.
#[tokio::test]
async fn a_verified_identity_links_into_the_account_that_owns_the_address() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));
        let answer = signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        assert_eq!(answer.status.as_u16(), 200);
        let user = user_id(&db, "learner@example.com").await;
        assert!(!is_verified(&db, "learner@example.com").await);

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 302);
        assert_eq!(user_count(&db).await, 1, "no second account was created");
        assert_eq!(user_id(&db, "learner@example.com").await, user);
        assert_eq!(oauth_link_count(&db, user).await, 1);
        // The password survives the link, and the provider's claim verifies the
        // address that the sign-up left unverified.
        assert!(password_hash(&db, "learner@example.com").await.is_some());
        assert!(is_verified(&db, "learner@example.com").await);
    })
    .await;
}

/// A returning federated identity signs in and writes no second link row.
#[tokio::test]
async fn a_returning_federated_identity_writes_no_second_link() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));
        let request = || {
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            )
        };

        let first = send(&app, request()).await;
        assert_eq!(first.status.as_u16(), 302);
        let second = send(&app, request()).await;
        assert_eq!(second.status.as_u16(), 302);

        assert_eq!(user_count(&db).await, 1);
        assert_eq!(link_rows(&db).await, 1, "the live link was written twice");
        assert_eq!(session_rows(&db).await, 2, "each sign-in opens a session");
    })
    .await;
}

// ---------------------------------------------------------------------------
// The refusals
// ---------------------------------------------------------------------------

/// A mismatched `state` is `400 oauth_error`, and it opens no session.
#[tokio::test]
async fn a_mismatched_state_is_400_with_no_session() {
    TestDb::with(|db| async move {
        let provider = Arc::new(google_verified());
        let app = oauth_app(&db, google_config(Arc::clone(&provider)));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-wrong-state",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(
            answer.body["error"]["message"],
            "OAuth state mismatch; please start sign-in again."
        );
        assert_eq!(user_count(&db).await, 0);
        assert_eq!(session_rows(&db).await, 0);
        assert!(
            cookies_of(&answer).is_empty(),
            "a refused callback sets no cookie"
        );
        assert!(
            provider.seen().is_empty(),
            "a mismatched state is refused before any provider call"
        );
    })
    .await;
}

/// A handshake minted for one provider never completes another provider's
/// callback.
#[tokio::test]
async fn a_handshake_for_another_provider_is_refused() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, github_config(Arc::new(FakeProvider::new())));

        // The cookie says `google`; the route is GitHub's.
        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/github/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(
            answer.body["error"]["message"],
            "OAuth state mismatch; please start sign-in again."
        );
        assert_eq!(session_rows(&db).await, 0);
    })
    .await;
}

/// A callback with no handshake cookie, no `code`, or no `state` is one `400`.
#[tokio::test]
async fn a_callback_without_a_whole_handshake_is_400() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));

        let cases: Vec<(&str, Option<String>)> = vec![
            (
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                None,
            ),
            (
                "/api/auth/oauth/google/callback?state=u5-state-value",
                Some(google_jar()),
            ),
            (
                "/api/auth/oauth/google/callback?code=u5-code",
                Some(google_jar()),
            ),
            (
                "/api/auth/oauth/google/callback?code=&state=u5-state-value",
                Some(google_jar()),
            ),
            (
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                Some("cadus_oauth_handshake=not-base64-at-all!!".to_string()),
            ),
        ];

        for (path, jar) in cases {
            let request = match &jar {
                Some(jar) => get_with_cookie(path, jar),
                None => get(path),
            };
            let answer = send(&app, request).await;
            assert_eq!(answer.status.as_u16(), 400, "{path} with {jar:?}");
            assert_eq!(answer.code(), "oauth_error", "{path} with {jar:?}");
            assert_eq!(
                answer.body["error"]["message"],
                "OAuth handshake is missing or invalid; please try again.",
                "{path} with {jar:?}"
            );
        }
        assert_eq!(session_rows(&db).await, 0);
    })
    .await;
}

/// A provider that reports its own failure is `400 oauth_error`.
#[tokio::test]
async fn a_provider_error_query_is_400() {
    TestDb::with(|db| async move {
        let provider = Arc::new(google_verified());
        let app = oauth_app(&db, google_config(Arc::clone(&provider)));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?error=access_denied&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(
            answer.body["error"]["message"],
            "OAuth sign-in was cancelled or failed."
        );
        assert!(provider.seen().is_empty());
        assert_eq!(session_rows(&db).await, 0);
    })
    .await;
}

/// An unverified provider address never links and never creates an account.
#[tokio::test]
async fn an_unverified_provider_email_never_links() {
    TestDb::with(|db| async move {
        let provider = FakeProvider::new()
            .answer(
                GOOGLE_TOKEN_URL,
                200,
                r#"{"access_token":"u5-google-access"}"#,
            )
            .answer(
                GOOGLE_USERINFO_URL,
                200,
                r#"{"sub":"google-subject-1","email":"learner@example.com","email_verified":false}"#,
            );
        let app = oauth_app(&db, google_config(Arc::new(provider)));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(user_count(&db).await, 0, "no account was created");
        assert_eq!(link_rows(&db).await, 0, "no link row was written");
        assert_eq!(session_rows(&db).await, 0, "no session was opened");
        assert!(cookies_of(&answer).is_empty());
    })
    .await;
}

/// An unverified provider address never links to an account that already owns
/// the address either.
///
/// This is the pre-registration takeover the rule closes: a provider account
/// that claims an address it did not verify must not reach the account that
/// owns it.
#[tokio::test]
async fn an_unverified_provider_email_never_reaches_an_existing_account() {
    TestDb::with(|db| async move {
        let provider = FakeProvider::new()
            .answer(
                GOOGLE_TOKEN_URL,
                200,
                r#"{"access_token":"u5-google-access"}"#,
            )
            .answer(
                GOOGLE_USERINFO_URL,
                200,
                r#"{"sub":"attacker-subject","email":"learner@example.com","email_verified":false}"#,
            );
        let app = oauth_app(&db, google_config(Arc::new(provider)));
        signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "learner@example.com").await;

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(oauth_link_count(&db, user).await, 0);
        assert_eq!(session_rows(&db).await, 0);
        assert!(
            !is_verified(&db, "learner@example.com").await,
            "an unverified claim must not verify the address"
        );
    })
    .await;
}

/// GitHub with no primary address gives no identity, so nothing is written.
#[tokio::test]
async fn a_github_account_with_no_primary_address_never_links() {
    TestDb::with(|db| async move {
        let provider = FakeProvider::new()
            .answer(
                GITHUB_TOKEN_URL,
                200,
                r#"{"access_token":"u5-github-access"}"#,
            )
            .answer(GITHUB_USER_URL, 200, r#"{"id":424242}"#)
            .answer(
                GITHUB_EMAILS_URL,
                200,
                r#"[{"email":"second@example.com","primary":false,"verified":true}]"#,
            );
        let app = oauth_app(&db, github_config(Arc::new(provider)));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/github/callback?code=u5-code&state=u5-github-state",
                &format!("cadus_oauth_handshake={GITHUB_COOKIE}"),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(user_count(&db).await, 0);
    })
    .await;
}

/// A token exchange the provider refuses is `400 oauth_error`, never a `500`.
#[tokio::test]
async fn a_refused_token_exchange_is_400_and_opens_no_session() {
    TestDb::with(|db| async move {
        let provider =
            FakeProvider::new().answer(GOOGLE_TOKEN_URL, 400, r#"{"error":"invalid_grant"}"#);
        let app = oauth_app(&db, google_config(Arc::new(provider)));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(
            answer.body["error"]["message"],
            "OAuth sign-in failed; please try again."
        );
        assert_eq!(user_count(&db).await, 0);
        assert_eq!(session_rows(&db).await, 0);
    })
    .await;
}

/// A userinfo answer that is not JSON is `400 oauth_error`, never a `500`.
#[tokio::test]
async fn an_unreadable_identity_document_is_400() {
    TestDb::with(|db| async move {
        let provider = FakeProvider::new()
            .answer(
                GOOGLE_TOKEN_URL,
                200,
                r#"{"access_token":"u5-google-access"}"#,
            )
            .answer(GOOGLE_USERINFO_URL, 200, "<html>not json</html>");
        let app = oauth_app(&db, google_config(Arc::new(provider)));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(session_rows(&db).await, 0);
    })
    .await;
}

/// A disabled account is refused with the SAME body as a mismatched `state`,
/// and nothing is written.
#[tokio::test]
async fn a_disabled_account_is_refused_with_the_generic_message() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));
        signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "learner@example.com").await;
        disable(&db, "learner@example.com").await;

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                &google_jar(),
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 400);
        assert_eq!(answer.code(), "oauth_error");
        assert_eq!(
            answer.body["error"]["message"],
            "OAuth state mismatch; please start sign-in again."
        );
        assert_eq!(oauth_link_count(&db, user).await, 0, "no link was written");
        assert_eq!(session_rows(&db).await, 0, "no session was opened");
        assert!(
            !is_verified(&db, "learner@example.com").await,
            "a refused account must not be stamped verified"
        );
    })
    .await;
}

/// A tampered target in the handshake cookie sends the browser to `/`.
///
/// The base64url is of
/// `{"next":"//evil.example","p":"google","s":"u5-state-value","v":"u5-pkce-verifier"}`.
#[tokio::test]
async fn a_tampered_target_sends_the_browser_to_the_root() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                "cadus_oauth_handshake=eyJuZXh0IjoiLy9ldmlsLmV4YW1wbGUiLCJwIjoiZ29vZ2xlIiwicyI6InU1LXN0YXRlLXZhbHVlIiwidiI6InU1LXBrY2UtdmVyaWZpZXIifQ",
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 302);
        assert_eq!(location_of(&answer), "/");
        assert!(maybe_user_id(&db, "learner@example.com").await.is_some());
    })
    .await;
}
