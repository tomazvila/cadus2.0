//! Single-concurrency leased report review; no model call runs in a web request.

use std::process::ExitCode;
use std::time::Duration;

use cadus_store::shutdown::{Shutdown, close_within};
use cadus_store::{Db, DbConfig};
use cadus_worker::reports::{self, ReportConfig};

#[tokio::main]
async fn main() -> ExitCode {
    let _ = tracing_subscriber::fmt()
        .with_target(false)
        .with_writer(std::io::stderr)
        .try_init();
    match execute().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => {
            tracing::error!("cadus-report-worker stopped after a configuration or store failure");
            ExitCode::from(2)
        }
    }
}

async fn execute() -> Result<(), Box<dyn std::error::Error>> {
    let mut shutdown = Shutdown::install("cadus-report-worker")?;
    let config = ReportConfig::from_env()?;
    let db_config = DbConfig::from_env()?;
    let db = tokio::select! {
        () = shutdown.wait() => return Ok(()),
        result = Db::connect(&db_config) => result?,
    };
    let result = reports::run(&db, &config, shutdown.wait()).await;
    close_within(Duration::from_secs(5), db.pool().close()).await;
    result?;
    Ok(())
}
