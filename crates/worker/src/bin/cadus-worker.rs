//! Entry point of the Cadus background worker (R4) and of the authoring pass
//! (M6 R8).
//!
//! With no argument the program reads `DATABASE_URL` and `WORKER_TICK_SECS`,
//! installs the stop signals, opens a pool, logs the identity of its database
//! role, and runs the tick loop until SIGTERM or SIGINT. It exits 0 after a
//! clean stop and 2 after an error.
//!
//! The signal handlers exist before the pool opens, so a signal during the
//! connect also gives exit code 0. The role report runs under the same signal
//! guard, and the pool close after the loop has a deadline.
//!
//! With `author` the program runs ONE authoring pass instead of the loop:
//! [`author`] prints the plan, and `--dry-run` stops there. The plan goes to
//! stdout, because it is the operator's output; the log stays on stderr.
//! `cadus_worker::authoring::cli` holds the parser and the plan.

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
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use cadus_core::curriculum::{Curriculum, CurriculumError, LoadError, load_curriculum};
use cadus_model_client::{API_KEY_VAR, Client, ModelConfig};
use cadus_store::{Db, DbConfig, bounded};
use cadus_worker::authoring::cli::{self, AuthorArgs, Command};
use cadus_worker::authoring::job::{self, AuthoringJob, run_batch};
use cadus_worker::{DiagnosisJob, RefillJob, WorkerConfig, WorkerError};

/// The environment variable that names the curriculum tree.
///
/// The refill job (D-O4) reads the authored exemplars from it for the A6
/// fallback, and it reads the topic answer kind for the gate re-run.
const CURRICULUM_ENV: &str = "CADUS_CURRICULUM";

/// The tree the worker reads when the variable names none.
///
/// The path is relative to the working directory. The image sets
/// `CADUS_CURRICULUM=/app/curriculum` and carries the tree there, and the
/// repository holds `curriculum/` at its root, so both the container and a run
/// from the repository root find a tree without an operator flag.
const DEFAULT_CURRICULUM: &str = "curriculum";

/// The bound on the pool close after the tick loop stops.
///
/// The worker reads no deadline from the environment, so the budget is a
/// constant. `cadus-web` uses `SHUTDOWN_DEADLINE_SECS` for the same job.
const POOL_CLOSE_DEADLINE: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> ExitCode {
    init_tracing();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(err) => return fail(&err.to_string()),
    };

    match command {
        Command::Help => {
            print!("{}", cli::HELP);
            ExitCode::SUCCESS
        }
        Command::Serve => match run().await {
            Ok(ticks) => {
                tracing::info!("cadus-worker: stop after {ticks} ticks");
                ExitCode::SUCCESS
            }
            Err(err) => fail(&err.to_string()),
        },
        Command::Author(author_args) => match author(&author_args).await {
            Ok(()) => ExitCode::SUCCESS,
            Err(err) => fail(&err.to_string()),
        },
    }
}

/// Report one failure on both channels and answer exit code 2.
fn fail(reason: &str) -> ExitCode {
    tracing::error!("cadus-worker: {reason}");
    eprintln!("cadus-worker: {reason}");
    ExitCode::from(2)
}

