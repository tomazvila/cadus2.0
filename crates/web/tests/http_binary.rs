//! Part of `tests/http.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::*;

use std::io::Write;
use std::net::TcpStream;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cadus_store::test_support::{DeafPostgres, TestDb};
use cadus_store::{Db, DbConfig, connect_options};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// (7) A client that holds a half-sent request does not block the stop, and the
/// whole stop stays inside one budget.
///
/// The client sends a request line and one header, and never sends the empty
/// line that ends the headers. The drain of `axum::serve` waits for that
/// connection, so without a deadline the process never exits and the container
/// runtime kills it (finding #9). `SHUTDOWN_DEADLINE_SECS=2` bounds the drain.
///
/// The test also measures SIGTERM to exit. `SHUTDOWN_DEADLINE_SECS` is one
/// budget for the drain and the pool close together, so the stop must end
/// within 2 s plus the 1 s close floor. The literal bound below is 4 s: it
/// leaves a second for a loaded machine and still fails a second full budget
/// (finding #7).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_zero_with_a_half_sent_request_open() {
    TestDb::with(|db| async move {
        let dsn = dsn_for(&db.name, Some("cadus_app"));
        let port = free_port();
        let address = format!("127.0.0.1:{port}");

        let mut child = KillOnDrop::new(
            Command::new(env!("CARGO_BIN_EXE_cadus-web"))
                .env("CADUS_CURRICULUM", curriculum_dir())
                .env("CADUS_CURRICULUM", curriculum_dir())
                .env("DATABASE_URL", &dsn)
                .env("BIND_ADDR", &address)
                .env("SHUTDOWN_DEADLINE_SECS", "2")
                .env("RUST_LOG", "info")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start cadus-web"),
        );

        let (code, _body) = wait_until_healthy(child.as_mut(), &address);
        assert_eq!(code, 200);

        // The half-sent request. The connection stays open for the whole test.
        let mut stalled = TcpStream::connect(&address).expect("open the stalled connection");
        write!(stalled, "GET /api/health HTTP/1.1\r\nHost: {address}\r\n")
            .expect("send half of a request");
        stalled.flush().expect("flush the stalled connection");

        let stop_started = Instant::now();
        send_sigterm(child.as_ref());

        wait_for_exit(child.as_mut(), Duration::from_secs(15), "bounded shutdown");
        let elapsed = stop_started.elapsed();
        let output = child
            .into_inner()
            .wait_with_output()
            .expect("collect the child output");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(0),
            "the server must exit 0 at the shutdown deadline; stderr:\n{stderr}"
        );
        assert!(
            elapsed < Duration::from_secs(4),
            "SIGTERM to exit must stay under 4 s with SHUTDOWN_DEADLINE_SECS=2, it took \
             {elapsed:?}; stderr:\n{stderr}"
        );

        drop(stalled);
    })
    .await;
}

/// (8) An absent `DATABASE_URL` is a start error: exit code exactly 2.
///
/// The module header documents "2 for a start error" and no test covered it
/// (finding #42).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_without_a_database_url() {
    let mut child = KillOnDrop::new(
        Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("CADUS_CURRICULUM", curriculum_dir())
            .env_remove("DATABASE_URL")
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web"),
    );

    wait_for_exit(child.as_mut(), Duration::from_secs(10), "start error");
    let output = child
        .into_inner()
        .wait_with_output()
        .expect("collect the child output");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("DATABASE_URL"),
        "stderr does not name the variable: {stderr}"
    );
}

/// (9) A `BIND_ADDR` that is not valid Unicode is a start error, not a silent
/// fall back to `0.0.0.0:8080` (finding #31).
///
/// The value below holds the byte `0xff`, which is not valid UTF-8. The
/// configuration check runs before the database connect, so the unreachable DSN
/// below costs the test no time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_a_bind_addr_that_is_not_unicode() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let broken = OsStr::from_bytes(b"127.0.0.1:19099\xff");
    let mut child = KillOnDrop::new(
        Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("CADUS_CURRICULUM", curriculum_dir())
            .env("DATABASE_URL", "postgresql://nobody@127.0.0.1:1/nodb")
            .env("BIND_ADDR", broken)
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web"),
    );

    wait_for_exit(child.as_mut(), Duration::from_secs(10), "bad BIND_ADDR");
    let output = child
        .into_inner()
        .wait_with_output()
        .expect("collect the child output");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("BIND_ADDR"),
        "stderr does not name the variable: {stderr}"
    );
}

