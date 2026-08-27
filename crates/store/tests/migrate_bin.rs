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
//! Roles are cluster-scoped. Every actor that alters a role takes the one
//! advisory lock of `ROLE_LOCK_KEY` on the maintenance database of the cluster:
//! the tests take it through `RoleLock`, and the binary takes it itself
//! (finding #2). An advisory lock is database-scoped, so the two holders
//! exclude each other only in one database. `RoleLock` and the binary therefore
//! read one name from one variable, `CADUS_MAINTENANCE_DB`, with one default,
//! the literal `postgres`: `maintenance_db_name()` below and
//! `DEFAULT_MAINTENANCE_DB` in `crates/store/src/bin/cadus-migrate.rs`
//! (finding #3).
//!
//! The two holders are separate sessions, so a test that holds `RoleLock` must
//! NOT let the binary take the same key on the same database: that pair waits
//! forever. Such a test therefore gives the binary `CADUS_MAINTENANCE_DB` with
//! the name of its own throwaway database. The `RoleLock` of the test then
//! covers the binary run too, and the lock of the binary lands in a database
//! that no other run reaches.
//!
//! One test is the exception:
//! `the_role_lock_of_the_binary_lives_in_the_maintenance_database` lets the
//! binary wait for the key that the test holds, and gives the key back before
//! it reads the exit code.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use cadus_store::test_support::TestDb;
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use uuid::Uuid;

/// The line that the operator reads in `docker compose logs migrate`.
const APPLIED_ALL: &str = "cadus-migrate: applied 9 migrations (9 total)";

/// The environment variable that names the lock database of the binary.
const MAINTENANCE_DB_VAR: &str = "CADUS_MAINTENANCE_DB";

/// The maintenance database that applies when `CADUS_MAINTENANCE_DB` is absent.
/// `crates/store/src/bin/cadus-migrate.rs` carries the same default.
const DEFAULT_MAINTENANCE_DB: &str = "postgres";

/// A password that follows the rule of the binary: 48 hexadecimal characters,
/// the shape that `openssl rand -hex 24` gives (finding #14).
const HEX_PASSWORD: &str = "9f2c1d4b7a6e0358cf91d24e7b60a5c38d1f4e29b70c6a55";

/// The line that the binary prints for a password outside its rule.
const PASSWORD_RULE_LINE: &str = "cadus-migrate: CADUS_APP_PASSWORD holds a character outside \
     [A-Za-z0-9_-] or a length outside 16..=128; generate one with: openssl rand -hex 24";

/// The line that the binary prints when a stop signal ends the run.
const STOPPED_BY_SIGNAL: &str = "cadus-migrate: stopped by signal";

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
const ROLE_LOCK_KEY: i64 = 7_241_001;

/// A cluster-wide lock on the `ALTER ROLE` statements of this file.
///
/// PostgreSQL scopes an advisory lock to the database of the session, so the
/// lock connection opens the maintenance database of the cluster and not the
/// throwaway database of the test. The binary takes the same key in the same
/// database, so the two exclude each other (finding #3).
struct RoleLock(PgConnection);