/// Run one authoring pass (M6 R8).
///
/// The order is fixed: read the curriculum, pick the knowledge points, open the
/// pool, count the bank slots, print the plan. A dry run stops at the print and
/// makes ZERO model calls: no client exists on that path, so no code of the pass
/// can reach an endpoint.
///
/// A run that is not a dry run then needs a model endpoint. An empty
/// `OPENAI_API_KEY` is an error here and not a quiet skip: the operator asked
/// for an authoring pass, and a pass with no endpoint authors nothing.
///
/// # Errors
///
/// Returns [`WorkerError::Config`] for a curriculum that does not load, a
/// knowledge point the tree does not hold, and a model configuration that does
/// not read; and the error of the store for a pool or a count that fails.
async fn author(args: &AuthorArgs) -> Result<(), WorkerError> {
    let curriculum = load_arena()?;
    let specs =
        cli::select(&curriculum, &args.kps).map_err(|err| WorkerError::Config(err.to_string()))?;
    let kinds = args.kinds();

    let db_cfg = DbConfig::from_env()?;
    let db = Db::connect(&db_cfg).await?;
    // `--stale` is a read and an exit. Spec section 2.2, "Prompt digest": the
    // list names the approved rows an older prompt wrote, so an operator reads
    // the re-authoring queue before a pass spends a token (M6 review finding
    // F4).
    if args.stale {
        let rows = job::stale_rows(&db, &kinds).await?;
        print!("{}", job::render_stale(&rows));
        close_within(POOL_CLOSE_DEADLINE, db.pool().close()).await;
        return Ok(());
    }

    let rows = cli::plan(&db, &specs, &kinds).await?;
    print!("{}", cli::render_plan(&rows, args.dry_run));

    if args.dry_run {
        close_within(POOL_CLOSE_DEADLINE, db.pool().close()).await;
        return Ok(());
    }

    let job = authoring_job()?;
    // The order of `kinds` is the order of `prompt::KINDS`, whatever order the
    // operator named on the command line (`cli::AuthorArgs::kinds`), and
    // `template` leads it. That order is a contract of the gate and not a
    // preference: the teach gate and the hint gate read the templates of the
    // knowledge point, `approved` AND `pending`, so a template this same process
    // stored minutes earlier gates the page and the ladder authored after it
    // (`job::served_instances`; M6 review 2, finding V1).
    for kind in kinds {
        let report = run_batch(&db, &job, kind, &specs).await?;
        print!("{}", cli::render_batch(kind, &report));
    }
    close_within(POOL_CLOSE_DEADLINE, db.pool().close()).await;
    Ok(())
}

/// Build the authoring job from the environment.
///
/// The token budget comes from [`ModelConfig::authoring_from_env`], so an
/// authoring call carries the authoring ceilings and not the diagnosis ones.
///
/// # Errors
///
/// Returns [`WorkerError::Config`] when the key is empty and when the rest of
/// the model configuration does not read.
fn authoring_job() -> Result<AuthoringJob, WorkerError> {
    if std::env::var(API_KEY_VAR)
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        return Err(WorkerError::Config(format!(
            "{API_KEY_VAR} is empty — an authoring run needs a model endpoint; \
             use `--dry-run` to print the plan without one"
        )));
    }
    // `authoring_from_env` and NOT `from_env`: an authoring call takes
    // `AUTHORING_OUTPUT_TOKENS` (4000) and `AUTHORING_REASONING_MAX_TOKENS`
    // (2000), never the diagnosis values. A diagnosis budget of 600 output
    // tokens with a reasoning ceiling of 600 beside it leaves zero visible
    // tokens for an authored document (finding F18).
    let model_cfg =
        ModelConfig::authoring_from_env().map_err(|err| WorkerError::Config(err.to_string()))?;
    let client = Client::new(model_cfg).map_err(|err| WorkerError::Config(err.to_string()))?;
    tracing::info!(
        model = %client.config().model,
        base_url = %client.config().base_url,
        output_tokens = client.config().output_tokens,
        reasoning_max_tokens = client.config().reasoning_max_tokens,
        "cadus-worker: the authoring pass is configured"
    );
    Ok(AuthoringJob::new(client))
}

/// Send the log to stderr. `RUST_LOG` overrides the default level.
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();
}

