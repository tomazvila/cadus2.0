//! Proof tests for the two probes, the C3 boot guard, and the start sequence of
//! the binary.
//!
//! Tests 1, 2, 3, 6, and 12 drive the router and the guard in process. Tests 4,
//! 5, 7, 8, 9, 10, 13, and 14 start the real binary as a child process, because
//! an exit code and a signal handler exist only in a real process.
//!
//! The `/api/ready` body follows ruling D-M5-6 of `docs/plans/M5.md`: the 1.0
//! `redis` field is gone (2.0 has no Redis, D8) and worker liveness comes from
//! the `diagnosis_jobs` claim age. `crates/web/tests/skeleton.rs` holds the
//! tests of that field; the ones here pin the two `db` verdicts.
//!
//! Every assertion names a literal value: a literal status code, literal body
//! bytes, a literal role name, a literal exit code.
//!
//! Every child process gets `RUST_LOG=info`. An ambient `RUST_LOG` of the
//! developer shell must not decide the result of a test (finding #12).
//!
//! Every test that needs a database uses `TestDb::with`, so a failed assertion
//! drops the throwaway database instead of leaving it on the shared cluster.
//! Every spawned binary sits inside `KillOnDrop`, so a failed assertion also
//! kills and reaps the child instead of leaving a server on a live port
//! (finding #14).
//!
//! Test 10 needs no database: `cadus_store::test_support::DeafPostgres` speaks
//! the Postgres wire protocol itself and stops answering at the exact moment
//! the test wants. Test 11 needs no
//! database either: it proves the `KillOnDrop` guard on an unwind. Test 12
//! points a lazy pool at that same server and measures the readiness probe
//! against the client-side query bound (L1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cadus_store::test_support::{DeafPostgres, TestDb};
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db, DbConfig, RoleInfo, StoreError, connect_options};
use cadus_web::{AppState, boot_check, create_app};
use http_body_util::BodyExt;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// Wrap a pool in the `Db` that `AppState` holds, with the documented default
/// client-side bound of 10000 ms.
///
/// `DEFAULT_CLIENT_TIMEOUT_MS` is the value that an absent `DB_CLIENT_TIMEOUT_MS`
/// gives, so these tests run the router exactly as the deployment does.
fn state_with(pool: PgPool) -> AppState {
    AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS))
}

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// The curriculum tree of the repository (M5 U6).
///
/// `cadus-web` loads the tree at boot and exits 2 when it does not load, so
/// every spawned binary here gets the reviewed tree. Cargo runs a test with the
/// PACKAGE directory as its working directory, so the default relative path
/// `curriculum` would resolve to `crates/web/curriculum`, which does not exist.
fn curriculum_dir() -> String {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap_or_else(|| panic!("`{}` has no grandparent", manifest.display()));
    root.join("curriculum").display().to_string()
}

/// Build a DSN for one database on the test cluster.
///
/// `user` selects the role. `None` keeps the superuser credentials of
/// `CADUS_TEST_DATABASE_URL`.
fn dsn_for(database: &str, user: Option<&str>) -> String {
    let base = std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
    let (scheme, rest) = base
        .split_once("://")
        .unwrap_or_else(|| panic!("{TEST_DSN_VAR} has no scheme: {base}"));
    let authority = match rest.split_once('/') {
        Some((authority, _path)) => authority,
        None => rest,
    };
    match user {
        Some(name) => {
            let host_port = match authority.rsplit_once('@') {
                Some((_credentials, host_port)) => host_port,
                None => authority,
            };
            format!("{scheme}://{name}@{host_port}/{database}")
        }
        None => format!("{scheme}://{authority}/{database}"),
    }
}

/// Ask the operating system for a free TCP port and give it back at once.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a probe listener");
    let port = listener.local_addr().expect("local address").port();
    drop(listener);
    port
}

