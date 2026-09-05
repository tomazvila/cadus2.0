//! The stop of the process: the signal handlers, and the one budget the drain
//! and the pool close share.

use std::future::Future;
use std::time::Duration;

use super::Fatal;

/// The least time the pool close gets after the drain.
///
/// A drain that spends the whole budget leaves nothing for the close. This
/// floor gives the checked-out connections a last second to come back.
const MIN_POOL_CLOSE: Duration = Duration::from_secs(1);

/// Give the pool close what is left of the stop budget.
///
/// `SHUTDOWN_DEADLINE_SECS` is one budget, not two. The old code spent the full
/// value on the drain and then a second full value on the pool close, so a stop
/// took up to 2 x `SHUTDOWN_DEADLINE_SECS`: 20.01 s at the compose default of
/// 10 s, past the `stop_grace_period` of 20 s, and Docker ended the container
/// with SIGKILL and exit 137 (finding #7).
///
/// The close still gets `MIN_POOL_CLOSE`, so a drain that spends the whole
/// budget leaves the checked-out connections a last second. Total stop time
/// <= SHUTDOWN_DEADLINE_SECS + 1 s < stop_grace_period 20 s.
pub(super) fn close_budget(deadline: Duration, drain_elapsed: Duration) -> Duration {
    let left = deadline.saturating_sub(drain_elapsed);
    if left < MIN_POOL_CLOSE {
        MIN_POOL_CLOSE
    } else {
        left
    }
}

/// Wait for `close` for at most `deadline`, then log the fact and give up.
///
/// `PgPool::close` waits for every checked-out connection to come back. A
/// database that answers nothing never gives one back, so the plain call runs
/// without end and defeats the drain deadline above: the process logs
/// `shutdown deadline reached; closing` and then stays alive until the
/// container runtime sends SIGKILL (finding #6). The bound below keeps the exit
/// inside the same budget. `close_budget` gives that bound: it is the rest of
/// the stop budget, not a second full one. The process exits 0 either way,
/// because the open sockets end with the process.
pub(super) async fn close_within<F: Future<Output = ()>>(deadline: Duration, close: F) {
    if tokio::time::timeout(deadline, close).await.is_err() {
        tracing::info!("pool close deadline reached");
    }
}

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal.
pub(super) struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    #[cfg(unix)]
    pub fn install() -> Result<Self, Fatal> {
        use tokio::signal::unix::{SignalKind, signal};

        let handlers = signal(SignalKind::terminate()).and_then(|terminate| {
            signal(SignalKind::interrupt()).map(|interrupt| (terminate, interrupt))
        });
        let (terminate, interrupt) =
            handlers.map_err(|err| Fatal::Startup(format!("the signal handlers failed: {err}")))?;
        Ok(Self {
            terminate,
            interrupt,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    pub fn install() -> Result<Self, Fatal> {
        Ok(Self {})
    }

    /// Complete on the first `SIGTERM` or `SIGINT`.
    #[cfg(unix)]
    pub async fn wait(&mut self) {
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
    pub async fn wait(&mut self) {
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

    /// One budget, not two: the pool close gets the rest of the drain budget.
    ///
    /// The old code passed the full `SHUTDOWN_DEADLINE_SECS` to the drain and
    /// then the full value again to the pool close, so a stop took twice the
    /// budget and Docker sent SIGKILL at the 20 s `stop_grace_period`
    /// (finding #7). Every number below is a literal.
    #[test]
    fn close_budget_is_the_rest_of_the_stop_budget() {
        // A drain that used 3 s of a 10 s budget leaves 7 s.
        assert_eq!(
            super::close_budget(Duration::from_secs(10), Duration::from_secs(3)),
            Duration::from_secs(7)
        );
        // A drain that used the whole budget still leaves the 1 s floor.
        assert_eq!(
            super::close_budget(Duration::from_secs(10), Duration::from_secs(10)),
            Duration::from_secs(1)
        );
        // The floor also covers a drain that overran the budget.
        assert_eq!(
            super::close_budget(Duration::from_secs(2), Duration::from_secs(30)),
            Duration::from_secs(1)
        );
        // A rest below the floor is raised to the floor.
        assert_eq!(
            super::close_budget(Duration::from_secs(10), Duration::from_millis(9500)),
            Duration::from_secs(1)
        );
        // A stop with no signal spends nothing, so the whole budget is left.
        assert_eq!(
            super::close_budget(Duration::from_secs(10), Duration::ZERO),
            Duration::from_secs(10)
        );
    }

    /// The total stop time stays under the `stop_grace_period` of 20 s.
    ///
    /// docker-compose.yml sets `stop_grace_period: 20s` and defaults
    /// `SHUTDOWN_DEADLINE_SECS` to 10. Drain plus close must stay below 20 s,
    /// or the container ends with SIGKILL and exit 137 (finding #7).
    #[test]
    fn drain_plus_close_stays_under_the_stop_grace_period() {
        let deadline = Duration::from_secs(crate::settings::DEFAULT_SHUTDOWN_DEADLINE_SECS);
        let worst = deadline + super::close_budget(deadline, deadline);

        assert_eq!(worst, Duration::from_secs(11));
        assert!(
            worst < Duration::from_secs(20),
            "the stop must end before stop_grace_period 20 s, it takes {worst:?}"
        );
    }

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
