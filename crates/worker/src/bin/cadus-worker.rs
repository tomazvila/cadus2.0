//! Entry point of the Cadus background worker (R4).
//!
//! The program reads `DATABASE_URL` and `WORKER_TICK_SECS`, installs the stop
//! signals, opens a pool, logs the identity of its database role, and runs the
//! tick loop until SIGTERM or SIGINT. It exits 0 after a clean stop and 2 after
//! an error.
//!
//! The signal handlers exist before the pool opens, so a signal during the
//! connect also gives exit code 0. The role report runs under the same signal
//! guard, and the pool close after the loop has a deadline.

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

use std::future::Future;
use std::process::ExitCode;
use std::time::Duration;

use cadus_store::DbConfig;
use cadus_worker::{WorkerConfig, WorkerError};

/// The bound on the pool close after the tick loop stops.
///
/// The worker reads no deadline from the environment, so the budget is a
/// constant. `cadus-web` uses `SHUTDOWN_DEADLINE_SECS` for the same job.
const POOL_CLOSE_DEADLINE: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    match run().await {
        Ok(ticks) => {
            tracing::info!("cadus-worker: stop after {ticks} ticks");
            ExitCode::SUCCESS
        }
        Err(err) => {
            tracing::error!("cadus-worker: {err}");
            eprintln!("cadus-worker: {err}");
            ExitCode::from(2)
        }
    }
}

/// Send the log to stderr. `RUST_LOG` overrides the default level.
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

async fn run() -> Result<u64, WorkerError> {
    let db = DbConfig::from_env()?;
    let cfg = WorkerConfig::from_env()?;

    // Install the stop signals before the connect. The handlers exist from this
    // point, so a SIGTERM during the connect gives exit code 0 instead of a kill
    // by signal (finding #39).
    let mut shutdown = Shutdown::install()?;

    let pool = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-worker: the stop signal came before the database connect");
            return Ok(0);
        }
        result = cadus_store::connect(&db) => result?,
    };

    // The worker connects as `cadus_admin`. That role holds BYPASSRLS by design:
    // it claims `diagnosis_jobs` and refills `serving_pool` across every tenant,
    // so a tenant policy would hide the rows it must process. For that reason the
    // worker logs the role but does NOT call `assert_rls_enforced`. The C3 boot
    // guard belongs to the request tier (`cadus-web`), which connects as
    // `cadus_app` and must stay inside row-level security.
    //
    // The report runs inside the same select as the connect above. A database
    // that accepts the connection and then answers no query made the old code
    // deaf to SIGTERM for the whole stall (finding #8).
    //
    // The stop branch returns without a pool close on purpose. The process ends
    // at that return, so the operating system closes the sockets. A wait for a
    // database that answers nothing only delays the stop the operator asked for.
    let role = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-worker: the stop signal came before the role report");
            return Ok(0);
        }
        result = cadus_store::current_role(&pool) => result?,
    };
    tracing::info!(
        role = %role.name,
        superuser = role.superuser,
        bypass_rls = role.bypass_rls,
        "cadus-worker: database role"
    );

    let ticks = cadus_worker::run(&pool, &cfg, shutdown.wait()).await?;
    close_within(POOL_CLOSE_DEADLINE, pool.close()).await;
    Ok(ticks)
}

/// Wait for `close` for at most `deadline`, then log the fact and give up.
///
/// `PgPool::close` waits for every checked-out connection to come back. A
/// database that answers nothing never gives one back, so the plain call runs
/// without end and the process stays alive after the stop signal until the
/// container runtime sends SIGKILL (finding #6). The bound below keeps the exit
/// inside the budget. The process exits 0 either way, because the open sockets
/// end with the process.
async fn close_within<F: Future<Output = ()>>(deadline: Duration, close: F) {
    if tokio::time::timeout(deadline, close).await.is_err() {
        tracing::info!("pool close deadline reached");
    }
}

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal. `docker stop`
/// sends SIGTERM, so that is the normal stop path of the deployment.
struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    #[cfg(unix)]
    fn install() -> Result<Self, WorkerError> {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = signal(SignalKind::terminate())
            .map_err(|err| WorkerError::Signal(format!("the SIGTERM handler failed: {err}")))?;
        let interrupt = signal(SignalKind::interrupt())
            .map_err(|err| WorkerError::Signal(format!("the SIGINT handler failed: {err}")))?;
        Ok(Self {
            terminate,
            interrupt,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    fn install() -> Result<Self, WorkerError> {
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
            _ = terminate.recv() => tracing::info!("cadus-worker: SIGTERM received"),
            _ = interrupt.recv() => tracing::info!("cadus-worker: SIGINT received"),
        }
    }

    /// Complete on Ctrl-C. A platform without unix signals has no `SIGTERM`.
    #[cfg(not(unix))]
    async fn wait(&mut self) {
        match tokio::signal::ctrl_c().await {
            Ok(()) => tracing::info!("cadus-worker: Ctrl-C received"),
            Err(err) => {
                tracing::error!(error = %err, "cadus-worker: the Ctrl-C handler failed");
                // The handler is gone. Park here, so the loop keeps running
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
