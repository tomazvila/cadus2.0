//! Tests of the public store API that the row-level-security suite does not
//! reach: the transaction-local tenant binding, both halves of the C3 boot
//! guard, the acquire timeout, the statement timeout, the redacting `Debug`
//! impl, and the cleanup contract of `TestDb::with` and `TestDb::with_role`.
//!
//! Every test that needs a database uses `TestDb::with`, so a failed assertion
//! drops its throwaway database instead of leaving it on the shared cluster.
//! Every test that needs a cluster role uses `TestDb::with_role`, so a failed
//! assertion drops that role too (finding #9).

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
use sqlx::{Connection, PgConnection};

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
/// reject it on the two flags separately. The role is cluster-scoped, so
/// `TestDb::with_role` creates it and drops it in every case (finding #9).
#[tokio::test]
async fn boot_guard_rejects_bypassrls_non_superuser() {
    TestDb::with(|db| async move {
        let (role, outcome) = TestDb::with_role(
            &db,
            "cadus2_t_bypass",
            "LOGIN NOSUPERUSER BYPASSRLS",
            |_db, role, pool| async move {
                let outcome = assert_rls_enforced(&pool).await;
                (role, outcome)
            },
        )
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
    })
    .await;
}

/// C3 boot guard: a superuser role without `BYPASSRLS` is rejected.
///
/// `CREATE ROLE ... SUPERUSER` leaves `rolbypassrls` false, and a superuser
/// still reads every tenant. The `info.superuser` half of the guard is the only
/// term that rejects this role, so this test pins that term (round-2 finding
/// #9). The role is cluster-scoped, so `TestDb::with_role` creates it and drops
/// it in every case (round-3 finding #9).
#[tokio::test]
async fn boot_guard_rejects_superuser_without_bypassrls() {
    TestDb::with(|db| async move {
        let ((role, flags), outcome) = TestDb::with_role(
            &db,
            "cadus2_t_super",
            "LOGIN SUPERUSER",
            |db, role, pool| async move {
                // Read the catalog first. The test is only about the superuser
                // term if the role really carries SUPERUSER without BYPASSRLS.
                let flags = sqlx::query!(
                    r#"SELECT rolsuper AS "superuser!", rolbypassrls AS "bypass_rls!"
                       FROM pg_roles WHERE rolname = $1"#,
                    role
                )
                .fetch_one(&db.admin)
                .await
                .unwrap();

                let outcome = assert_rls_enforced(&pool).await;
                ((role, (flags.superuser, flags.bypass_rls)), outcome)
            },
        )
        .await;

        assert_eq!(
            flags,
            (true, false),
            "CREATE ROLE ... SUPERUSER must leave rolbypassrls false"
        );
        let err = outcome.expect_err("the guard must reject a superuser role");
        let StoreError::RlsBypass {
            role: rejected,
            superuser,
            bypass_rls,
        } = err
        else {
            panic!("the guard must fail with StoreError::RlsBypass, not with {err}");
        };
        assert_eq!(rejected, role);
        assert_eq!((superuser, bypass_rls), (true, false));
    })
    .await;
}

/// Finding #9: `TestDb::with_role` drops the role of a test body that panics.
///
/// The body runs in a task of its own, so this test reads the panic as a
/// `JoinError` and then asks the cluster for the role of that run. The role
/// carries `LOGIN SUPERUSER`, the exact shape that the boot-guard test needs
/// and that a leak turns into a password-free superuser login on a `trust`
/// cluster.
#[tokio::test]
async fn with_role_drops_the_role_of_a_panicking_body() {
    let (name_tx, name_rx) = tokio::sync::oneshot::channel();

    let body = tokio::spawn(async move {
        TestDb::with(|db| async move {
            TestDb::with_role(
                &db,
                "cadus2_t_leak",
                "LOGIN SUPERUSER",
                |_db, role, _pool| async move {
                    let _ = name_tx.send(role);
                    panic!("this test body fails on purpose");
                },
            )
            .await
        })
        .await
    });

    let role = name_rx.await.expect("the body must report its role");
    let outcome = body.await;
    assert!(
        outcome.is_err(),
        "with_role() must raise the panic of the body again"
    );

    let dsn = std::env::var("CADUS_TEST_DATABASE_URL").unwrap();
    let mut conn = PgConnection::connect(&dsn).await.unwrap();
    let left_behind = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM pg_roles WHERE rolname = $1"#,
        role
    )
    .fetch_one(&mut conn)
    .await
    .unwrap();
    conn.close().await.unwrap();

    assert_eq!(left_behind, 0, "the panicking body left the role {role}");
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

