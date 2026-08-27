//! Entry point of the Cadus HTTP server.
//!
//! The start sequence is:
//!
//! 1. Start the tracing subscriber. `RUST_LOG` selects the level.
//! 2. Read `DATABASE_URL`, `BIND_ADDR` (default `0.0.0.0:8080`),
//!    `SHUTDOWN_DEADLINE_SECS` (default 10), `CADUS_WEB_INSECURE_COOKIE`
//!    (default 0), and `PUBLIC_ORIGIN` (default: rebuild from
//!    `X-Forwarded-Proto` and `Host`).
//! 3. Run the cookie-posture guard. A `__Host-` cookie without `Secure` stops
//!    the process with exit code 2.
//! 4. Install the stop signals. The handlers exist before the pool opens, so a
//!    signal during the connect gives a clean stop.
//! 5. Open the connection pool.
//! 6. Run the C3 boot guard. A role that bypasses row-level security stops the
//!    process with exit code 3.
//! 7. Bind the address and serve.
//! 8. Stop on `SIGTERM` or `SIGINT`, let the open requests finish, and exit 0.
//!    The drain has a deadline: at the deadline the process closes the open
//!    connections and still exits 0.
//!
//! `SHUTDOWN_DEADLINE_SECS` is ONE budget for the whole stop. The drain gets
//! the budget, and the pool close gets what is left of it, at least 1 second.
//! The total stop time is therefore `SHUTDOWN_DEADLINE_SECS` + 1 s or less.
//!
//! Exit codes: 0 for a clean stop, 2 for a start error, 3 for the boot guard.

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

use std::future::{Future, IntoFuture};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use cadus_store::{Db, DbConfig, StoreError};
use cadus_web::auth::password::{ARGON2_PROFILE_VAR, Argon2Profile};
use cadus_web::cookie::{CookiePosture, INSECURE_COOKIE_VAR};
use cadus_web::origin::{OriginPolicy, PUBLIC_ORIGIN_VAR};
use cadus_web::{AppState, BIND_ADDR_VAR, create_app};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

/// The environment variable that bounds the drain after the stop signal.
const SHUTDOWN_DEADLINE_VAR: &str = "SHUTDOWN_DEADLINE_SECS";

/// The drain deadline in seconds when `SHUTDOWN_DEADLINE_SECS` is absent.
const DEFAULT_SHUTDOWN_DEADLINE_SECS: u64 = 10;

