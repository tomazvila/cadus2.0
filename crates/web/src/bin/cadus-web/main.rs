//! Entry point of the Cadus HTTP server.
//!
//! The start sequence is:
//!
//! 1. Start the tracing subscriber. `RUST_LOG` selects the level.
//! 2. Install stop signals before configuration and curriculum loading.
//! 3. Read `DATABASE_URL`, `BIND_ADDR` (default `0.0.0.0:8080`),
//!    `SHUTDOWN_DEADLINE_SECS` (default 10), `CADUS_WEB_INSECURE_COOKIE`
//!    (default 0), and `PUBLIC_ORIGIN` (default: rebuild from
//!    `X-Forwarded-Proto` and `Host`).
//! 4. Run the cookie-posture guard. A `__Host-` cookie without `Secure` stops
//!    the process with exit code 2.
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

use std::convert::identity;
use std::future::IntoFuture;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::Router;
use cadus_store::shutdown::{Shutdown, close_budget, close_within};
use cadus_store::{Db, DbConfig, StoreError};
use cadus_web::diagnosis::DiagnosisHub;
use cadus_web::{AppState, create_app};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tracing_subscriber::EnvFilter;

mod settings;

use settings::{ADMIN_DSN_VAR, Settings, admin_dsn};

/// The process name that every log line of the stop carries.
const PROCESS: &str = "cadus-web";

/// The reason that stops the start sequence.
enum Fatal {
    /// C3: the database role escapes row-level security. Exit code 3.
    RlsBypass { role: String },
    /// Any other start error. Exit code 2.
    Startup(String),
}

impl Fatal {
    /// A start error that carries the text of `err`.
    fn startup(err: impl ToString) -> Self {
        Self::Startup(err.to_string())
    }
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

/// The start sequence of the module note, then the serve, then the stop.
async fn run() -> Result<(), Fatal> {
    // Curriculum loading and certificate fingerprints can take time. Register
    // signals first so a stop during that synchronous work is delivered to the
    // guarded connect and exits cleanly when loading finishes.
    let mut shutdown = Shutdown::install(PROCESS).map_err(Fatal::startup)?;
    let settings = Settings::read()?;
    let Some(db) = connect_guarded(&settings.cfg, &mut shutdown).await? else {
        return Ok(());
    };

    let listener = bind(&settings.addr).await?;
    // M5 U9, D7. ONE `LISTEN diagnosis_done` connection feeds every open
    // `/api/diagnosis/stream` of this process. The task runs beside the server:
    // a listener that cannot start leaves the poll fallback serving, which is
    // the required fallback anyway (spec section 2.1), so it never stops the
    // start.
    let hub = Arc::new(DiagnosisHub::new());
    let listener_task = spawn_listener(&hub, &db);
    let admin = open_admin(&settings.cfg).await?;

    let app = build_app(settings.state(db.clone()), admin.clone(), hub);
    let (result, drain_elapsed) =
        serve_until_stop(listener, app, shutdown, settings.deadline).await;

    // The pool close ends the listener: the close event of the pool cancels
    // its wait, the listener gives its connection back, and the close
    // completes. The bounded wait after it is for the line the listener logs
    // on its way out.
    let budget = close_budget(settings.deadline, drain_elapsed);
    close_within(budget, db.pool().close()).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), listener_task).await;
    if let Some(admin) = admin {
        close_within(budget, admin.pool().close()).await;
    }
    result
}

/// Open the pool and run the C3 boot guard, both inside the stop signal.
///
/// `None` means the stop signal came first: the process ends with exit code 0
/// and no pool close on purpose. The process ends at that return, so the
/// operating system closes the sockets. A wait for a database that answers
/// nothing only delays the stop the operator asked for.
///
/// `Db::connect` opens the pool AND keeps the client-side bound of
/// `DB_CLIENT_TIMEOUT_MS`. Every query after it therefore runs inside that
/// bound (L1). The guard runs inside the same select as the connect. A database
/// that accepts the connection and then answers no query made the old code deaf
/// to SIGTERM and SIGINT for the whole stall (finding #7). `boot_check` also
/// applies the bound, so a database that never answers ends the start with
/// exit code 2 instead of a wait without end.
async fn connect_guarded(cfg: &DbConfig, shutdown: &mut Shutdown) -> Result<Option<Db>, Fatal> {
    let db = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-web: the stop signal came before the database connect");
            return Ok(None);
        }
        result = Db::connect(cfg) => result.map_err(Fatal::startup)?,
    };
    let guard = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-web: the stop signal came before the boot guard");
            return Ok(None);
        }
        result = cadus_web::boot_check(&db) => result,
    };
    match guard {
        Ok(info) => tracing::info!(role = %info.name, "cadus-web: the boot guard passed"),
        Err(StoreError::RlsBypass { role, .. }) => return Err(Fatal::RlsBypass { role }),
        Err(err) => return Err(Fatal::startup(err)),
    }
    Ok(Some(db))
}

