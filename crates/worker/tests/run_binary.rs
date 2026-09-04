//! Proof tests for the worker binary (R4, C3).
//!
//! Every test starts the real binary, stops it with a signal or waits for its
//! exit, and reads its exit code and its log. `run_loop.rs` drives the loop in
//! process.
//!
//! Every expected value below is a literal: a literal exit code, a literal log
//! line.
//!
//! Every child process gets `RUST_LOG=info` (`common::worker_command`). An
//! ambient `RUST_LOG` of the developer shell must not decide the result of a
//! test (finding #12). Every spawned binary sits inside `common::KillOnDrop`, so
//! a failed assertion also kills and reaps the child instead of leaving a
//! worker process behind (finding #14).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::os::unix::ffi::OsStrExt;
use std::time::{Duration, Instant};

use cadus_store::test_support::{DeafPostgres, TestDb};

use common::{
    Run, fixture_curriculum, role_dsn, send_signal, spawn, superuser_dsn, wait_output,
    worker_command,
};

/// Start the tick loop against this database with a 1 s tick, wait `millis`
/// milliseconds, send `signal`, and read what the process wrote.
async fn tick_then_signal(dsn: &str, env: &[(&str, &str)], millis: u64, signal: i32) -> Run {
    let mut command = worker_command(dsn);
    command.env("WORKER_TICK_SECS", "1");
    for (name, value) in env {
        command.env(name, value);
    }
    let child = spawn(&mut command);
    let pid = child.pid();

    tokio::time::sleep(Duration::from_millis(millis)).await;
    send_signal(pid, signal);

    wait_output(child, 10).await
}

/// (4) The binary starts, ticks once per second, and exits 0 on SIGTERM.
#[tokio::test]
async fn binary_ticks_and_exits_zero_on_sigterm() {
    TestDb::with(|db| async move {
        let run = tick_then_signal(&superuser_dsn(&db.name), &[], 2500, libc::SIGTERM).await;

        let log = run.log();
        assert_eq!(
            run.code,
            Some(0),
            "the worker must exit 0 after SIGTERM; log:\n{log}"
        );
        assert!(
            log.contains("heartbeat tick=2"),
            "the log must hold `heartbeat tick=2`; log:\n{log}"
        );
    })
    .await;
}

/// SIGINT stops the loop the way SIGTERM does, and the log names the signal.
#[tokio::test]
async fn binary_exits_zero_on_sigint() {
    TestDb::with(|db| async move {
        let run = tick_then_signal(&superuser_dsn(&db.name), &[], 1500, libc::SIGINT).await;

        let log = run.log();
        assert_eq!(
            run.code,
            Some(0),
            "the worker must exit 0 after SIGINT; log:\n{log}"
        );
        assert!(
            log.contains("cadus-worker: SIGINT received"),
            "the log must name the signal; log:\n{log}"
        );
    })
    .await;
}

/// (4b) The diagnosis job keeps the DIAGNOSIS budget of 600 and 600 (T5,
/// finding F18), and the T4 cap reads from its variable.
///
/// The authoring pass takes a wider budget of its own, so the two paths must not
/// share one knob. This test reads the configuration line of the tick loop: the
/// run names no token variable at all, so both ceilings are the shipped
/// defaults.
///
/// The endpoint is a closed port. The loop makes no model call, because the
/// queue holds no row, so the port is never opened.
#[tokio::test]
async fn binary_configures_the_diagnosis_job_with_the_diagnosis_budget() {
    TestDb::with(|db| async move {
        let env = [
            ("OPENAI_API_KEY", "test-key"),
            ("OPENAI_BASE_URL", "http://127.0.0.1:1/v1"),
            ("OPENAI_MODEL", "qwen3.6"),
            ("DIAGNOSIS_CALLS_PER_SESSION", "3"),
        ];

        let run = tick_then_signal(&superuser_dsn(&db.name), &env, 1500, libc::SIGTERM).await;

        let log = run.log();
        assert!(
            log.contains("cadus-worker: the diagnosis job is configured"),
            "the log must hold the configuration line; log:\n{log}"
        );
        assert!(
            log.contains("output_tokens=600 reasoning_max_tokens=600"),
            "the diagnosis job must keep the 600 and 600 ceilings; log:\n{log}"
        );
        assert!(
            log.contains("calls_per_session=3"),
            "the T4 cap must reach the configuration line; log:\n{log}"
        );
    })
    .await;
}

