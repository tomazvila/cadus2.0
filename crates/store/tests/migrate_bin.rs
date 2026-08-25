//! Process tests for the `cadus-migrate` binary.
//!
//! The compose stack runs `cadus-migrate --admin-login` as its one-shot step.
//! The flag replaces the `psql` call of the earlier stack, so the runtime image
//! carries no database client. These tests spawn the real binary and read the
//! literal exit code, the literal report line, and the catalog.
//!
//! Every test uses `TestDb::with`, so a failed assertion drops the throwaway
//! database instead of leaving it on the shared cluster.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::process::{Command, Output};

use cadus_store::test_support::TestDb;
use sqlx::AssertSqlSafe;
use uuid::Uuid;

/// The line that the operator reads in `docker compose logs migrate`.
const APPLIED_SIX: &str = "cadus-migrate: applied 6 migrations (6 total)";

/// Roles are cluster-scoped, and two `ALTER ROLE` statements on one role at the
/// same time give "tuple concurrently updated". The tests of this file run in
/// parallel, so every test that alters a role takes this lock first.
static ROLE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Run the binary against `dsn` with the given arguments.
///
/// The two password variables are removed, so a variable of the shell that
/// starts the test cannot change the output of the run.
fn run_migrate(dsn: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cadus-migrate"))
        .args(args)
        .env("DATABASE_URL", dsn)
        .env_remove("CADUS_APP_PASSWORD")
        .env_remove("CADUS_ADMIN_PASSWORD")
        .output()
        .unwrap()
}

/// D9: a fresh database gets the whole migration set, and the binary reports
/// the literal count that `docs/SELF_HOST.md` tells the operator to read.
#[tokio::test]
async fn fresh_database_reports_six_applied() {
    TestDb::with(|db| async move {
        // The database of `TestDb` is migrated already, so this test makes a
        // second, unmigrated one on the same cluster.
        let fresh = format!(
            "cadus2_t_fresh_{}",
            &Uuid::new_v4().simple().to_string()[..8]
        );
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{fresh}\"")))
            .execute(&db.admin)
            .await
            .unwrap();

        let output = run_migrate(&TestDb::superuser_dsn_for(&fresh), &[]);

        // Drop the extra database before the assertions, so a failed assertion
        // leaves nothing behind.
        let dropped = sqlx::query(AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS \"{fresh}\" WITH (FORCE)"
        )))
        .execute(&db.admin)
        .await;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(
            output.status.code(),
            Some(0),
            "stdout: {stdout}stderr: {stderr}"
        );
        assert!(stdout.contains(APPLIED_SIX), "stdout: {stdout}");
        dropped.unwrap();
    })
    .await;
}

/// `--admin-login` exits 0 and gives `cadus_admin` a login.
#[tokio::test]
async fn admin_login_flag_grants_the_login() {
    TestDb::with(|db| async move {
        let _role_lock = ROLE_LOCK.lock().await;

        // Roles are cluster-scoped, so an earlier run leaves the login behind.
        // Put the role back to the state of migration 0001 first. Without this
        // step the test passes on a binary that does nothing.
        sqlx::query("ALTER ROLE cadus_admin NOLOGIN")
            .execute(&db.admin)
            .await
            .unwrap();
        assert!(!admin_can_login(&db).await, "the precondition is NOLOGIN");

        let output = run_migrate(&db.superuser_dsn(), &["--admin-login"]);

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
        // The migrations of this database ran already, so this run applies none.
        assert!(
            stdout.contains("cadus-migrate: applied 0 migrations (6 total)"),
            "stdout: {stdout}"
        );
        // No password variable is set, so the binary reports no password.
        assert!(!stdout.contains("password set for"), "stdout: {stdout}");

        assert!(
            admin_can_login(&db).await,
            "cadus_admin must have a login after the flag"
        );
    })
    .await;
}

/// C3 deployment step: `--admin-login` with `CADUS_APP_PASSWORD` gives
/// `cadus_app` a password, so the cluster can refuse `trust` authentication.
///
/// The role is cluster-scoped. The test clears the password again, in every
/// case, before it asserts.
#[tokio::test]
async fn admin_login_sets_the_app_password() {
    TestDb::with(|db| async move {
        let _role_lock = ROLE_LOCK.lock().await;

        sqlx::query("ALTER ROLE cadus_app PASSWORD NULL")
            .execute(&db.admin)
            .await
            .unwrap();
        assert!(
            !app_has_password(&db).await,
            "the precondition is a role with no password"
        );

        let password = format!("pw-{}", &Uuid::new_v4().simple().to_string()[..8]);
        let output = Command::new(env!("CARGO_BIN_EXE_cadus-migrate"))
            .arg("--admin-login")
            .env("DATABASE_URL", db.superuser_dsn())
            .env("CADUS_APP_PASSWORD", &password)
            .env_remove("CADUS_ADMIN_PASSWORD")
            .output()
            .unwrap();

        let has_password = app_has_password(&db).await;

        // Leave the shared cluster as this test found it.
        sqlx::query("ALTER ROLE cadus_app PASSWORD NULL")
            .execute(&db.admin)
            .await
            .unwrap();

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(
            output.status.code(),
            Some(0),
            "stdout: {stdout}stderr: {stderr}"
        );
        assert!(
            stdout.contains("cadus-migrate: password set for cadus_app"),
            "stdout: {stdout}"
        );
        assert!(
            !stdout.contains("cadus-migrate: password set for cadus_admin"),
            "CADUS_ADMIN_PASSWORD is unset, so no admin line belongs here: {stdout}"
        );
        assert!(
            !stdout.contains(&password) && !stderr.contains(&password),
            "the binary must not print the password"
        );
        assert!(has_password, "cadus_app must hold a password after the run");
    })
    .await;
}

/// An empty password variable stops the run with the documented exit code 2.
#[tokio::test]
async fn empty_password_variable_exits_with_code_two() {
    TestDb::with(|db| async move {
        let _role_lock = ROLE_LOCK.lock().await;

        let output = Command::new(env!("CARGO_BIN_EXE_cadus-migrate"))
            .arg("--admin-login")
            .env("DATABASE_URL", db.superuser_dsn())
            .env("CADUS_APP_PASSWORD", "")
            .env_remove("CADUS_ADMIN_PASSWORD")
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
        assert!(
            stderr.contains("cadus-migrate: configuration error: CADUS_APP_PASSWORD is empty"),
            "stderr: {stderr}"
        );
    })
    .await;
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

/// Report whether `cadus_app` holds a password. Only a superuser reads
/// `pg_authid`, and the admin pool is the superuser of the test cluster.
async fn app_has_password(db: &TestDb) -> bool {
    sqlx::query_scalar!(
        r#"SELECT (rolpassword IS NOT NULL) AS "has_password!"
           FROM pg_authid WHERE rolname = 'cadus_app'"#
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// An unknown argument prints the usage and exits 2. The DSN is valid, so the
/// exit code comes from the argument check and from nothing else.
#[tokio::test]
async fn unknown_flag_exits_with_code_two() {
    TestDb::with(|db| async move {
        let output = run_migrate(&db.superuser_dsn(), &["--bogus"]);

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
        assert!(
            stderr.contains("usage: cadus-migrate [--admin-login]"),
            "stderr: {stderr}"
        );
    })
    .await;
}
