use std::env::VarError;
use std::sync::Arc;

use sqlx::postgres::PgConnectOptions;
use sqlx::{Connection, PgConnection};

use super::{TestDb, drop_database_at, dsn_for, dsn_from, or_stop, resume};

/// Open a throwaway database with `role` as its app role, and answer the
/// database name.
async fn open_as(role: &'static str) -> String {
    TestDb::with_app_role(role, |db| async move { db.name.clone() }).await
}

/// Open a pool as a fresh role of `attributes`, and answer the role name.
async fn pool_as_fresh_role(db: &Arc<TestDb>, attributes: &str) -> String {
    TestDb::with_role(
        db,
        "cadus2_t_role",
        attributes,
        |_, role, _| async move { role },
    )
    .await
}

/// Report whether the cluster holds the database `name`.
async fn database_exists(name: &str) -> bool {
    let mut conn = PgConnection::connect_with(&TestDb::maintenance_options())
        .await
        .unwrap();
    let found: bool =
        sqlx::query_scalar("SELECT exists(SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(name)
            .fetch_one(&mut conn)
            .await
            .unwrap();
    let _ = conn.close().await;
    found
}

/// Report whether the cluster holds the role `name`.
async fn role_exists(db: &TestDb, name: &str) -> bool {
    sqlx::query_scalar("SELECT exists(SELECT 1 FROM pg_roles WHERE rolname = $1)")
        .bind(name)
        .fetch_one(&db.admin)
        .await
        .unwrap()
}

/// The message of a panic payload that `or_stop` raised.
fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload.downcast_ref::<String>().unwrap().clone()
}

/// A setup that works runs the body. A setup that fails drops the
/// database and raises the panic of the setup again (finding #10).
#[tokio::test]
async fn a_failed_setup_drops_the_database_and_raises_its_panic() {
    let name = open_as("cadus_app").await;
    assert!(name.starts_with("cadus2_t_"), "{name}");
    assert!(!database_exists(&name).await);

    let failed = tokio::spawn(open_as("cadus2_t_no_such_role"))
        .await
        .unwrap_err();
    let message = panic_message(failed.into_panic());
    assert!(message.starts_with("app pool for cadus2_t_"), "{message}");
    let name = message.split(' ').nth(3).unwrap();
    assert!(!database_exists(name).await, "{message}");
}

/// A pool that opens gives the body its role. A pool that does not open
/// drops the role and raises the panic of the open again (finding #9).
#[tokio::test]
async fn a_pool_that_does_not_open_drops_the_role_and_raises_its_panic() {
    TestDb::with(|db| async move {
        let role = pool_as_fresh_role(&db, "LOGIN").await;
        assert!(role.starts_with("cadus2_t_role_"), "{role}");
        assert!(!role_exists(&db, &role).await);

        let failed = {
            let db = Arc::clone(&db);
            tokio::spawn(async move { pool_as_fresh_role(&db, "NOLOGIN").await })
        }
        .await
        .unwrap_err();
        let message = panic_message(failed.into_panic());
        assert!(message.starts_with("pool as cadus2_t_role_"), "{message}");
        let role = message.split(' ').nth(2).unwrap();
        assert!(!role_exists(&db, role).await, "{message}");
    })
    .await;
}

/// `or_stop` gives the value back, or panics with the two parts.
#[test]
fn or_stop_gives_the_value_or_the_two_part_message() {
    assert_eq!(or_stop("x", Ok::<u8, &str>(7)), 7);
    let outcome =
        std::panic::catch_unwind(|| or_stop("CREATE ROLE r failed", Err::<u8, _>("boom")));
    let payload = outcome.unwrap_err();
    assert_eq!(
        payload.downcast_ref::<String>().map(String::as_str),
        Some("CREATE ROLE r failed: boom")
    );
}

/// A task that panicked raises its own payload again; a task that was
/// cancelled panics with the given words.
#[tokio::test]
async fn resume_raises_the_panic_or_names_the_other_end() {
    let panicked = tokio::spawn(async { panic!("inside the body") })
        .await
        .unwrap_err();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        resume(panicked, "the test body did not finish")
    }));
    assert_eq!(
        outcome.unwrap_err().downcast_ref::<&str>().copied(),
        Some("inside the body")
    );

    let handle = tokio::spawn(std::future::pending::<()>());
    handle.abort();
    let cancelled = handle.await.unwrap_err();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        resume(cancelled, "the test body did not finish")
    }));
    let message = outcome.unwrap_err();
    let message = message.downcast_ref::<String>().unwrap();
    assert!(
        message.starts_with("the test body did not finish: "),
        "{message}"
    );
}

/// The DSN read refuses an absent and an empty variable with the line that
/// names the variable and the example, and passes a value through.
#[test]
fn the_dsn_read_refuses_an_absent_or_empty_variable() {
    assert_eq!(dsn_from(Ok("postgresql://x".to_string())), "postgresql://x");
    for read in [Err(VarError::NotPresent), Ok(String::new())] {
        let outcome = std::panic::catch_unwind(|| dsn_from(read));
        let message = outcome.unwrap_err();
        let message = message.downcast_ref::<String>().unwrap();
        assert_eq!(
            message,
            "CADUS_TEST_DATABASE_URL is not set: point it at a superuser DSN of a \
                 throwaway Postgres, for example postgresql://test:test@127.0.0.1:55434/postgres"
        );
    }
}

/// The last path segment of the DSN is the database, and a DSN with no
/// path segment stops the run.
#[test]
fn the_database_of_a_dsn_is_its_last_path_segment() {
    assert_eq!(
        dsn_for("postgresql://u:p@h:1/postgres", "cadus2_t_1"),
        "postgresql://u:p@h:1/cadus2_t_1"
    );
    let outcome = std::panic::catch_unwind(|| dsn_for("no-slash", "x"));
    let message = outcome.unwrap_err();
    assert_eq!(
        message.downcast_ref::<String>().map(String::as_str),
        Some("CADUS_TEST_DATABASE_URL carries no database path segment: no-slash")
    );
}

/// A best-effort drop against an endpoint that answers nothing reports the
/// connect error and drops nothing.
#[tokio::test]
async fn a_drop_of_a_database_on_a_dead_endpoint_is_an_error() {
    let options: PgConnectOptions = "postgresql://x@127.0.0.1:1/x"
        .parse()
        .expect("the DSN parses");
    assert!(drop_database_at(&options, "cadus2_t_none").await.is_err());
}
