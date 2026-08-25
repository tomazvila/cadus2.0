//! Entry point of the Cadus HTTP server.
//!
//! The start sequence is:
//!
//! 1. Start the tracing subscriber. `RUST_LOG` selects the level.
//! 2. Read `DATABASE_URL`, `BIND_ADDR` (default `0.0.0.0:8080`), and
//!    `SHUTDOWN_DEADLINE_SECS` (default 10).
//! 3. Install the stop signals. The handlers exist before the pool opens, so a
//!    signal during the connect gives a clean stop.
//! 4. Open the connection pool.
//! 5. Run the C3 boot guard. A role that bypasses row-level security stops the
//!    process with exit code 3.
//! 6. Bind the address and serve.
//! 7. Stop on `SIGTERM` or `SIGINT`, let the open requests finish, and exit 0.
//!    The drain has a deadline: at the deadline the process closes the open
//!    connections and still exits 0.
//!
//! Exit codes: 0 for a clean stop, 2 for a start error, 3 for the boot guard.

#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

use std::future::{Future, IntoFuture};
use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use cadus_store::{DbConfig, StoreError};
use cadus_web::{AppState, router};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// The environment variable that holds the listen address.
const BIND_ADDR_VAR: &str = "BIND_ADDR";

/// The address to bind when `BIND_ADDR` is absent.
const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";

/// The environment variable that bounds the drain after the stop signal.
const SHUTDOWN_DEADLINE_VAR: &str = "SHUTDOWN_DEADLINE_SECS";

/// The drain deadline in seconds when `SHUTDOWN_DEADLINE_SECS` is absent.
const DEFAULT_SHUTDOWN_DEADLINE_SECS: u64 = 10;

/// The reason that stops the start sequence.
enum Fatal {
    /// C3: the database role escapes row-level security. Exit code 3.
    RlsBypass { role: String },
    /// Any other start error. Exit code 2.
    Startup(String),
}

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    match run().await {
        Ok(()) => {
            tracing::info!("cadus-web: stopped");
            ExitCode::SUCCESS
        }
        Err(Fatal::RlsBypass { role }) => {
            tracing::error!("refusing to start: role {role} bypasses RLS");
            eprintln!("cadus-web: refusing to start: role {role} bypasses RLS");
            ExitCode::from(3)
        }
        Err(Fatal::Startup(message)) => {
            tracing::error!("cadus-web: {message}");
            eprintln!("cadus-web: {message}");
            ExitCode::from(2)
        }
    }
}

/// Send every log line to stderr, so stdout stays free for program output.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

async fn run() -> Result<(), Fatal> {
    let cfg = DbConfig::from_env().map_err(|err| Fatal::Startup(err.to_string()))?;
    // Read every configuration value before the pool opens. A bad value then
    // stops the process at once instead of after the connect timeout.
    let addr = bind_addr()?;
    let deadline = shutdown_deadline()?;

    // Install the stop signals before the connect. The handlers exist from this
    // point, so a SIGTERM during the connect gives exit code 0 instead of a kill
    // by signal (finding #39).
    let mut shutdown = Shutdown::install()?;

    let pool = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-web: the stop signal came before the database connect");
            return Ok(());
        }
        result = cadus_store::connect(&cfg) => {
            result.map_err(|err| Fatal::Startup(err.to_string()))?
        }
    };

    // C3: stop here if row-level security does not apply to this role.
    //
    // The guard runs inside the same select as the connect above. A database
    // that accepts the connection and then answers no query made the old code
    // deaf to SIGTERM and SIGINT for the whole stall (finding #7).
    //
    // The stop branch returns without a pool close on purpose. The process ends
    // at that return, so the operating system closes the sockets. A wait for a
    // database that answers nothing only delays the stop the operator asked for.
    let guard = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-web: the stop signal came before the boot guard");
            return Ok(());
        }
        result = cadus_web::boot_check(&pool) => result,
    };
    match guard {
        Ok(info) => tracing::info!(role = %info.name, "cadus-web: the boot guard passed"),
        Err(StoreError::RlsBypass { role, .. }) => return Err(Fatal::RlsBypass { role }),
        Err(err) => return Err(Fatal::Startup(err.to_string())),
    }

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|err| Fatal::Startup(format!("bind {addr} failed: {err}")))?;
    let local = listener
        .local_addr()
        .map_err(|err| Fatal::Startup(format!("local address of the listener failed: {err}")))?;
    tracing::info!(address = %local, "cadus-web: listening");

    let app = router(AppState { pool: pool.clone() });

    // `fired_rx` reports the moment of the stop signal, so the deadline below
    // starts at the signal and not at the start of the process.
    let (fired_tx, fired_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut shutdown = shutdown;
            shutdown.wait().await;
            tracing::info!("cadus-web: graceful shutdown starts");
            let _ = fired_tx.send(());
        })
        .into_future();
    let mut server = std::pin::pin!(server);

    // The drain has a deadline. Without one, a client that opened a request and
    // never finished the headers keeps the process alive without end, and the
    // container runtime kills it (finding #9).
    let result = tokio::select! {
        outcome = &mut server => outcome,
        _ = fired_rx => match tokio::time::timeout(deadline, &mut server).await {
            Ok(outcome) => outcome,
            Err(_elapsed) => {
                tracing::info!("shutdown deadline reached; closing");
                Ok(())
            }
        },
    };

    close_within(deadline, pool.close()).await;
    result.map_err(|err| Fatal::Startup(format!("serve failed: {err}")))
}

