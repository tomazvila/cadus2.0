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

pub mod authoring;
pub mod diagnosis;
pub mod model_log;
pub mod readiness;
pub mod refill;
pub mod reports;

use std::future::Future;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cadus_store::{Db, StoreError, bounded};

pub use authoring::cost::{
    ATTEMPT_ALERT as AUTHORING_ATTEMPT_ALERT, Alert as AuthoringAlert,
    alerting as authoring_alerting,
};
pub use authoring::job::{
    AUTHORING_ATTEMPTS, AuthoringJob, BANK_TARGET, BatchReport, Decline as AuthoringDecline,
    Outcome as AuthoringOutcome, Report as AuthoringReport, Stored as AuthoringStored, author_one,
    run_batch,
};
pub use authoring::prompt::{AuthoringSpec, KINDS, Kind as AuthoringKind};
pub use diagnosis::{DiagnosisJob, Outcome as DiagnosisOutcome, Report as DiagnosisReport};
pub use model_log::{CallRecord, PURPOSE_AUTHORING, PURPOSE_DIAGNOSIS};
pub use readiness::{
    ContractCheck, ReadinessRun, render_json as render_readiness_json,
    render_markdown as render_readiness_markdown, render_prereq_markdown, run as readiness_run,
};
pub use refill::{
    EMPTY_FILLS_BEFORE_BACKOFF, EXHAUSTED_BACKOFF, REFILL_BACKOFF, RefillConfig, RefillJob,
    RefillReport, RefillState, batch_seed, refill_once, refill_once_at,
};

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

    /// One knowledge point did not refill (D-O4).
    ///
    /// The refill pass records the failure and keeps going, so this error names
    /// one pair and never stops the loop.
    #[error("refill error: {0}")]
    Refill(String),
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
/// The heartbeat runs inside `cadus_store::bounded`, so `DB_CLIENT_TIMEOUT_MS`
/// bounds it (L1). A bound that expires logs `heartbeat timed out after <n> ms`
/// at warn level and the loop takes the next tick. A stalled database must not
/// kill the worker, and it must not make the worker deaf to SIGTERM.
///
/// M0 runs no jobs. Two later milestones add work inside this loop:
///
/// - M4 adds pool refill (D-O4): instantiate a template, verify the answer, and
///   insert the problem into `serving_pool` ahead of need. [`run_with`] runs it.
/// - M5 adds the diagnosis queue claim (D-O5): take one `diagnosis_jobs` row
///   with `SELECT ... FOR UPDATE SKIP LOCKED`, so two workers never claim the
///   same job and neither one blocks the other.
pub async fn run(db: &Db, cfg: &WorkerConfig, shutdown: impl Future) -> Result<u64, WorkerError> {
    run_with(db, cfg, None, None, shutdown).await
}

/// The batch nonce of one refill pass: the UTC clock in microseconds.
///
/// The nonce is an input of `batch_seed`, so it decides which tuples one pass
/// draws. A process-local tick counter restarts at zero with every process, so a
/// restart replayed the batches the pool already held and `ON CONFLICT DO
/// NOTHING` wrote no row: the pool of an active learner stalled at depth 0 for
/// the whole window the old process had already walked, and two replicas drew
/// identical batches (finding #10). The wall clock does not restart, so a new
/// process and a second replica both draw a batch nobody drew before.
///
/// A clock before the epoch gives 0, and a clock past the range of a `u64` gives
/// the largest `u64`. Neither one panics, and neither one is reachable on a
/// machine whose clock is set at all.
#[must_use]
pub fn batch_nonce() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0_u128, |elapsed| elapsed.as_micros());
    u64::try_from(since_epoch).unwrap_or(u64::MAX)
}

