//! Process tests for `cadus-migrate --admin-login`: the login grant, the role
//! passwords and their rule, the stop signal, and the argument errors. The
//! file comment of `migrate_bin.rs` explains the role lock these tests take.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::time::Duration;

use cadus_store::test_support::TestDb;
use common::migrate::{
    HEX_PASSWORD, PASSWORD_RULE_LINE, RoleLock, STOPPED_BY_SIGNAL, admin_can_login,
    app_has_password, app_is_superuser, clear_app_password, expect_exit_two, expect_exit_zero,
    maintenance_db_name, migrate_command, run_migrate, signal_child, wait_within,
};
use sqlx::{Connection, PgConnection};
use uuid::Uuid;

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

        let stdout = expect_exit_zero(&output);
        assert!(
            stdout.contains("cadus-migrate: cadus_admin LOGIN granted"),
            "stdout: {stdout}"
        );
        // The migrations of this database ran already, so this run applies none.
        assert!(
            stdout.contains("cadus-migrate: applied 0 migrations (23 total)"),
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

        let stdout = expect_exit_zero(&output);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
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

        expect_exit_two(
            &output,
            "cadus-migrate: configuration error: CADUS_APP_PASSWORD is empty",
        );
        role_lock.release().await;
    })
    .await;
}

/// An unknown argument prints the usage and exits 2. The DSN is valid, so the
/// exit code comes from the argument check and from nothing else.
#[tokio::test]
async fn unknown_flag_exits_with_code_two() {
    TestDb::with(|db| async move {
        let output = run_migrate(&db.superuser_dsn(), None, &["--bogus"]);

        expect_exit_two(&output, "usage: cadus-migrate [--admin-login]");
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

        expect_exit_two(&output, "usage: cadus-migrate [--admin-login]");
    })
    .await;
}
