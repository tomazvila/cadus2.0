//! The stop plumbing of the two binaries: the signal handlers, the budget of
//! the pool close, and the bounded close.
//!
//! `cadus-web` and `cadus-worker` both open a `PgPool` and both end on
//! `SIGTERM` or `SIGINT`, so the stop of a process belongs beside the pool it
//! closes. The process name is a parameter, so each binary keeps its own log
//! lines.

use std::future::Future;
use std::time::Duration;

/// The least time the pool close gets after the drain.
///
/// A drain that spends the whole budget leaves nothing for the close. This
/// floor gives the checked-out connections a last second to come back.
const MIN_POOL_CLOSE: Duration = Duration::from_secs(1);

/// Give the pool close what is left of the stop budget.
///
/// The stop deadline is one budget, not two. The old code spent the full value
/// on the drain and then a second full value on the pool close, so a stop took
/// up to twice the deadline: 20.01 s at the compose default of 10 s, past the
/// `stop_grace_period` of 20 s, and Docker ended the container with SIGKILL and
/// exit 137 (finding #7).
///
/// The close still gets [`MIN_POOL_CLOSE`], so a drain that spends the whole
/// budget leaves the checked-out connections a last second. The total stop time
/// is the deadline plus 1 s or less.
#[must_use]
pub fn close_budget(deadline: Duration, drain_elapsed: Duration) -> Duration {
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
/// without end and defeats the drain deadline: the process logs the deadline
/// line and then stays alive until the container runtime sends SIGKILL
/// (finding #6). The bound below keeps the exit inside the same budget.
/// [`close_budget`] gives that bound: it is the rest of the stop budget, not a
/// second full one. The process exits 0 either way, because the open sockets
/// end with the process.
pub async fn close_within<F: Future<Output = ()>>(deadline: Duration, close: F) {
    if tokio::time::timeout(deadline, close).await.is_err() {
        tracing::info!("pool close deadline reached");
    }
}

/// The stop signal that the operating system did not register.
#[derive(Debug, thiserror::Error)]
#[error("the {signal} handler failed: {source}")]
pub struct SignalError {
    /// The name of the signal: `SIGTERM` or `SIGINT`.
    pub signal: &'static str,
    /// The error the operating system reported.
    #[source]
    pub source: std::io::Error,
}

/// The installed stop signals of one process.
///
/// [`Shutdown::install`] registers the handlers at once, so a signal from that
/// moment on reaches the program. [`Shutdown::wait`] completes on the first
/// signal. `docker stop` sends SIGTERM, so that is the normal stop path of the
/// deployment.
pub struct Shutdown {
    /// The process name that every log line of the stop carries.
    process: &'static str,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT` under `process`.
    ///
    /// # Errors
    ///
    /// Returns [`SignalError`] when the operating system refuses a handler.
    #[cfg(unix)]
    pub fn install(process: &'static str) -> Result<Self, SignalError> {
        use tokio::signal::unix::SignalKind;

        Self::install_kinds(process, SignalKind::terminate(), SignalKind::interrupt())
    }

    /// Register the handlers for these two signal kinds: the stop signal and
    /// the interrupt.
    ///
    /// The kinds are parameters, so a test drives the refusal of the operating
    /// system with a signal number the kernel does not know.
    ///
    /// # Errors
    ///
    /// Returns [`SignalError`] when the operating system refuses a handler.
    #[cfg(unix)]
    pub fn install_kinds(
        process: &'static str,
        terminate: tokio::signal::unix::SignalKind,
        interrupt: tokio::signal::unix::SignalKind,
    ) -> Result<Self, SignalError> {
        Ok(Self {
            process,
            terminate: listen(terminate, "SIGTERM")?,
            interrupt: listen(interrupt, "SIGINT")?,
        })
    }

    /// A platform without unix signals has nothing to register here.
    ///
    /// # Errors
    ///
    /// Returns no error on such a platform.
    #[cfg(not(unix))]
    pub fn install(process: &'static str) -> Result<Self, SignalError> {
        Ok(Self { process })
    }

    /// Complete on the first `SIGTERM` or `SIGINT`.
    #[cfg(unix)]
    pub async fn wait(&mut self) {
        let Self {
            process,
            terminate,
            interrupt,
        } = self;
        tokio::select! {
            _ = terminate.recv() => tracing::info!("{process}: SIGTERM received"),
            _ = interrupt.recv() => tracing::info!("{process}: SIGINT received"),
        }
    }

    /// Complete on Ctrl-C. A platform without unix signals has no `SIGTERM`.
    #[cfg(not(unix))]
    pub async fn wait(&mut self) {
        let process = self.process;
        match tokio::signal::ctrl_c().await {
            Ok(()) => tracing::info!("{process}: Ctrl-C received"),
            Err(err) => {
                tracing::error!(error = %err, "{process}: the Ctrl-C handler failed");
                // The handler is gone. Park here, so the process keeps working
                // instead of a stop at once.
                std::future::pending::<()>().await;
            }
        }
    }
}

/// Register the handler of one signal kind, named for the error.
#[cfg(unix)]
fn listen(
    kind: tokio::signal::unix::SignalKind,
    signal: &'static str,
) -> Result<tokio::signal::unix::Signal, SignalError> {
    tokio::signal::unix::signal(kind).map_err(|source| SignalError { signal, source })
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::pin::Pin;
    use std::time::{Duration, Instant};

    use tokio::signal::unix::SignalKind;

    use super::{Shutdown, close_budget, close_within};

    /// A close future behind one pointer type, so the two calls of a test run
    /// the same instantiation of `close_within`.
    type Close = Pin<Box<dyn Future<Output = ()>>>;

    /// One budget, not two: the pool close gets the rest of the drain budget.
    ///
    /// The old code passed the full stop deadline to the drain and then the
    /// full value again to the pool close, so a stop took twice the budget and
    /// Docker sent SIGKILL at the 20 s `stop_grace_period` (finding #7). Every
    /// number below is a literal.
    #[test]
    fn close_budget_is_the_rest_of_the_stop_budget() {
        // A drain that used 3 s of a 10 s budget leaves 7 s.
        assert_eq!(
            close_budget(Duration::from_secs(10), Duration::from_secs(3)),
            Duration::from_secs(7)
        );
        // A drain that used the whole budget still leaves the 1 s floor.
        assert_eq!(
            close_budget(Duration::from_secs(10), Duration::from_secs(10)),
            Duration::from_secs(1)
        );
        // The floor also covers a drain that overran the budget.
        assert_eq!(
            close_budget(Duration::from_secs(2), Duration::from_secs(30)),
            Duration::from_secs(1)
        );
        // A rest below the floor is raised to the floor.
        assert_eq!(
            close_budget(Duration::from_secs(10), Duration::from_millis(9500)),
            Duration::from_secs(1)
        );
        // A stop with no signal spends nothing, so the whole budget is left.
        assert_eq!(
            close_budget(Duration::from_secs(10), Duration::ZERO),
            Duration::from_secs(10)
        );
    }

    /// `close_within` returns at its deadline, even when the close never ends,
    /// and it returns at once when the close ends first.
    ///
    /// `PgPool::close` waits for every checked-out connection, so a database
    /// that answers nothing makes the plain call run without end (finding #6).
    /// The never-resolving future below stands for that case. The outer timeout
    /// of 5 s fails the test when the bound is gone. A close that ends at once
    /// goes through the same instantiation first, so one record holds both
    /// arms.
    #[tokio::test]
    async fn close_within_returns_at_the_deadline() {
        let ready: Close = Box::pin(std::future::ready(()));
        close_within(Duration::from_millis(200), ready).await;

        let never: Close = Box::pin(std::future::pending());
        let start = Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(5),
            close_within(Duration::from_millis(200), never),
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

    /// A signal number the kernel does not know makes the registration fail,
    /// on the first handler and on the second one alike.
    #[tokio::test]
    async fn a_handler_that_does_not_register_names_its_signal() {
        let bad = SignalKind::from_raw(-1);
        let first = Shutdown::install_kinds("cadus-test", bad, SignalKind::interrupt())
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        assert!(first.starts_with("the SIGTERM handler failed: "), "{first}");
        let second = Shutdown::install_kinds("cadus-test", SignalKind::terminate(), bad)
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        assert!(
            second.starts_with("the SIGINT handler failed: "),
            "{second}"
        );
        assert!(Shutdown::install("cadus-test").is_ok());
    }
}