/// (10) A stop signal during the C3 boot guard gives exit code 0.
///
/// The DSN points at a server that finishes the handshake and then answers no
/// query, so `boot_check` never returns. The test waits for the query to reach
/// that server, so the process is inside the guard when the signal arrives. A
/// bare `await` there makes the process deaf to SIGTERM and SIGINT for the
/// whole stall, with no listener bound and no log line (finding #7).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_zero_on_sigterm_during_the_boot_guard() {
    let deaf = DeafPostgres::start();

    let mut child = KillOnDrop::new(
        Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("CADUS_CURRICULUM", curriculum_dir())
            .env("DATABASE_URL", deaf.dsn())
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("SHUTDOWN_DEADLINE_SECS", "1")
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web"),
    );

    // Wait until the guard query reaches the deaf server. The process is then
    // inside `boot_check` and answers only through the signal path.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !deaf.query_seen() {
        assert!(
            Instant::now() < deadline,
            "cadus-web must send its boot-guard query within 10 s"
        );
        sleep(Duration::from_millis(50));
    }
    sleep(Duration::from_millis(300));

    send_sigterm(child.as_ref());

    wait_for_exit(
        child.as_mut(),
        Duration::from_secs(5),
        "stop during the boot guard",
    );
    let output = child
        .into_inner()
        .wait_with_output()
        .expect("collect the child output");
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(
        output.status.code(),
        Some(0),
        "the server must exit 0 after SIGTERM during the boot guard; stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("cadus-web: the stop signal came before the boot guard"),
        "the log must name the boot guard as the point of the stop; stderr:\n{stderr}"
    );
}

/// (11) A panic between the spawn and the stop leaves no live child.
///
/// `std::process::Child` neither kills nor reaps on drop, so the old tests left
/// a `cadus-web` process that reparented to PID 1 and held its listen port
/// forever (finding #14). The closure below spawns the binary and then panics,
/// exactly as a failed assertion does. `catch_unwind` catches the panic, so the
/// test itself passes, and `kill -0` then proves the child is gone.
///
/// The DSN points at a closed port, so the child stays inside the sqlx connect
/// for its whole 30 s acquire timeout. The child is therefore alive at the
/// moment of the panic, and only the guard can end it.
///
/// The panic message below reaches the test log. It is expected.
#[test]
fn kill_on_drop_ends_the_child_when_the_test_body_panics() {
    let pid_slot = Arc::new(AtomicU32::new(0));
    let inner = Arc::clone(&pid_slot);

    let outcome = std::panic::catch_unwind(move || {
        let child = KillOnDrop::new(
            Command::new(env!("CARGO_BIN_EXE_cadus-web"))
                .env("CADUS_CURRICULUM", curriculum_dir())
                .env("CADUS_CURRICULUM", curriculum_dir())
                .env("DATABASE_URL", "postgresql://x@127.0.0.1:1/x")
                .env("BIND_ADDR", "127.0.0.1:0")
                .env("RUST_LOG", "info")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start cadus-web"),
        );
        let pid = child.as_ref().id();
        inner.store(pid, Ordering::SeqCst);

        sleep(Duration::from_millis(300));
        assert!(process_is_alive(pid), "the child must run before the panic");

        panic!("expected panic: this stands for a failed assertion");
    });

    assert!(outcome.is_err(), "the closure must unwind");

    let pid = pid_slot.load(Ordering::SeqCst);
    assert_ne!(pid, 0, "the closure must report the pid of the child");
    assert!(
        !process_is_alive(pid),
        "the guard must kill and reap the child; pid {pid} is still there"
    );
}

