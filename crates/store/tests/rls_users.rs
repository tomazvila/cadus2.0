//! Proof tests for the database-level guarantees of M0, part 2: the three
//! per-command policies of `users` (C3).

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
use common::sqlstate;
use uuid::Uuid;

/// C3, finding #4: the runtime role updates its own `users` row and nothing else.
///
/// `users` is keyed by `id`, so it stays outside the `tenant_isolation` set and
/// carries three per-command policies instead. Table-wide UPDATE let a session
/// bound to tenant A rewrite tenant B's `password_hash`. A column list keeps
/// `is_admin` out of reach of the runtime role in every case.
#[tokio::test]
async fn app_role_updates_only_its_own_user_row() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("self-a@example.test").await;
        let user_b = db.seed_user("self-b@example.test").await;

        // Both catalog flags are on, so a non-superuser owner stays inside the
        // policies too.
        let flags = sqlx::query!(
            r#"
            SELECT c.relrowsecurity      AS "rls_enabled!",
                   c.relforcerowsecurity AS "rls_forced!"
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relname = 'users'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(flags.rls_enabled, "relrowsecurity must be true on users");
        assert!(
            flags.rls_forced,
            "relforcerowsecurity must be true on users"
        );

        // Bound to A, an UPDATE of B's row matches no row.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let cross_tenant = sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(cross_tenant, 0);
        tx.commit().await.unwrap();

        // B is untouched.
        let b_unverified = sqlx::query_scalar!(
            r#"SELECT (email_verified_at IS NULL) AS "unverified!" FROM users WHERE id = $1"#,
            user_b
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(b_unverified, "tenant B keeps email_verified_at NULL");

        // Bound to A, an UPDATE of A's own row matches exactly one row.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let own_row = sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user_a
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(own_row, 1);
        tx.commit().await.unwrap();

        // #4: the column list stops the admin flag before any policy runs.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let admin_err = sqlx::query!("UPDATE users SET is_admin = true WHERE id = $1", user_a)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&admin_err), "42501");
        let _ = tx.rollback().await;
    })
    .await;
}

/// C3, findings #4 and #5: the runtime role inserts a `users` row and decides
/// neither `id` nor `is_admin`.
///
/// The round-2 fix narrowed UPDATE only. INSERT stayed table-wide over every
/// column, so one sign-up statement minted an account with `is_admin = true` and
/// a chosen primary key. A column list on INSERT closes that door.
#[tokio::test]
async fn app_role_inserts_no_id_and_no_admin_flag() {
    TestDb::with(|db| async move {
        // #5: an INSERT that names is_admin stops at the grant, before any policy.
        let admin_err = sqlx::query!(
            "INSERT INTO users (email, password_hash, is_admin)
             VALUES ($1::text::citext, $2, true)",
            "mint-admin@example.test",
            "MINT-HASH"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&admin_err), "42501");

        // #4: an INSERT that names id stops at the grant too.
        let chosen = Uuid::parse_str("00000000-0000-0000-0000-0000000000ff").unwrap();
        let id_err = sqlx::query!(
            "INSERT INTO users (id, email, password_hash) VALUES ($1, $2::text::citext, $3)",
            chosen,
            "chosen-id@example.test",
            "CHOSEN-HASH"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&id_err), "42501");

        // The sign-up shape succeeds. `users_insert` allows the row, and the
        // grant covers both named columns.
        let inserted = sqlx::query!(
            "INSERT INTO users (email, password_hash) VALUES ($1::text::citext, $2)",
            "signup@example.test",
            "SIGNUP-HASH"
        )
        .execute(&db.app)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(inserted, 1);

        // Both withheld columns come from their defaults.
        let row = sqlx::query!(
            r#"
            SELECT id AS "id!", is_admin AS "is_admin!"
            FROM users WHERE email = $1::text::citext
            "#,
            "signup@example.test"
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!row.is_admin, "a sign-up row carries is_admin = false");
        assert_ne!(row.id, chosen);

        // Neither denied row exists.
        let denied = sqlx::query_scalar!(
            r#"
            SELECT count(*) AS "count!"
            FROM users WHERE email IN ($1::text::citext, $2::text::citext)
            "#,
            "mint-admin@example.test",
            "chosen-id@example.test"
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(denied, 0);

        // The column ACL is the reason, and it is pinned here.
        let columns = sqlx::query!(
            r#"
            SELECT has_column_privilege('cadus_app', 'users', 'is_admin', 'INSERT')
                       AS "may_insert_is_admin!",
                   has_column_privilege('cadus_app', 'users', 'id', 'INSERT')
                       AS "may_insert_id!",
                   has_column_privilege('cadus_app', 'users', 'email', 'INSERT')
                       AS "may_insert_email!"
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(
            !columns.may_insert_is_admin,
            "cadus_app must not insert users.is_admin"
        );
        assert!(!columns.may_insert_id, "cadus_app must not insert users.id");
        assert!(
            columns.may_insert_email,
            "cadus_app must insert users.email"
        );
    })
    .await;
}