async fn run() -> Result<u64, WorkerError> {
    let db_cfg = DbConfig::from_env()?;
    let cfg = WorkerConfig::from_env()?;

    // Install the stop signals before the connect. The handlers exist from this
    // point, so a SIGTERM during the connect gives exit code 0 instead of a kill
    // by signal (finding #39).
    let mut shutdown = Shutdown::install()?;

    // The curriculum load runs BEFORE the connect, so a deployment with no
    // curriculum tree fails at once and needs no database to say so.
    let curriculum = load_arena()?;

    // `Db::connect` opens the pool AND keeps the client-side bound of
    // `DB_CLIENT_TIMEOUT_MS`. Every query below therefore runs inside that
    // bound (L1).
    let db = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-worker: the stop signal came before the database connect");
            return Ok(0);
        }
        result = Db::connect(&db_cfg) => result?,
    };

    // The worker connects as `cadus_admin`. That role holds BYPASSRLS by design:
    // it claims `diagnosis_jobs` and refills `serving_pool` across every tenant,
    // so a tenant policy would hide the rows it must process. For that reason the
    // worker logs the role but does NOT call `assert_rls_enforced`. The C3 boot
    // guard belongs to the request tier (`cadus-web`), which connects as
    // `cadus_app` and must stay inside row-level security.
    //
    // The report runs inside the same select as the connect above. A database
    // that accepts the connection and then answers no query made the old code
    // deaf to SIGTERM for the whole stall (finding #8). The report also runs
    // inside the client-side bound of `DB_CLIENT_TIMEOUT_MS` (L1).
    //
    // The stop branch returns without a pool close on purpose. The process ends
    // at that return, so the operating system closes the sockets. A wait for a
    // database that answers nothing only delays the stop the operator asked for.
    let role = tokio::select! {
        biased;
        () = shutdown.wait() => {
            tracing::info!("cadus-worker: the stop signal came before the role report");
            return Ok(0);
        }
        result = role_report(&db) => result?,
    };
    tracing::info!(
        role = %role.name,
        superuser = role.superuser,
        bypass_rls = role.bypass_rls,
        "cadus-worker: database role"
    );

    // The refill job (D-O4) reads the curriculum for the A6 exemplar fallback
    // and for the gate's knowledge-point half. A tree that does not load is
    // fatal: without it the fallback is gone, and a worker that heartbeats
    // forever while `serving_pool` stays empty gives the operator one warn line
    // and no other signal (findings #5 and #6). `load_arena` above therefore
    // ends the process with exit code 2, and its message names the path and the
    // first finding.
    let job = RefillJob::new(&curriculum);

    // The A4 diagnosis job (D-O5). A deployment with no endpoint keeps its
    // refill worker and leaves the queue standing: the learner already holds the
    // verdict, the worked solution and the re-solve instruction, so an absent
    // model costs prose and nothing else (spec section 6.5).
    let mut diagnosis = diagnosis_job()?;

    let ticks =
        cadus_worker::run_with(&db, &cfg, Some(&job), diagnosis.as_mut(), shutdown.wait()).await?;
    close_within(POOL_CLOSE_DEADLINE, db.pool().close()).await;
    Ok(ticks)
}

/// Build the diagnosis job, or `None` when the environment configures no model.
///
/// An absent or empty `OPENAI_API_KEY` is a deployment that spends no model
/// tokens. It is not an error: the queue costs nothing while it waits, and every
/// learner path stays deterministic (A3, L2).
///
/// A key that IS set makes every other model variable binding. A wrong provider
/// order or an unparseable base URL then ends the process with exit code 2,
/// because T5 pins both and a silent fallback ships 1.0's unset routing again.
///
/// # Errors
///
/// Returns [`WorkerError::Config`] when a key is set and the rest of the model
/// configuration does not read.
fn diagnosis_job() -> Result<Option<DiagnosisJob>, WorkerError> {
    let key = std::env::var(API_KEY_VAR).unwrap_or_default();
    if key.trim().is_empty() {
        tracing::info!(
            "cadus-worker: {API_KEY_VAR} is empty; the diagnosis queue waits and no model is called"
        );
        return Ok(None);
    }

    let model_cfg = ModelConfig::from_env().map_err(|err| WorkerError::Config(err.to_string()))?;
    let calls_per_session = DiagnosisJob::calls_per_session_from_env()?;
    let client = Client::new(model_cfg).map_err(|err| WorkerError::Config(err.to_string()))?;
    tracing::info!(
        model = %client.config().model,
        base_url = %client.config().base_url,
        output_tokens = client.config().output_tokens,
        reasoning_max_tokens = client.config().reasoning_max_tokens,
        calls_per_session,
        "cadus-worker: the diagnosis job is configured"
    );
    Ok(Some(DiagnosisJob::new(client, calls_per_session)))
}

