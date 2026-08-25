//! Proof tests for the worker skeleton (R4, C3).
//!
//! Test 1 drives the loop in process and counts the ticks. Test 2 starts the
//! real binary, stops it with SIGTERM, and reads its exit code and its log.
//!
//! Every expected value below is a literal: a literal tick count, a literal
//! exit code, a literal log line.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::time::Duration;

use cadus_store::test_support::TestDb;
use cadus_worker::WorkerConfig;

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

/// A 50 ms tick and a 180 ms run give 3 or 4 ticks. The first tick starts at
/// once, so the loop reaches 4 on an idle machine and 3 under load.
#[tokio::test]
async fn run_counts_ticks_until_shutdown() {
    let db = TestDb::create().await;
    let cfg = WorkerConfig {
        tick: Duration::from_millis(50),
    };

    let ticks = cadus_worker::run(
        &db.admin,
        &cfg,
        tokio::time::sleep(Duration::from_millis(180)),
    )
    .await
    .expect("the tick loop must not fail");

    assert!(
        ticks >= 3,
        "the loop must reach at least 3 ticks in 180 ms, it reached {ticks}"
    );
    assert!(
        ticks <= 4,
        "the loop must stop at 4 ticks or fewer in 180 ms, it reached {ticks}"
    );

    db.drop().await;
}

/// The binary starts, ticks once per second, and exits 0 on SIGTERM.
#[tokio::test]
async fn binary_ticks_and_exits_zero_on_sigterm() {
    let db = TestDb::create().await;
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

    db.drop().await;
}
