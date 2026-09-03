//! The stop signals of the process: `SIGTERM` and `SIGINT`.

use cadus_worker::WorkerError;

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal. `docker stop`
/// sends SIGTERM, so that is the normal stop path of the deployment.
pub(crate) struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    #[cfg(unix)]
    pub(crate) fn install() -> Result<Self, WorkerError> {
        use tokio::signal::unix::SignalKind;

        Self::install_kinds(SignalKind::terminate(), SignalKind::interrupt())
    }

    /// Register the handlers for these two signal kinds: the stop signal and
    /// the interrupt.
    #[cfg(unix)]
    fn install_kinds(
        terminate: tokio::signal::unix::SignalKind,
        interrupt: tokio::signal::unix::SignalKind,
    ) -> Result<Self, WorkerError> {
        Ok(Self {
            terminate: listen(terminate, "SIGTERM")?,
            interrupt: listen(interrupt, "SIGINT")?,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    pub(crate) fn install() -> Result<Self, WorkerError> {
        Ok(Self {})
    }

    /// Complete on the first `SIGTERM` or `SIGINT`.
    #[cfg(unix)]
    pub(crate) async fn wait(&mut self) {
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
    pub(crate) async fn wait(&mut self) {
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

/// Register the handler of one signal kind, named for the error.
#[cfg(unix)]
fn listen(
    kind: tokio::signal::unix::SignalKind,
    name: &str,
) -> Result<tokio::signal::unix::Signal, WorkerError> {
    tokio::signal::unix::signal(kind)
        .map_err(|err| WorkerError::Signal(format!("the {name} handler failed: {err}")))
}

#[cfg(test)]
mod tests {
    use tokio::signal::unix::SignalKind;

    use super::Shutdown;

    /// A signal number the kernel does not know makes the registration fail,
    /// on the first handler and on the second one alike.
    #[tokio::test]
    async fn a_handler_that_does_not_register_names_its_signal() {
        let bad = SignalKind::from_raw(-1);
        let first = Shutdown::install_kinds(bad, SignalKind::interrupt())
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        assert!(
            first.starts_with("signal error: the SIGTERM handler failed: "),
            "{first}"
        );
        let second = Shutdown::install_kinds(SignalKind::terminate(), bad)
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        assert!(
            second.starts_with("signal error: the SIGINT handler failed: "),
            "{second}"
        );
        assert!(Shutdown::install().is_ok());
    }
}
