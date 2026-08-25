//! Proof tests for the two database-level guarantees of M0.
//!
//! - C2: `events` is append-only for the runtime role. The grant enforces it.
//! - C3: row-level security isolates the tenants, and the boot guard refuses a
//!   role that bypasses it.
//!
//! Every test creates its own database, proves one property with literal
//! expected values, and drops the database again.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, assert_rls_enforced, begin_tenant};

/// The 10 tables that carry a tenant policy (`docs/SCHEMA.md`, C3).
const RLS_TABLES: [&str; 10] = [
    "anki_cards_created",
    "anki_queue",
    "diag_states",
    "events",
    "learner_models",
    "profiles",
    "serving_pool",
    "session_plans",
    "user_settings",
    "web_states",
];

/// The 6 tables that carry a `user_id` and stay outside row-level security.
const EXEMPT_TABLES: [&str; 6] = [
    "auth_sessions",
    "auth_tokens",
    "diagnosis_jobs",
    "email_outbox",
    "model_call_log",
    "oauth_accounts",
];

/// Return the SQLSTATE of a database error, or a message that names the miss.
fn sqlstate(err: &sqlx::Error) -> String {
    match err.as_database_error().and_then(|db| db.code()) {
        Some(code) => code.into_owned(),
        None => format!("not a database error: {err}"),
    }
}

fn to_owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// C2: the app role appends to `events` and never edits or erases a row.
#[tokio::test]
async fn app_role_cannot_update_events() {
    let db = TestDb::create().await;
    let user = db.seed_user("append-only@example.test").await;

    sqlx::query!(
        "INSERT INTO events (user_id, seq, ts, type, payload)
         VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
        user
    )
    .execute(&db.admin)
    .await
    .unwrap();

    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    let update_err = sqlx::query!("UPDATE events SET type = 'x'")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&update_err), "42501");
    let _ = tx.rollback().await;

    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    let delete_err = sqlx::query!("DELETE FROM events")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert_eq!(sqlstate(&delete_err), "42501");
    let _ = tx.rollback().await;

    // Append-only, not read-only: the insert of a second event succeeds.
    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    sqlx::query!(
        "INSERT INTO events (user_id, seq, ts, type, payload)
         VALUES ($1, 2, now(), 'attempt', '{}'::jsonb)",
        user
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let mut tx = begin_tenant(&db.app, user).await.unwrap();
    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(count, 2);
    let _ = tx.rollback().await;

    db.drop().await;
}

/// C3: a tenant reads only its own rows, an unbound connection reads nothing,
/// and a write into another tenant fails.
#[tokio::test]
async fn rls_isolates_tenants() {
    let db = TestDb::create().await;
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

    db.drop().await;
}

/// C3 boot guard: a role that bypasses row-level security is rejected.
#[tokio::test]
async fn boot_guard() {
    let db = TestDb::create().await;

    let err = assert_rls_enforced(&db.admin).await.unwrap_err();
    let StoreError::RlsBypass { superuser, .. } = err else {
        panic!("the admin pool must be rejected with StoreError::RlsBypass");
    };
    assert!(superuser, "the test cluster admin is a superuser");

    let info = assert_rls_enforced(&db.app).await.unwrap();
    assert_eq!(info.name, "cadus_app");
    assert!(!info.superuser);
    assert!(!info.bypass_rls);

    db.drop().await;
}

/// C3: the set of protected tables is the literal list of `docs/SCHEMA.md`.
/// A new tenant table without its own policy fails this test.
#[tokio::test]
async fn rls_coverage_is_the_literal_list() {
    let db = TestDb::create().await;

    let rows = sqlx::query!(
        r#"
        SELECT c.relname::text        AS "table_name!",
               c.relrowsecurity       AS "rls_enabled!",
               c.relforcerowsecurity  AS "rls_forced!"
        FROM pg_class c
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_attribute a ON a.attrelid = c.oid
                           AND a.attname = 'user_id'
                           AND a.attnum > 0
                           AND NOT a.attisdropped
        WHERE n.nspname = 'public' AND c.relkind = 'r'
        ORDER BY c.relname
        "#
    )
    .fetch_all(&db.admin)
    .await
    .unwrap();

    let mut forced: Vec<String> = Vec::new();
    let mut not_forced: Vec<String> = Vec::new();
    for row in &rows {
        if row.rls_enabled && row.rls_forced {
            forced.push(row.table_name.clone());
        } else {
            not_forced.push(row.table_name.clone());
        }
    }
    assert_eq!(forced, to_owned(&RLS_TABLES));
    assert_eq!(not_forced, to_owned(&EXEMPT_TABLES));

    let policies = sqlx::query!(
        r#"
        SELECT c.relname::text AS "table_name!",
               p.polname::text AS "policy_name!"
        FROM pg_policy p
        JOIN pg_class c ON c.oid = p.polrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE n.nspname = 'public'
        ORDER BY c.relname, p.polname
        "#
    )
    .fetch_all(&db.admin)
    .await
    .unwrap();

    let found: Vec<(String, String)> = policies
        .iter()
        .map(|row| (row.table_name.clone(), row.policy_name.clone()))
        .collect();
    let expected: Vec<(String, String)> = RLS_TABLES
        .iter()
        .map(|table| ((*table).to_string(), "tenant_isolation".to_string()))
        .collect();
    assert_eq!(found, expected);

    db.drop().await;
}

/// D9: a second migration run applies nothing and leaves the six rows.
#[tokio::test]
async fn migrate_is_idempotent() {
    let db = TestDb::create().await;

    cadus_store::migrate(&db.admin).await.unwrap();

    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM _sqlx_migrations"#)
        .fetch_one(&db.admin)
        .await
        .unwrap();
    assert_eq!(count, 6);

    db.drop().await;
}
