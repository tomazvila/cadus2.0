//! Proof tests for the database-level guarantees of M0, part 4: tenant
//! isolation, the worker queues, the boot guard, the role attributes, the
//! idempotency index, and the reset of the tenant setting (C3, D9).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, assert_rls_enforced, begin_tenant};
use common::rls::APP_ROLE_LINE;
use common::sqlstate;

/// C3: a tenant reads only its own rows, an unbound connection reads nothing,
/// and a write into another tenant fails.
#[tokio::test]
async fn rls_isolates_tenants() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("tenant-a@example.test").await;
        let user_b = db.seed_user("tenant-b@example.test").await;

        for user in [user_a, user_b] {
            sqlx::query!(
                "INSERT INTO events (user_id, seq, ts, type, payload)
                 VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
                user
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let bound_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bound_count, 1);
        let _ = tx.rollback().await;

        // No tenant context: the policy resolves to NULL and the query returns
        // nothing. The failure mode is closed.
        let unbound_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(unbound_count, 0);

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let cross_tenant_err = sqlx::query!(
            "INSERT INTO learner_models
                 (user_id, model, through_seq, projector_version, config_hash)
             VALUES ($1, '{}'::jsonb, 0, 1, 'test')",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&cross_tenant_err), "42501");
        let _ = tx.rollback().await;
    })
    .await;
}

/// C3, finding #15: the policy also covers the two worker queues.
///
/// One tenant reads its own `diagnosis_jobs` and `email_outbox` rows and nothing
/// of the other tenant. The worker role keeps the cross-tenant view, because it
/// holds BYPASSRLS.
#[tokio::test]
async fn rls_covers_the_worker_queues() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("queue-a@example.test").await;
        let user_b = db.seed_user("queue-b@example.test").await;

        for user in [user_a, user_b] {
            sqlx::query!(
                "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload)
                 VALUES ($1, 'attempt-1', '{\"secret\": \"answer\"}'::jsonb)",
                user
            )
            .execute(&db.admin)
            .await
            .unwrap();

            sqlx::query!(
                "INSERT INTO email_outbox (user_id, to_addr, kind)
                 VALUES ($1, 'someone@example.test', 'verify')",
                user
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let jobs = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM diagnosis_jobs"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(jobs, 1);
        let mail = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM email_outbox"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(mail, 1);
        let _ = tx.rollback().await;

        // No tenant context: both queues read nothing.
        let unbound_jobs =
            sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM diagnosis_jobs"#)
                .fetch_one(&db.app)
                .await
                .unwrap();
        assert_eq!(unbound_jobs, 0);
        let unbound_mail = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM email_outbox"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(unbound_mail, 0);
    })
    .await;
}

/// C3 boot guard: a role that bypasses row-level security is rejected.
#[tokio::test]
async fn boot_guard() {
    TestDb::with(|db| async move {
        let err = assert_rls_enforced(&db.admin).await.unwrap_err();
        let StoreError::RlsBypass { superuser, .. } = err else {
            panic!("the admin pool must be rejected with StoreError::RlsBypass");
        };
        assert!(superuser, "the test cluster admin is a superuser");

        let info = assert_rls_enforced(&db.app).await.unwrap();
        assert_eq!(info.name, "cadus_app");
        assert!(!info.superuser);
        assert!(!info.bypass_rls);
    })
    .await;
}

