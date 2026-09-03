//! The M5 auth contract, part 2: the guarded request, the two bulk sign
//! outs, the token creation, and the OAuth link, each inside the bound
//! account (specification section 3.3).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_store::auth::{self, PURPOSE_RESET, PURPOSE_VERIFY, SignUp};
use cadus_store::begin_tenant;
use cadus_store::test_support::TestDb;
use common::auth::{assert_session_counts, new_session, plus_secs};
use common::{db_message, store_sqlstate as sqlstate};
use sqlx::types::chrono::Utc;

// ---------------------------------------------------------------------------
// The rest of the call order.
// ---------------------------------------------------------------------------

/// The cookie check of every guarded request: two unbound lookups, the bind,
/// the hourly touch, and the sign-out.
///
/// The touch and the delete are bound writes, so a caller bound to another
/// account writes zero rows in place of an error.
#[tokio::test]
async fn the_cookie_check_touches_and_ends_the_bound_session_only() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("cookie-a@example.test").await;
        let user_b = db.seed_user("cookie-b@example.test").await;
        auth::start_session(&db.app, user_a, &new_session("cookie-hash-of-a"))
            .await
            .unwrap();
        auth::start_session(&db.app, user_b, &new_session("cookie-hash-of-b"))
            .await
            .unwrap();

        // Step 1 and step 2 are unbound.
        let session = auth::session_by_token_hash(&db.app, "cookie-hash-of-a")
            .await
            .unwrap()
            .expect("the session lookup must read the row");
        assert_eq!(session.user_id, user_a);
        let account = auth::user_by_id(&db.app, session.user_id)
            .await
            .unwrap()
            .expect("the account lookup must read the row");
        assert_eq!(account.disabled_at, None);

        // Step 4 is bound and writes one row.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        assert_eq!(
            auth::touch_last_seen(&mut *tx, "cookie-hash-of-a")
                .await
                .unwrap(),
            1
        );
        // The policy holds the touch to the bound tenant: B's cookie hash reads
        // zero rows, so the statement never slides another account's window.
        assert_eq!(
            auth::touch_last_seen(&mut *tx, "cookie-hash-of-b")
                .await
                .unwrap(),
            0
        );
        tx.commit().await.unwrap();

        // Step 5, sign-out: bound, one row, and B keeps its session.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        assert_eq!(
            auth::delete_session(&mut *tx, "cookie-hash-of-b")
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            auth::delete_session(&mut *tx, "cookie-hash-of-a")
                .await
                .unwrap(),
            1
        );
        tx.commit().await.unwrap();
        assert_session_counts(&db, user_a, 0, user_b, 1).await;
        assert_eq!(
            auth::session_by_token_hash(&db.app, "cookie-hash-of-a")
                .await
                .unwrap(),
            None
        );
    })
    .await;
}

/// Sign out everywhere drops every session of the bound account and nothing
/// else. A password change keeps the current session and drops the rest.
#[tokio::test]
async fn the_two_bulk_sign_outs_stay_inside_the_bound_account() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("bulk-a@example.test").await;
        let user_b = db.seed_user("bulk-b@example.test").await;
        for hash in ["a-one", "a-two", "a-three"] {
            auth::start_session(&db.app, user_a, &new_session(hash))
                .await
                .unwrap();
        }
        auth::start_session(&db.app, user_b, &new_session("b-one"))
            .await
            .unwrap();

        // A password change keeps the current cookie and ends the other two.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        assert_eq!(
            auth::delete_other_sessions(&mut *tx, "a-one")
                .await
                .unwrap(),
            2
        );
        tx.commit().await.unwrap();
        assert_session_counts(&db, user_a, 1, user_b, 1).await;

        // Sign out everywhere carries no WHERE clause; the policy bounds it.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        assert_eq!(auth::delete_all_sessions(&mut *tx).await.unwrap(), 1);
        tx.commit().await.unwrap();
        assert_session_counts(&db, user_a, 0, user_b, 1).await;
    })
    .await;
}

