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

use cadus_store::auth::{
    self, NewSession, PURPOSE_RESET, PURPOSE_VERIFY, SignUp, TokenConsumed, TokenEffect,
};
use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, begin_tenant};
use sqlx::types::chrono::{DateTime, Utc};
use uuid::Uuid;

/// Return the SQLSTATE of a store error, or a message that names the miss.
fn sqlstate(err: &StoreError) -> String {
    let StoreError::Db(inner) = err else {
        return format!("not a database error: {err}");
    };
    match inner.as_database_error().and_then(|db| db.code()) {
        Some(code) => code.into_owned(),
        None => format!("not a database error: {inner}"),
    }
}

/// Return the Postgres message of a store error, or a message that names the
/// miss.
fn db_message(err: &StoreError) -> String {
    let StoreError::Db(inner) = err else {
        return format!("not a database error: {err}");
    };
    match inner.as_database_error() {
        Some(db) => db.message().to_string(),
        None => format!("not a database error: {inner}"),
    }
}

/// Add `secs` to an instant. The test cluster never reaches the range bound of
/// a timestamp, so an out-of-range sum is a defect of the test, not a case.
fn plus_secs(now: DateTime<Utc>, secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(now.timestamp() + secs, 0).expect("the instant is in range")
}

/// Build a session row for `token_hash` with a 30-day window.
fn new_session(token_hash: &str) -> NewSession<'_> {
    let now = Utc::now();
    NewSession {
        token_hash,
        created_at: now,
        last_seen_at: now,
        expires_at: plus_secs(now, 2_592_000),
        ip: Some("203.0.113.7"),
        user_agent: Some("cadus-test/1.0"),
    }
}

/// Count the session rows of one account, with the admin pool.
async fn session_count(db: &TestDb, user_id: Uuid) -> i64 {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM auth_sessions WHERE user_id = $1"#,
        user_id
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// Read one account row with the admin pool.
async fn read_account(db: &TestDb, user_id: Uuid) -> (Option<String>, Option<DateTime<Utc>>) {
    let row = sqlx::query!(
        r#"SELECT password_hash, email_verified_at FROM users WHERE id = $1"#,
        user_id
    )
    .fetch_one(&db.admin)
    .await
    .unwrap();
    (row.password_hash, row.email_verified_at)
}

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
        assert_eq!(session_count(&db, user_a).await, 1);
        assert_eq!(session_count(&db, user_b).await, 0);

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
        assert_eq!(session_count(&db, user_a).await, 0);
        assert_eq!(session_count(&db, user_b).await, 1);
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
        assert_eq!(session_count(&db, user_a).await, 1);
        assert_eq!(session_count(&db, user_b).await, 1);

        // Sign out everywhere carries no WHERE clause; the policy bounds it.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        assert_eq!(auth::delete_all_sessions(&mut *tx).await.unwrap(), 1);
        tx.commit().await.unwrap();
        assert_eq!(session_count(&db, user_a).await, 0);
        assert_eq!(session_count(&db, user_b).await, 1);
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

// ---------------------------------------------------------------------------
// The rate counters.
// ---------------------------------------------------------------------------

/// Specification section 3.2: the counter is keyed by scope, key, and window
/// start. The first call of a window writes 1, and each later call adds 1.
///
/// The email counter and the IP counter are separate rows, so one address does
/// not spend the budget of an address that shares its network.
#[tokio::test]
async fn the_rate_counter_upsert_counts_one_window_at_a_time() {
    TestDb::with(|db| async move {
        let window = DateTime::from_timestamp(1_756_000_000, 0).unwrap();
        let next = DateTime::from_timestamp(1_756_003_600, 0).unwrap();

        for expected in [1, 2, 3] {
            let count =
                auth::bump_rate_counter(&db.app, "signup_email", "learner@example.test", window)
                    .await
                    .unwrap();
            assert_eq!(count, expected);
        }

        // A different scope is a different row.
        assert_eq!(
            auth::bump_rate_counter(&db.app, "signup_ip", "learner@example.test", window)
                .await
                .unwrap(),
            1
        );
        // A different key is a different row.
        assert_eq!(
            auth::bump_rate_counter(&db.app, "signup_email", "other@example.test", window)
                .await
                .unwrap(),
            1
        );
        // A different window is a different row.
        assert_eq!(
            auth::bump_rate_counter(&db.app, "signup_email", "learner@example.test", next)
                .await
                .unwrap(),
            1
        );

        // The table carries no tenant column and no policy, so the upsert works
        // inside a bound transaction too.
        let user = db.seed_user("counter@example.test").await;
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        assert_eq!(
            auth::bump_rate_counter(&mut *tx, "signup_email", "learner@example.test", window)
                .await
                .unwrap(),
            4
        );
        tx.commit().await.unwrap();

        let rows = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM auth_rate_counters"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(rows, 4);
    })
    .await;
}

/// The window start is floored from the 1970 epoch, so every process of a
/// deployment derives the same bucket from the same instant.
#[test]
fn the_window_start_floors_from_the_epoch() {
    let hour = 3_600;
    // 1 756 000 000 = 2025-08-24T01:46:40Z. It is 2 800 s into the hour that
    // starts at 1 755 997 200 = 2025-08-24T01:00:00Z.
    let now = DateTime::from_timestamp(1_756_000_000, 0).unwrap();
    assert_eq!(
        auth::window_start(now, hour),
        DateTime::from_timestamp(1_755_997_200, 0).unwrap()
    );
    // The first instant of a window is its own start.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(1_755_997_200, 0).unwrap(), hour),
        DateTime::from_timestamp(1_755_997_200, 0).unwrap()
    );
    // The last instant of a window still belongs to it. 1 756 000 799 is
    // 2025-08-24T01:59:59Z.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(1_756_000_799, 0).unwrap(), hour),
        DateTime::from_timestamp(1_755_997_200, 0).unwrap()
    );
    // The next second opens the next window: 2025-08-24T02:00:00Z.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(1_756_000_800, 0).unwrap(), hour),
        DateTime::from_timestamp(1_756_000_800, 0).unwrap()
    );
    // The 5-minute window of the login rule: 2025-08-24T01:45:00Z.
    assert_eq!(
        auth::window_start(now, 300),
        DateTime::from_timestamp(1_755_999_900, 0).unwrap()
    );
    // An instant before the epoch floors down, not toward zero.
    assert_eq!(
        auth::window_start(DateTime::from_timestamp(-1, 0).unwrap(), hour),
        DateTime::from_timestamp(-3_600, 0).unwrap()
    );
    // A width of 0 gives the instant unchanged.
    assert_eq!(auth::window_start(now, 0), now);
}

