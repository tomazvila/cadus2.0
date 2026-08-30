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
//! Every spawned binary sits inside `KillOnDrop`, so a failed assertion also
//! kills and reaps the child instead of leaving a worker process behind
//! (finding #14).
//!
//! Test 6 and test 7 need no database:
//! `cadus_store::test_support::DeafPostgres` speaks the Postgres wire protocol
//! itself and stops answering at the exact moment the test wants. Test 7 also
//! reads the log of the loop with a tracing subscriber of its own (L1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cadus_store::test_support::{DeafPostgres, TestDb};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, DbConfig, connect_options};
use cadus_worker::WorkerConfig;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Wrap a pool in the `Db` that `run` takes, with the documented default
/// client-side bound of 10000 ms.
///
/// `DEFAULT_CLIENT_TIMEOUT_MS` is the value that an absent `DB_CLIENT_TIMEOUT_MS`
/// gives, so these tests run the loop exactly as the deployment does.
fn db_with(pool: PgPool) -> Db {
    Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS)
}

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// A child process that never outlives the test that made it.
///
/// `tokio::process::Child` neither kills nor reaps the process on drop, so a
/// panic between the spawn and the SIGTERM left a live `cadus-worker` process
/// that reparented to PID 1 and ticked forever (finding #14). This guard kills
/// the child and reaps it on every path, the unwind path included.
struct KillOnDrop(Option<tokio::process::Child>);

impl KillOnDrop {
    /// Take ownership of a spawned child.
    fn new(child: tokio::process::Child) -> Self {
        Self(Some(child))
    }

    /// Borrow the child.
    fn as_ref(&self) -> &tokio::process::Child {
        self.0.as_ref().expect("the guard still holds the child")
    }

    /// Give the child back for a call that consumes it, such as
    /// `wait_with_output`. The guard is empty from here, so its `Drop` does
    /// nothing.
    fn into_inner(mut self) -> tokio::process::Child {
        self.0.take().expect("the guard still holds the child")
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let Some(mut child) = self.0.take() else {
            return;
        };
        // `start_kill` sends SIGKILL and returns at once. `try_wait` then reaps
        // the child. A short poll is enough: SIGKILL is not catchable.
        let _ = child.start_kill();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
}

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
            &db_with(db.admin.clone()),
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

        let result = cadus_worker::run(
            &db_with(pool),
            &cfg,
            tokio::time::sleep(Duration::from_secs(5)),
        )
        .await;

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
        cadus_worker::run(
            &db_with(pool),
            &cfg,
            tokio::time::sleep(Duration::from_millis(200)),
        ),
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

        let child = KillOnDrop::new(
            tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"))
                .env("DATABASE_URL", &dsn)
                .env("WORKER_TICK_SECS", "1")
                .env("CADUS_CURRICULUM", fixture_curriculum())
                .env("RUST_LOG", "info")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("the worker binary must start"),
        );

        let pid = child.as_ref().id().expect("the child must report a pid");

        tokio::time::sleep(Duration::from_millis(2500)).await;

        // SAFETY: `pid` names a child process of this test, and the process is
        // still alive because nothing reaped it yet.
        let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        assert_eq!(sent, 0, "kill(SIGTERM) must return 0");

        let output = tokio::time::timeout(
            Duration::from_secs(10),
            child.into_inner().wait_with_output(),
        )
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

/// (4b) The diagnosis job keeps the DIAGNOSIS budget of 600 and 600 (T5,
/// finding F18).
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
        let dsn = superuser_dsn(&db.name);

        let child = KillOnDrop::new(
            tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"))
                .env("DATABASE_URL", &dsn)
                .env("WORKER_TICK_SECS", "1")
                .env("CADUS_CURRICULUM", fixture_curriculum())
                .env("OPENAI_API_KEY", "test-key")
                .env("OPENAI_BASE_URL", "http://127.0.0.1:1/v1")
                .env("OPENAI_MODEL", "qwen3.6")
                .env("RUST_LOG", "info")
                // `tracing_subscriber` colors its fields on a pipe too, so the
                // line reaches this test with escape bytes inside
                // `output_tokens=600`. `NO_COLOR` turns the color off.
                .env("NO_COLOR", "1")
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .expect("the worker binary must start"),
        );

        let pid = child.as_ref().id().expect("the child must report a pid");
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // SAFETY: `pid` names a child process of this test, and the process is
        // still alive because nothing reaped it yet.
        let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
        assert_eq!(sent, 0, "kill(SIGTERM) must return 0");

        let output = tokio::time::timeout(
            Duration::from_secs(10),
            child.into_inner().wait_with_output(),
        )
        .await
        .expect("the worker must exit within 10 s after SIGTERM")
        .expect("reading the worker output must succeed");

        let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
        log.push_str(&String::from_utf8_lossy(&output.stderr));

        assert!(
            log.contains("cadus-worker: the diagnosis job is configured"),
            "the log must hold the configuration line; log:\n{log}"
        );
        assert!(
            log.contains("output_tokens=600 reasoning_max_tokens=600"),
            "the diagnosis job must keep the 600 and 600 ceilings; log:\n{log}"
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
    let child = KillOnDrop::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"))
            .env("DATABASE_URL", "postgresql://x@127.0.0.1:1/x")
            .env("WORKER_TICK_SECS", "1")
            .env("CADUS_CURRICULUM", fixture_curriculum())
            .env("RUST_LOG", "info")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the worker binary must start"),
    );

    let pid = child.as_ref().id().expect("the child must report a pid");

    tokio::time::sleep(Duration::from_millis(300)).await;

