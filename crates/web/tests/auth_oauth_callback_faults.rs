//! The OAuth callback under a store fault.
//!
//! Each test makes ONE statement of the callback fail on purpose, and reads
//! the `500 internal_error` envelope back.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

/// Run the default Google callback on `app` and read the `500` back.
async fn assert_callback_internal(app: &Router) {
    let answer = google_callback(app, GOOGLE_QUERY).await;
    assert_eq!(answer.status.as_u16(), 500, "{}", answer.body);
    assert_eq!(answer.code(), "internal_error");
}

/// A user insert that fails stops the callback of a new address.
#[tokio::test]
async fn a_user_insert_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        fail_writes(&db, "users", "true").await;
        assert_callback_internal(&app).await;
        assert_eq!(user_count(&db).await, 0);
    })
    .await;
}

/// A link insert that fails stops the callback before the session.
#[tokio::test]
async fn a_link_insert_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        fail_writes(&db, "oauth_accounts", "true").await;
        assert_callback_internal(&app).await;
        assert_eq!(session_rows(&db).await, 0);
    })
    .await;
}

/// A session insert that fails stops the callback after the link.
#[tokio::test]
async fn a_session_insert_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        fail_writes(&db, "auth_sessions", "true").await;
        assert_callback_internal(&app).await;
        assert_eq!(
            link_rows(&db).await,
            0,
            "the link rolled back with the session"
        );
    })
    .await;
}

/// The three first-sign-in writes of an unverified account each stop the
/// callback: the password clear, the session sweep, and the stamp.
#[tokio::test]
async fn a_first_sign_in_write_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let (app, _user) = google_app_with_learner(&db).await;
        fail_writes(&db, "users", "true").await;
        assert_callback_internal(&app).await;
    })
    .await;
    TestDb::with(|db| async move {
        let (app, user) = google_app_with_learner(&db).await;
        seed_live_session(&db, user, SESSION_TOKEN_ONE.1).await;
        fail_deletes(&db, "auth_sessions", "true").await;
        assert_callback_internal(&app).await;
    })
    .await;
    TestDb::with(|db| async move {
        let (app, _user) = google_app_with_learner(&db).await;
        fail_writes(&db, "users", "NEW.email_verified_at IS NOT NULL").await;
        assert_callback_internal(&app).await;
    })
    .await;
}
