//! Background worker: model calls and pool generation run here, never in a request handler (R4).
//!
//! M0 gives the process its skeleton only: configuration, a tick loop, a
//! heartbeat query, and a clean stop. The queue work arrives in later
//! milestones. See the doc comment on [`run`].

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
use std::time::Duration;

use sqlx::PgPool;

/// The environment variable that holds the tick period in whole seconds.
const TICK_SECS_VAR: &str = "WORKER_TICK_SECS";

/// The tick period that the worker uses when the environment sets none.
const DEFAULT_TICK_SECS: u64 = 5;

/// The runtime configuration of the worker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub tick: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(DEFAULT_TICK_SECS),
        }
    }
}

impl WorkerConfig {
    /// Read `WORKER_TICK_SECS` from the environment.
    ///
    /// An absent variable gives the default of 5 seconds. A present value that
    /// is not a positive whole number of seconds is a configuration error,
    /// because a silent fallback hides an operator mistake.
    pub fn from_env() -> Result<Self, WorkerError> {
        let raw = match std::env::var(TICK_SECS_VAR) {
            Ok(raw) => raw,
            Err(std::env::VarError::NotPresent) => return Ok(Self::default()),
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(WorkerError::Config(format!(
                    "{TICK_SECS_VAR} is not valid Unicode"
                )));
            }
        };
        Self::from_raw(&raw)
    }

    /// Parse one present value of `WORKER_TICK_SECS`.
    ///
    /// An empty value and a whitespace value are configuration errors. The
    /// variable is set, so the operator intended a period and gave none. A
    /// fallback to the default hides that mistake (finding #40).
    fn from_raw(raw: &str) -> Result<Self, WorkerError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(WorkerError::Config(format!(
                "{TICK_SECS_VAR} is empty; give a whole number of seconds or remove the variable"
            )));
        }

        let secs: u64 = trimmed.parse().map_err(|_| {
            WorkerError::Config(format!(
                "{TICK_SECS_VAR} must be a whole number of seconds, not {trimmed:?}"
            ))
        })?;
        if secs == 0 {
            return Err(WorkerError::Config(format!(
                "{TICK_SECS_VAR} must be 1 or more"
            )));
        }

        Ok(Self {
            tick: Duration::from_secs(secs),
        })
    }
}

/// The error type of the worker.
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    /// The configuration is absent or malformed.
    #[error("configuration error: {0}")]
    Config(String),

    /// The database rejected a statement or the connection failed.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    /// The store refused to give a pool or a role report.
    #[error("store error: {0}")]
    Store(#[from] cadus_store::StoreError),

    /// The process could not install a stop-signal handler.
    #[error("signal error: {0}")]
    Signal(String),
}

/// Run the tick loop until the shutdown future completes. Return the number of
/// ticks that the loop finished.
///
/// Each tick runs one `SELECT 1` on the pool and logs `heartbeat tick=<n>` at
/// info level. The query proves that the pool still reaches the database, so a
/// dead connection shows up in the log instead of at the first real job.
///
/// The loop takes the first tick at once, then one tick per configured period.
/// A tick that overruns the period delays the next tick; the loop never bursts
/// to catch up.
///
/// The shutdown future also runs against the heartbeat query. A query that never
/// answers therefore does not block the stop: the loop leaves the query and
/// returns the tick count that it completed.
///
/// M0 runs no jobs. Two later milestones add work inside this loop:
///
/// - M4 adds pool refill (D-O4): instantiate a template, verify the answer, and
///   insert the problem into `serving_pool` ahead of need.
/// - M5 adds the diagnosis queue claim (D-O5): take one `diagnosis_jobs` row
///   with `SELECT ... FOR UPDATE SKIP LOCKED`, so two workers never claim the
///   same job and neither one blocks the other.
pub async fn run(
    pool: &PgPool,
    cfg: &WorkerConfig,
    shutdown: impl Future,
) -> Result<u64, WorkerError> {
    let mut ticks: u64 = 0;
    let mut interval = tokio::time::interval(cfg.tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut shutdown = std::pin::pin!(shutdown);

    tracing::info!(tick_ms = cfg.tick.as_millis() as u64, "worker: loop starts");

    loop {
        // Step 1: wait for the next tick. `biased` gives the shutdown branch the
        // first look on every pass, so a ready shutdown always wins over a ready
        // tick.
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            _ = interval.tick() => {}
        }

        // Step 2: run the heartbeat as a branch of the same kind of select, not
        // inside a branch body. A branch body that waits keeps the shutdown
        // future unpolled, so a stuck query made the worker deaf to SIGTERM
        // (finding #10). Here the shutdown future wins while the query is in
        // flight, and the loop drops the query.
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            result = heartbeat(pool) => {
                result?;
                ticks += 1;
                tracing::info!("heartbeat tick={ticks}");
            }
        }
    }

    tracing::info!("worker: loop stops after {ticks} ticks");
    Ok(ticks)
}

/// Run the heartbeat query. The compiler checks it against the schema (R2).
async fn heartbeat(pool: &PgPool) -> Result<(), WorkerError> {
    let one = sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)
        .fetch_one(pool)
        .await?;
    tracing::trace!(one, "worker: heartbeat query is complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{WorkerConfig, WorkerError};
    use std::time::Duration;

    /// The parse of one raw value. Each expected string below is a literal.
    #[test]
    fn from_raw_rejects_an_empty_value() {
        let err = WorkerConfig::from_raw("").expect_err("an empty value is an error");
        assert_eq!(
            err.to_string(),
            "configuration error: WORKER_TICK_SECS is empty; give a whole number of seconds or \
             remove the variable"
        );
    }

    #[test]
    fn from_raw_rejects_a_whitespace_value() {
        let err = WorkerConfig::from_raw("   ").expect_err("a whitespace value is an error");
        assert_eq!(
            err.to_string(),
            "configuration error: WORKER_TICK_SECS is empty; give a whole number of seconds or \
             remove the variable"
        );
    }

    #[test]
    fn from_raw_rejects_zero() {
        let err = WorkerConfig::from_raw("0").expect_err("zero is an error");
        assert_eq!(
            err.to_string(),
            "configuration error: WORKER_TICK_SECS must be 1 or more"
        );
    }

    #[test]
    fn from_raw_rejects_a_word() {
        let err = WorkerConfig::from_raw("soon").expect_err("a word is an error");
        assert_eq!(
            err.to_string(),
            "configuration error: WORKER_TICK_SECS must be a whole number of seconds, not \"soon\""
        );
    }

    #[test]
    fn from_raw_accepts_a_whole_number() {
        let cfg = WorkerConfig::from_raw(" 7 ").expect("7 seconds is a valid period");
        assert_eq!(
            cfg,
            WorkerConfig {
                tick: Duration::from_secs(7)
            }
        );
    }

    /// The error type keeps its variant. A test that only reads the text passes
    /// with any variant, so this one names the variant too.
    #[test]
    fn an_empty_value_gives_the_config_variant() {
        match WorkerConfig::from_raw("") {
            Err(WorkerError::Config(_)) => {}
            other => panic!("the parse must give WorkerError::Config, it gave {other:?}"),
        }
    }
}