/// Bind `addr` and log the address the listener took.
///
/// `bind_addr` accepted the string only after a `SocketAddr` parse, so this
/// bind resolves the literal address and asks no name server.
async fn bind(addr: &str) -> Result<TcpListener, Fatal> {
    let bound = TcpListener::bind(addr).await;
    let (listener, local) = bound
        .and_then(|listener| listener.local_addr().map(|local| (listener, local)))
        .map_err(|err| Fatal::Startup(format!("bind {addr} failed: {err}")))?;
    // The address belongs in the message text, not in a structured field. The
    // compose comment and docs/SELF_HOST.md tell the operator to look for the
    // literal `cadus-web: listening on`, and a field renders as `address=...`
    // after the message, so that literal never appeared (finding #10).
    tracing::info!("cadus-web: listening on {local}");
    Ok(listener)
}

/// Start the `LISTEN diagnosis_done` task of `hub`.
fn spawn_listener(hub: &Arc<DiagnosisHub>, db: &Db) -> JoinHandle<()> {
    let hub = Arc::clone(hub);
    let db = db.clone();
    tokio::spawn(async move {
        // The listener returns only when its connection ends, so its answer is
        // always the reason it stopped.
        let reason = hub
            .listen(&db)
            .await
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default();
        tracing::error!(
            error = %reason,
            "cadus-web: the diagnosis listener stopped; clients fall back to polling"
        );
    })
}

/// Open the admin pool of the content store (M6 R5), when the operator named
/// one.
///
/// It is the second pool of the process, and the only writer of
/// `content_store` on this tier. It opens AFTER the C3 boot guard, and the
/// guard never runs on it: this role bypasses row-level security by design,
/// and no learner route takes it.
async fn open_admin(cfg: &DbConfig) -> Result<Option<Db>, Fatal> {
    let Some(admin_cfg) = admin_dsn(cfg)? else {
        tracing::info!(
            "cadus-web: {ADMIN_DSN_VAR} is not set, so the review writes of \
             /api/admin/content answer 503"
        );
        return Ok(None);
    };
    Db::connect(&admin_cfg)
        .await
        .map(Some)
        .map_err(|err| Fatal::Startup(format!("{ADMIN_DSN_VAR}: {err}")))
}

/// The router of the process on `state`, with the admin pool and the hub.
fn build_app(state: AppState, admin: Option<Db>, hub: Arc<DiagnosisHub>) -> Router {
    let state = match admin {
        Some(admin) => state.with_admin(admin),
        None => state,
    };
    create_app(state.with_diagnosis(hub))
}

/// Serve `app` on `listener` until the stop signal, then drain for at most
/// `deadline`. The answer is the outcome of the serve and the time the drain
/// spent.
///
/// The drain has a deadline. Without one, a client that opened a request and
/// never finished the headers keeps the process alive without end, and the
/// container runtime kills it (finding #9). The drain and the pool close share
/// ONE budget, so the caller gets the part the drain spent (finding #7).
async fn serve_until_stop(
    listener: TcpListener,
    app: Router,
    shutdown: Shutdown,
    deadline: Duration,
) -> (Result<(), Fatal>, Duration) {
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
    let mut server = tokio::spawn(server);

    // The sender lives inside the server task, so this wait ends at the stop
    // signal, or at the end of the server, whichever comes first.
    let _ = fired_rx.await;
    let started = Instant::now();
    let result = match tokio::time::timeout(deadline, &mut server).await {
        Ok(joined) => joined.map_err(std::io::Error::other).and_then(identity),
        Err(_elapsed) => {
            tracing::info!("shutdown deadline reached; closing");
            server.abort();
            Ok(())
        }
    };
    (result.map_err(Fatal::startup), started.elapsed())
}