/// `TestDb::with` drops the database when the setup fails after the
/// `CREATE DATABASE` statement.
///
/// `cadus2_no_such_role` does not exist, so the app pool of the setup fails
/// after the database exists. The panic message names the database, so this
/// test reads the name from it and then asks the cluster for that database
/// (finding #10).
#[tokio::test]
async fn with_drops_the_database_of_a_failed_setup() {
    let outcome = tokio::spawn(async move {
        TestDb::with_app_role("cadus2_no_such_role", |_db| async move {}).await
    })
    .await;

    let err = outcome.expect_err("a role that does not exist must make the setup panic");
    let payload = err.into_panic();
    let message = payload
        .downcast_ref::<String>()
        .expect("the setup panic carries a String")
        .clone();

    // The literal message is `app pool for <database> failed: <error>`.
    let tail = message
        .strip_prefix("app pool for ")
        .unwrap_or_else(|| panic!("unexpected panic message: {message}"));
    let (name, reason) = tail
        .split_once(" failed: ")
        .unwrap_or_else(|| panic!("unexpected panic message: {message}"));
    assert!(
        reason.contains("cadus2_no_such_role"),
        "the panic must name the role that failed: {message}"
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

    assert_eq!(left_behind, 0, "the failed setup left {name} behind");
}

/// R2: `connect` pins the acquire timeout, so the readiness probe of
/// `cadus-web` answers within seconds during a database outage.
#[tokio::test]
async fn connect_pins_the_pool_limits() {
    TestDb::with(|db| async move {
        let cfg = DbConfig::new(db.superuser_dsn());
        assert_eq!(cfg.statement_timeout_ms, 5000);
        let pool = cadus_store::connect(&cfg).await.unwrap();
        assert_eq!(pool.options().get_acquire_timeout(), Duration::from_secs(5));
        assert_eq!(pool.options().get_max_connections(), 16);

        // The default reaches the session, so a query of the pool carries the
        // bound and needs no per-statement setting.
        let timeout = sqlx::query_scalar::<_, String>("SHOW statement_timeout")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(timeout, "5s");
        pool.close().await;
    })
    .await;
}

/// R4, finding #15: a query that runs longer than `statement_timeout` ends with
/// SQLSTATE 57014, so the readiness probe of `cadus-web` and the tick of
/// `cadus-worker` are bounded after the checkout too.
///
/// `pg_sleep(5)` sleeps 25 times longer than the 200 ms bound of this pool. The
/// elapsed bound of 3 s separates the cancelled statement from a statement that
/// ran to the end.
#[tokio::test]
async fn statement_timeout_cancels_a_query_that_runs_too_long() {
    TestDb::with(|db| async move {
        let cfg = DbConfig {
            database_url: db.superuser_dsn(),
            statement_timeout_ms: 200,
        };
        let pool = cadus_store::connect(&cfg).await.unwrap();

        let start = Instant::now();
        let outcome = sqlx::query("SELECT pg_sleep(5)").execute(&pool).await;
        let elapsed = start.elapsed();
        pool.close().await;

        let err = outcome.expect_err("statement_timeout must cancel pg_sleep(5)");
        let sqlx::Error::Database(db_err) = &err else {
            panic!("expected a database error, got {err}");
        };
        assert_eq!(
            db_err.code().as_deref(),
            Some("57014"),
            "expected query_canceled, got {err}"
        );
        assert!(
            elapsed < Duration::from_secs(3),
            "the query ran {elapsed:?}, so statement_timeout did not apply"
        );
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
    let cfg = DbConfig::new("postgresql://cadus_app@127.0.0.1:1/cadus");

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
    let cfg = DbConfig::new("postgresql://u:secret@h/d");

    let rendered = format!("{cfg:?}");

    assert_eq!(
        rendered,
        r#"DbConfig { database_url: "<redacted>", statement_timeout_ms: 5000 }"#
    );
    assert!(
        !rendered.contains("secret"),
        "the Debug output leaks the password: {rendered}"
    );
}
