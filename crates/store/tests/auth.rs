//! The M5 auth contract through `cadus_store::auth` (specification section
//! 3.3, `docs/SCHEMA.md` "The M5 auth contract").
//!
//! The four acceptance checks of specification section 11, row U3:
//!
//! 1. A bound caller gets zero rows from all five SECURITY DEFINER lookups.
//! 2. Sign-up with a `RETURNING` clause fails, and sign-up without one succeeds.
//! 3. A session INSERT that names another `user_id` is refused by `WITH CHECK`.
//! 4. A token UPDATE that returns zero rows rolls the transaction back.
//!
//! Every value this file asserts is a literal: a SQLSTATE, a Postgres message, a
//! row count, a counter value, or an instant. No assertion re-reads a constant
//! of the code under test (HANDOVER section 3).
//!
//! Every test that needs a database uses `TestDb::with`, so a failed assertion
//! drops its throwaway database instead of leaving it on the shared cluster.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_store::auth::{self, SignUp, TokenConsumed, TokenEffect};
use cadus_store::begin_tenant;
use cadus_store::test_support::TestDb;
use common::auth::{assert_session_counts, new_session, read_account, session_count};
use common::{db_message, store_sqlstate as sqlstate};

// ---------------------------------------------------------------------------
// Acceptance check 1: a bound caller gets zero rows from all five lookups.
// ---------------------------------------------------------------------------

/// C3: the guard inside every SECURITY DEFINER body tests the CALLER, not the
/// argument.
///
/// The unbound pool reads B's account, session, token, and provider link. The
/// same five calls inside a transaction bound to A return `None`, and so does
/// the call that a bound caller makes for its OWN account.
#[tokio::test]
async fn a_bound_caller_gets_zero_rows_from_all_five_lookups() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("bound-a@example.test").await;
        let user_b = db.seed_user("bound-b@example.test").await;
        sqlx::query!(
            "UPDATE users SET password_hash = 'HASH-OF-B', is_admin = true WHERE id = $1",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO auth_sessions
                 (token_hash, user_id, created_at, last_seen_at, expires_at)
             VALUES ('cookie-hash-of-b', $1, now(), now(), now() + interval '1 day')",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at)
             VALUES ('token-hash-of-b', $1, 'reset', now() + interval '1 day')",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO oauth_accounts
                 (provider, provider_account_id, user_id, email_at_link)
             VALUES ('google', 'google-id-of-b', $1, 'bound-b@example.test')",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // Unbound: every lookup answers with B's one row.
        let by_email = auth::user_by_email(&db.app, "bound-b@example.test")
            .await
            .unwrap()
            .expect("the unbound lookup by email must read the row");
        assert_eq!(by_email.id, user_b);
        assert_eq!(by_email.password_hash.as_deref(), Some("HASH-OF-B"));
        assert!(by_email.is_admin);
        assert_eq!(by_email.email_verified_at, None);
        assert_eq!(by_email.disabled_at, None);

        let by_id = auth::user_by_id(&db.app, user_b)
            .await
            .unwrap()
            .expect("the unbound lookup by id must read the row");
        assert_eq!(by_id.id, user_b);

        let session = auth::session_by_token_hash(&db.app, "cookie-hash-of-b")
            .await
            .unwrap()
            .expect("the unbound session lookup must read the row");
        assert_eq!(session.user_id, user_b);

        let token = auth::token_by_hash(&db.app, "token-hash-of-b")
            .await
            .unwrap()
            .expect("the unbound token lookup must read the row");
        assert_eq!(token.user_id, user_b);
        assert_eq!(token.purpose, "reset");
        assert_eq!(token.consumed_at, None);

        let link = auth::oauth_account_user(&db.app, "google", "google-id-of-b")
            .await
            .unwrap();
        assert_eq!(link, Some(user_b));

        // Bound to A: all five answer with zero rows, with B's own keys.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        assert_eq!(
            auth::user_by_email(&mut *tx, "bound-b@example.test")
                .await
                .unwrap(),
            None
        );
        assert_eq!(auth::user_by_id(&mut *tx, user_b).await.unwrap(), None);
        assert_eq!(
            auth::session_by_token_hash(&mut *tx, "cookie-hash-of-b")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            auth::token_by_hash(&mut *tx, "token-hash-of-b")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            auth::oauth_account_user(&mut *tx, "google", "google-id-of-b")
                .await
                .unwrap(),
            None
        );

        // The guard tests the caller, so a bound caller reads zero rows for its
        // OWN account too. A handler must not call a function after the bind.
        assert_eq!(auth::user_by_id(&mut *tx, user_a).await.unwrap(), None);
        assert_eq!(
            auth::user_by_email(&mut *tx, "bound-a@example.test")
                .await
                .unwrap(),
            None
        );
        tx.commit().await.unwrap();
    })
    .await;
}