/// C3, findings #11 (round 3) and #2 (round 4): the runtime role reads its own
/// `users` row and no other, through a plain SELECT and through the login
/// functions alike.
///
/// `users_read` was `USING (true)` over a table-wide SELECT grant, so a bound
/// tenant read every account's `email`, `password_hash`, `is_admin`, and
/// `disabled_at`. The SELECT policy now carries the same `id` predicate as the
/// UPDATE policy.
///
/// Round-4 finding #2: the two login functions gave the same read back, because
/// a SECURITY DEFINER body runs with the rights of the superuser owner and
/// neither body looked at the caller. The old assertion here pinned that
/// behavior as intended. Both functions now answer an unbound caller only.
#[tokio::test]
async fn app_role_reads_only_its_own_user_row() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("read-a@example.test").await;
        let user_b = db.seed_user("read-b@example.test").await;
        sqlx::query!(
            "UPDATE users SET password_hash = $1 WHERE id = $2",
            "VICTIM-HASH",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // Bound to A, exactly one row of users is visible, and it is A's row.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let bound_rows = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bound_rows, 1);
        let bound_id = sqlx::query_scalar!(r#"SELECT id AS "id!" FROM users"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bound_id, user_a);
        tx.commit().await.unwrap();

        // Unbound, no row of users is visible at all.
        let unbound_rows = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(unbound_rows, 0);

        // A plain SELECT reaches no other account's password hash.
        let hashes = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM users WHERE password_hash = $1"#,
            "VICTIM-HASH"
        )
        .fetch_one(&db.app)
        .await
        .unwrap();
        assert_eq!(hashes, 0);

        // Round-4 finding #2: the login functions answer an UNBOUND caller only.
        // The old suite asserted the opposite here, so no test could fail on a
        // bound tenant that read another account's password_hash and is_admin.
        // `pre_tenant_lookups_answer_an_unbound_caller_only` proves the whole
        // shape, for all five functions.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let by_email_bound = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_user_by_email($1::text::citext)"#,
            "read-b@example.test"
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(by_email_bound, 0);
        let by_id_bound = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM auth_user_by_id($1)"#,
            user_b
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(by_id_bound, 0);
        tx.commit().await.unwrap();

        // Unbound, the login lookup reaches B and returns the columns that an
        // account-status decision needs.
        let by_id = sqlx::query!(
            r#"
            SELECT id AS "id!", password_hash AS "password_hash?", is_admin AS "is_admin!"
            FROM auth_user_by_id($1)
            "#,
            user_b
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0].id, user_b);
        assert_eq!(by_id[0].password_hash.as_deref(), Some("VICTIM-HASH"));
        assert!(!by_id[0].is_admin);
    })
    .await;
}