/// Read the curriculum tree that `CADUS_CURRICULUM` names.
///
/// # Errors
///
/// Returns [`WorkerError::Config`] when the tree does not load. The message names
/// the path and the first finding, so an operator reads the cause in the one line
/// the process prints before it exits 2.
fn load_arena() -> Result<Curriculum, WorkerError> {
    let path = PathBuf::from(
        std::env::var(CURRICULUM_ENV).unwrap_or_else(|_| DEFAULT_CURRICULUM.to_string()),
    );
    match load_curriculum(&path) {
        Ok((curriculum, findings)) => {
            tracing::info!(
                path = %path.display(),
                topics = curriculum.topic_count(),
                findings = findings.len(),
                "cadus-worker: curriculum is loaded"
            );
            Ok(curriculum)
        }
        Err(err) => {
            let reason = first_reason(&err);
            tracing::error!(
                path = %path.display(),
                error = %reason,
                "cadus-worker: the curriculum did not load; set CADUS_CURRICULUM to a tree that does"
            );
            Err(WorkerError::Config(format!(
                "the curriculum at {} did not load: {reason}",
                path.display()
            )))
        }
    }
}

/// The first finding of a load error, or the error itself.
///
/// A fatal parse stage carries every finding, and the joined text of a large tree
/// runs to many lines. The first one names the file the operator must fix.
fn first_reason(err: &LoadError) -> String {
    let LoadError::Curriculum(CurriculumError::FatalFindings { findings }) = err else {
        return err.to_string();
    };
    match findings.first() {
        Some(finding) => format!("[{}] {}", finding.code, finding.message),
        None => err.to_string(),
    }
}

/// Read the identity of the database role under the client-side bound.
///
/// `bounded` takes a future that gives `Result<T, sqlx::Error>`, and
/// `cadus_store::current_role` gives `Result<RoleInfo, StoreError>`. The future
/// below therefore wraps the answer of the report in `Ok`, and the `?` takes it
/// out again. `bounded` adds the bound and nothing else.
async fn role_report(db: &Db) -> Result<cadus_store::RoleInfo, cadus_store::StoreError> {
    let report = async { Ok(cadus_store::current_role(db.pool()).await) };
    bounded(db, report).await?
}

/// Wait for `close` for at most `deadline`, then log the fact and give up.
///
/// `PgPool::close` waits for every checked-out connection to come back. A
/// database that answers nothing never gives one back, so the plain call runs
/// without end and the process stays alive after the stop signal until the
/// container runtime sends SIGKILL (finding #6). The bound below keeps the exit
/// inside the budget. The process exits 0 either way, because the open sockets
/// end with the process.
async fn close_within<F: Future<Output = ()>>(deadline: Duration, close: F) {
    if tokio::time::timeout(deadline, close).await.is_err() {
        tracing::info!("pool close deadline reached");
    }
}

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal. `docker stop`
/// sends SIGTERM, so that is the normal stop path of the deployment.
struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    #[cfg(unix)]
    fn install() -> Result<Self, WorkerError> {
        use tokio::signal::unix::{SignalKind, signal};

        let terminate = signal(SignalKind::terminate())
            .map_err(|err| WorkerError::Signal(format!("the SIGTERM handler failed: {err}")))?;
        let interrupt = signal(SignalKind::interrupt())
            .map_err(|err| WorkerError::Signal(format!("the SIGINT handler failed: {err}")))?;
        Ok(Self {
            terminate,
            interrupt,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    fn install() -> Result<Self, WorkerError> {
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
            _ = terminate.recv() => tracing::info!("cadus-worker: SIGTERM received"),
            _ = interrupt.recv() => tracing::info!("cadus-worker: SIGINT received"),
        }
    }

    /// Complete on Ctrl-C. A platform without unix signals has no `SIGTERM`.
    #[cfg(not(unix))]
    async fn wait(&mut self) {
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

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

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
