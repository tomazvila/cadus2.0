//! The fixtures of `tests/http.rs` and its parts.

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

use super::*;

/// Wrap a pool in the `Db` that `AppState` holds, with the documented default
/// client-side bound of 10000 ms.
///
/// `DEFAULT_CLIENT_TIMEOUT_MS` is the value that an absent `DB_CLIENT_TIMEOUT_MS`
/// gives, so these tests run the router exactly as the deployment does.
pub fn state_with(pool: PgPool) -> AppState {
    AppState::new(Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS))
}

/// The environment variable that holds the superuser DSN of the test cluster.
pub const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// The curriculum tree of the repository (M5 U6).
///
/// `cadus-web` loads the tree at boot and exits 2 when it does not load, so
/// every spawned binary here gets the reviewed tree. Cargo runs a test with the
/// PACKAGE directory as its working directory, so the default relative path
/// `curriculum` would resolve to `crates/web/curriculum`, which does not exist.
pub fn curriculum_dir() -> String {
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
pub fn dsn_for(database: &str, user: Option<&str>) -> String {
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
pub fn free_port() -> u16 {
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
pub struct KillOnDrop(Option<Child>);

impl KillOnDrop {
    /// Take ownership of a spawned child.
    pub fn new(child: Child) -> Self {
        Self(Some(child))
    }

    /// Borrow the child.
    pub fn as_ref(&self) -> &Child {
        self.0.as_ref().expect("the guard still holds the child")
    }

    /// Borrow the child for a call that needs it mutable.
    pub fn as_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("the guard still holds the child")
    }

    /// Give the child back for a call that consumes it, such as
    /// `wait_with_output`. The guard is empty from here, so its `Drop` does
    /// nothing.
    pub fn into_inner(mut self) -> Child {
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
pub fn process_is_alive(pid: u32) -> bool {
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
pub fn http_get(address: &str, path: &str) -> Option<(u16, String)> {
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
pub fn wait_for_exit(child: &mut Child, limit: Duration, what: &str) {
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
pub fn wait_until_healthy(child: &mut Child, address: &str) -> (u16, String) {
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
pub fn send_sigterm(child: &Child) {
    send_signal(child, "-TERM");
}

/// Send the signal `flag` names, as `kill` spells it, to one child process.
pub fn send_signal(child: &Child, flag: &str) {
    let killed = Command::new("kill")
        .arg(flag)
        .arg(child.id().to_string())
        .status()
        .expect("run kill");
    assert_eq!(killed.code(), Some(0));
}

/// The command of the binary under test: the fixture curriculum, an info log,
/// and both output pipes. The caller adds the variables of its case.
pub fn web_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cadus-web"));
    command
        .env("CADUS_CURRICULUM", curriculum_dir())
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}

/// Start `command` under the drop guard.
pub fn spawn_web(command: &mut Command) -> KillOnDrop {
    KillOnDrop::new(command.spawn().expect("start cadus-web"))
}

/// Read the exit code and the stderr of a child that ended.
pub fn collect_exit(child: KillOnDrop) -> (Option<i32>, String) {
    let output = child
        .into_inner()
        .wait_with_output()
        .expect("collect the child output");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Wait for the child to end within `limit`, then read its exit code and its
/// stderr. `what` names the case for the panic message.
pub fn exit_of(mut child: KillOnDrop, limit: Duration, what: &str) -> (Option<i32>, String) {
    wait_for_exit(child.as_mut(), limit, what);
    collect_exit(child)
}

/// `GET /api/ready` on the router over `pool`.
pub async fn ready_response(pool: PgPool) -> axum::response::Response {
    create_app(state_with(pool))
        .oneshot(
            Request::builder()
                .uri("/api/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

/// The `503` body of a readiness probe with the database down.
pub const READY_DOWN: &[u8] =
    b"{\"db\":\"down\",\"ok\":false,\"worker\":{\"claim_age_secs\":null,\"stale\":false}}";

/// Start the binary on a free port against the app role of `db`, with `extra`
/// variables. The answer is the child and the address it serves.
pub fn web_on_free_port(db: &TestDb, extra: &[(&str, &str)]) -> (KillOnDrop, String) {
    let dsn = dsn_for(&db.name, Some("cadus_app"));
    let address = format!("127.0.0.1:{}", free_port());
    let mut command = web_command();
    command.env("DATABASE_URL", &dsn).env("BIND_ADDR", &address);
    for (name, value) in extra {
        command.env(name, value);
    }
    (spawn_web(&mut command), address)
}

/// Fail the test when `response` is not the `503` of a readiness probe with
/// the database down.
pub async fn assert_ready_down(response: axum::response::Response) {
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], READY_DOWN);
}
