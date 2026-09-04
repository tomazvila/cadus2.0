//! The stop signals of `cadus-migrate`: the handlers of `SIGTERM` and
//! `SIGINT`, and the race between the work and the first signal.

use std::future::Future;

use cadus_store::StoreError;

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal. `docker stop`
/// sends SIGTERM, so that is the normal stop path of the deployment.
/// `cadus-web` and `cadus-worker` carry the same shape.
pub struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    ///
    /// Both registrations run before the first error stops the install, so the
    /// two results join in one place.
    #[cfg(unix)]
    pub fn install() -> Result<Self, StoreError> {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = handler("SIGTERM", signal(SignalKind::terminate()));
        let interrupt = handler("SIGINT", signal(SignalKind::interrupt()));
        let (terminate, interrupt) =
            terminate.and_then(|terminate| interrupt.map(|interrupt| (terminate, interrupt)))?;
        Ok(Self {
            terminate,
            interrupt,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    pub fn install() -> Result<Self, StoreError> {
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
            _ = terminate.recv() => (),
            _ = interrupt.recv() => (),
        }
    }

    /// Complete on Ctrl-C. A platform without unix signals has no `SIGTERM`.
    #[cfg(not(unix))]
    async fn wait(&mut self) {
        if tokio::signal::ctrl_c().await.is_err() {
            // The handler is gone. Park here, so the run goes on instead of a
            // stop that no operator asked for.
            std::future::pending::<()>().await;
        }
    }
}

/// The registered handler of `name`, or the configuration error that names
/// the signal.
#[cfg(unix)]
fn handler<T>(name: &str, registered: std::io::Result<T>) -> Result<T, StoreError> {
    registered.map_err(|err| StoreError::Config(format!("the {name} handler failed: {err}")))
}

/// Run `work` until it ends or a stop signal arrives.
///
/// `Ok(None)` means the signal came first. The caller then returns
/// `Outcome::Stopped`, and the process exits 3.
///
/// The drop of the work future closes its connection, so a statement that is
/// still in flight rolls back. A stop signal is an explicit request of the
/// operator, and a cancel is the answer to it. The `statement_timeout` of 0 of
/// the migrate config covers the other case: a bound that no operator asked
/// for.
pub async fn until_signal<T>(
    shutdown: &mut Shutdown,
    work: impl Future<Output = Result<T, StoreError>>,
) -> Result<Option<T>, StoreError> {
    tokio::select! {
        biased;
        () = shutdown.wait() => Ok(None),
        result = work => result.map(Some),
    }
}

/// The tests that send a signal to this process, or that wait on one, run one
/// at a time: a signal reaches every waiting handler of the process.
#[cfg(test)]
pub static SIGNAL_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use cadus_store::StoreError;

    use super::{SIGNAL_TESTS, Shutdown, handler, until_signal};

    /// A handler that did not register is a configuration error that names
    /// the signal.
    #[test]
    fn a_handler_that_does_not_register_names_its_signal() {
        assert_eq!(handler("SIGTERM", Ok::<u8, std::io::Error>(1)).unwrap(), 1);
        let err = handler::<u8>("SIGINT", Err(std::io::Error::other("no driver"))).unwrap_err();
        assert_eq!(
            err.to_string(),
            "configuration error: the SIGINT handler failed: no driver"
        );
    }

    /// A runtime whose signal driver is gone refuses both registrations, and
    /// the install reports the first one.
    #[test]
    fn a_gone_signal_driver_stops_the_install() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let handle = runtime.handle().clone();
        drop(runtime);
        let _context = handle.enter();
        let err = Shutdown::install().err().unwrap();
        assert_eq!(
            err.to_string(),
            "configuration error: the SIGTERM handler failed: signal driver gone"
        );
    }

    /// The signal wins over the work: an interrupt or a terminate sent to this
    /// process ends `until_signal` with `None`, and work that finishes first
    /// gives its value.
    #[tokio::test]
    async fn until_signal_answers_none_on_a_signal_and_some_on_finished_work() {
        let _serial = SIGNAL_TESTS.lock().await;
        let mut shutdown = Shutdown::install().unwrap();
        let done = until_signal(&mut shutdown, async { Ok::<u8, StoreError>(7) })
            .await
            .unwrap();
        assert_eq!(done, Some(7));

        let pid = std::process::id().to_string();
        for signal in ["-INT", "-TERM"] {
            let sent = std::process::Command::new("kill")
                .args([signal, &pid])
                .status()
                .unwrap();
            assert!(sent.success());
            let stopped = until_signal(
                &mut shutdown,
                std::future::pending::<Result<u8, StoreError>>(),
            )
            .await
            .unwrap();
            assert_eq!(stopped, None, "{signal}");
        }
    }
}
