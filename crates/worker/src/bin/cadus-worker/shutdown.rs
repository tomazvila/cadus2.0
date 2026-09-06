//! The stop signals of the process, under the error type of the worker.
//!
//! `cadus_store::shutdown` holds the handlers themselves. This module names the
//! process for the log lines and turns a refusal of the operating system into a
//! [`WorkerError`].

use cadus_store::shutdown::{Shutdown, SignalError};
use cadus_worker::WorkerError;

/// The process name that every log line of the stop carries.
const PROCESS: &str = "cadus-worker";

/// Register the handlers for `SIGTERM` and `SIGINT`.
pub(crate) fn install() -> Result<Shutdown, WorkerError> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::SignalKind;

        install_kinds(SignalKind::terminate(), SignalKind::interrupt())
    }
    #[cfg(not(unix))]
    {
        Shutdown::install(PROCESS).map_err(signal_error)
    }
}

/// Register the handlers for these two signal kinds.
///
/// The kinds are parameters, so a test drives the refusal of the operating
/// system with a signal number the kernel does not know.
#[cfg(unix)]
fn install_kinds(
    terminate: tokio::signal::unix::SignalKind,
    interrupt: tokio::signal::unix::SignalKind,
) -> Result<Shutdown, WorkerError> {
    Shutdown::install_kinds(PROCESS, terminate, interrupt).map_err(signal_error)
}

/// The refusal of the operating system, as the error of the worker.
fn signal_error(err: SignalError) -> WorkerError {
    WorkerError::Signal(err.to_string())
}

#[cfg(test)]
mod tests {
    use tokio::signal::unix::SignalKind;

    use super::{install, install_kinds};

    /// A signal number the kernel does not know makes the registration fail,
    /// on the first handler and on the second one alike.
    #[tokio::test]
    async fn a_handler_that_does_not_register_names_its_signal() {
        let bad = SignalKind::from_raw(-1);
        let first = install_kinds(bad, SignalKind::interrupt())
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        assert!(
            first.starts_with("signal error: the SIGTERM handler failed: "),
            "{first}"
        );
        let second = install_kinds(SignalKind::terminate(), bad)
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        assert!(
            second.starts_with("signal error: the SIGINT handler failed: "),
            "{second}"
        );
        assert!(install().is_ok());
    }
}
