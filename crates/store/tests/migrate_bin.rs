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
//! (finding #2). The two holders are separate sessions, so a test that holds
//! `RoleLock` must NOT let the binary take the same key on the same database:
//! that pair waits forever. Such a test therefore gives the binary
//! `CADUS_MAINTENANCE_DB` with the name of its own throwaway database. The
//! `RoleLock` of the test then covers the binary run too, and the lock of the
//! binary lands in a database that no other run reaches.
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
use std::process::{Command, Output};
use std::time::{Duration, Instant};

use cadus_store::test_support::TestDb;
use sqlx::{AssertSqlSafe, Connection, PgConnection};
use uuid::Uuid;

/// The line that the operator reads in `docker compose logs migrate`.
const APPLIED_SIX: &str = "cadus-migrate: applied 6 migrations (6 total)";

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// The environment variable that names the lock database of the binary.
const MAINTENANCE_DB_VAR: &str = "CADUS_MAINTENANCE_DB";

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
fn migrate_command(dsn: &str, lock_db: Option<&str>) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cadus-migrate"));
    command
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
async fn fresh_database_reports_six_applied() {
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
        assert!(stdout.contains(APPLIED_SIX), "stdout: {stdout}");
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
        assert!(stdout.contains(APPLIED_SIX), "stdout: {stdout}");
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

        let lock = RoleLock::acquire().await;
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
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(output.status.code(), Some(0), "stderr: {stderr}");
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

/// The database that `CADUS_TEST_DATABASE_URL` names. Every run on the cluster
/// shares it, so it is the maintenance database of the test cluster.
fn maintenance_db_name() -> String {
    let dsn = std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
    let (_, name) = dsn
        .rsplit_once('/')
        .unwrap_or_else(|| panic!("{TEST_DSN_VAR} carries no database path segment"));
    name.to_string()
}

/// The outcome of one round of two `cadus-migrate --admin-login` runs.
struct PairedRound {
    round: i32,
    left_code: Option<i32>,
    right_code: Option<i32>,
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
            let left = migrate_command(&first_dsn, None)
                .arg("--admin-login")
                .env("CADUS_APP_PASSWORD", format!("pw-left-{round}"))
                .spawn()
                .unwrap();
            let right = migrate_command(&second_dsn, None)
                .arg("--admin-login")
                .env("CADUS_APP_PASSWORD", format!("pw-right-{round}"))
                .spawn()
                .unwrap();

            let left = left.wait_with_output().unwrap();
            let right = right.wait_with_output().unwrap();
            rounds.push(PairedRound {
                round,
                left_code: left.status.code(),
                right_code: right.status.code(),
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

        clear_app_password(&db).await;
        assert!(
            !app_has_password(&db).await,
            "the precondition is a role with no password"
        );

        let password = format!("pw-{}", &Uuid::new_v4().simple().to_string()[..8]);
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

/// Finding #8: a password with a single quote reaches the role.
///
/// `ALTER ROLE` takes no bind parameter, so the password goes into the
/// statement text as an SQL string literal. Without the doubled quote the text
/// of `x'y` reads `ALTER ROLE cadus_app PASSWORD 'x'y'`, which PostgreSQL
/// rejects, and a password of the form `x' SUPERUSER --` adds role options to a
/// statement that a superuser runs.
#[tokio::test]
async fn a_quote_in_the_app_password_reaches_the_role() {
    TestDb::with(|db| async move {
        let role_lock = RoleLock::acquire().await;

        clear_app_password(&db).await;
        assert!(
            !app_has_password(&db).await,
            "the precondition is a role with no password"
        );

        let output = migrate_command(&db.superuser_dsn(), Some(&db.name))
            .arg("--admin-login")
            .env("CADUS_APP_PASSWORD", "x'y")
            .output()
            .unwrap();

        let has_password = app_has_password(&db).await;
        let is_superuser = app_is_superuser(&db).await;

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
            has_password,
            "cadus_app must hold the password with the quote"
        );
        assert!(!is_superuser, "the password must not add a role option");
        role_lock.release().await;
    })
    .await;
}

/// Round-2 finding #11: the role lock lives in the cluster, not in this process.
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