/// (5) A stop signal during the database connect gives exit code 0.
///
/// The DSN points at a closed port, so the process stays inside `connect` for
/// the whole sqlx acquire timeout of 30 s. The signal handlers install before
/// the connect, so the SIGTERM at 300 ms is handled. A process that installs
/// them later dies by the signal and reports no exit code at all (finding #39).
#[tokio::test]
async fn binary_exits_zero_on_sigterm_during_the_connect() {
    let run = tick_then_signal("postgresql://x@127.0.0.1:1/x", &[], 300, libc::SIGTERM).await;

    assert_eq!(
        run.code,
        Some(0),
        "the worker must exit 0 after a SIGTERM during the connect; log:\n{}",
        run.log()
    );
}

/// (6) A stop signal during the role report gives exit code 0.
///
/// The DSN points at a server that finishes the handshake and then answers no
/// query, so `cadus_store::current_role` never returns. The test waits for the
/// query to reach that server, so the process is inside the role report when
/// the signal arrives. A bare `await` there makes the process deaf to SIGTERM
/// for the whole stall (finding #8).
#[tokio::test]
async fn binary_exits_zero_on_sigterm_during_the_role_report() {
    let deaf = DeafPostgres::start();
    let mut command = worker_command(&deaf.dsn());
    command.env("WORKER_TICK_SECS", "1");
    let child = spawn(&mut command);
    let pid = child.pid();

    // Wait until the role query reaches the deaf server. The process is then
    // inside `current_role` and answers only through the signal path.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !deaf.query_seen() {
        assert!(
            Instant::now() < deadline,
            "the worker must send its role query within 10 s"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    send_signal(pid, libc::SIGTERM);

    let run = wait_output(child, 5).await;

    let log = run.log();
    assert_eq!(
        run.code,
        Some(0),
        "the worker must exit 0 after SIGTERM during the role report; log:\n{log}"
    );
    assert!(
        log.contains("cadus-worker: the stop signal came before the role report"),
        "the log must name the role report as the point of the stop; log:\n{log}"
    );
}

/// A role report that never answers ends the process with code 2 at the
/// client-side bound (L1).
///
/// `DB_CLIENT_TIMEOUT_MS=200` is the bound, and the deaf server answers no
/// query, so the report times out and the loop never starts.
#[tokio::test]
async fn binary_exits_two_when_the_role_report_times_out() {
    let deaf = DeafPostgres::start();
    let mut command = worker_command(&deaf.dsn());
    command
        .env("WORKER_TICK_SECS", "1")
        .env("DB_CLIENT_TIMEOUT_MS", "200");

    let run = wait_output(spawn(&mut command), 10).await;

    let log = run.log();
    assert_eq!(run.code, Some(2), "log:\n{log}");
    assert!(
        log.contains("cadus-worker: store error: the query did not answer within 200 ms"),
        "the message must name the bound; log:\n{log}"
    );
    assert!(!log.contains("heartbeat tick="), "log:\n{log}");
}

/// (7) A curriculum path that does not exist ends the process with exit code 2.
///
/// The refill (D-O4) needs the tree for the A6 exemplar fallback and for the
/// knowledge-point half of the gate re-run. The old binary logged one warn line
/// and kept its tick loop, so the shipped image -- which carried no curriculum at
/// all -- refilled nothing for any learner and said nothing about it (review
/// round 1, findings #5 and #6).
///
/// The load runs before the database connect, so this test needs no database.
/// The run names no `WORKER_TICK_SECS`, so the loop takes its default period.
#[tokio::test]
async fn binary_exits_two_when_the_curriculum_does_not_load() {
    let missing = "/home/deploy/dev/cadus2.0/no-such-curriculum";
    let mut command = worker_command("postgresql://x@127.0.0.1:1/x");
    command
        .env("CADUS_CURRICULUM", missing)
        .env_remove("WORKER_TICK_SECS");

    let run = wait_output(spawn(&mut command), 10).await;

    let log = run.log();
    assert_eq!(
        run.code,
        Some(2),
        "a curriculum that does not load must end the process with 2; log:\n{log}"
    );
    assert!(
        log.contains(
            "cadus-worker: configuration error: the curriculum at \
             /home/deploy/dev/cadus2.0/no-such-curriculum did not load: no courses.yaml under \
             /home/deploy/dev/cadus2.0/no-such-curriculum"
        ),
        "the message must name the path and the reason; log:\n{log}"
    );
    assert!(
        !log.contains("heartbeat tick="),
        "the process must not reach its tick loop; log:\n{log}"
    );
}

/// With no `CADUS_CURRICULUM` the worker reads `./curriculum`, so a working
/// directory without one ends the process with code 2 and names that path.
#[tokio::test]
async fn binary_reads_the_default_curriculum_path() {
    let mut command = worker_command("postgresql://x@127.0.0.1:1/x");
    command
        .env_remove("CADUS_CURRICULUM")
        .current_dir(env!("CARGO_MANIFEST_DIR"));

    let run = wait_output(spawn(&mut command), 10).await;

    assert_eq!(run.code, Some(2));
    assert!(
        run.stderr.contains(
            "the curriculum at curriculum did not load: no courses.yaml under curriculum"
        ),
        "stderr:\n{}",
        run.stderr
    );
}

/// A variable the process cannot read ends it with code 2 and the sentence of
/// the reader that refused it: a bad tick period, a tick period that is not
/// Unicode, a bad database URL, and a bad model endpoint.
#[tokio::test]
async fn a_configuration_the_process_cannot_read_exits_two() {
    TestDb::with(|db| async move {
        let dsn = superuser_dsn(&db.name);
        let cases: [(&str, &str, &str, &str); 4] = [
            (
                &dsn,
                "WORKER_TICK_SECS",
                "soon",
                "WORKER_TICK_SECS must be a whole number of seconds, not \"soon\"",
            ),
            ("", "WORKER_TICK_SECS", "1", "DATABASE_URL is empty"),
            (
                &dsn,
                "OPENAI_BASE_URL",
                "::not a url::",
                "OPENAI_BASE_URL does not parse",
            ),
            (
                &dsn,
                "DIAGNOSIS_CALLS_PER_SESSION",
                "many",
                "DIAGNOSIS_CALLS_PER_SESSION must be a whole number, not \"many\"",
            ),
        ];
        for (dsn, name, value, sentence) in cases {
            let mut command = worker_command(dsn);
            command
                .env("WORKER_TICK_SECS", "1")
                .env("OPENAI_API_KEY", "test-key")
                .env("OPENAI_BASE_URL", "http://127.0.0.1:1/v1")
                .env(name, value);

            let run = wait_output(spawn(&mut command), 10).await;

            assert_eq!(run.code, Some(2), "{name}={value}; log:\n{}", run.log());
            assert!(
                run.stderr.contains(sentence),
                "{name}={value}; stderr:\n{}",
                run.stderr
            );
        }

        let mut command = worker_command(&dsn);
        command.env("WORKER_TICK_SECS", std::ffi::OsStr::from_bytes(b"\xff"));
        let run = wait_output(spawn(&mut command), 10).await;
        assert_eq!(run.code, Some(2), "log:\n{}", run.log());
        assert!(
            run.stderr.contains("WORKER_TICK_SECS is not valid Unicode"),
            "stderr:\n{}",
            run.stderr
        );
    })
    .await;
}

/// A database that goes away under the loop ends the process with code 2.
///
/// The worker connects as a role of its own. The test takes the role's login
/// away and ends its sessions, so the next heartbeat cannot reconnect: that is
/// the one heartbeat error that stops the loop, and the process exits 2.
#[tokio::test]
async fn binary_exits_two_when_the_database_goes_away() {
    TestDb::with(|db| async move {
        let dsn = superuser_dsn(&db.name);
        let name = db.name.clone();
        cadus_store::test_support::TestDb::with_role(
            &db,
            "worker",
            "LOGIN BYPASSRLS",
            move |db, role, pool| async move {
                pool.close().await;
                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "GRANT cadus_admin TO \"{role}\""
                )))
                .execute(&db.admin)
                .await
                .unwrap();
                let mut command = worker_command(&role_dsn(&name, &role));
                command.env("WORKER_TICK_SECS", "1");
                let child = spawn(&mut command);
                tokio::time::sleep(Duration::from_millis(1500)).await;

                sqlx::query(sqlx::AssertSqlSafe(format!(
                    "ALTER ROLE \"{role}\" NOLOGIN"
                )))
                .execute(&db.admin)
                .await
                .unwrap();
                sqlx::query(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = $1",
                )
                .bind(&role)
                .execute(&db.admin)
                .await
                .unwrap();

                let run = wait_output(child, 10).await;

                let log = run.log();
                assert_eq!(run.code, Some(2), "log:\n{log}");
                assert!(
                    log.contains("heartbeat tick=1"),
                    "the loop ran before the database went away; log:\n{log}"
                );
                assert!(
                    log.contains("cadus-worker: database error:")
                        || log.contains("cadus-worker: store error:"),
                    "the message names the database; log:\n{log}"
                );
            },
        )
        .await;
        drop(dsn);
    })
    .await;
}

/// The fixture curriculum tree of this crate is the one every binary test names.
#[test]
fn the_fixture_curriculum_is_the_pool_tree() {
    assert!(fixture_curriculum().ends_with("tests/fixtures/pool"));
}
