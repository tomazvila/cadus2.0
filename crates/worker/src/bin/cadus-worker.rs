//! Entry point of the Cadus background worker (R4).
//!
//! The program reads `DATABASE_URL`, opens a pool, logs the identity of its
//! database role, and runs the tick loop until SIGTERM or Ctrl-C. It exits 0
//! after a clean stop and 2 after an error.

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
    let pool = cadus_store::connect(&db).await?;

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

    let ticks = cadus_worker::run(&pool, &cfg, shutdown_signal()).await?;
    pool.close().await;
    Ok(ticks)
}

/// Complete on SIGTERM or on Ctrl-C. `docker stop` sends SIGTERM, so this is the
/// normal stop path of the deployment.
async fn shutdown_signal() {
    let ctrl_c = async {
        if tokio::signal::ctrl_c().await.is_err() {
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            // The handler did not install. Leave the Ctrl-C branch to stop us.
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("cadus-worker: Ctrl-C received"),
        _ = terminate => tracing::info!("cadus-worker: SIGTERM received"),
    }
}
