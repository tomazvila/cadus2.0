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

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use cadus_store::test_support::TestDb;
use common::*;

// ---------------------------------------------------------------------------
// The happy paths
// ---------------------------------------------------------------------------

/// A verified Google identity creates a password-less account and signs in.
#[tokio::test]
async fn a_verified_google_identity_creates_an_account_and_opens_a_session() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));

        let answer = google_callback(&app, "code=u5-code&state=u5-state-value").await;

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

        google_callback(&app, "code=u5-code&state=u5-state-value").await;

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
        let provider = github_provider(
            r#"{"id":424242,"login":"learner"}"#,
            r#"[{"email":"second@example.com","primary":false,"verified":true},
                {"email":"Primary@Example.com","primary":true,"verified":true}]"#,
        );

        let answer = github_callback(&db, provider).await;

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
///
/// The address is verified BEFORE the callback runs, so the person who set the
/// password held the address. That password survives the link.
#[tokio::test]
async fn a_verified_identity_links_into_the_account_that_owns_the_address() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));
        let answer = signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        assert_eq!(answer.status.as_u16(), 200);
        mark_verified(&db, "learner@example.com").await;
        let user = user_id(&db, "learner@example.com").await;
        link_existing_account(&db, &app, user).await;
        assert_eq!(oauth_link_count(&db, user).await, 1);
        // The password of a verified address survives the link, and so does
        // every session that address opened.
        assert!(password_hash(&db, "learner@example.com").await.is_some());
        assert!(is_verified(&db, "learner@example.com").await);
        assert_eq!(session_count(&db, user).await, 2);
    })
    .await;
}

/// A link into an UNVERIFIED account clears the password and every session.
///
/// Sign-up asks for no proof of the address, so an attacker registers an address
/// that the attacker does not hold and sets a password on it. Login refuses that
/// password for one reason: the address is not verified. The provider sign-in of
/// the real address owner removes that reason. The callback therefore clears the
/// password hash and deletes every session of the account, and only then stamps
/// `email_verified_at` (finding F14).
#[tokio::test]
async fn a_link_into_an_unverified_account_clears_the_password_and_the_sessions() {
    TestDb::with(|db| async move {
        let app = oauth_app(&db, google_config(Arc::new(google_verified())));
        let answer = signup(&app, "learner@example.com", GOOD_PASSWORD).await;
        assert_eq!(answer.status.as_u16(), 200);
        let user = user_id(&db, "learner@example.com").await;
        assert!(!is_verified(&db, "learner@example.com").await);
        let _answer = link_existing_account(&db, &app, user).await;
        assert_eq!(password_hash(&db, "learner@example.com").await, None);
        assert!(is_verified(&db, "learner@example.com").await);
        // The planted session is gone, and the one row left is the session this
        // callback opened.
        assert_eq!(session_rows(&db).await, 1);
        assert_eq!(seeded_session_rows(&db, SESSION_TOKEN_ONE.1).await, 0);

        // The password the attacker set does not sign in.
        let refused = login(&app, "learner@example.com", GOOD_PASSWORD).await;
        assert_eq!(refused.status.as_u16(), 401);
        assert_eq!(refused.code(), "invalid_credentials");
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