/// Token creation: the unbound lookup, the bind, the supersede, and the bound
/// INSERT. An unbound INSERT of the same row is refused.
#[tokio::test]
async fn token_creation_writes_after_the_bind_and_supersedes_the_older_token() {
    TestDb::with(|db| async move {
        let user = db.seed_user("mint@example.test").await;
        let expires = plus_secs(Utc::now(), 1_800);

        // Unbound, the INSERT fails the WITH CHECK clause of auth_tokens.
        let err = auth::insert_token(&db.app, user, "unbound-hash", PURPOSE_RESET, expires)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&err), "42501");

        // Bound, two reset tokens and one verify token.
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        auth::insert_token(&mut *tx, user, "first-reset-hash", PURPOSE_RESET, expires)
            .await
            .unwrap();
        auth::insert_token(&mut *tx, user, "verify-hash", PURPOSE_VERIFY, expires)
            .await
            .unwrap();
        // A fresh reset request supersedes the older reset link and keeps the
        // verify link.
        assert_eq!(
            auth::delete_tokens_for_purpose(&mut *tx, PURPOSE_RESET)
                .await
                .unwrap(),
            1
        );
        auth::insert_token(&mut *tx, user, "second-reset-hash", PURPOSE_RESET, expires)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            auth::token_by_hash(&db.app, "first-reset-hash")
                .await
                .unwrap(),
            None
        );
        let live = auth::token_by_hash(&db.app, "second-reset-hash")
            .await
            .unwrap()
            .expect("the new reset token must be live");
        assert_eq!(live.purpose, "reset");
        assert_eq!(live.consumed_at, None);
        let verify = auth::token_by_hash(&db.app, "verify-hash")
            .await
            .unwrap()
            .expect("the verify token must survive a reset supersede");
        assert_eq!(verify.purpose, "verify");
    })
    .await;
}

/// The OAuth callback order: the unbound link lookup, the unbound lookup by
/// email, the bind, and the bound link INSERT.
///
/// A sign-up from a provider carries a NULL `password_hash`.
#[tokio::test]
async fn the_oauth_callback_links_only_the_bound_account() {
    TestDb::with(|db| async move {
        let other = db.seed_user("oauth-other@example.test").await;

        // Step 1: no link yet.
        assert_eq!(
            auth::oauth_account_user(&db.app, "github", "github-id-42")
                .await
                .unwrap(),
            None
        );
        // Step 2: no account for the address, so the sign-up steps run with a
        // NULL password hash.
        assert_eq!(
            auth::user_by_email(&db.app, "oauth-new@example.test")
                .await
                .unwrap(),
            None
        );
        let outcome = auth::sign_up(&db.app, "oauth-new@example.test", None)
            .await
            .unwrap();
        let SignUp::Created(user) = outcome else {
            panic!("the OAuth sign-up must be SignUp::Created, got {outcome:?}");
        };
        assert_eq!(user.password_hash, None);

        // Bound to the new account, a link for another account is refused.
        let mut tx = begin_tenant(&db.app, user.id).await.unwrap();
        let err = auth::insert_oauth_account(
            &mut *tx,
            other,
            "github",
            "github-id-42",
            "oauth-other@example.test",
        )
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&err), "42501");
        assert_eq!(
            db_message(&err),
            "new row violates row-level security policy for table \"oauth_accounts\""
        );
        tx.rollback().await.unwrap();

        // Bound to the new account, its own link is written.
        let mut tx = begin_tenant(&db.app, user.id).await.unwrap();
        auth::insert_oauth_account(
            &mut *tx,
            user.id,
            "github",
            "github-id-42",
            "oauth-new@example.test",
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();

        assert_eq!(
            auth::oauth_account_user(&db.app, "github", "github-id-42")
                .await
                .unwrap(),
            Some(user.id)
        );
    })
    .await;
}
