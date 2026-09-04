//! The process fixtures of the `cadus-migrate` tests: the cluster-wide role
//! lock, the command of a binary run, the throwaway databases beside the one
//! of `TestDb`, the catalog reads of the two roles, and the signal helpers.

use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use cadus_store::test_support::TestDb;
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use uuid::Uuid;

/// The line that the operator reads in `docker compose logs migrate`.
pub const APPLIED_ALL: &str = "cadus-migrate: applied 12 migrations (12 total)";

/// The environment variable that names the lock database of the binary.
pub const MAINTENANCE_DB_VAR: &str = "CADUS_MAINTENANCE_DB";

/// The maintenance database that applies when `CADUS_MAINTENANCE_DB` is absent.
/// `crates/store/src/bin/cadus-migrate.rs` carries the same default.
pub const DEFAULT_MAINTENANCE_DB: &str = "postgres";

/// A password that follows the rule of the binary: 48 hexadecimal characters,
/// the shape that `openssl rand -hex 24` gives (finding #14).
pub const HEX_PASSWORD: &str = "9f2c1d4b7a6e0358cf91d24e7b60a5c38d1f4e29b70c6a55";

/// The line that the binary prints for a password outside its rule.
pub const PASSWORD_RULE_LINE: &str = "cadus-migrate: CADUS_APP_PASSWORD holds a character outside \
     [A-Za-z0-9_-] or a length outside 16..=128; generate one with: openssl rand -hex 24";

/// The line that the binary prints when a stop signal ends the run.
pub const STOPPED_BY_SIGNAL: &str = "cadus-migrate: stopped by signal";

/// The key of the advisory lock that guards the `ALTER ROLE` statements.
///
/// Roles are cluster-scoped, and two `ALTER ROLE` statements on one role at the
/// same time give "tuple concurrently updated". A mutex of this process
/// serializes the threads of this test binary only, and `docs/plans/M0.md` runs
/// several worktrees against one cluster, so the lock must live in the cluster
/// (round-2 finding #11). `pg_advisory_lock` gives such a lock.
///
/// 7241001 is an arbitrary but fixed number. It has one rule: every caller that
/// alters a cluster role in this repository takes this one key.
/// `crates/store/src/bin/cadus-migrate.rs` takes the same key. Nothing else in
/// the repository takes an advisory lock, so the key collides with nothing.
pub const ROLE_LOCK_KEY: i64 = 7_241_001;

/// A cluster-wide lock on the `ALTER ROLE` statements of this file.
///
/// PostgreSQL scopes an advisory lock to the database of the session, so the
/// lock connection opens the maintenance database of the cluster and not the
/// throwaway database of the test. The binary takes the same key in the same
/// database, so the two exclude each other (finding #3).
pub struct RoleLock(PgConnection);

