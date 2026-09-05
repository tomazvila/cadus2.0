//! The `cadus-worker` process: how a test starts it, stops it, and reads it.

use std::time::{Duration, Instant};

use cadus_testkit::process::with_role;

use super::{TEST_DSN_VAR, fixture_curriculum};

/// Build the superuser DSN of one throwaway database.
///
/// `TestDb` names the database it made. The cluster DSN points at the
/// maintenance database, so this function replaces the last path segment and
/// keeps any query string.
pub fn superuser_dsn(db_name: &str) -> String {
    let base = std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
    let (prefix, tail) = base
        .rsplit_once('/')
        .unwrap_or_else(|| panic!("{TEST_DSN_VAR} has no database path: {base}"));
    match tail.split_once('?') {
        Some((_, query)) => format!("{prefix}/{db_name}?{query}"),
        None => format!("{prefix}/{db_name}"),
    }
}

/// The DSN of one throwaway database for this cluster role.
///
/// The test cluster uses trust authentication, so the role needs no password.
pub fn role_dsn(db_name: &str, role: &str) -> String {
    with_role(&superuser_dsn(db_name), role)
}

/// A child process that never outlives the test that made it.
///
/// `tokio::process::Child` neither kills nor reaps the process on drop, so a
/// panic between the spawn and the SIGTERM left a live `cadus-worker` process
/// that reparented to PID 1 and ticked forever (finding #14). This guard kills
/// the child and reaps it on every path, the unwind path included.
pub struct KillOnDrop(Option<tokio::process::Child>);

impl KillOnDrop {
    /// Take ownership of a spawned child.
    pub fn new(child: tokio::process::Child) -> Self {
        Self(Some(child))
    }

    /// The process id of the child.
    pub fn pid(&self) -> u32 {
        self.0
            .as_ref()
            .expect("the guard still holds the child")
            .id()
            .expect("the child must report a pid")
    }

    /// Give the child back for a call that consumes it, such as
    /// `wait_with_output`. The guard is empty from here, so its `Drop` does
    /// nothing.
    pub fn into_inner(mut self) -> tokio::process::Child {
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

/// What one run of the binary produced.
pub struct Run {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Run {
    /// The whole output, stdout then stderr.
    pub fn log(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// The command that starts `cadus-worker` against this database and the
/// fixture curriculum, with the log at `info` and no color.
///
/// Every child process gets `RUST_LOG=info`. An ambient `RUST_LOG` of the
/// developer shell must not decide the result of a test (finding #12).
/// `tracing_subscriber` colors its fields on a pipe too, so a log line reaches a
/// test with escape bytes inside `output_tokens=4000`; `NO_COLOR` turns the
/// color off and leaves the text readable.
pub fn worker_command(dsn: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_cadus-worker"));
    command
        .env("DATABASE_URL", dsn)
        .env("CADUS_CURRICULUM", fixture_curriculum())
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    command
}

/// Start the worker process from this command.
pub fn spawn(command: &mut tokio::process::Command) -> KillOnDrop {
    KillOnDrop::new(command.spawn().expect("the worker binary must start"))
}

/// Wait at most `secs` seconds for the process to end, and read what it wrote.
pub async fn wait_output(child: KillOnDrop, secs: u64) -> Run {
    let output = tokio::time::timeout(
        Duration::from_secs(secs),
        child.into_inner().wait_with_output(),
    )
    .await
    .unwrap_or_else(|_| panic!("the worker must exit within {secs} s"))
    .expect("reading the worker output must succeed");
    Run {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Run `cadus-worker` with these arguments, this database and this endpoint.
///
/// `OPENAI_API_KEY` and `OPENAI_BASE_URL` always name the fake endpoint, so a
/// pass that calls a model reaches the fake and a pass that must call none is
/// caught by the fake's own counter.
pub async fn run_binary(dsn: &str, base_url: &str, args: &[&str]) -> Run {
    run_binary_with(dsn, base_url, args, &[]).await
}

/// [`run_binary`], plus these environment variables.
///
/// The pairs go in last, so a test names the exact value of a knob the run
/// reads. A knob the pairs do not name keeps the value of the shell that started
/// the test.
pub async fn run_binary_with(
    dsn: &str,
    base_url: &str,
    args: &[&str],
    env: &[(&str, &str)],
) -> Run {
    let mut command = worker_command(dsn);
    command
        .args(args)
        .env("OPENAI_API_KEY", "test-key")
        .env("OPENAI_BASE_URL", base_url)
        .env("OPENAI_MODEL", "qwen3.6");
    for (name, value) in env {
        command.env(name, value);
    }
    wait_output(spawn(&mut command), 30).await
}

/// Send this signal to the process, and demand that the kernel took it.
pub fn send_signal(pid: u32, signal: i32) {
    // SAFETY: `pid` names a child process of this test, and the process is
    // still alive because nothing reaped it yet.
    let sent = unsafe { libc::kill(pid as libc::pid_t, signal) };
    assert_eq!(sent, 0, "kill({signal}) must return 0");
}