/// [`run`] with the M4 pool refill job and the M5 diagnosis job in the loop
/// (D-O4, D-O5).
///
/// Each tick runs the heartbeat, then one refill pass, then one diagnosis pass.
/// A tick skips a pass whose job is `None` and runs every other pass.
/// The refill pass takes the UTC microsecond clock as its nonce, so the batch
/// seed of a pair changes every tick and a second refill draws past the tuples
/// the pool already holds.
///
/// A job failure is news, not a fatal error: the pass logs it and the loop takes
/// the next tick. A database that is gone shows up on the heartbeat, which is
/// the branch that stops the loop.
///
/// The two jobs are independent of each other.
///
/// A `None` diagnosis job runs the loop with no model call at all. The binary
/// passes `None` when the environment configures no endpoint, so a deployment
/// with no API key keeps its refill worker (T2: the queue costs nothing while it
/// waits).
///
/// A `None` refill job runs the diagnosis pass alone. `None` for both jobs runs
/// the heartbeat alone, which is what [`run`] does.
///
/// Each job runs as a branch of a `select`, not inside a branch body, for the
/// reason the heartbeat does: a branch body that waits keeps the shutdown future
/// unpolled, and a long job would make the worker deaf to SIGTERM. The model
/// call is the longest wait in the process — up to 60 s per HTTP attempt — so
/// this matters most here.
///
/// # Errors
///
/// Returns the error of [`run`].
pub async fn run_with(
    db: &Db,
    cfg: &WorkerConfig,
    refill: Option<&RefillJob<'_>>,
    diagnosis: Option<&mut DiagnosisJob>,
    shutdown: impl Future,
) -> Result<u64, WorkerError> {
    let mut diagnosis = diagnosis;
    let mut state = RefillState::new();
    let mut ticks: u64 = 0;
    let mut interval = tokio::time::interval(cfg.tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut shutdown = std::pin::pin!(shutdown);

    let tick_ms = cfg.tick.as_millis() as u64;
    tracing::info!(tick_ms, "worker: loop starts");

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
            result = heartbeat(db) => match result {
                Ok(()) => {
                    ticks += 1;
                    tracing::info!("heartbeat tick={ticks}");
                }
                // Step 3: a bound that expires is news, not a fatal error. The
                // database stalled; the worker keeps its loop and stays open to
                // SIGTERM. Every other database error still stops the worker,
                // because it names a fault that a retry does not repair.
                Err(WorkerError::Store(StoreError::Timeout { after_ms })) => {
                    tracing::warn!("heartbeat timed out after {after_ms} ms");
                }
                Err(err) => return Err(err),
            }
        }

        // Step 4: the M4 refill pass (D-O4). The nonce is the UTC microsecond
        // clock, so each pass draws a different batch for the same pair and a
        // restart does not replay the batches the pool already holds.
        //
        // A `None` refill job skips this pass and the tick goes on to step 5. A
        // `continue` here made the two jobs one job: a worker with a diagnosis
        // job and no refill job never ran the diagnosis pass (finding V7).
        if let Some(job) = refill {
            let nonce = batch_nonce();
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                result = refill::refill_once(db, job, &mut state, nonce) => match result {
                    Ok(report) => tracing::info!(
                        targets = report.targets,
                        inserted = report.inserted,
                        from_template = report.from_template,
                        from_exemplar = report.from_exemplar,
                        without_source = report.without_source,
                        failed = report.failed,
                        refused_instances = report.refused_instances,
                        flagged_refusals = report.flagged_refusals,
                        skipped_starved = report.skipped_starved,
                        exhausted = report.exhausted,
                        retired_unapproved = report.retired_unapproved,
                        nonce,
                        "refill tick={ticks}"
                    ),
                    Err(err) => tracing::warn!(error = %err, "refill: the pass did not run"),
                }
            }
        }

        // Step 5: the M5 diagnosis pass (D-O5). One claim, one model call, one
        // end state. A pass that claims nothing returns at once, so an empty
        // queue costs one indexed read per tick.
        //
        // A `None` diagnosis job skips this pass, exactly as step 4 skips its
        // own.
        if let Some(job) = diagnosis.as_deref_mut() {
            tokio::select! {
                biased;
                _ = &mut shutdown => break,
                result = diagnosis::run_once(db, job) => match result {
                    Ok(report) => log_diagnosis(&report, ticks),
                    Err(err) => tracing::warn!(error = %err, "diagnosis: the pass did not run"),
                }
            }
        }
    }

    tracing::info!("worker: loop stops after {ticks} ticks");
    Ok(ticks)
}

/// Log one diagnosis pass that did something.
///
/// An idle queue is the common case. It says nothing.
fn log_diagnosis(report: &diagnosis::Report, ticks: u64) {
    if report.outcome == diagnosis::Outcome::Idle {
        return;
    }
    let http_attempts = report.attempts.len();
    tracing::info!(
        outcome = ?report.outcome,
        job = ?report.job_id,
        http_attempts,
        "diagnosis tick={ticks}"
    );
}

/// Run the heartbeat query. The compiler checks it against the schema (R2).
///
/// The query runs inside `cadus_store::bounded`, so a database that accepts the
/// socket and then answers nothing gives `StoreError::Timeout` instead of a
/// wait without end (L1).
async fn heartbeat(db: &Db) -> Result<(), WorkerError> {
    let query = sqlx::query_scalar!(r#"SELECT 1 AS "one!""#).fetch_one(db.pool());
    let one = bounded(db, query).await?;
    tracing::trace!(one, "worker: heartbeat query is complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{WorkerConfig, WorkerError, batch_nonce};
    use std::time::Duration;

    /// The batch nonce is the UTC clock in microseconds, not a tick counter.
    ///
    /// The literal below is the microsecond count of 2026-01-01T00:00:00Z. A
    /// nonce that counts ticks, or that reads milliseconds or seconds, is far
    /// under it and fails here (finding #10).
    #[test]
    fn the_batch_nonce_is_the_utc_microsecond_clock() {
        const Y2026: u64 = 1_767_225_600_000_000;
        const Y2100: u64 = 4_102_444_800_000_000;

        let nonce = batch_nonce();
        assert!(
            nonce > Y2026,
            "the nonce must be a microsecond clock past 2026, it read {nonce}"
        );
        assert!(
            nonce < Y2100,
            "the nonce must be a microsecond clock before 2100, it read {nonce}"
        );
    }

    /// Two nonces of one process never go backwards, and a restart repeats none.
    #[test]
    fn the_batch_nonce_does_not_go_backwards() {
        let first = batch_nonce();
        let second = batch_nonce();
        assert!(
            second >= first,
            "the clock must not go backwards: {first} then {second}"
        );
    }

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
        let err = WorkerConfig::from_raw("").expect_err("an empty value does not parse");
        assert_eq!(
            std::mem::discriminant(&err),
            std::mem::discriminant(&WorkerError::Config(String::new())),
            "the parse must give WorkerError::Config, it gave {err:?}"
        );
    }

    /// The default period is 5 seconds, the value an absent variable gives.
    #[test]
    fn the_default_tick_is_five_seconds() {
        assert_eq!(WorkerConfig::default().tick, Duration::from_secs(5));
    }
}
