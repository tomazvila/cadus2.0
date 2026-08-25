//! Proof tests for the M0 HTTP surface and the C3 boot guard.
//!
//! Tests 1, 2, 3, and 6 drive the router and the guard in process. Tests 4, 5,
//! 7, 8, and 9 start the real binary as a child process, because an exit code
//! and a signal handler exist only in a real process.
//!
//! Every assertion names a literal value: a literal status code, literal body
//! bytes, a literal role name, a literal exit code.
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

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cadus_store::test_support::TestDb;
use cadus_store::{RoleInfo, StoreError};
use cadus_web::{AppState, boot_check, router};
use http_body_util::BodyExt;
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

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
    let app = router(AppState { pool });

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
        let app = router(AppState {
            pool: db.app.clone(),
        });

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
        assert_eq!(&body[..], b"{\"ready\":true}");
    })
    .await;
}

/// (3) C3: the guard rejects the superuser pool and accepts the app pool.
#[tokio::test]
async fn boot_check_rejects_a_role_that_bypasses_rls() {
    TestDb::with(|db| async move {
        match boot_check(&db.admin).await {
            Err(StoreError::RlsBypass { superuser, .. }) => {
                assert!(superuser, "the test cluster admin is a superuser")
            }
            Err(other) => panic!("the guard gave the wrong error: {other}"),
            Ok(info) => panic!("the guard accepted the superuser role {}", info.name),
        }

        let info = boot_check(&db.app)
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

        let mut child = Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("DATABASE_URL", &dsn)
            .env("BIND_ADDR", "127.0.0.1:0")
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web");

        wait_for_exit(&mut child, Duration::from_secs(10), "boot guard");
        let output = child.wait_with_output().expect("collect the child output");

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
/// with exit code 0.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_serves_health_and_stops_on_sigterm() {
    TestDb::with(|db| async move {
        let dsn = dsn_for(&db.name, Some("cadus_app"));
        let port = free_port();
        let address = format!("127.0.0.1:{port}");

        let mut child = Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("DATABASE_URL", &dsn)
            .env("BIND_ADDR", &address)
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web");

        let (code, body) = wait_until_healthy(&mut child, &address);
        assert_eq!(code, 200);
        assert_eq!(body, "{\"ok\":true}");

        send_sigterm(&child);

        wait_for_exit(&mut child, Duration::from_secs(5), "graceful shutdown");
        let output = child.wait_with_output().expect("collect the child output");
        assert_eq!(output.status.code(), Some(0));
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
        let app = router(AppState { pool });

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
        assert_eq!(&body[..], b"{\"ready\":false}");
    })
    .await;
}

/// (7) A client that holds a half-sent request does not block the stop.
///
/// The client sends a request line and one header, and never sends the empty
/// line that ends the headers. The drain of `axum::serve` waits for that
/// connection, so without a deadline the process never exits and the container
/// runtime kills it (finding #9). `SHUTDOWN_DEADLINE_SECS=1` bounds the drain.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn binary_exits_zero_with_a_half_sent_request_open() {
    TestDb::with(|db| async move {
        let dsn = dsn_for(&db.name, Some("cadus_app"));
        let port = free_port();
        let address = format!("127.0.0.1:{port}");

        let mut child = Command::new(env!("CARGO_BIN_EXE_cadus-web"))
            .env("DATABASE_URL", &dsn)
            .env("BIND_ADDR", &address)
            .env("SHUTDOWN_DEADLINE_SECS", "1")
            .env("RUST_LOG", "info")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start cadus-web");

        let (code, _body) = wait_until_healthy(&mut child, &address);
        assert_eq!(code, 200);

        // The half-sent request. The connection stays open for the whole test.
        let mut stalled = TcpStream::connect(&address).expect("open the stalled connection");
        write!(stalled, "GET /api/health HTTP/1.1\r\nHost: {address}\r\n")
            .expect("send half of a request");
        stalled.flush().expect("flush the stalled connection");

        send_sigterm(&child);

        wait_for_exit(&mut child, Duration::from_secs(15), "bounded shutdown");
        let output = child.wait_with_output().expect("collect the child output");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            output.status.code(),
            Some(0),
            "the server must exit 0 at the shutdown deadline; stderr:\n{stderr}"
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
    let mut child = Command::new(env!("CARGO_BIN_EXE_cadus-web"))
        .env_remove("DATABASE_URL")
        .env("BIND_ADDR", "127.0.0.1:0")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cadus-web");

    wait_for_exit(&mut child, Duration::from_secs(10), "start error");
    let output = child.wait_with_output().expect("collect the child output");

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
    let mut child = Command::new(env!("CARGO_BIN_EXE_cadus-web"))
        .env("DATABASE_URL", "postgresql://nobody@127.0.0.1:1/nodb")
        .env("BIND_ADDR", broken)
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start cadus-web");

    wait_for_exit(&mut child, Duration::from_secs(10), "bad BIND_ADDR");
    let output = child.wait_with_output().expect("collect the child output");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("BIND_ADDR"),
        "stderr does not name the variable: {stderr}"
    );
}
