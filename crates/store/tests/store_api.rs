//! Tests of the public store API that the row-level-security suite does not
//! reach: the transaction-local tenant binding, the `BYPASSRLS` half of the C3
//! boot guard, the pool timeout, the redacting `Debug` impl, and the cleanup
//! contract of `TestDb::with`.
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

use std::time::{Duration, Instant};

use cadus_store::test_support::TestDb;
use cadus_store::{DbConfig, StoreError, assert_rls_enforced, begin_tenant};
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use uuid::Uuid;

/// C3: `begin_tenant` binds the tenant to the transaction only.
///
/// The pool holds one connection, so the transaction and the query after it run
/// on the same session. `COMMIT` keeps a session-level setting and discards a
/// transaction-local one, so the second count separates the two forms. With a
/// session-level binding the second count reads the row of the tenant that the
/// finished unit of work bound.
#[tokio::test]
async fn tenant_binding_is_transaction_local() {
    TestDb::with(|db| async move {
        let user = db.seed_user("tenant-local@example.test").await;
        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let pool = db.pool_as("cadus_app", 1).await;

        let mut tx = begin_tenant(&pool, user).await.unwrap();
        let bound_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bound_count, 1, "the bound transaction reads its own row");
        tx.commit().await.unwrap();

        let after_commit = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            after_commit, 0,
            "the tenant context outlived the transaction on the pooled connection"
        );

        pool.close().await;
    })
    .await;
}

/// C3 boot guard: a role with `BYPASSRLS` and without superuser is rejected.
///
/// `cadus_admin` has this exact shape in the deployment, so the guard must
/// reject it on the two flags separately. The role is cluster-scoped: the test
/// gives it a unique name and drops it before the assertions run.
#[tokio::test]
async fn boot_guard_rejects_bypassrls_non_superuser() {
    TestDb::with(|db| async move {
        let role = format!(
            "cadus2_t_bypass_{}",
            &Uuid::new_v4().simple().to_string()[..8]
        );
        sqlx::query(AssertSqlSafe(format!(
            "CREATE ROLE \"{role}\" LOGIN NOSUPERUSER BYPASSRLS"
        )))
        .execute(&db.admin)
        .await
        .unwrap();

        let pool = db.pool_as(&role, 1).await;
        let outcome = assert_rls_enforced(&pool).await;
        pool.close().await;

        let dropped = sqlx::query(AssertSqlSafe(format!("DROP ROLE IF EXISTS \"{role}\"")))
            .execute(&db.admin)
            .await;

        let err = outcome.expect_err("the guard must reject a BYPASSRLS role");
        let StoreError::RlsBypass {
            role: rejected,
            superuser,
            bypass_rls,
        } = err
        else {
            panic!("the guard must fail with StoreError::RlsBypass, not with {err}");
        };
        assert_eq!(rejected, role);
        assert_eq!((superuser, bypass_rls), (false, true));
        dropped.unwrap();
    })
    .await;
}

/// `TestDb::with` drops the database of a test body that panics.
///
/// The body runs in a task of its own, so this test reads the panic as a
/// `JoinError` and then asks the cluster for the database of that run.
#[tokio::test]
async fn with_drops_the_database_of_a_panicking_body() {
    let (name_tx, name_rx) = tokio::sync::oneshot::channel();

    let body = tokio::spawn(async move {
        TestDb::with(|db| async move {
            let _ = name_tx.send(db.name.clone());
            panic!("this test body fails on purpose");
        })
        .await
    });

    let name = name_rx.await.expect("the body must report its database");
    let outcome = body.await;
    assert!(
        outcome.is_err(),
        "with() must raise the panic of the body again"
    );

    let dsn = std::env::var("CADUS_TEST_DATABASE_URL").unwrap();
    let mut conn = PgConnection::connect(&dsn).await.unwrap();
    let left_behind = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM pg_database WHERE datname = $1"#,
        name
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();

    assert_eq!(left_behind, 0, "the panicking test left {name} behind");
}

/// R2: `connect` pins the acquire timeout, so the readiness probe of
/// `cadus-web` answers within seconds during a database outage.
#[tokio::test]
async fn connect_pins_the_pool_limits() {
    TestDb::with(|db| async move {
        let cfg = DbConfig {
            database_url: db.superuser_dsn(),
        };
        let pool = cadus_store::connect(&cfg).await.unwrap();
        assert_eq!(pool.options().get_acquire_timeout(), Duration::from_secs(5));
        assert_eq!(pool.options().get_max_connections(), 16);
        pool.close().await;
    })
    .await;
}

/// R2: `connect` reports a dead database within the pinned timeout.
///
/// Port 1 accepts no connection. The sqlx default of 30 s would hold the caller
/// for half a minute, so the bound of 15 s separates the pinned timeout from
/// the default.
#[tokio::test]
async fn connect_gives_up_within_the_pinned_timeout() {
    let cfg = DbConfig {
        database_url: "postgresql://cadus_app@127.0.0.1:1/cadus".to_string(),
    };

    let start = Instant::now();
    let err = cadus_store::connect(&cfg)
        .await
        .expect_err("port 1 is closed");
    let elapsed = start.elapsed();

    assert!(
        matches!(err, StoreError::Db(_)),
        "a closed port must give StoreError::Db, not {err}"
    );
    assert!(
        elapsed < Duration::from_secs(15),
        "connect took {elapsed:?}, so the acquire timeout is not pinned"
    );
}

/// R2, C3: the `Debug` impl of `DbConfig` keeps the password out of every log
/// line. The assertion pins the whole rendered string, so a `derive(Debug)`
/// fails here.
#[test]
fn db_config_debug_redacts_the_password() {
    let cfg = DbConfig {
        database_url: "postgresql://u:secret@h/d".to_string(),
    };

    let rendered = format!("{cfg:?}");

    assert_eq!(rendered, r#"DbConfig { database_url: "<redacted>" }"#);
    assert!(
        !rendered.contains("secret"),
        "the Debug output leaks the password: {rendered}"
    );
}