/// (12) L1: `/api/ready` answers 503 inside the client-side bound when the
/// database accepts the socket and then answers nothing.
///
/// `DeafPostgres::start_silent` accepts the connection and writes nothing, so
/// the sqlx connect never finishes. `statement_timeout` cannot help here: it is
/// a server-side bound and it needs a live server. sqlx 0.9 sets no TCP
/// keepalive, so the read never ends either. Only the client-side bound of
/// `cadus_store::bounded` ends the wait, and the handler then gives the
/// documented 503.
///
/// The pool is lazy, so the connect starts inside the handler. The acquire
/// timeout of 5 s is the backstop of the test itself: it is longer than the 2 s
/// that the assertion allows, so a pass proves the 300 ms bound and not the
/// acquire timeout.
///
/// `client_timeout_ms: 300` is the value that `DB_CLIENT_TIMEOUT_MS=300` gives.
/// The unit test `the_client_timeout_reads_the_same_three_rules` in
/// `crates/store/src/lib.rs` pins that step, so this test sets the field and
/// touches no process environment: a `set_var` reaches every other test in this
/// binary.
#[tokio::test]
async fn ready_returns_503_when_the_database_answers_nothing() {
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
    let app = create_app(AppState::new(Db::new(pool.clone(), cfg.client_timeout_ms)));

    let start = Instant::now();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let elapsed = start.elapsed();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        &body[..],
        b"{\"db\":\"down\",\"ok\":false,\"worker\":{\"claim_age_secs\":null,\"stale\":false}}"
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the readiness probe took {elapsed:?}, so the client-side bound did not apply"
    );

    pool.close().await;
}

/// (13) The cookie-posture guard stops the start: a `CADUS_WEB_INSECURE_COOKIE`
/// value that is neither `0` nor `1` gives exit code exactly 2.
///
/// Spec section 3.1, row "Guards", and unit U1 of section 11. An insecure cookie
/// posture must be a deliberate choice. A silent fallback on
/// `CADUS_WEB_INSECURE_COOKIE=true` would ship the production `__Host-` cookie
/// to a developer on `http://`, and the browser would discard it without a word
/// (trap W9).
///
/// The guard runs before the database connect, so the unreachable DSN below
/// costs the test no time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_a_bad_insecure_cookie_value() {
    let mut child = KillOnDrop::new(
        Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("CADUS_CURRICULUM", curriculum_dir())
            .env("DATABASE_URL", "postgresql://nobody@127.0.0.1:1/nodb")
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("CADUS_WEB_INSECURE_COOKIE", "true")
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web"),
    );

    wait_for_exit(child.as_mut(), Duration::from_secs(10), "cookie posture");
    let output = child
        .into_inner()
        .wait_with_output()
        .expect("collect the child output");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("CADUS_WEB_INSECURE_COOKIE must be 0 or 1"),
        "stderr does not name the rule: {stderr}"
    );
}

/// (14) A `PUBLIC_ORIGIN` that is not an origin gives exit code exactly 2.
///
/// Trap W10. `PUBLIC_ORIGIN` is what takes the CSRF origin comparison off the
/// proxy header. A value with a path or a trailing slash matches no `Origin`
/// header at all, so the deployment would refuse every browser write. The
/// process refuses to start instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_2_with_a_public_origin_that_is_not_an_origin() {
    let mut child = KillOnDrop::new(
        Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("CADUS_CURRICULUM", curriculum_dir())
            .env("DATABASE_URL", "postgresql://nobody@127.0.0.1:1/nodb")
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("PUBLIC_ORIGIN", "https://tutor.example/app")
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web"),
    );

    wait_for_exit(child.as_mut(), Duration::from_secs(10), "public origin");
    let output = child
        .into_inner()
        .wait_with_output()
        .expect("collect the child output");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("PUBLIC_ORIGIN"),
        "stderr does not name the variable: {stderr}"
    );
}
