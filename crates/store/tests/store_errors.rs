//! The error paths of the store root: a connection string that does not
//! parse, a cluster that does not answer, a closed pool, a bounded query that
//! fails inside its bound, and a tenant bind whose `set_config` is refused.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use cadus_store::{
    Db, DbConfig, StoreError, assert_rls_enforced, begin_tenant, bounded, connect, current_role,
    migrate,
};
use common::fault::{closed_pool, dead_pool, revoke_set_config};
use common::{db_message, store_sqlstate};

/// A connection string that does not parse and a port that nobody listens on
/// are both `StoreError::Db`, from `connect` and from `Db::connect` alike.
#[tokio::test]
async fn a_bad_connection_string_and_a_closed_port_are_database_errors() {
    let unparsed = connect(&DbConfig::new("not a url")).await.unwrap_err();
    assert!(matches!(unparsed, StoreError::Db(_)), "{unparsed}");
    let refused = Db::connect(&DbConfig::new("postgresql://x@127.0.0.1:1/x"))
        .await
        .err()
        .map(|err| err.to_string());
    assert!(
        refused
            .as_deref()
            .is_some_and(|m| m.starts_with("database error: ")),
        "{refused:?}"
    );
}

/// `Db::connect` keeps the bound of its configuration.
#[tokio::test]
async fn db_connect_keeps_the_bound_of_the_configuration() {
    TestDb::with(|db| async move {
        let mut cfg = DbConfig::new(db.superuser_dsn());
        cfg.client_timeout_ms = 300;
        let handle = Db::connect(&cfg).await.unwrap();
        assert_eq!(handle.client_timeout_ms(), 300);
        assert_eq!(
            handle.client_timeout(),
            Some(std::time::Duration::from_millis(300))
        );
        handle.pool().close().await;
    })
    .await;
}

/// A query that fails inside its bound reports the database error, with and
/// without a bound.
#[tokio::test]
async fn a_query_that_fails_inside_the_bound_reports_the_database_error() {
    TestDb::with(|db| async move {
        for bound in [0, 5000] {
            let handle = Db::new(db.admin.clone(), bound);
            let err = bounded(&handle, sqlx::query("SELECT 1 / 0").execute(handle.pool()))
                .await
                .unwrap_err();
            assert_eq!(store_sqlstate(&err), "22012", "bound {bound}: {err}");
        }
    })
    .await;
}

/// Every root statement reports a closed pool: the migration, the role read,
/// the boot guard, and the tenant bind.
#[tokio::test]
async fn every_root_statement_reports_a_closed_pool() {
    TestDb::with(|db| async move {
        let pool = closed_pool(&db).await;
        let user = db.seed_user("closed@example.test").await;
        let closed = "attempted to acquire a connection on a closed pool";
        assert_eq!(
            migrate(&pool).await.unwrap_err().to_string(),
            format!("migration error: while executing migrations: {closed}")
        );
        assert_eq!(db_message(&current_role(&pool).await.unwrap_err()), closed);
        assert_eq!(
            db_message(&assert_rls_enforced(&pool).await.unwrap_err()),
            closed
        );
        assert_eq!(
            db_message(&begin_tenant(&pool, user).await.unwrap_err()),
            closed
        );
    })
    .await;
}

/// A backend that ends under the pool fails the `BEGIN` of the tenant bind,
/// and a refused `set_config` fails the bind after the `BEGIN`.
#[tokio::test]
async fn the_tenant_bind_reports_a_dead_backend_and_a_refused_set_config() {
    TestDb::with(|db| async move {
        let user = db.seed_user("bind@example.test").await;
        let dead = dead_pool(&db).await;
        let err = begin_tenant(&dead, user).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "57P01", "{err}");

        revoke_set_config(&db).await;
        let err = begin_tenant(&db.app, user).await.unwrap_err();
        assert_eq!(store_sqlstate(&err), "42501", "{err}");
        assert_eq!(
            db_message(&err),
            "permission denied for function set_config"
        );
    })
    .await;
}