/// Wait for `close` for at most `deadline`, then log the fact and give up.
///
/// `PgPool::close` waits for every checked-out connection to come back. A
/// database that answers nothing never gives one back, so the plain call runs
/// without end and defeats the drain deadline above: the process logs
/// `shutdown deadline reached; closing` and then stays alive until the
/// container runtime sends SIGKILL (finding #6). The bound below keeps the exit
/// inside the same budget. The process exits 0 either way, because the open
/// sockets end with the process.
async fn close_within<F: Future<Output = ()>>(deadline: Duration, close: F) {
    if tokio::time::timeout(deadline, close).await.is_err() {
        tracing::info!("pool close deadline reached");
    }
}

/// Read `BIND_ADDR`, or use the default.
///
/// A value that is not valid Unicode is a start error. The old code sent that
/// value to the default and bound every interface without a word (finding #31).
fn bind_addr() -> Result<SocketAddr, Fatal> {
    let raw = match std::env::var(BIND_ADDR_VAR) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => DEFAULT_BIND_ADDR.to_string(),
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Fatal::Startup(format!(
                "{BIND_ADDR_VAR} is not valid Unicode"
            )));
        }
    };
    raw.parse().map_err(|err| {
        Fatal::Startup(format!(
            "{BIND_ADDR_VAR} {raw} is not a socket address: {err}"
        ))
    })
}

/// Read `SHUTDOWN_DEADLINE_SECS`, or use the default of 10 seconds.
///
/// A present value that is not a positive whole number of seconds is a start
/// error, because a silent fallback hides an operator mistake.
fn shutdown_deadline() -> Result<Duration, Fatal> {
    let raw = match std::env::var(SHUTDOWN_DEADLINE_VAR) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => {
            return Ok(Duration::from_secs(DEFAULT_SHUTDOWN_DEADLINE_SECS));
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Fatal::Startup(format!(
                "{SHUTDOWN_DEADLINE_VAR} is not valid Unicode"
            )));
        }
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} is empty; give a whole number of seconds or remove the \
             variable"
        )));
    }
    let secs: u64 = trimmed.parse().map_err(|_| {
        Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} must be a whole number of seconds, not {trimmed:?}"
        ))
    })?;
    if secs == 0 {
        return Err(Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} must be 1 or more"
        )));
    }
    Ok(Duration::from_secs(secs))
}

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal.
struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    #[cfg(unix)]
    fn install() -> Result<Self, Fatal> {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = signal(SignalKind::terminate())
            .map_err(|err| Fatal::Startup(format!("the SIGTERM handler failed: {err}")))?;
        let interrupt = signal(SignalKind::interrupt())
            .map_err(|err| Fatal::Startup(format!("the SIGINT handler failed: {err}")))?;
        Ok(Self {
            terminate,
            interrupt,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    fn install() -> Result<Self, Fatal> {
        Ok(Self {})
    }

    /// Complete on the first `SIGTERM` or `SIGINT`.
    #[cfg(unix)]
    async fn wait(&mut self) {
        let Self {
            terminate,
            interrupt,
        } = self;
        tokio::select! {
            _ = terminate.recv() => tracing::info!("cadus-web: SIGTERM received"),
            _ = interrupt.recv() => tracing::info!("cadus-web: SIGINT received"),
        }
    }

    /// Complete on Ctrl-C. A platform without unix signals has no `SIGTERM`.
    #[cfg(not(unix))]
    async fn wait(&mut self) {
        match tokio::signal::ctrl_c().await {
            Ok(()) => tracing::info!("cadus-web: Ctrl-C received"),
            Err(err) => {
                tracing::error!(error = %err, "cadus-web: the Ctrl-C handler failed");
                // The handler is gone. Park here, so the process keeps serving
                // instead of a stop at once.
                std::future::pending::<()>().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    /// `close_within` returns at its deadline, even when the close never ends.
    ///
    /// `PgPool::close` waits for every checked-out connection, so a database
    /// that answers nothing makes the plain call run without end (finding #6).
    /// The never-resolving future below stands for that case. The outer timeout
    /// of 5 s fails the test when the bound is gone.
    #[tokio::test]
    async fn close_within_returns_at_the_deadline() {
        let start = Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            super::close_within(Duration::from_millis(200), std::future::pending::<()>()),
        )
        .await;
        let elapsed = start.elapsed();

        assert!(
            outcome.is_ok(),
            "close_within must return within 5 s, it took {elapsed:?} or more"
        );
        assert!(
            elapsed >= Duration::from_millis(200),
            "close_within must wait for the whole deadline, it took {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "close_within must return soon after the deadline, it took {elapsed:?}"
        );
    }
}
