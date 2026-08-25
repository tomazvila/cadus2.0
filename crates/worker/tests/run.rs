//! Proof tests for the worker skeleton (R4, C3).
//!
//! Tests 1, 2, and 3 drive the loop in process. Tests 4 and 5 start the real
//! binary, stop it with SIGTERM, and read its exit code and its log.
//!
//! Every expected value below is a literal: a literal tick count, a literal
//! exit code, a literal log line.
//!
//! Every child process gets `RUST_LOG=info`. An ambient `RUST_LOG` of the
//! developer shell must not decide the result of a test (finding #12).
//!
//! Every test that needs a database uses `TestDb::with`, so a failed assertion
//! drops the throwaway database instead of leaving it on the shared cluster.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::time::{Duration, Instant};

use cadus_store::test_support::TestDb;
use cadus_worker::WorkerConfig;
use sqlx::postgres::PgPoolOptions;

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// Build the superuser DSN of one throwaway database.
///
/// `TestDb` names the database it made. The cluster DSN points at the
/// maintenance database, so this function replaces the last path segment and
/// keeps any query string.
fn superuser_dsn(db_name: &str) -> String {
    let base = std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
    let (prefix, tail) = base
        .rsplit_once('/')
        .unwrap_or_else(|| panic!("{TEST_DSN_VAR} has no database path: {base}"));
    match tail.split_once('?') {
        Some((_, query)) => format!("{prefix}/{db_name}?{query}"),
        None => format!("{prefix}/{db_name}"),
    }
}

/// (1) A 50 ms tick and a 400 ms run give 3 ticks or more.
///
/// The budget holds 8 periods but the test demands 3. Each tick makes one real
/// Postgres round trip, and the test bounds neither the database latency nor the
/// scheduler, so a tight count is a wall-clock flake on a loaded machine
/// (finding #43). The upper bound of 12 keeps the test honest: a loop that
/// ignores the period, or a tick counter that counts without a tick, still
/// fails.
#[tokio::test]
async fn run_counts_ticks_until_shutdown() {
    TestDb::with(|db| async move {
        let cfg = WorkerConfig {
            tick: Duration::from_millis(50),
        };

        let ticks = cadus_worker::run(
            &db.admin,
            &cfg,
            tokio::time::sleep(Duration::from_millis(400)),
        )
        .await
        .expect("the tick loop must not fail");

        assert!(
            ticks >= 3,
            "the loop must reach at least 3 ticks in 400 ms, it reached {ticks}"
        );
        assert!(
            ticks <= 12,
            "the loop must stop at 12 ticks or fewer in 400 ms, it reached {ticks}"
        );
    })
    .await;
}

/// (2) R4: the heartbeat touches the database.
///
/// A closed pool answers every query with an error, so `run` must return `Err`.
/// A heartbeat that does no database work returns `Ok` here and the tick counter
/// alone keeps the old tests green (finding #19).
#[tokio::test]
async fn run_fails_when_the_pool_is_closed() {
    TestDb::with(|db| async move {
        let pool = db.admin.clone();
        pool.close().await;

        let cfg = WorkerConfig {
            tick: Duration::from_millis(20),
        };

        let result =
            cadus_worker::run(&pool, &cfg, tokio::time::sleep(Duration::from_secs(5))).await;

        assert!(
            result.is_err(),
            "a closed pool must make the tick loop fail, it gave {result:?}"
        );
    })
    .await;
}

/// (3) The stop signal wins while a heartbeat query is in flight.
///
/// The pool points at a closed port and keeps the sqlx default acquire timeout
/// of 30 s, so the heartbeat does not answer inside this test. The shutdown
/// future completes after 200 ms. A loop that waits for the query inside a
/// select branch body needs the whole 30 s (finding #10).
#[tokio::test]
async fn shutdown_wins_over_a_heartbeat_that_does_not_answer() {
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(30))
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    let cfg = WorkerConfig {
        tick: Duration::from_millis(10),
    };

    let start = Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        cadus_worker::run(&pool, &cfg, tokio::time::sleep(Duration::from_millis(200))),
    )
    .await;
    let elapsed = start.elapsed();

    let ticks = outcome
        .expect("run must return within 5 s")
        .expect("the tick loop must not fail");
    assert_eq!(ticks, 0, "no heartbeat completed, so the count must be 0");
    assert!(
        elapsed < Duration::from_secs(2),
        "run must return within 2 s of the stop signal, it took {elapsed:?}"
    );
}

/// (4) The binary starts, ticks once per second, and exits 0 on SIGTERM.
#[tokio::test]
async fn binary_ticks_and_exits_zero_on_sigterm() {
    TestDb::with(|db| async move {
        let dsn = superuser_dsn(&db.name);

        let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"))
            .env("DATABASE_URL", &dsn)
            .env("WORKER_TICK_SECS", "1")
            .env("RUST_LOG", "info")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the worker binary must start");

        let pid = child.id().expect("the child must report a pid");

        tokio::time::sleep(Duration::from_millis(2500)).await;

        // SAFETY: `pid` names a child process of this test, and the process is
        // still alive because nothing reaped it yet.
        let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        assert_eq!(sent, 0, "kill(SIGTERM) must return 0");

        let output = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output())
            .await
            .expect("the worker must exit within 10 s after SIGTERM")
            .expect("reading the worker output must succeed");

        let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
        log.push_str(&String::from_utf8_lossy(&output.stderr));

        assert_eq!(
            output.status.code(),
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

/// (5) A stop signal during the database connect gives exit code 0.
///
/// The DSN points at a closed port, so the process stays inside `connect` for
/// the whole sqlx acquire timeout of 30 s. The signal handlers install before
/// the connect, so the SIGTERM at 300 ms is handled. A process that installs
/// them later dies by the signal and reports no exit code at all (finding #39).
#[tokio::test]
async fn binary_exits_zero_on_sigterm_during_the_connect() {
    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"))
        .env("DATABASE_URL", "postgresql://x@127.0.0.1:1/x")
        .env("WORKER_TICK_SECS", "1")
        .env("RUST_LOG", "info")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("the worker binary must start");

    let pid = child.id().expect("the child must report a pid");

    tokio::time::sleep(Duration::from_millis(300)).await;

    // SAFETY: `pid` names a child process of this test, and the process is
    // still alive because nothing reaped it yet.
    let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    assert_eq!(sent, 0, "kill(SIGTERM) must return 0");

    let output = tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .expect("the worker must exit within 15 s after SIGTERM")
        .expect("reading the worker output must succeed");

    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&output.stderr));

    assert_eq!(
        output.status.code(),
        Some(0),
        "the worker must exit 0 after a SIGTERM during the connect; log:\n{log}"
    );
}