impl RoleLock {
    /// Take the lock. The call waits until every other holder gives it back.
    ///
    /// The connection opens the maintenance database, which is the database
    /// that `maintenance_db_name()` names and the binary locks in too.
    async fn acquire() -> RoleLock {
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

    /// The database of the lock session.
    ///
    /// Finding #3: a lock in another database than the lock of the binary
    /// excludes nothing, so a test reads this name and compares it.
    async fn database(&mut self) -> String {
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
fn run_migrate(dsn: &str, lock_db: Option<&str>, args: &[&str]) -> Output {
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
fn migrate_command(dsn: &str, lock_db: Option<&str>) -> Command {
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
async fn create_database(db: &TestDb, tag: &str) -> String {
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
async fn drop_database(db: &TestDb, name: &str) -> Result<(), sqlx::Error> {
    sqlx::query(AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
    )))
    .execute(&db.admin)
    .await
    .map(|_| ())
}

/// Put the password of `cadus_app` back to the state of migration 0001.
async fn clear_app_password(db: &TestDb) {
    sqlx::query("ALTER ROLE cadus_app PASSWORD NULL")
        .execute(&db.admin)
        .await
        .unwrap();
}

/// D9: a fresh database gets the whole migration set, and the binary reports
/// the literal count that `docs/SELF_HOST.md` tells the operator to read.
#[tokio::test]
async fn fresh_database_reports_every_migration_applied() {
    TestDb::with(|db| async move {
        // The database of `TestDb` is migrated already, so this test makes a
        // second, unmigrated one on the same cluster.
        let fresh = create_database(&db, "fresh").await;

        let output = run_migrate(&TestDb::superuser_dsn_for(&fresh), None, &[]);

        // Drop the extra database before the assertions, so a failed assertion
        // leaves nothing behind.
        let dropped = drop_database(&db, &fresh).await;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(
            output.status.code(),
            Some(0),
            "stdout: {stdout}stderr: {stderr}"
        );
        assert!(stdout.contains(APPLIED_ALL), "stdout: {stdout}");
        dropped.unwrap();
    })
    .await;
}

/// Finding #3: `cadus-migrate` runs with `statement_timeout` off, so
/// `DB_STATEMENT_TIMEOUT_MS` never cancels a migration.
///
/// 1 ms is shorter than every statement of the migration set, so the run
/// applies nothing and exits 2 as soon as the bound reaches the pool. The
/// binary overrides the bound, so the run applies the whole set and exits 0.
#[tokio::test]
async fn a_short_statement_timeout_does_not_reach_the_migrations() {
    TestDb::with(|db| async move {
        let fresh = create_database(&db, "timeout").await;

        let output = migrate_command(&TestDb::superuser_dsn_for(&fresh), None)
            .env("DB_STATEMENT_TIMEOUT_MS", "1")
            .output()
            .unwrap();

        let dropped = drop_database(&db, &fresh).await;

        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(
            output.status.code(),
            Some(0),
            "stdout: {stdout}stderr: {stderr}"
        );
        assert!(stdout.contains(APPLIED_ALL), "stdout: {stdout}");
        dropped.unwrap();
    })
    .await;
}

/// Finding #2: the binary takes its role lock in the maintenance database, not
/// in the database of `DATABASE_URL`.
///
/// The test holds `RoleLock` on the maintenance database and starts the binary
/// against another database. A lock in the database of `DATABASE_URL` reaches
/// no holder, so the binary runs to the end at once. A lock in the maintenance
/// database waits, and `pg_locks` shows the row that waits. The DSN of the
/// binary carries an `application_name` of this run alone, so the row belongs
/// to this binary and to no other run of the cluster.
///
/// This is the one test that lets the binary take the key that the test holds.
/// It gives the lock back before it reads the exit code, so the pair never
/// waits forever.
#[tokio::test]
async fn the_role_lock_of_the_binary_lives_in_the_maintenance_database() {
    TestDb::with(|db| async move {
        let fresh = create_database(&db, "lockdb").await;
        let tag = format!("cadus2_t_tag_{}", &Uuid::new_v4().simple().to_string()[..8]);
        let dsn = format!(
            "{}?application_name={tag}",
            TestDb::superuser_dsn_for(&fresh)
        );
        let maintenance = maintenance_db_name();

        let mut lock = RoleLock::acquire().await;
        // Finding #3: the lock of this file must live in the database that the
        // binary locks in, or neither holder excludes the other.
        let lock_database = lock.database().await;
        let mut child = migrate_command(&dsn, Some(&maintenance))
            .arg("--admin-login")
            .spawn()
            .unwrap();

        // Wait until the binary waits for the key, or until it exits without
        // the key. 20 s is longer than the whole run and shorter than a hang.
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut waiting: i64 = 0;
        let mut early_exit: Option<i32> = None;
        while Instant::now() < deadline {
            waiting = waiting_role_locks(&db, &tag, &maintenance).await;
            if waiting > 0 {
                break;
            }
            if let Some(status) = child.try_wait().unwrap() {
                early_exit = Some(status.code().unwrap_or(-1));
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        lock.release().await;
        let output = child.wait_with_output().unwrap();
        let dropped = drop_database(&db, &fresh).await;

        assert_eq!(
            early_exit, None,
            "the binary finished while another session held the role lock"
        );
        assert_eq!(
            waiting, 1,
            "the binary must wait for the key in the maintenance database"
        );
        // Finding #3: `CADUS_MAINTENANCE_DB` names the one lock database, and
        // the default of both holders is the literal "postgres".
        assert_eq!(
            lock_database,
            std::env::var(MAINTENANCE_DB_VAR).unwrap_or_else(|_| "postgres".to_string()),
            "the lock of this file must live in the maintenance database"
        );
        assert_eq!(lock_database, maintenance);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(
            output.status.code(),
            Some(0),
            "stdout: {stdout}stderr: {stderr}"
        );
        // Finding #10: the pipes carry the text of the child. Without them this
        // line is empty and the failure message above says nothing.
        assert!(
            stdout.contains("cadus-migrate: cadus_admin LOGIN granted"),
            "stdout: {stdout}"
        );
        dropped.unwrap();
    })
    .await;
}

/// Count the sessions of `application_name` that wait for the role lock in the
/// database that `maintenance` names.
async fn waiting_role_locks(db: &TestDb, application_name: &str, maintenance: &str) -> i64 {
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
fn maintenance_db_name() -> String {
    match std::env::var(MAINTENANCE_DB_VAR) {
        Ok(name) if !name.is_empty() => name,
        _ => DEFAULT_MAINTENANCE_DB.to_string(),
    }
}

/// The outcome of one round of two `cadus-migrate --admin-login` runs.
struct PairedRound {
    round: i32,
    left_code: Option<i32>,
    right_code: Option<i32>,
    left_stdout: String,
    right_stdout: String,
    left_stderr: String,
    right_stderr: String,
}

/// Finding #2: two `--admin-login` runs on two databases of one cluster both
/// exit 0.
///
/// The two runs alter the same cluster-scoped roles at the same time. A lock in
/// the database of `DATABASE_URL` serializes nothing here, because the two runs
/// use two databases; one run then dies with "tuple concurrently updated" and
/// exit code 2. The binary takes its lock in the maintenance database that
/// every run of the cluster shares, so the two runs serialize.
///
/// The test takes no `RoleLock`: the two binary runs take that lock themselves,
/// and a test that held it too would wait for its own children forever.
#[tokio::test]
async fn two_migrate_runs_on_two_databases_both_exit_zero() {
    TestDb::with(|db| async move {
        let first = create_database(&db, "pair_a").await;
        let second = create_database(&db, "pair_b").await;
        let first_dsn = TestDb::superuser_dsn_for(&first);
        let second_dsn = TestDb::superuser_dsn_for(&second);

        // Apply the migrations first, so the paired runs below do the role work
        // and nothing else.
        let setup = [
            run_migrate(&first_dsn, None, &[]),
            run_migrate(&second_dsn, None, &[]),
        ];

        // Three rounds. One round is enough for a green run, and the failure of
        // the review appeared in 8 rounds out of 8.
        let mut rounds: Vec<PairedRound> = Vec::new();
        for round in 1..=3 {
            // The password follows the rule of the binary: allowed characters
            // only, and 16 characters or more (finding #14).
            let left = migrate_command(&first_dsn, None)
                .arg("--admin-login")
                .env(
                    "CADUS_APP_PASSWORD",
                    format!("pw-left-{round}-{HEX_PASSWORD}"),
                )
                .spawn()
                .unwrap();
            let right = migrate_command(&second_dsn, None)
                .arg("--admin-login")
                .env(
                    "CADUS_APP_PASSWORD",
                    format!("pw-right-{round}-{HEX_PASSWORD}"),
                )
                .spawn()
                .unwrap();

            let left = left.wait_with_output().unwrap();
            let right = right.wait_with_output().unwrap();
            rounds.push(PairedRound {
                round,
                left_code: left.status.code(),
                right_code: right.status.code(),
                left_stdout: String::from_utf8_lossy(&left.stdout).into_owned(),
                right_stdout: String::from_utf8_lossy(&right.stdout).into_owned(),
                left_stderr: String::from_utf8_lossy(&left.stderr).into_owned(),
                right_stderr: String::from_utf8_lossy(&right.stderr).into_owned(),
            });
        }

        // Leave the shared cluster as this test found it.
        let role_lock = RoleLock::acquire().await;
        clear_app_password(&db).await;
        role_lock.release().await;
        let dropped_first = drop_database(&db, &first).await;
        let dropped_second = drop_database(&db, &second).await;

        for output in &setup {
            assert_eq!(
                output.status.code(),
                Some(0),
                "the setup run failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        for round in &rounds {
            assert_eq!(
                (round.left_code, round.right_code),
                (Some(0), Some(0)),
                "round {}: left stderr: {}right stderr: {}",
                round.round,
                round.left_stderr,
                round.right_stderr
            );
            // Finding #10: the pipes carry the text of both children. Without
            // them the two lines above are empty on every failure.
            let line = "cadus-migrate: password set for cadus_app";
            assert!(
                round.left_stdout.contains(line),
                "round {}: left stdout: {}",
                round.round,
                round.left_stdout
            );
            assert!(
                round.right_stdout.contains(line),
                "round {}: right stdout: {}",
                round.round,
                round.right_stdout
            );
        }
        dropped_first.unwrap();
        dropped_second.unwrap();
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

        let output = run_migrate(&db.superuser_dsn(), Some(&db.name), &["--admin-login"]);

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
            stdout.contains("cadus-migrate: applied 0 migrations (9 total)"),
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

        clear_app_password(&db).await;
        assert!(
            !app_has_password(&db).await,
            "the precondition is a role with no password"
        );

        // The password follows the rule of the binary: allowed characters
        // only, and 16 characters or more (finding #14).
        let password = format!("pw-{}", Uuid::new_v4().simple());
        let output = migrate_command(&db.superuser_dsn(), Some(&db.name))
            .arg("--admin-login")
            .env("CADUS_APP_PASSWORD", &password)
            .output()
            .unwrap();

        let has_password = app_has_password(&db).await;

        // Leave the shared cluster as this test found it.
        clear_app_password(&db).await;

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

/// Finding #14: a password outside the character rule stops the run with exit
/// code 2, and no `ALTER ROLE` statement runs.
///
/// The three runtime DSNs of `docker-compose.yml` carry the password inside a
/// URL with no percent-encoding, so `@`, `#`, and `%` each make the DSN name
/// another host or another password with no error. The run must therefore
/// refuse such a value before it rotates the role: a rotated role plus an
/// unusable DSN takes the site down and `.env` alone does not bring it back.
///
/// The quote of finding #8 is one such value. The escape of
/// `alter_role_password_statement` stays as the second guard, and the unit
/// tests of `crates/store/src/bin/cadus-migrate.rs` pin it.
///
/// The last case is the answer to the first: 48 hexadecimal characters, the
/// output shape of `openssl rand -hex 24`, reach the role and the run exits 0.
#[tokio::test]
async fn the_password_rule_refuses_a_bad_password_and_takes_a_generated_one() {
    TestDb::with(|db| async move {
        let role_lock = RoleLock::acquire().await;

        clear_app_password(&db).await;
        assert!(
            !app_has_password(&db).await,
            "the precondition is a role with no password"
        );

        // The first two values break the rule on the character and on the
        // length, the third on the character alone, and the fourth on the
        // length alone.
        let mut refused: Vec<(String, Option<i32>, String, bool)> = Vec::new();
        for password in ["a@b", "x'y", "corr@horse#batteryStaple", "0123456789abcde"] {
            let output = migrate_command(&db.superuser_dsn(), Some(&db.name))
                .arg("--admin-login")
                .env("CADUS_APP_PASSWORD", password)
                .output()
                .unwrap();
            refused.push((
                password.to_string(),
                output.status.code(),
                String::from_utf8_lossy(&output.stderr).into_owned(),
                app_has_password(&db).await,
            ));
        }
        let is_superuser = app_is_superuser(&db).await;

        // The password that follows the rule reaches the role.
        let taken = migrate_command(&db.superuser_dsn(), Some(&db.name))
            .arg("--admin-login")
            .env("CADUS_APP_PASSWORD", HEX_PASSWORD)
            .output()
            .unwrap();
        let has_password = app_has_password(&db).await;

        // Leave the shared cluster as this test found it.
        clear_app_password(&db).await;

        for (password, code, stderr, altered) in &refused {
            assert_eq!(*code, Some(2), "{password}: stderr: {stderr}");
            assert!(stderr.contains(PASSWORD_RULE_LINE), "{password}: {stderr}");
            assert!(
                !altered,
                "{password}: rolpassword must stay NULL, so no ALTER ROLE ran"
            );
        }
        assert!(!is_superuser, "a refused password must add no role option");

        let taken_stdout = String::from_utf8_lossy(&taken.stdout).into_owned();
        let taken_stderr = String::from_utf8_lossy(&taken.stderr).into_owned();
        assert_eq!(
            taken.status.code(),
            Some(0),
            "stdout: {taken_stdout}stderr: {taken_stderr}"
        );
        assert!(
            has_password,
            "cadus_app must hold the password that follows the rule"
        );
        role_lock.release().await;
    })
    .await;
}

/// Finding #12: a stop signal ends the run with exit code 3.
///
/// The DSN names a TCP server that accepts the connection and answers nothing,
/// so the run waits inside the connect, the same shape as the unbounded wait of
/// `RoleLock::acquire`. `SIGTERM` after 300 ms must end the run. Without the
/// handlers the process stays deaf: the wait then runs to the acquire timeout
/// of the pool, and the run exits 2 five seconds later, or never ends at all as
/// PID 1 of a container.
#[tokio::test]
async fn a_stop_signal_ends_the_run_with_code_three() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // The thread accepts every connection and holds it. It answers nothing, so
    // the startup message of the client waits without an end.
    std::thread::spawn(move || {
        let mut held = Vec::new();
        while let Ok((stream, _)) = listener.accept() {
            held.push(stream);
        }
    });

    let dsn = format!("postgresql://test:test@127.0.0.1:{port}/deaf");
    let mut child = migrate_command(&dsn, None)
        .arg("--admin-login")
        .env("CADUS_APP_PASSWORD", HEX_PASSWORD)
        .spawn()
        .unwrap();

    tokio::time::sleep(Duration::from_millis(300)).await;
    signal_child("TERM", child.id());
    let status = wait_within(&mut child, Duration::from_secs(5)).await;
    if status.is_none() {
        let _ = child.kill();
    }
    let output = child.wait_with_output().unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let status = status.unwrap_or_else(|| {
        panic!("the run did not stop inside 5 s after SIGTERM; stderr: {stderr}")
    });
    assert_eq!(status.code(), Some(3), "stderr: {stderr}");
    assert!(stderr.contains(STOPPED_BY_SIGNAL), "stderr: {stderr}");
}

/// Send a signal to a child process.
///
/// The test calls `kill(1)`, so the crate needs no `libc` dependency for one
/// test.
fn signal_child(signal: &str, pid: u32) {
    let status = Command::new("kill")
        .arg(format!("-{signal}"))
        .arg(pid.to_string())
        .status()
        .unwrap_or_else(|e| panic!("kill -{signal} {pid} failed: {e}"));
    assert!(status.success(), "kill -{signal} {pid} exited {status}");
}

/// Wait for the exit of a child for at most `limit`. Return `None` at the
/// deadline, so the caller reports a run that does not stop.
async fn wait_within(child: &mut Child, limit: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            return Some(status);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    None
}

/// Round-2 finding #11: the role lock lives in the cluster, not in this process.
///
/// A second connection reads `pg_locks`. The granted row must carry the key of
/// this file and the OID of the maintenance database that `maintenance_db_name`
/// names. A lock on another database, or no lock at all, gives a count of 0:
/// the binary and a second gate run take the key in the maintenance database,
/// so a lock anywhere else excludes nothing (finding #3).
#[tokio::test]
async fn the_role_lock_is_cluster_wide() {
    let lock = RoleLock::acquire().await;

    let maintenance = maintenance_db_name();
    let dsn = TestDb::superuser_dsn_for(&maintenance);
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
             AND database = (SELECT oid FROM pg_database WHERE datname = $1)"#,
        maintenance
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

/// An empty password variable stops the run with the documented exit code 2.
#[tokio::test]
async fn empty_password_variable_exits_with_code_two() {
    TestDb::with(|db| async move {
        let role_lock = RoleLock::acquire().await;

        let output = migrate_command(&db.superuser_dsn(), Some(&db.name))
            .arg("--admin-login")
            .env("CADUS_APP_PASSWORD", "")
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

/// Report whether `cadus_app` is a cluster superuser. The password step must
/// never change this flag.
async fn app_is_superuser(db: &TestDb) -> bool {
    sqlx::query_scalar!(
        r#"SELECT rolsuper AS "rolsuper!" FROM pg_roles WHERE rolname = 'cadus_app'"#
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
        let output = run_migrate(&db.superuser_dsn(), None, &["--bogus"]);

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
        assert!(
            stderr.contains("usage: cadus-migrate [--admin-login]"),
            "stderr: {stderr}"
        );
    })
    .await;
}

/// Finding #13: an argument that is not valid Unicode prints the usage and
/// exits 2.
///
/// The byte 0xff is not valid UTF-8. `std::env::args` unwraps such an argument
/// and aborts the process with exit code 101, which is neither the documented
/// exit code nor a message that names the usage.
#[tokio::test]
async fn a_non_unicode_argument_exits_with_code_two() {
    TestDb::with(|db| async move {
        let bad = OsString::from_vec(vec![0xff]);
        let output = migrate_command(&db.superuser_dsn(), None)
            .arg(&bad)
            .output()
            .unwrap();

        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(output.status.code(), Some(2), "stderr: {stderr}");
        assert!(
            stderr.contains("usage: cadus-migrate [--admin-login]"),
            "stderr: {stderr}"
        );
    })
    .await;
}
