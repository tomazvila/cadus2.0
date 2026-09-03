//! The error paths of the auth statements: a closed pool under every
//! executor, a refused statement or a failed commit inside each composed
//! order, and the sign-up whose lookup reads nothing back.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_store::auth::{
    NewSession, PURPOSE_RESET, SignUp, TokenEffect, account_profile, bump_rate_counter,
    clear_password_hash, consume_token, consume_token_tx, delete_all_sessions,
    delete_other_sessions, delete_session, delete_tokens_for_purpose, insert_oauth_account,
    insert_session, insert_token, insert_user, mark_email_verified, oauth_account_user,
    session_by_token_hash, set_password_hash, sign_up, start_session, token_by_hash,
    touch_last_seen, user_by_email, user_by_id,
};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use common::fault::{bound_session_pool, closed_pool, fail_commit_on, revoke};
use common::store_sqlstate;
use sqlx::types::chrono::Utc;
use uuid::Uuid;

/// A session row with the hash `token_hash` and a one-hour window.
fn session(token_hash: &str) -> NewSession<'_> {
    let now = Utc::now();
    NewSession {
        token_hash,
        created_at: now,
        last_seen_at: now,
        expires_at: now + std::time::Duration::from_secs(3600),
        ip: None,
        user_agent: None,
    }
}

/// Every executor-taking statement reports a closed pool.
#[tokio::test]
async fn every_statement_reports_a_closed_pool() {
    TestDb::with(|db| async move {
        let pool = closed_pool(&db).await;
        let user = Uuid::nil();
        let now = Utc::now();
        assert!(user_by_email(&pool, "a@example.test").await.is_err());
        assert!(user_by_id(&pool, user).await.is_err());
        assert!(session_by_token_hash(&pool, "h").await.is_err());
        assert!(token_by_hash(&pool, "h").await.is_err());
        assert!(oauth_account_user(&pool, "google", "1").await.is_err());
        assert!(insert_user(&pool, "a@example.test", None).await.is_err());
        assert!(insert_session(&pool, user, &session("h")).await.is_err());
        assert!(touch_last_seen(&pool, "h").await.is_err());
        assert!(delete_session(&pool, "h").await.is_err());
        assert!(delete_all_sessions(&pool).await.is_err());
        assert!(delete_other_sessions(&pool, "h").await.is_err());
        assert!(
            insert_token(&pool, user, "h", PURPOSE_RESET, now)
                .await
                .is_err()
        );
        assert!(
            delete_tokens_for_purpose(&pool, PURPOSE_RESET)
                .await
                .is_err()
        );
        assert!(consume_token(&pool, "h").await.is_err());
        assert!(set_password_hash(&pool, user, "phc").await.is_err());
        assert!(clear_password_hash(&pool, user).await.is_err());
        assert!(mark_email_verified(&pool, user).await.is_err());
        assert!(account_profile(&pool).await.is_err());
        assert!(
            insert_oauth_account(&pool, user, "google", "1", "a@example.test")
                .await
                .is_err()
        );
        assert!(
            bump_rate_counter(&pool, "login_ip", "::1", now)
                .await
                .is_err()
        );
        assert!(sign_up(&pool, "a@example.test", None).await.is_err());
        assert!(start_session(&pool, user, &session("h")).await.is_err());
        assert!(
            consume_token_tx(&pool, user, "h", TokenEffect::EmailVerify)
                .await
                .is_err()
        );
    })
    .await;
}

/// The bound account drops its password hash and keeps every other column.
#[tokio::test]
async fn the_bound_account_clears_its_password_hash() {
    TestDb::with(|db| async move {
        sign_up(&db.app, "alice@example.test", Some("$argon2$hash"))
            .await
            .unwrap();
        let user = user_by_email(&db.app, "alice@example.test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(user.password_hash.as_deref(), Some("$argon2$hash"));

        let mut tx = begin_tenant(&db.app, user.id).await.unwrap();
        assert_eq!(clear_password_hash(&mut *tx, user.id).await.unwrap(), 1);
        tx.commit().await.unwrap();
        let cleared = user_by_id(&db.app, user.id).await.unwrap().unwrap();
        assert_eq!(cleared.password_hash, None);
        assert_eq!(cleared.email_verified_at, None);
    })
    .await;
}

/// A sign-up whose INSERT wrote a row that the lookup did not read back is
/// the typed auth error. A session that carries a tenant binding is that
/// case: the INSERT policy accepts the row and the unbound lookup sees a
/// bound caller.
#[tokio::test]
async fn a_sign_up_that_reads_nothing_back_is_an_auth_error() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        let bound = bound_session_pool(&db, alice).await;
        let err = sign_up(&bound, "bob@example.test", None).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            "auth error: the sign-up INSERT wrote a row that auth_user_by_email did not read back"
        );
        assert!(matches!(err, StoreError::Auth(_)));
    })
    .await;
}

/// The login order reports a refused session insert and a failed commit.
#[tokio::test]
async fn the_login_order_reports_a_refused_insert_and_a_failed_commit() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        fail_commit_on(&db, "auth_sessions").await;
        let err = start_session(&db.app, alice, &session("h1"))
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "P0001");

        revoke(&db, "INSERT", "auth_sessions").await;
        let err = start_session(&db.app, alice, &session("h2"))
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501");
    })
    .await;
}

/// The token order reports a refused token update, a refused effect of either
/// kind, and a failed commit.
#[tokio::test]
async fn the_token_order_reports_every_refused_step() {
    TestDb::with(|db| async move {
        let alice = db.seed_user("alice@example.test").await;
        let now = Utc::now();
        let mut tx = begin_tenant(&db.app, alice).await.unwrap();
        for hash in ["t1", "t2", "t3", "t4"] {
            insert_token(
                &mut *tx,
                alice,
                hash,
                PURPOSE_RESET,
                now + std::time::Duration::from_secs(3600),
            )
            .await
            .unwrap();
        }
        tx.commit().await.unwrap();

        fail_commit_on(&db, "auth_tokens").await;
        let err = consume_token_tx(&db.app, alice, "t1", TokenEffect::EmailVerify)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "P0001");

        revoke(&db, "UPDATE (password_hash)", "users").await;
        let reset = TokenEffect::PasswordReset {
            password_hash: "phc",
        };
        let err = consume_token_tx(&db.app, alice, "t2", reset)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501");

        revoke(&db, "UPDATE (email_verified_at)", "users").await;
        let err = consume_token_tx(&db.app, alice, "t3", TokenEffect::EmailVerify)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501");

        revoke(&db, "UPDATE", "auth_tokens").await;
        let err = consume_token_tx(&db.app, alice, "t4", TokenEffect::EmailVerify)
            .await
            .unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501");
    })
    .await;
}

/// The sign-up of a taken address is `EmailTaken` and never an error.
#[tokio::test]
async fn a_taken_address_is_the_email_taken_answer() {
    TestDb::with(|db| async move {
        db.seed_user("alice@example.test").await;
        assert_eq!(
            sign_up(&db.app, "alice@example.test", None).await.unwrap(),
            SignUp::EmailTaken
        );
    })
    .await;
}
