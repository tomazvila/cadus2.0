//! Process tests for the `cadus-migrate` binary.
//!
//! The compose stack runs `cadus-migrate --admin-login` as its one-shot step.
//! The flag replaces the `psql` call of the earlier stack, so the runtime image
//! carries no database client. These tests spawn the real binary and read the
//! literal exit code and the catalog.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::process::Command;

use cadus_store::test_support::TestDb;

/// `--admin-login` exits 0 and gives `cadus_admin` a login.
#[tokio::test]
async fn admin_login_flag_grants_the_login() {
    let db = TestDb::create().await;

    // Roles are cluster-scoped, so an earlier run leaves the login behind. Put
    // the role back to the state of migration 0001 first. Without this step the
    // test passes on a binary that does nothing.
    sqlx::query("ALTER ROLE cadus_admin NOLOGIN")
        .execute(&db.admin)
        .await
        .unwrap();
    assert!(!admin_can_login(&db).await, "the precondition is NOLOGIN");

    let output = Command::new(env!("CARGO_BIN_EXE_cadus-migrate"))
        .arg("--admin-login")
        .env("DATABASE_URL", db.superuser_dsn())
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {stdout}stderr: {stderr}"
    );
    assert!(
        stdout.contains("cadus-migrate: cadus_admin LOGIN granted"),
        "stdout: {stdout}"
    );

    assert!(
        admin_can_login(&db).await,
        "cadus_admin must have a login after the flag"
    );

    db.drop().await;
}

/// Read `rolcanlogin` of the `cadus_admin` role.
async fn admin_can_login(db: &TestDb) -> bool {
    sqlx::query_scalar!(
        r#"SELECT rolcanlogin AS "rolcanlogin!" FROM pg_roles WHERE rolname = 'cadus_admin'"#
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// An unknown argument prints the usage and exits 2. The DSN is valid, so the
/// exit code comes from the argument check and from nothing else.
#[tokio::test]
async fn unknown_flag_exits_with_code_two() {
    let db = TestDb::create().await;

    let output = Command::new(env!("CARGO_BIN_EXE_cadus-migrate"))
        .arg("--bogus")
        .env("DATABASE_URL", db.superuser_dsn())
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(
        stderr.contains("usage: cadus-migrate [--admin-login]"),
        "stderr: {stderr}"
    );

    db.drop().await;
}