// ---------------------------------------------------------------------------
// Acceptance check 2: sign-up with RETURNING fails, without it succeeds.
// ---------------------------------------------------------------------------

/// C3: the SELECT policy of `users` hides the new row from the unbound session
/// that wrote it, so a `RETURNING` clause fails with SQLSTATE 42501.
///
/// `auth::sign_up` therefore inserts without `RETURNING` and reads the new id
/// with `auth_user_by_email`.
#[tokio::test]
async fn sign_up_writes_without_returning_and_a_returning_clause_fails() {
    TestDb::with(|db| async move {
        // The same statement WITH a RETURNING clause, on the same unbound pool.
        let err = sqlx::query_scalar!(
            "INSERT INTO users (email, password_hash)
             VALUES ($1::text::citext, $2) RETURNING id",
            "returning@example.test",
            "PHC-STRING"
        )
        .fetch_one(&db.app)
        .await
        .unwrap_err();
        assert_eq!(
            err.as_database_error().and_then(|e| e.code()).as_deref(),
            Some("42501")
        );
        assert_eq!(
            err.as_database_error().map(|e| e.message().to_string()),
            Some("new row violates row-level security policy for table \"users\"".to_string())
        );
        // The refused statement wrote nothing.
        let after_refusal = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM users WHERE email = $1::text::citext"#,
            "returning@example.test"
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(after_refusal, 0);

        // Without the clause the sign-up succeeds and the lookup reads the id.
        let outcome = auth::sign_up(&db.app, "signup@example.test", Some("PHC-STRING"))
            .await
            .unwrap();
        let SignUp::Created(user) = outcome else {
            panic!("the first sign-up must be SignUp::Created, got {outcome:?}");
        };
        assert_eq!(user.password_hash.as_deref(), Some("PHC-STRING"));
        assert_eq!(user.email_verified_at, None);
        assert_eq!(user.disabled_at, None);
        assert!(!user.is_admin, "the column grant holds no is_admin");

        // The row the lookup read is the row the INSERT wrote.
        let stored = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM users WHERE email = $1::text::citext"#,
            "signup@example.test"
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(user.id, stored);

        // A second sign-up for the same address writes no row and opens no
        // account. The route answers the same generic body either way.
        let repeat = auth::sign_up(&db.app, "signup@example.test", Some("OTHER-PHC"))
            .await
            .unwrap();
        assert_eq!(repeat, SignUp::EmailTaken);
        let (password_hash, _) = read_account(&db, stored).await;
        assert_eq!(password_hash.as_deref(), Some("PHC-STRING"));
        let count = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM users WHERE email = $1::text::citext"#,
            "signup@example.test"
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(count, 1);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Acceptance check 3: the session INSERT is held to the bound account.
// ---------------------------------------------------------------------------

/// C3: the `WITH CHECK` clause of `tenant_isolation` refuses a session row for
/// any account but the bound one.
///
/// One forged INSERT would otherwise mint a live cookie for another account.
#[tokio::test]
async fn a_session_insert_for_another_account_is_refused_by_with_check() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("session-a@example.test").await;
        let user_b = db.seed_user("session-b@example.test").await;

        // Bound to A, the INSERT that names B fails.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let err = auth::insert_session(&mut *tx, user_b, &new_session("forged-cookie-hash"))
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&err), "42501");
        assert_eq!(
            db_message(&err),
            "new row violates row-level security policy for table \"auth_sessions\""
        );
        tx.rollback().await.unwrap();
        assert_eq!(session_count(&db, user_b).await, 0);

        // Bound to A, the INSERT that names A writes the row.
        auth::start_session(&db.app, user_a, &new_session("cookie-hash-of-a"))
            .await
            .unwrap();
        assert_session_counts(&db, user_a, 1, user_b, 0).await;

        // An unbound INSERT writes no session row either: every write of the
        // contract happens after the bind.
        let unbound = auth::insert_session(&db.app, user_a, &new_session("unbound-cookie-hash"))
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&unbound), "42501");
        assert_eq!(session_count(&db, user_a).await, 1);
    })
    .await;
}

