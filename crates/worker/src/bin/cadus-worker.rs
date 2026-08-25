//! Entry point of the Cadus background worker (R4).
//!
//! The program reads `DATABASE_URL` and `WORKER_TICK_SECS`, installs the stop
//! signals, opens a pool, logs the identity of its database role, and runs the
//! tick loop until SIGTERM or SIGINT. It exits 0 after a clean stop and 2 after
//! an error.
//!
//! The signal handlers exist before the pool opens, so a signal during the
//! connect also gives exit code 0.

use std::process::ExitCode;

use cadus_store::DbConfig;
use cadus_worker::{WorkerConfig, WorkerError};

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
    let role = cadus_store::current_role(&pool).await?;
    tracing::info!(
        role = %role.name,
        superuser = role.superuser,
        bypass_rls = role.bypass_rls,
        "cadus-worker: database role"
    );

    let ticks = cadus_worker::run(&pool, &cfg, shutdown.wait()).await?;
    pool.close().await;
    Ok(ticks)
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
