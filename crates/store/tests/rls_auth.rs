//! Proof tests for the database-level guarantees of M0, part 5: the auth
//! tables under the tenant policy and the five SECURITY DEFINER lookups (C3).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_store::begin_tenant;
use cadus_store::test_support::TestDb;
use common::rls::seed_auth_rows;
use common::sqlstate;

/// C3, round-4 finding #1: the runtime role writes no auth row of another
/// account and reads no auth row of another account.
///
/// `auth_sessions`, `auth_tokens`, and `oauth_accounts` each carry `user_id` and
/// each kept table-wide SELECT, INSERT, UPDATE, and DELETE for `cadus_app` with
/// no policy. One INSERT into `auth_sessions` therefore minted a live cookie for
/// any account, and the web tier bound `app.user_id` to that account and opened
/// every other tenant table behind it. The three tables now carry the same
/// `tenant_isolation` policy as `events`.
#[tokio::test]
async fn app_role_cannot_forge_an_auth_row_for_another_account() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("forge-a@example.test").await;
        let user_b = db.seed_user("forge-b@example.test").await;

        // B holds one live session, one reset token, and one linked provider.
        seed_auth_rows(
            &db,
            user_b,
            "session-of-b",
            "token-of-b",
            "forge-b@example.test",
        )
        .await;

        // Bound to A, the forged session for B fails the WITH CHECK clause.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let session_err = sqlx::query!(
            "INSERT INTO auth_sessions
                 (token_hash, user_id, created_at, last_seen_at, expires_at)
             VALUES ('forged-cookie', $1, now(), now(), now() + interval '30 days')",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&session_err), "42501");
        let _ = tx.rollback().await;

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let token_err = sqlx::query!(
            "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at)
             VALUES ('forged-reset', $1, 'password_reset', now() + interval '1 day')",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&token_err), "42501");
        let _ = tx.rollback().await;

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let oauth_err = sqlx::query!(
            "INSERT INTO oauth_accounts
                 (provider, provider_account_id, user_id, email_at_link)
             VALUES ('google', 'attacker-provider-id', $1, 'forge-a@example.test')",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&oauth_err), "42501");
        let _ = tx.rollback().await;

        // Bound to A, none of B's auth rows is visible, and a DELETE of the whole
        // table reaches no row of B.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let sessions = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM auth_sessions"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(sessions, 0);
        let tokens = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM auth_tokens"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(tokens, 0);
        let links = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM oauth_accounts"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(links, 0);
        let wiped = sqlx::query!("DELETE FROM auth_sessions")
            .execute(&mut *tx)
            .await
            .unwrap()
            .rows_affected();
        assert_eq!(wiped, 0);
        tx.commit().await.unwrap();

        // B's session survived the DELETE that A ran.
        let survivors = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM auth_sessions"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(survivors, 1);

        // After the bind, B writes its own session, touches it, and consumes its
        // own token. The policy admits every write of the account itself.
        let mut tx = begin_tenant(&db.app, user_b).await.unwrap();
        let created = sqlx::query!(
            "INSERT INTO auth_sessions
                 (token_hash, user_id, created_at, last_seen_at, expires_at)
             VALUES ('own-session-of-b', $1, now(), now(), now() + interval '30 days')",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(created, 1);
        let touched = sqlx::query!(
            "UPDATE auth_sessions SET last_seen_at = now() WHERE token_hash = 'session-of-b'"
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(touched, 1);
        let consumed = sqlx::query!(
            "UPDATE auth_tokens SET consumed_at = now() WHERE token_hash = 'token-of-b'"
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(consumed, 1);
        tx.commit().await.unwrap();
    })
    .await;
}

/// C3, round-4 findings #1 and #2: every pre-tenant lookup function answers an
/// unbound caller and gives a bound caller zero rows.
///
/// A SECURITY DEFINER body runs with the rights of the owner, and the owner is a
/// superuser in the shipped stack, so an unguarded function is a hole straight
/// through every policy: a tenant bound to A called `auth_user_by_email` and read
/// B's `password_hash` and `is_admin`. Each body now carries
/// `nullif(current_setting('app.user_id', true), '') IS NULL`. The auth paths are
/// unbound by definition, and every read after the bind goes through a policy.
#[tokio::test]
async fn pre_tenant_lookups_answer_an_unbound_caller_only() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("lookup-a@example.test").await;
        let user_b = db.seed_user("lookup-b@example.test").await;

        sqlx::query!(
            "UPDATE users SET password_hash = 'VICTIM-HASH', is_admin = true WHERE id = $1",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();
        seed_auth_rows(
            &db,
            user_b,
            "cookie-of-b",
            "reset-of-b",
            "lookup-b@example.test",
        )
        .await;

        // Unbound: each function returns exactly B's one row.
        let session = sqlx::query!(
            r#"SELECT user_id AS "user_id!" FROM auth_session_by_token_hash($1)"#,
            "cookie-of-b"
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(session.len(), 1);
        assert_eq!(session[0].user_id, user_b);

        let token = sqlx::query!(
            r#"
            SELECT user_id AS "user_id!", purpose AS "purpose!", consumed_at AS "consumed_at?"
            FROM auth_token_by_hash($1)
            "#,
            "reset-of-b"
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(token.len(), 1);
        assert_eq!(token[0].user_id, user_b);
        assert_eq!(token[0].purpose, "password_reset");
        assert_eq!(token[0].consumed_at, None);

        let link = sqlx::query!(
            r#"SELECT user_id AS "user_id!" FROM oauth_account_lookup($1, $2)"#,
            "google",
            "provider-id-of-b"
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(link.len(), 1);
        assert_eq!(link[0].user_id, user_b);

        let by_email = sqlx::query!(
            r#"
            SELECT id AS "id!", is_admin AS "is_admin!"
            FROM auth_user_by_email($1::text::citext)
            "#,
            "lookup-b@example.test"
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(by_email.len(), 1);
        assert_eq!(by_email[0].id, user_b);
        assert!(by_email[0].is_admin);

        // Bound to A: every one of the five functions returns zero rows, with B's
        // own key as the argument.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let bound_session = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_session_by_token_hash($1)"#,
            "cookie-of-b"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(bound_session, 0);
        let bound_token = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_token_by_hash($1)"#,
            "reset-of-b"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(bound_token, 0);
        let bound_link = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM oauth_account_lookup($1, $2)"#,
            "google",
            "provider-id-of-b"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(bound_link, 0);
        let bound_email = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_user_by_email($1::text::citext)"#,
            "lookup-b@example.test"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(bound_email, 0);
        let bound_id = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_user_by_id($1)"#,
            user_b
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(bound_id, 0);

        // A bound caller reads its own account through the function too: zero
        // rows, because the guard tests the caller, not the argument.
        let bound_self = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_user_by_id($1)"#,
            user_a
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(bound_self, 0);
        tx.commit().await.unwrap();
    })
    .await;
}

/// C3, round-4 finding #4: a temp table named `users` does not reach the body of
/// a SECURITY DEFINER function.
///
/// Postgres searches the temporary schema BEFORE every schema that `search_path`
/// names whenever `pg_temp` is not written out, so `SET search_path = public`
/// resolved to the effective list `pg_temp, public`. A caller ran `CREATE TEMP
/// TABLE users` plus one INSERT, and `auth_user_by_email` then returned the
/// attacker's row with `is_admin = true` and an attacker-chosen `password_hash`.
/// `SET search_path = public, pg_temp` puts the temporary schema last.
#[tokio::test]
async fn security_definer_functions_ignore_a_temp_users_table() {
    TestDb::with(|db| async move {
        let user_b = db.seed_user("temp-b@example.test").await;
        sqlx::query!(
            "UPDATE users SET password_hash = 'REAL-HASH' WHERE id = $1",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // One fixed connection: a temp table belongs to one session.
        let app = db.pool_as("cadus_app", 1).await;
        sqlx::query(
            "CREATE TEMP TABLE users (
                 id                uuid,
                 email             citext,
                 password_hash     text,
                 email_verified_at timestamptz,
                 is_admin          boolean,
                 disabled_at       timestamptz,
                 created_at        timestamptz
             )",
        )
        .execute(&app)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO pg_temp.users VALUES
                 ('00000000-0000-0000-0000-0000deadbeef', 'ghost@example.test',
                  '$argon2-attacker', now(), true, NULL, now()),
                 ('00000000-0000-0000-0000-0000deadbeee', 'temp-b@example.test',
                  '$argon2-attacker', now(), true, NULL, now())",
        )
        .execute(&app)
        .await
        .unwrap();

        // The temp table holds both rows, so the fixture itself is sound.
        // `pg_temp` exists in this one session only, so the compile-time checked
        // macro cannot see it. This one query stays a plain query (R2).
        let planted: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_temp.users")
            .fetch_one(&app)
            .await
            .unwrap();
        assert_eq!(planted, 2);

        // The account that exists only in the temp table is invisible.
        let ghost = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_user_by_email($1::text::citext)"#,
            "ghost@example.test"
        )
        .fetch_one(&app)
        .await
        .unwrap();
        assert_eq!(ghost, 0);

        // The account that both tables hold comes back from `public.users`.
        let real = sqlx::query!(
            r#"
            SELECT id AS "id!", password_hash AS "password_hash?", is_admin AS "is_admin!"
            FROM auth_user_by_email($1::text::citext)
            "#,
            "temp-b@example.test"
        )
        .fetch_all(&app)
        .await
        .unwrap();
        assert_eq!(real.len(), 1);
        assert_eq!(real[0].id, user_b);
        assert_eq!(real[0].password_hash.as_deref(), Some("REAL-HASH"));
        assert!(!real[0].is_admin);

        // `auth_user_by_id` reads the real table too.
        let by_id = sqlx::query!(
            r#"SELECT password_hash AS "password_hash?" FROM auth_user_by_id($1)"#,
            user_b
        )
        .fetch_all(&app)
        .await
        .unwrap();
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0].password_hash.as_deref(), Some("REAL-HASH"));

        app.close().await;
    })
    .await;
}