impl RoleLock {
    /// Take the lock. The call waits until every other holder gives it back.
    ///
    /// The connection opens the maintenance database, which is the database
    /// that `maintenance_db_name()` names and the binary locks in too.
    pub async fn acquire() -> RoleLock {
        let dsn = TestDb::superuser_dsn_for(&maintenance_db_name());
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
    pub async fn release(self) {
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

    /// The database of the lock session.
    ///
    /// Finding #3: a lock in another database than the lock of the binary
    /// excludes nothing, so a test reads this name and compares it.
    pub async fn database(&mut self) -> String {
        sqlx::query_scalar!(r#"SELECT current_database()::text AS "name!""#)
            .fetch_one(&mut self.0)
            .await
            .unwrap_or_else(|e| panic!("current_database() failed: {e}"))
    }
}

/// Run the binary against `dsn` with the given arguments.
///
/// `lock_db` names the database in which the binary takes the role lock.
/// `Some(name)` gives the binary that database, which a test that holds
/// `RoleLock` needs (see the file comment). `None` removes the variable, so the
/// binary uses its own default and the run reproduces the deployment.
///
/// The two password variables are removed, so a variable of the shell that
/// starts the test cannot change the output of the run.
pub fn run_migrate(dsn: &str, lock_db: Option<&str>, args: &[&str]) -> Output {
    migrate_command(dsn, lock_db).args(args).output().unwrap()
}

/// Build the command of a binary run. The caller adds the arguments and the
/// variables of its own case.
///
/// Both output streams are pipes. `wait_with_output` fills its `stdout` and
/// `stderr` from a pipe only: for an inherited handle it gives an empty `Vec`,
/// so a failure message that reads the text of an inherited child is always
/// empty and the real error line of the child goes to the terminal of the test
/// binary instead (finding #10).
pub fn migrate_command(dsn: &str, lock_db: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cadus-migrate"));
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("DATABASE_URL", dsn)
        .env_remove("CADUS_APP_PASSWORD")
        .env_remove("CADUS_ADMIN_PASSWORD");
    match lock_db {
        Some(name) => command.env(MAINTENANCE_DB_VAR, name),
        None => command.env_remove(MAINTENANCE_DB_VAR),
    };
    command
}

/// Create a database on the test cluster and return its name.
pub async fn create_database(db: &TestDb, tag: &str) -> String {
    let name = format!(
        "cadus2_t_{tag}_{}",
        &Uuid::new_v4().simple().to_string()[..8]
    );
    sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
        .execute(&db.admin)
        .await
        .unwrap();
    name
}

/// Drop a database of the test cluster. The drop runs before the assertions, so
/// a failed assertion leaves nothing behind.
pub async fn drop_database(db: &TestDb, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query(AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
    )))
    .execute(&db.admin)
    .await
    .map(|_| ())
}

/// Put the password of `cadus_app` back to the state of migration 0001.
pub async fn clear_app_password(db: &TestDb) {
    sqlx::query("ALTER ROLE cadus_app PASSWORD NULL")
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Count the sessions of `application_name` that wait for the role lock in the
/// database that `maintenance` names.
pub async fn waiting_role_locks(db: &TestDb, application_name: &str, maintenance: &str) -> i64 {
    sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!"
           FROM pg_locks l
           JOIN pg_stat_activity a ON a.pid = l.pid
           WHERE l.locktype = 'advisory'
             AND l.classid = 0
             AND l.objid = 7241001
             AND l.objsubid = 1
             AND NOT l.granted
             AND a.application_name = $1
             AND l.database = (SELECT oid FROM pg_database WHERE datname = $2)"#,
        application_name,
        maintenance
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The maintenance database of the test cluster.
///
/// The name comes from `CADUS_MAINTENANCE_DB`, and an absent or empty variable
/// gives the literal `postgres`. `crates/store/src/bin/cadus-migrate.rs` reads
/// the same variable with the same default, so this function names the database
/// in which the binary takes its role lock. The host and the credentials come
/// from `CADUS_TEST_DATABASE_URL` (finding #3).
pub fn maintenance_db_name() -> String {
    match std::env::var(MAINTENANCE_DB_VAR) {
        Ok(name) if !name.is_empty() => name,
        _ => DEFAULT_MAINTENANCE_DB.to_string(),
    }
}

/// Send a signal to a child process.
///
/// The test calls `kill(1)`, so the crate needs no `libc` dependency for one
/// test.
pub fn signal_child(signal: &str, pid: u32) {
    let status = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .status()
        .unwrap_or_else(|e| panic!("kill -{signal} {pid} failed: {e}"));
    assert!(status.success(), "kill -{signal} {pid} exited {status}");
}

/// Wait for the exit of a child for at most `limit`. Return `None` at the
/// deadline, so the caller reports a run that does not stop.
pub async fn wait_within(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    None
}

/// Read `rolcanlogin` of the `cadus_admin` role.
pub async fn admin_can_login(db: &TestDb) -> bool {
    sqlx::query_scalar!(
        r#"SELECT rolcanlogin AS "rolcanlogin!" FROM pg_roles WHERE rolname = 'cadus_admin'"#
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// Report whether `cadus_app` holds a password. Only a superuser reads
/// `pg_authid`, and the admin pool is the superuser of the test cluster.
pub async fn app_has_password(db: &TestDb) -> bool {
    sqlx::query_scalar!(
        r#"SELECT (rolpassword IS NOT NULL) AS "has_password!"
           FROM pg_authid WHERE rolname = 'cadus_app'"#
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// Report whether `cadus_app` is a cluster superuser. The password step must
/// never change this flag.
pub async fn app_is_superuser(db: &TestDb) -> bool {
    sqlx::query_scalar!(
        r#"SELECT rolsuper AS "rolsuper!" FROM pg_roles WHERE rolname = 'cadus_app'"#
    )
    .fetch_one(&db.admin)
    .await
    .unwrap()
}

/// The stdout of a run that exited 0. A run that exited otherwise fails
/// with both streams in the message.
pub fn expect_exit_zero(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout: {stdout}stderr: {stderr}"
    );
    stdout
}

/// Assert that a run exited 2 with `needle` on its stderr.
pub fn expect_exit_two(output: &Output, needle: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
    assert!(stderr.contains(needle), "stderr: {stderr}");
}
