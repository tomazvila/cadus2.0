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

mod common;

use std::time::{Duration, Instant};

use cadus_store::test_support::TestDb;
use common::migrate::{
    APPLIED_ALL, HEX_PASSWORD, MAINTENANCE_DB_VAR, RoleLock, clear_app_password, create_database,
    drop_database, expect_exit_zero, maintenance_db_name, migrate_command, run_migrate,
    waiting_role_locks,
};
use uuid::Uuid;

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

        let stdout = expect_exit_zero(&output);
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

        let stdout = expect_exit_zero(&output);
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
        let stdout = expect_exit_zero(&output);
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
