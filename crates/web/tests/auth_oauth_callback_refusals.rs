//! Part of `tests/auth_oauth_callback.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

// ---------------------------------------------------------------------------
// The refusals
// ---------------------------------------------------------------------------

/// A mismatched `state` is `400 oauth_error`, and it opens no session.
#[tokio::test]
async fn a_mismatched_state_is_400_with_no_session() {
    TestDb::with(|db| async move {
        let (provider, answer) =
            verified_google_callback(&db, "code=u5-code&state=u5-wrong-state").await;

        assert_state_mismatch(&answer);
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
        let answer = callback_with(
            &app,
            "github",
            "code=u5-code&state=u5-state-value",
            &google_jar(),
        )
        .await;

        assert_state_mismatch(&answer);
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
        let (provider, answer) =
            verified_google_callback(&db, "error=access_denied&state=u5-state-value").await;

        assert_oauth_error(&answer);
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
        let provider = google_provider(
            r#"{"sub":"google-subject-1","email":"learner@example.com","email_verified":false}"#,
        );
        let answer = refused_google_callback(&db, provider).await;
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
        let provider = google_provider(
            r#"{"sub":"attacker-subject","email":"learner@example.com","email_verified":false}"#,
        );
        let app = oauth_app(&db, google_config(Arc::new(provider)));
        signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        let user = user_id(&db, "learner@example.com").await;

        let answer = google_callback(&app, "code=u5-code&state=u5-state-value").await;

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
        let provider = github_provider(
            r#"{"id":424242}"#,
            r#"[{"email":"second@example.com","primary":false,"verified":true}]"#,
        );

        let answer = github_callback(&db, provider).await;
        assert_oauth_error(&answer);
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
        let answer = refused_google_callback(&db, provider).await;
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
        let provider = google_provider("<html>not json</html>");
        let _answer = refused_google_callback(&db, provider).await;
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

        let answer = google_callback(&app, "code=u5-code&state=u5-state-value").await;

        assert_state_mismatch(&answer);
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

/// A tab inside the target sends the browser to `/`.
///
/// A browser removes every ASCII tab from a URL before it parses the URL, so a
/// `Location` of `/`, one tab, `/evil.example` reaches the parser as the
/// scheme-relative `//evil.example` and leaves the site (finding F5).
///
/// The base64url is of
/// `{"next":"/<TAB>/evil.example","p":"google","s":"u5-state-value","v":"u5-pkce-verifier"}`,
/// where `<TAB>` marks the two characters `\t`, the JSON escape of one tab.
#[tokio::test]
async fn a_tab_in_the_target_sends_the_browser_to_the_root() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));

        let answer = send(
            &app,
            get_with_cookie(
                "/api/auth/oauth/google/callback?code=u5-code&state=u5-state-value",
                "cadus_oauth_handshake=eyJuZXh0IjoiL1x0L2V2aWwuZXhhbXBsZSIsInAiOiJnb29nbGUiLCJzIjoidTUtc3RhdGUtdmFsdWUiLCJ2IjoidTUtcGtjZS12ZXJpZmllciJ9",
            ),
        )
        .await;

        assert_eq!(answer.status.as_u16(), 302);
        assert_eq!(location_of(&answer), "/");
    })
    .await;
}
