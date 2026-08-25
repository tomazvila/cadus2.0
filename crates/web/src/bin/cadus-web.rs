//! Entry point of the Cadus HTTP server.
//!
//! The start sequence is:
//!
//! 1. Start the tracing subscriber. `RUST_LOG` selects the level.
//! 2. Read `DATABASE_URL` and open the connection pool.
//! 3. Run the C3 boot guard. A role that bypasses row-level security stops the
//!    process with exit code 3.
//! 4. Bind `BIND_ADDR` (default `0.0.0.0:8080`) and serve.
//! 5. Stop on `SIGTERM` or `Ctrl-C`, let the open requests finish, and exit 0.
//!
//! Exit codes: 0 for a clean stop, 2 for a start error, 3 for the boot guard.

use std::net::SocketAddr;
use std::process::ExitCode;

use cadus_store::{DbConfig, StoreError};
use cadus_web::{AppState, router};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// The address to bind when `BIND_ADDR` is absent.
const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";

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
            ExitCode::from(3)
        }
        Err(Fatal::Startup(message)) => {
            tracing::error!("cadus-web: {message}");
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
    let pool = cadus_store::connect(&cfg)
        .await
        .map_err(|err| Fatal::Startup(err.to_string()))?;

    // C3: stop here if row-level security does not apply to this role.
    match cadus_web::boot_check(&pool).await {
        Ok(info) => tracing::info!(role = %info.name, "cadus-web: the boot guard passed"),
        Err(StoreError::RlsBypass { role, .. }) => return Err(Fatal::RlsBypass { role }),
        Err(err) => return Err(Fatal::Startup(err.to_string())),
    }

    let addr = bind_addr()?;
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|err| Fatal::Startup(format!("bind {addr} failed: {err}")))?;
    let local = listener
        .local_addr()
        .map_err(|err| Fatal::Startup(format!("local address of the listener failed: {err}")))?;
    tracing::info!(address = %local, "cadus-web: listening");

    let app = router(AppState { pool: pool.clone() });
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    pool.close().await;
    result.map_err(|err| Fatal::Startup(format!("serve failed: {err}")))
}

/// Read `BIND_ADDR`, or use the default.
fn bind_addr() -> Result<SocketAddr, Fatal> {
    let raw = std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());
    raw.parse()
        .map_err(|err| Fatal::Startup(format!("BIND_ADDR {raw} is not a socket address: {err}")))
}

/// Wait for `SIGTERM` or `Ctrl-C`.
///
/// The function returns on the first of the two signals. `axum::serve` then
/// stops the accept loop and lets the open requests finish.
async fn shutdown_signal() {
    let ctrl_c = async {
        match tokio::signal::ctrl_c().await {
            Ok(()) => tracing::info!("cadus-web: Ctrl-C received"),
            Err(err) => {
                tracing::error!(error = %err, "cadus-web: the Ctrl-C handler failed");
                // The handler is gone. Park this branch, so the SIGTERM branch
                // stays in charge instead of an immediate shutdown.
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
                tracing::info!("cadus-web: SIGTERM received");
            }
            Err(err) => {
                tracing::error!(error = %err, "cadus-web: the SIGTERM handler failed");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }

    tracing::info!("cadus-web: graceful shutdown starts");
}