/// The least time the pool close gets after the drain.
///
/// A drain that spends the whole budget leaves nothing for the close. This
/// floor gives the checked-out connections a last second to come back.
const MIN_POOL_CLOSE: Duration = Duration::from_secs(1);

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
            eprintln!("cadus-web: refusing to start: role {role} bypasses RLS");
            ExitCode::from(3)
        }
        Err(Fatal::Startup(message)) => {
            tracing::error!("cadus-web: {message}");
            eprintln!("cadus-web: {message}");
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
    // Read every configuration value before the pool opens. A bad value then
    // stops the process at once instead of after the connect timeout.
    let addr = bind_addr()?;
    let deadline = shutdown_deadline()?;

    // The cookie-posture guard (spec section 3.1, "Guards"; unit U1). A
    // `__Host-` cookie without `Secure` is discarded by the browser without a
    // word, so the login appears to work and no session ever persists (trap
    // W9). An insecure posture must be a deliberate choice, never an accident,
    // so both the guard and a bad value of the knob stop the start here.
    let posture = CookiePosture::from_env(std::env::var_os(INSECURE_COOKIE_VAR))
        .map_err(|err| Fatal::Startup(err.to_string()))?;
    posture
        .assert_safe()
        .map_err(|err| Fatal::Startup(err.to_string()))?;
    if !posture.secure {
        tracing::warn!(
            "cadus-web: {INSECURE_COOKIE_VAR}=1, so the session cookie is {} without Secure; use \
             this for local http:// development only",
            posture.name
        );
    }

    // The Argon2id parameter profile of the auth routes (spec section 3.1). A
    // bad value stops the start: a silent fallback to the fast test parameters
    // would ship a production deployment with a cheap password hash.
    let argon2 = Argon2Profile::from_env(std::env::var_os(ARGON2_PROFILE_VAR))
        .map_err(|err| Fatal::Startup(err.to_string()))?;
    if argon2 != Argon2Profile::PROD {
        tracing::warn!(
            "cadus-web: {ARGON2_PROFILE_VAR}={}, so the password hash uses the {} parameters; use              this for tests only",
            argon2.name,
            argon2.name
        );
    }

    // How the CSRF origin layer names this deployment's own origin (trap W10).
    let origin = OriginPolicy::from_env(std::env::var_os(PUBLIC_ORIGIN_VAR))
        .map_err(|err| Fatal::Startup(err.to_string()))?;
    if origin.public_origin.is_none() {
        tracing::info!(
            "cadus-web: {PUBLIC_ORIGIN_VAR} is not set, so the CSRF origin check rebuilds the \
             origin from X-Forwarded-Proto and Host; a proxy that drops X-Forwarded-Proto then \
             makes an https deployment rebuild as http://"
        );
    }

    // Install the stop signals before the connect. The handlers exist from this
    // point, so a SIGTERM during the connect gives exit code 0 instead of a kill
    // by signal (finding #39).
    let mut shutdown = Shutdown::install()?;

    // `Db::connect` opens the pool AND keeps the client-side bound of
    // `DB_CLIENT_TIMEOUT_MS`. Every query below therefore runs inside that
    // bound (L1).
    let db = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-web: the stop signal came before the database connect");
            return Ok(());
        }
        result = Db::connect(&cfg) => {
            result.map_err(|err| Fatal::Startup(err.to_string()))?
        }
    };

    // C3: stop here if row-level security does not apply to this role.
    //
    // The guard runs inside the same select as the connect above. A database
    // that accepts the connection and then answers no query made the old code
    // deaf to SIGTERM and SIGINT for the whole stall (finding #7).
    //
    // `boot_check` also applies the client-side bound of `DB_CLIENT_TIMEOUT_MS`,
    // so a database that never answers ends the start with exit code 2 instead
    // of a wait without end (L1).
    //
    // The stop branch returns without a pool close on purpose. The process ends
    // at that return, so the operating system closes the sockets. A wait for a
    // database that answers nothing only delays the stop the operator asked for.
    let guard = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-web: the stop signal came before the boot guard");
            return Ok(());
        }
        result = cadus_web::boot_check(&db) => result,
    };
    match guard {
        Ok(info) => tracing::info!(role = %info.name, "cadus-web: the boot guard passed"),
        Err(StoreError::RlsBypass { role, .. }) => return Err(Fatal::RlsBypass { role }),
        Err(err) => return Err(Fatal::Startup(err.to_string())),
    }

    // `bind_addr` accepted the string only after a `SocketAddr` parse, so this
    // bind resolves the literal address and asks no name server.
    let listener = TcpListener::bind(addr.as_str())
        .await
        .map_err(|err| Fatal::Startup(format!("bind {addr} failed: {err}")))?;
    let local = listener
        .local_addr()
        .map_err(|err| Fatal::Startup(format!("local address of the listener failed: {err}")))?;
    // The address belongs in the message text, not in a structured field. The
    // compose comment and docs/SELF_HOST.md tell the operator to look for the
    // literal `cadus-web: listening on`, and a field renders as `address=...`
    // after the message, so that literal never appeared (finding #10).
    tracing::info!("cadus-web: listening on {local}");

    let app = create_app(
        AppState::new(db.clone())
            .with_posture(posture)
            .with_origin(origin)
            .with_argon2(argon2),
    );
    // The per-address rate rules key on the client address, so the service needs
    // the peer address of the socket. `axum::serve` carries it only through this
    // make-service. Without it every request from every host shares one bucket,
    // and 5 sign-ups an hour would bound the whole deployment (spec section 3.2).
    let app = app.into_make_service_with_connect_info::<std::net::SocketAddr>();

    // `fired_rx` reports the moment of the stop signal, so the deadline below
    // starts at the signal and not at the start of the process.
    let (fired_tx, fired_rx) = tokio::sync::oneshot::channel::<()>();
    let server = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let mut shutdown = shutdown;
            shutdown.wait().await;
            tracing::info!("cadus-web: graceful shutdown starts");
            let _ = fired_tx.send(());
        })
        .into_future();
    let mut server = std::pin::pin!(server);

    // The drain has a deadline. Without one, a client that opened a request and
    // never finished the headers keeps the process alive without end, and the
    // container runtime kills it (finding #9).
    //
    // The drain and the pool close below share ONE budget. `drain_elapsed`
    // holds the part of it that the drain spent (finding #7).
    let mut drain_elapsed = Duration::ZERO;
    let result = tokio::select! {
        outcome = &mut server => outcome,
        _ = fired_rx => {
            let started = Instant::now();
            let outcome = match tokio::time::timeout(deadline, &mut server).await {
                Ok(outcome) => outcome,
                Err(_elapsed) => {
                    tracing::info!("shutdown deadline reached; closing");
                    Ok(())
                }
            };
            drain_elapsed = started.elapsed();
            outcome
        }
    };

    close_within(close_budget(deadline, drain_elapsed), db.pool().close()).await;
    result.map_err(|err| Fatal::Startup(format!("serve failed: {err}")))
}

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
fn close_budget(deadline: Duration, drain_elapsed: Duration) -> Duration {
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
async fn close_within<F: Future<Output = ()>>(deadline: Duration, close: F) {
    if tokio::time::timeout(deadline, close).await.is_err() {
        tracing::info!("pool close deadline reached");
    }
}

/// Read `BIND_ADDR`, or use the default.
///
/// The rules live in `cadus_web::bind_addr`, a pure function. This wrapper only
/// reads the environment and maps the error to an exit code, so the unit tests
/// of the library cover every rule without a bind (item FIX10b/a).
fn bind_addr() -> Result<String, Fatal> {
    cadus_web::bind_addr(std::env::var_os(BIND_ADDR_VAR))
        .map_err(|err| Fatal::Startup(err.to_string()))
}

/// Read `SHUTDOWN_DEADLINE_SECS`, or use the default of 10 seconds.
///
/// A present value that is not a positive whole number of seconds is a start
/// error, because a silent fallback hides an operator mistake.
fn shutdown_deadline() -> Result<Duration, Fatal> {
    let raw = match std::env::var(SHUTDOWN_DEADLINE_VAR) {
        Ok(raw) => raw,
        Err(std::env::VarError::NotPresent) => {
            return Ok(Duration::from_secs(DEFAULT_SHUTDOWN_DEADLINE_SECS));
        }
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(Fatal::Startup(format!(
                "{SHUTDOWN_DEADLINE_VAR} is not valid Unicode"
            )));
        }
    };

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} is empty; give a whole number of seconds or remove the \
             variable"
        )));
    }
    let secs: u64 = trimmed.parse().map_err(|_| {
        Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} must be a whole number of seconds, not {trimmed:?}"
        ))
    })?;
    if secs == 0 {
        return Err(Fatal::Startup(format!(
            "{SHUTDOWN_DEADLINE_VAR} must be 1 or more"
        )));
    }
    Ok(Duration::from_secs(secs))
}

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal.
struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    #[cfg(unix)]
    fn install() -> Result<Self, Fatal> {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = signal(SignalKind::terminate())
            .map_err(|err| Fatal::Startup(format!("the SIGTERM handler failed: {err}")))?;
        let interrupt = signal(SignalKind::interrupt())
            .map_err(|err| Fatal::Startup(format!("the SIGINT handler failed: {err}")))?;
        Ok(Self {
            terminate,
            interrupt,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    fn install() -> Result<Self, Fatal> {
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
            _ = terminate.recv() => tracing::info!("cadus-web: SIGTERM received"),
            _ = interrupt.recv() => tracing::info!("cadus-web: SIGINT received"),
        }
    }

    /// Complete on Ctrl-C. A platform without unix signals has no `SIGTERM`.
    #[cfg(not(unix))]
    async fn wait(&mut self) {
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
        let deadline = Duration::from_secs(super::DEFAULT_SHUTDOWN_DEADLINE_SECS);
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
