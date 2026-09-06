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

/// The account read behind an existing provider link fails: the callback is
/// `500`. The link names a user, so the resolve reads that row.
#[tokio::test]
async fn a_linked_account_read_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let user = db.seed_user("linked@example.com").await;
        sqlx::query(
            "INSERT INTO oauth_accounts (user_id, provider, provider_account_id, email_at_link) \
             VALUES ($1, 'google', 'google-subject-1', 'linked@example.com')",
        )
        .bind(user)
        .execute(&db.admin)
        .await
        .unwrap();
        let app = google_app(&db, Arc::new(google_verified()));
        drop_function(&db, "auth_user_by_id(uuid)").await;

        assert_callback_internal(&app).await;
    })
    .await;
}

/// The address lookup of a first federated sign-in fails: no link exists, so
/// the resolve reads by address, and the callback is `500`.
#[tokio::test]
async fn an_address_lookup_that_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        let app = google_app(&db, Arc::new(google_verified()));
        drop_function(&db, "auth_user_by_email(citext)").await;

        assert_callback_internal(&app).await;
    })
    .await;
}

/// The address of a first federated sign-in is taken between the read and the
/// insert, and the read-back then fails: the callback is `500`.
#[tokio::test]
async fn a_sign_up_race_whose_read_back_fails_is_500_on_the_callback() {
    TestDb::with(|db| async move {
        db.seed_user("learner@example.com").await;
        let app = google_app(&db, Arc::new(google_verified()));
        fail_user_by_email_after_a_miss(&db).await;
        assert_callback_internal(&app).await;
    })
    .await;
}