// ---------------------------------------------------------------------------
// Migration 0008 and the bound profile read
// ---------------------------------------------------------------------------

/// The session lookup gives `created_at`, so the 90-day absolute window has a
/// value to test BEFORE the tenant bind.
///
/// Specification section 3.3, "Cookie check", step 1 puts both refusals in the
/// unbound step. Migration 0006 returned three columns and left the second one
/// with nothing to read; migration 0008 adds the fourth.
#[tokio::test]
async fn the_session_lookup_gives_the_created_at_of_the_row() {
    TestDb::with(|db| async move {
        let user = db.seed_user("created-at@example.test").await;
        // 2025-08-24T00:00:00Z, written as an epoch second.
        let minted = DateTime::from_timestamp(1_755_993_600, 0).unwrap();
        // 30 days later: 1 755 993 600 + 2 592 000.
        let ends = DateTime::from_timestamp(1_758_585_600, 0).unwrap();
        sqlx::query!(
            "INSERT INTO auth_sessions
                 (token_hash, user_id, created_at, last_seen_at, expires_at)
             VALUES ('created-at-hash', $1, $2, $3, $4)",
            user,
            minted,
            minted,
            ends
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let session = auth::session_by_token_hash(&db.app, "created-at-hash")
            .await
            .unwrap()
            .expect("the unbound session lookup must read the row");

        assert_eq!(session.user_id, user);
        assert_eq!(session.created_at.timestamp(), 1_755_993_600);
        assert_eq!(session.last_seen_at.timestamp(), 1_755_993_600);
        assert_eq!(session.expires_at.timestamp(), 1_758_585_600);
    })
    .await;
}

/// The bound profile read answers the CALLER's row and no other tenant's.
///
/// `docs/SCHEMA.md`: "after the bind a plain SELECT on `users` reads the
/// caller's own rows". The `users_read_self` policy is what holds it, so the
/// statement carries no `WHERE` clause of its own.
#[tokio::test]
async fn the_bound_profile_read_answers_the_callers_row_alone() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("profile-a@example.test").await;
        let user_b = db.seed_user("profile-b@example.test").await;
        sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user_a
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let mine = auth::account_profile(&mut *tx)
            .await
            .unwrap()
            .expect("the bound read must answer the caller's row");
        let _ = tx.rollback().await;

        assert_eq!(mine.id, user_a);
        assert_eq!(mine.email, "profile-a@example.test");
        assert!(mine.email_verified_at.is_some());
        assert_ne!(mine.id, user_b);

        // Bound to B the same statement answers B's row, never A's.
        let mut tx = begin_tenant(&db.app, user_b).await.unwrap();
        let yours = auth::account_profile(&mut *tx).await.unwrap().unwrap();
        let _ = tx.rollback().await;

        assert_eq!(yours.id, user_b);
        assert_eq!(yours.email, "profile-b@example.test");
        assert_eq!(yours.email_verified_at, None);

        // Unbound the same statement reads nothing at all.
        assert_eq!(auth::account_profile(&db.app).await.unwrap(), None);
    })
    .await;
}