/// C3, finding #6: the role attributes match `migrations/0001_roles.sql`.
///
/// The roles are cluster-scoped and each `CREATE ROLE` is guarded by a `pg_roles`
/// test, so a long-lived test cluster keeps the attributes it was built with. A
/// catalog assertion alone therefore proves nothing about the migration text.
/// This test reads both: the live attributes and the literal line of the file.
#[tokio::test]
async fn role_attributes_match_the_migration() {
    TestDb::with(|db| async move {
        let app = sqlx::query!(
            r#"
            SELECT rolsuper      AS "superuser!",
                   rolbypassrls  AS "bypass_rls!",
                   rolcanlogin   AS "can_login!",
                   rolcreatedb   AS "create_db!",
                   rolcreaterole AS "create_role!"
            FROM pg_roles WHERE rolname = 'cadus_app'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!app.superuser, "cadus_app must not be a superuser");
        assert!(
            !app.bypass_rls,
            "cadus_app must not bypass row-level security"
        );
        assert!(app.can_login, "cadus_app must log in");
        assert!(!app.create_db, "cadus_app must not create a database");
        assert!(!app.create_role, "cadus_app must not create a role");

        // cadus_admin holds BYPASSRLS for the cross-tenant sweep. Its LOGIN flag
        // stays out of this test: another suite toggles it.
        let admin = sqlx::query!(
            r#"
            SELECT rolsuper     AS "superuser!",
                   rolbypassrls AS "bypass_rls!"
            FROM pg_roles WHERE rolname = 'cadus_admin'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!admin.superuser, "cadus_admin must not be a superuser");
        assert!(
            admin.bypass_rls,
            "cadus_admin must bypass row-level security"
        );

        let owner = sqlx::query!(
            r#"
            SELECT rolsuper    AS "superuser!",
                   rolcanlogin AS "can_login!"
            FROM pg_roles WHERE rolname = 'cadus_owner'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!owner.superuser, "cadus_owner must not be a superuser");
        assert!(!owner.can_login, "cadus_owner must not log in");

        // The migration text itself. `CARGO_MANIFEST_DIR` is `crates/store`.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../migrations/0001_roles.sql");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read of {} failed: {e}", path.display()));
        assert!(
            text.contains(APP_ROLE_LINE),
            "migrations/0001_roles.sql must contain the line: {APP_ROLE_LINE}"
        );
    })
    .await;
}

/// C2, finding #41: `events_attempt_idem` is a UNIQUE index.
///
/// FR-14 idempotency lives in the database: a repeated `attempt_id` makes the
/// INSERT a no-op under `ON CONFLICT ... DO NOTHING`. A plain index makes the
/// targeted `ON CONFLICT` raise SQLSTATE `42P10` instead.
#[tokio::test]
async fn events_attempt_idem_is_unique() {
    TestDb::with(|db| async move {
        let user = db.seed_user("attempt-idem@example.test").await;

        let is_unique = sqlx::query_scalar!(
            r#"
            SELECT i.indisunique AS "is_unique!"
            FROM pg_index i
            JOIN pg_class c ON c.oid = i.indexrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relname = 'events_attempt_idem'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(is_unique, "events_attempt_idem must be a UNIQUE index");

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        for seq in [1_i64, 2_i64] {
            sqlx::query!(
                "INSERT INTO events (user_id, seq, ts, type, attempt_id, payload)
                 VALUES ($1, $2, now(), 'attempt', 'attempt-1', '{}'::jsonb)
                 ON CONFLICT (user_id, attempt_id) WHERE attempt_id IS NOT NULL DO NOTHING",
                user,
                seq
            )
            .execute(&mut *tx)
            .await
            .unwrap();
        }

        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let _ = tx.rollback().await;
    })
    .await;
}

/// D9: a second migration run applies nothing and leaves the twelve rows.
#[tokio::test]
async fn migrate_is_idempotent() {
    TestDb::with(|db| async move {
        cadus_store::migrate(&db.admin).await.unwrap();

        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM _sqlx_migrations"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(count, 21);
    })
    .await;
}

/// C3: a reset tenant context yields zero rows and raises no error.
///
/// The `nullif(..., '')` guard in the policy is load-bearing. `RESET` leaves the
/// empty string in the GUC, not NULL, and a raw `''::uuid` cast raises SQLSTATE
/// `22P02`. That error breaks a pooled connection that a unit of work released.
/// The guard maps `''` to NULL, so a cleared context reads nothing.
#[tokio::test]
async fn reset_guc_yields_zero_rows() {
    TestDb::with(|db| async move {
        let user = db.seed_user("reset-guc@example.test").await;

        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // One connection for the whole test. A session-level set_config outlives a
        // transaction, so the reset case needs the same connection throughout.
        let mut conn = db.app.acquire().await.unwrap();

        sqlx::query!(
            "SELECT set_config('app.user_id', $1, false)",
            user.to_string()
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let bound_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(bound_count, 1);

        sqlx::query("RESET app.user_id")
            .execute(&mut *conn)
            .await
            .unwrap();

        let reset_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(reset_count, 0);

        drop(conn);
    })
    .await;
}