    // SAFETY: `pid` names a child process of this test, and the process is
    // still alive because nothing reaped it yet.
    let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    assert_eq!(sent, 0, "kill(SIGTERM) must return 0");

    let output = tokio::time::timeout(
        Duration::from_secs(15),
        child.into_inner().wait_with_output(),
    )
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

    let child = KillOnDrop::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"))
            .env("DATABASE_URL", deaf.dsn())
            .env("WORKER_TICK_SECS", "1")
            .env("CADUS_CURRICULUM", fixture_curriculum())
            .env("RUST_LOG", "info")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the worker binary must start"),
    );

    let pid = child.as_ref().id().expect("the child must report a pid");

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

    // SAFETY: `pid` names a child process of this test, and the process is
    // still alive because nothing reaped it yet.
    let sent = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) };
    assert_eq!(sent, 0, "kill(SIGTERM) must return 0");

    let output = tokio::time::timeout(
        Duration::from_secs(5),
        child.into_inner().wait_with_output(),
    )
    .await
    .expect("the worker must exit within 5 s after SIGTERM during the role report")
    .expect("reading the worker output must succeed");

    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&output.stderr));

    assert_eq!(
        output.status.code(),
        Some(0),
        "the worker must exit 0 after SIGTERM during the role report; log:\n{log}"
    );
    assert!(
        log.contains("cadus-worker: the stop signal came before the role report"),
        "the log must name the role report as the point of the stop; log:\n{log}"
    );
}

/// The fixture curriculum tree of this crate.
///
/// The worker refuses to start without a curriculum, so every binary test names
/// one. The tree is the same fixture `tests/refill.rs` reads.
fn fixture_curriculum() -> String {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pool")
        .to_string_lossy()
        .into_owned()
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
#[tokio::test]
async fn binary_exits_two_when_the_curriculum_does_not_load() {
    let missing = "/home/deploy/dev/cadus2.0/no-such-curriculum";
    let child = KillOnDrop::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"))
            .env("DATABASE_URL", "postgresql://x@127.0.0.1:1/x")
            .env("WORKER_TICK_SECS", "1")
            .env("CADUS_CURRICULUM", missing)
            .env("RUST_LOG", "info")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .expect("the worker binary must start"),
    );

    let output = tokio::time::timeout(
        Duration::from_secs(10),
        child.into_inner().wait_with_output(),
    )
    .await
    .expect("the worker must exit within 10 s")
    .expect("reading the worker output must succeed");

    let mut log = String::from_utf8_lossy(&output.stdout).into_owned();
    log.push_str(&String::from_utf8_lossy(&output.stderr));

    assert_eq!(
        output.status.code(),
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

/// A writer that keeps every log byte in memory.
///
/// `tracing_subscriber::fmt` needs a `MakeWriter`. This one hands out a clone of
/// itself, and every clone appends to the same buffer, so the test reads the
/// whole log after the loop stops.
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    /// The captured log as one string.
    fn text(&self) -> String {
        let bytes = self.0.lock().expect("the capture lock is not poisoned");
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl std::io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("the capture lock is not poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Capture;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// (7) L1: a heartbeat that does not answer logs a warning and the loop goes on.
///
/// `DeafPostgres::start_silent` accepts the connection and writes nothing, so
/// the sqlx connect never finishes. Only the client-side bound of
/// `cadus_store::bounded` ends the wait. A stalled database must not kill the
/// worker: the loop logs `heartbeat timed out after 300 ms` at warn level and
/// takes the next tick, and it still returns when the shutdown future resolves.
///
/// The pool is lazy, so the connect starts inside the heartbeat. The acquire
/// timeout of 5 s is the backstop of the test itself: it is longer than the 1 s
/// run, so a warning proves the 300 ms bound and not the acquire timeout.
///
/// `client_timeout_ms: 300` is the value that `DB_CLIENT_TIMEOUT_MS=300` gives.
/// The unit test `the_client_timeout_reads_the_same_three_rules` in
/// `crates/store/src/lib.rs` pins that step, so this test sets the field and
/// touches no process environment: a `set_var` reaches every other test in this
/// binary.
///
/// `tracing::subscriber::set_default` binds the subscriber to THIS thread only,
/// so the other tests of this binary keep their own log.
#[tokio::test]
async fn run_logs_a_heartbeat_timeout_and_keeps_ticking() {
    let deaf = DeafPostgres::start_silent();
    let cfg = DbConfig {
        database_url: deaf.dsn(),
        statement_timeout_ms: 0,
        client_timeout_ms: 300,
    };
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_lazy_with(connect_options(&cfg).expect("the deaf DSN parses"));
    let db = Db::new(pool.clone(), cfg.client_timeout_ms);

    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let recorder = tracing::subscriber::set_default(subscriber);

    let start = Instant::now();
    let outcome = tokio::time::timeout(
        Duration::from_secs(5),
        cadus_worker::run(
            &db,
            &WorkerConfig {
                tick: Duration::from_millis(50),
            },
            tokio::time::sleep(Duration::from_secs(1)),
        ),
    )
    .await;
    let elapsed = start.elapsed();
    drop(recorder);

    let log = capture.text();
    let ticks = outcome
        .expect("run must return within 5 s")
        .expect("a heartbeat timeout must not fail the loop");

    assert_eq!(ticks, 0, "no heartbeat answered, so the count must be 0");
    assert!(
        log.contains("heartbeat timed out after 300 ms"),
        "the log must hold the literal `heartbeat timed out after 300 ms`; log:\n{log}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "run must return soon after the 1 s shutdown, it took {elapsed:?}"
    );

    pool.close().await;
}