// ---------------------------------------------------------------------------
// Acceptance check 4: a zero-row token UPDATE rolls the transaction back.
// ---------------------------------------------------------------------------

/// The single-use rule: `consumed_at IS NULL` in the `WHERE` clause makes the
/// token UPDATE the race winner, and a zero row count means another request
/// spent the token first.
///
/// The second call must leave the account exactly as the first call left it. A
/// commit there would apply a stale password to a live account.
#[tokio::test]
async fn a_zero_row_token_update_rolls_the_transaction_back() {
    TestDb::with(|db| async move {
        let user = db.seed_user("reset@example.test").await;
        sqlx::query!(
            "UPDATE users SET password_hash = 'OLD-HASH' WHERE id = $1",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at)
             VALUES ('reset-token-hash', $1, 'reset', now() + interval '30 minutes')",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // The first request spends the token and writes the new hash.
        let first = auth::consume_token_tx(
            &db.app,
            user,
            "reset-token-hash",
            TokenEffect::PasswordReset {
                password_hash: "NEW-HASH",
            },
        )
        .await
        .unwrap();
        assert_eq!(first, TokenConsumed::Consumed);
        let (password_hash, verified) = read_account(&db, user).await;
        assert_eq!(password_hash.as_deref(), Some("NEW-HASH"));
        assert_eq!(verified, None);

        // The second request reads zero rows and rolls back: the account keeps
        // the hash of the first request.
        let second = auth::consume_token_tx(
            &db.app,
            user,
            "reset-token-hash",
            TokenEffect::PasswordReset {
                password_hash: "STALE-HASH",
            },
        )
        .await
        .unwrap();
        assert_eq!(second, TokenConsumed::AlreadySpent);
        let (password_hash, verified) = read_account(&db, user).await;
        assert_eq!(password_hash.as_deref(), Some("NEW-HASH"));
        assert_eq!(verified, None);

        // The verify effect follows the same rule: the spent token stamps no
        // email_verified_at.
        let verify =
            auth::consume_token_tx(&db.app, user, "reset-token-hash", TokenEffect::EmailVerify)
                .await
                .unwrap();
        assert_eq!(verify, TokenConsumed::AlreadySpent);
        let (_, verified) = read_account(&db, user).await;
        assert_eq!(verified, None);

        // A token of the verify purpose stamps the account once.
        sqlx::query!(
            "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at)
             VALUES ('verify-token-hash', $1, 'verify', now() + interval '24 hours')",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();
        let stamped =
            auth::consume_token_tx(&db.app, user, "verify-token-hash", TokenEffect::EmailVerify)
                .await
                .unwrap();
        assert_eq!(stamped, TokenConsumed::Consumed);
        let (_, verified) = read_account(&db, user).await;
        assert!(verified.is_some(), "the verify effect must stamp the row");
    })
    .await;
}