/// A child process that never outlives the test that made it.
///
/// `std::process::Child` neither kills nor reaps on drop. A panic between the
/// spawn and the stop therefore left a live `cadus-web` process: it reparented
/// to PID 1, kept its listen port, and answered `/api/health` forever, because
/// the liveness handler touches no database (finding #14). This guard kills the
/// child and reaps it on every path, the unwind path included.
struct KillOnDrop(Option<Child>);

impl KillOnDrop {
    /// Take ownership of a spawned child.
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// Borrow the child.
    fn as_ref(&self) -> &Child {
        self.0.as_ref().expect("the guard still holds the child")
    }

    /// Borrow the child for a call that needs it mutable.
    fn as_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("the guard still holds the child")
    }

    /// Give the child back for a call that consumes it, such as
    /// `wait_with_output`. The guard is empty from here, so its `Drop` does
    /// nothing.
    fn into_inner(mut self) -> Child {
        self.0.take().expect("the guard still holds the child")
    }
}

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            // `kill` on a process that already exited gives an error. Ignore
            // both results: the wait reaps the child either way.
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Report whether a process id still names a process.
///
/// `kill -0` sends no signal and reports the right to send one. A reaped child
/// is gone, so the command fails; an unreaped zombie still answers, which is
/// why `KillOnDrop` waits as well as kills.
fn process_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("run kill -0")
        .success()
}

/// Send one HTTP/1.0 request and return the status code and the body.
///
/// HTTP/1.0 makes the server close the connection after the answer, so a read
/// to end of file gets the whole message and needs no length parser.
fn http_get(address: &str, path: &str) -> Option<(u16, String)> {
    let mut stream = TcpStream::connect(address).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    write!(stream, "GET {path} HTTP/1.0\r\nHost: {address}\r\n\r\n").ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let text = String::from_utf8(raw).ok()?;
    let status_line = text.lines().next()?;
    let code: u16 = status_line.split(' ').nth(1)?.parse().ok()?;
    let body = text.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    Some((code, body))
}

/// Wait until the child process ends, or kill it at the deadline.
fn wait_for_exit(child: &mut Child, limit: Duration, what: &str) {
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait().expect("try_wait on the child") {
            Some(_status) => return,
            None => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("{what}: the child did not exit within {limit:?}");
                }
                sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Poll `/api/health` until the server answers, and return the answer.
///
/// The server needs a pool and a listener first, so the poll runs for at most
/// 10 seconds. A child that ends early fails the test at once.
fn wait_until_healthy(child: &mut Child, address: &str) -> (u16, String) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(early) = child.try_wait().expect("try_wait on the child") {
            let _ = child.kill();
            panic!("cadus-web exited early with {early:?}");
        }
        if let Some(result) = http_get(address, "/api/health") {
            return result;
        }
        sleep(Duration::from_millis(100));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("cadus-web did not answer /api/health within 10 s");
}

/// Send `SIGTERM` to one child process.
fn send_sigterm(child: &Child) {
    let killed = Command::new("kill")
        .arg("-TERM")
        .arg(child.id().to_string())
        .status()
        .expect("run kill");
    assert_eq!(killed.code(), Some(0));
}

/// (1) `/api/health` answers 200 with exactly `{"ok":true}`.
///
/// The pool is lazy and points at an address with no server, so the test also
/// proves that liveness does not depend on the database.
#[tokio::test]
async fn health_returns_200_and_exact_body() {
    let pool = PgPoolOptions::new()
        .connect_lazy("postgresql://nobody@127.0.0.1:1/nodb")
        .expect("a lazy pool needs no server");
    let app = create_app(state_with(pool));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok()),
        Some("application/json")
    );

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"{\"ok\":true}");
}

/// (2) `/api/ready` answers 200 with `{"ready":true}` on a live app pool.
#[tokio::test]
async fn ready_returns_200_on_a_live_pool() {
    TestDb::with(|db| async move {
        let app = create_app(state_with(db.app.clone()));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            &body[..],
            b"{\"db\":\"ok\",\"ok\":true,\"worker\":{\"claim_age_secs\":null,\"stale\":false}}"
        );
    })
    .await;
}

