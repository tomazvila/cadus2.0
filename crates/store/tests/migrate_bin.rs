//! Process tests for the `cadus-migrate` binary.
//!
//! The compose stack runs `cadus-migrate --admin-login` as its one-shot step.
//! The flag replaces the `psql` call of the earlier stack, so the runtime image
//! carries no database client. These tests spawn the real binary and read the
//! literal exit code, the literal report line, and the catalog.
//!
//! Every test uses `TestDb::with`, so a failed assertion drops the throwaway
//! database instead of leaving it on the shared cluster.
//!
//! Roles are cluster-scoped. Every test that alters a role takes `RoleLock`
//! first, a PostgreSQL advisory lock on the maintenance database, so two gate
//! runs on one cluster serialize their `ALTER ROLE` statements.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::process::{Command, Output};

use cadus_store::test_support::TestDb;
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use uuid::Uuid;

/// The line that the operator reads in `docker compose logs migrate`.
const APPLIED_SIX: &str = "cadus-migrate: applied 6 migrations (6 total)";

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// The key of the advisory lock that guards the `ALTER ROLE` statements.
///
/// Roles are cluster-scoped, and two `ALTER ROLE` statements on one role at the
/// same time give "tuple concurrently updated". A mutex of this process
/// serializes the threads of this test binary only, and `docs/plans/M0.md` runs
/// several worktrees against one cluster, so the lock must live in the cluster
/// (finding #11). `pg_advisory_lock` gives such a lock.
///
/// 7241001 is an arbitrary but fixed number. It has one rule: every caller that
/// alters a cluster role in this repository takes this one key. Nothing else in
/// the repository takes an advisory lock, so the key collides with nothing.
const ROLE_LOCK_KEY: i64 = 7_241_001;

/// A cluster-wide lock on the `ALTER ROLE` statements of this file and of the
/// `cadus-migrate` run that each test starts.
///
/// PostgreSQL scopes an advisory lock to the database of the session, so the
/// lock connection opens the maintenance database that `CADUS_TEST_DATABASE_URL`
/// names and not the throwaway database of the test. Every run on this cluster
/// shares that maintenance database, so the lock serializes the runs.
struct RoleLock(PgConnection);

impl RoleLock {
    /// Take the lock. The call waits until every other holder gives it back.
    async fn acquire() -> RoleLock {
        let dsn =
            std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
        let mut conn = PgConnection::connect(&dsn)
            .await
            .unwrap_or_else(|e| panic!("the lock connection failed: {e}"));
        // The key is a constant of this file, so no input reaches the text.
        // `pg_advisory_lock` returns void, which the checked macros do not map,
        // so this statement stays outside them (R2).
        sqlx::query(AssertSqlSafe(format!(
            "SELECT pg_advisory_lock({ROLE_LOCK_KEY})"
        )))
        .execute(&mut conn)
        .await
        .unwrap_or_else(|e| panic!("pg_advisory_lock({ROLE_LOCK_KEY}) failed: {e}"));
        RoleLock(conn)
    }

    /// Give the lock back.
    ///
    /// A panic of the test body skips this call. The session then ends with the
    /// dropped connection, and PostgreSQL releases the lock of a session that
    /// ends, so a red test also gives the lock back.
    async fn release(self) {
        let mut conn = self.0;
        let released = sqlx::query_scalar::<_, bool>(AssertSqlSafe(format!(
            "SELECT pg_advisory_unlock({ROLE_LOCK_KEY})"
        )))
        .fetch_one(&mut conn)
        .await
        .unwrap_or_else(|e| panic!("pg_advisory_unlock({ROLE_LOCK_KEY}) failed: {e}"));
        assert!(released, "this session did not hold the role lock");
        let _ = conn.close().await;
    }
}

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
        let role_lock = RoleLock::acquire().await;

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
        role_lock.release().await;
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
        let role_lock = RoleLock::acquire().await;

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
        role_lock.release().await;
    })
    .await;
}

/// An empty password variable stops the run with the documented exit code 2.
#[tokio::test]
async fn empty_password_variable_exits_with_code_two() {
    TestDb::with(|db| async move {
        let role_lock = RoleLock::acquire().await;

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
        role_lock.release().await;
    })
    .await;
}

/// Finding #11: the role lock lives in the cluster, not in this process.
///
/// A second connection reads `pg_locks`. The granted row must carry the key of
/// this file and the OID of the maintenance database. A lock on a throwaway
/// database, or no lock at all, gives a count of 0, because a second gate run
/// connects to its own throwaway database and would not wait.
#[tokio::test]
async fn the_role_lock_is_cluster_wide() {
    let lock = RoleLock::acquire().await;

    let dsn = std::env::var(TEST_DSN_VAR).unwrap();
    let mut observer = PgConnection::connect(&dsn).await.unwrap();
    // A bigint advisory key of this size gives classid 0, objid = the key, and
    // objsubid 1. `database` is the OID of the database of the lock session.
    let granted = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!"
           FROM pg_locks
           WHERE locktype = 'advisory'
             AND classid = 0
             AND objid = 7241001
             AND objsubid = 1
             AND granted
             AND database = (SELECT oid FROM pg_database WHERE datname = current_database())"#
    )
    .fetch_one(&mut observer)
    .await
    .unwrap();
    observer.close().await.unwrap();

    lock.release().await;

    assert_eq!(
        granted, 1,
        "the role lock must be one granted advisory lock on the maintenance database"
    );
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