/// (3) C3: the guard rejects the superuser pool and accepts the app pool.
#[tokio::test]
async fn boot_check_rejects_a_role_that_bypasses_rls() {
    TestDb::with(|db| async move {
        match boot_check(&Db::new(db.admin.clone(), DEFAULT_CLIENT_TIMEOUT_MS)).await {
            Err(StoreError::RlsBypass { superuser, .. }) => {
                assert!(superuser, "the test cluster admin is a superuser")
            }
            Err(other) => panic!("the guard gave the wrong error: {other}"),
            Ok(info) => panic!("the guard accepted the superuser role {}", info.name),
        }

        let info = boot_check(&Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .await
            .expect("the guard accepts the app role");
        assert_eq!(
            info,
            RoleInfo {
                name: "cadus_app".to_string(),
                superuser: false,
                bypass_rls: false,
            }
        );
    })
    .await;
}

/// (4) The binary refuses to start with a role that bypasses row-level
/// security. The exit code is exactly 3 and stderr names the reason.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_3_with_a_superuser_dsn() {
    TestDb::with(|db| async move {
        let dsn = dsn_for(&db.name, None);

        let mut child = KillOnDrop::new(
            Command::new(env!("CARGO_BIN_EXE_cadus-web"))
                .env("CADUS_CURRICULUM", curriculum_dir())
                .env("CADUS_CURRICULUM", curriculum_dir())
                .env("DATABASE_URL", &dsn)
                .env("BIND_ADDR", "127.0.0.1:0")
                .env("RUST_LOG", "info")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start cadus-web"),
        );

        wait_for_exit(child.as_mut(), Duration::from_secs(10), "boot guard");
        let output = child
            .into_inner()
            .wait_with_output()
            .expect("collect the child output");

        assert_eq!(output.status.code(), Some(3));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("bypasses RLS"),
            "stderr does not name the reason: {stderr}"
        );
    })
    .await;
}

/// (5) The binary serves `/api/health` with the app role and stops on SIGTERM
/// with exit code 0, and its log names the port it bound.
///
/// The log line is part of the operator contract: docker-compose.yml and
/// docs/SELF_HOST.md both tell the operator that `cadus-web: listening on`
/// proves the web tier is up, and `web` carries no healthcheck for that reason.
/// The old code put the address in a structured field, so the literal never
/// appeared (finding #10).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_serves_health_and_stops_on_sigterm() {
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
                .env("RUST_LOG", "info")
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start cadus-web"),
        );

        let (code, body) = wait_until_healthy(child.as_mut(), &address);
        assert_eq!(code, 200);
        assert_eq!(body, "{\"ok\":true}");

        send_sigterm(child.as_ref());

        wait_for_exit(child.as_mut(), Duration::from_secs(5), "graceful shutdown");
        let output = child
            .into_inner()
            .wait_with_output()
            .expect("collect the child output");
        assert_eq!(output.status.code(), Some(0));

        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("listening on 127.0.0.1:"),
            "the log must carry the literal `listening on 127.0.0.1:`; stderr:\n{stderr}"
        );
        assert!(
            stderr.contains(&format!("cadus-web: listening on {address}")),
            "the log must name the bound address in the message; stderr:\n{stderr}"
        );
    })
    .await;
}

/// (6) `/api/ready` answers 503 with `{"ready":false}` when the pool is closed.
///
/// A closed pool is the deterministic stand-in for a database that does not
/// answer.
#[tokio::test]
async fn ready_returns_503_on_a_closed_pool() {
    TestDb::with(|db| async move {
        let pool = db.app.clone();
        pool.close().await;
        let app = create_app(state_with(pool));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            &body[..],
            b"{\"db\":\"down\",\"ok\":false,\"worker\":{\"claim_age_secs\":null,\"stale\":false}}"
        );
    })
    .await;
}

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
