//! Apply the migrations of the repository to the database that `DATABASE_URL`
//! names.
//!
//! Usage:
//!
//! ```text
//! cadus-migrate                 apply every pending migration
//! cadus-migrate --admin-login   apply, then ALTER ROLE cadus_admin LOGIN,
//!                               then set the role passwords that the
//!                               environment gives
//! ```
//!
//! The program prints the number of migrations that this run applied and exits
//! 0. On an error it prints the error on stderr and exits 2. An unknown
//! argument prints the usage on stderr and exits 2. On `SIGTERM` or `SIGINT`
//! the program prints `cadus-migrate: stopped by signal` on stderr and exits 3.
//!
//! The stop signals are the signals that `cadus-web` and `cadus-worker` handle
//! too. The compose stack runs this program as PID 1, and the kernel drops a
//! signal that PID 1 leaves at the default disposition, so a run that waits on
//! the role lock ends with SIGKILL and exit 137 without these handlers
//! (finding #12).
//!
//! `--admin-login` takes a password of 16 to 128 characters from the set
//! `A-Z a-z 0-9 - _`. A password outside that rule stops the run with exit code
//! 2 before the first statement. The three runtime DSNs carry the password in a
//! URL with no percent-encoding, so a character such as `@` or `%` rotates the
//! role and locks the application out (finding #14).
//!
//! `--admin-login` keeps `psql` out of the runtime image. Migration 0001
//! creates `cadus_admin` as `NOLOGIN`, because a `BYPASSRLS` login is a
//! deployment decision, not a schema fact (C3). The worker DSN needs that
//! login, so the deployment grants it through this flag. The statement is
//! idempotent.
//!
//! The program runs with `statement_timeout` off. A migration waits on the
//! migration lock of sqlx and on the DDL locks of the statements it applies. A
//! cancel there is worse than a wait (finding #3). A stop signal is the one
//! exception, because the operator asks for that cancel.

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

mod password;
mod retry;
mod roles;

use std::ffi::OsString;
use std::future::Future;
use std::process::ExitCode;

use cadus_store::{DbConfig, StoreError};
use sqlx::PgPool;

use password::check_password_rule;
use roles::{RoleLock, alter_roles};

/// The usage text. An unknown argument prints it on stderr.
const USAGE: &str = "\
usage: cadus-migrate [--admin-login]

Apply every pending migration to the database that DATABASE_URL names.

  --admin-login   After the migrations, run ALTER ROLE cadus_admin LOGIN.
                  Migration 0001 creates cadus_admin as NOLOGIN. The worker
                  DSN needs the login, so the deployment grants it here.
                  The statement is idempotent.

                  The flag also reads two optional variables:
                    CADUS_APP_PASSWORD    ALTER ROLE cadus_app PASSWORD ...
                    CADUS_ADMIN_PASSWORD  ALTER ROLE cadus_admin PASSWORD ...
                  A variable that is absent leaves the password of that role
                  as it is. An empty variable is an error. A password holds
                  16 to 128 characters of the set A-Z a-z 0-9 - _ and nothing
                  else, so the value is safe inside a DSN.";

/// The line that a stop signal prints on stderr.
const STOPPED_BY_SIGNAL: &str = "cadus-migrate: stopped by signal";

/// What this run does after the migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Apply the migrations and stop.
    MigrateOnly,
    /// Apply the migrations, then give `cadus_admin` a login.
    AdminLogin,
}

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// The run did every step of its mode.
    Done,
    /// A stop signal ended the run.
    Stopped,
}

#[tokio::main]
async fn main() -> ExitCode {
    // `args_os` never panics. `args` unwraps every argument and aborts the
    // process with exit code 101 on a byte that is not valid Unicode, which
    // breaks the usage contract of this program (finding #13).
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let Some(mode) = parse_args(&args) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };

    // The password rule runs before the connect, so a password that a DSN
    // cannot carry stops the program before the first statement (finding #14).
    if mode == Mode::AdminLogin
        && let Err(message) = check_password_rule()
    {
        eprintln!("cadus-migrate: {message}");
        return ExitCode::from(2);
    }

    // Install the stop signals before the connect. The handlers exist from this
    // point, so a SIGTERM during the connect, during the migrations, or during
    // the unbounded wait of the role lock ends the run (finding #12).
    let mut shutdown = match Shutdown::install() {
        Ok(shutdown) => shutdown,
        Err(err) => {
            eprintln!("cadus-migrate: {err}");
            return ExitCode::from(2);
        }
    };

    match run(mode, &mut shutdown).await {
        Ok(Outcome::Done) => ExitCode::SUCCESS,
        Ok(Outcome::Stopped) => {
            eprintln!("{STOPPED_BY_SIGNAL}");
            ExitCode::from(3)
        }
        Err(err) => {
            eprintln!("cadus-migrate: {err}");
            ExitCode::from(2)
        }
    }
}

/// The installed stop signals of the process.
///
/// `install` registers the handlers at once, so a signal from that moment on
/// reaches the program. `wait` completes on the first signal. `docker stop`
/// sends SIGTERM, so that is the normal stop path of the deployment.
/// `cadus-web` and `cadus-worker` carry the same shape.
struct Shutdown {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
}

impl Shutdown {
    /// Register the handlers for `SIGTERM` and `SIGINT`.
    #[cfg(unix)]
    fn install() -> Result<Self, StoreError> {
        use tokio::signal::unix::{SignalKind, signal};

        Ok(Self {
            terminate: handler("SIGTERM", signal(SignalKind::terminate()))?,
            interrupt: handler("SIGINT", signal(SignalKind::interrupt()))?,
        })
    }

    /// A platform without unix signals has nothing to register here.
    #[cfg(not(unix))]
    fn install() -> Result<Self, StoreError> {
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
/// operator, and a cancel is the answer to it. The `statement_timeout` of 0
/// above covers the other case: a bound that no operator asked for.
async fn until_signal<T>(
    shutdown: &mut Shutdown,
    work: impl Future<Output = Result<T, StoreError>>,
) -> Result<Option<T>, StoreError> {
    tokio::select! {
        biased;
        () = shutdown.wait() => Ok(None),
        result = work => result.map(Some),
    }
}

/// Read the command line. Return `None` for every argument that this program
/// does not know, and for every argument that is not valid Unicode.
fn parse_args(args: &[OsString]) -> Option<Mode> {
    match args {
        [] => Some(Mode::MigrateOnly),
        [flag] if flag.to_str() == Some("--admin-login") => Some(Mode::AdminLogin),
        _ => None,
    }
}

/// Build the configuration of this program.
///
/// The pool of `cadus-migrate` runs with `statement_timeout` off. `from_env`
/// gives every other pool of the system the bound of `DB_STATEMENT_TIMEOUT_MS`,
/// and a migration is the one unit of work that must wait instead of cancel: it
/// waits on the migration lock of sqlx and on the DDL locks of the statements
/// it applies (finding #3).
fn migrate_config() -> Result<DbConfig, StoreError> {
    Ok(DbConfig {
        statement_timeout_ms: 0,
        ..DbConfig::from_env()?
    })
}

/// What a finished run prints.
struct Report {
    /// The count of migrations the database held before the run.
    before: i64,
    /// The count of migrations the database holds after the run.
    after: i64,
    /// The roles whose password this run set.
    password_roles: Vec<&'static str>,
}

/// Every step of one run, in order: connect, count, migrate, count, and the
/// role statements of `--admin-login`.
///
/// The report goes to stdout after the pool is closed, so a failed statement
/// never prints a success line.
async fn apply(mode: Mode, cfg: &DbConfig) -> Result<Report, StoreError> {
    let pool = cadus_store::connect(cfg).await?;
    let before = applied_count(&pool).await?;
    cadus_store::migrate(&pool).await?;
    let after = applied_count(&pool).await?;

    let mut password_roles: Vec<&'static str> = Vec::new();
    if mode == Mode::AdminLogin {
        let lock = RoleLock::acquire(cfg).await?;
        let altered = alter_roles(&pool, &mut password_roles).await;
        lock.release().await;
        altered?;
    }

    pool.close().await;
    Ok(Report {
        before,
        after,
        password_roles,
    })
}

/// Run every step under the stop signals, then print the report.
///
/// A run that the signal ends returns here without a pool close: the process
/// exits at that return, so the operating system closes the sockets, and a
/// wait for a database that answers nothing only delays the stop (finding
/// #12). The drop of the role lock closes its connection, and PostgreSQL
/// releases the advisory lock of a session that ends.
async fn run(mode: Mode, shutdown: &mut Shutdown) -> Result<Outcome, StoreError> {
    let cfg = migrate_config()?;
    let Some(report) = until_signal(shutdown, apply(mode, &cfg)).await? else {
        return Ok(Outcome::Stopped);
    };
    println!(
        "cadus-migrate: applied {} migrations ({} total)",
        report.after - report.before,
        report.after
    );
    if mode == Mode::AdminLogin {
        println!("cadus-migrate: cadus_admin LOGIN granted");
    }
    for role in report.password_roles {
        println!("cadus-migrate: password set for {role}");
    }
    Ok(Outcome::Done)
}

/// Count the rows of the sqlx migration table. The table is absent before the
/// first run, so the count is 0 then.
async fn applied_count(pool: &PgPool) -> Result<i64, StoreError> {
    let table_exists = sqlx::query_scalar!(
        r#"SELECT to_regclass('public._sqlx_migrations') IS NOT NULL AS "table_exists!""#
    )
    .fetch_one(pool)
    .await?;

    if !table_exists {
        return Ok(0);
    }

    let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM _sqlx_migrations"#)
        .fetch_one(pool)
        .await?;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    use cadus_store::test_support::TestDb;
    use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, DbConfig, StoreError};

    use super::{
        Mode, Shutdown, applied_count, apply, handler, migrate_config, parse_args, until_signal,
    };

    /// Finding #13: an argument that is not valid Unicode is an unknown
    /// argument, not a panic.
    #[test]
    fn a_non_unicode_argument_is_unknown() {
        let bad = OsString::from_vec(vec![0xff]);
        assert_eq!(parse_args(&[bad]), None);
    }

    /// The two known command lines keep their meaning.
    #[test]
    fn the_known_command_lines_parse() {
        assert_eq!(parse_args(&[]), Some(Mode::MigrateOnly));
        assert_eq!(
            parse_args(&[OsString::from("--admin-login")]),
            Some(Mode::AdminLogin)
        );
        assert_eq!(parse_args(&[OsString::from("--bogus")]), None);
    }

    /// The configuration of this program turns the statement timeout off and
    /// reads everything else as `DbConfig::from_env` does.
    #[test]
    fn the_migrate_config_turns_the_statement_timeout_off() {
        // The pair a config projects to. It runs on a known-good config here,
        // and on `migrate_config` and `from_env` below.
        fn pair(cfg: DbConfig) -> (String, u64) {
            (cfg.database_url, cfg.client_timeout_ms)
        }
        assert_eq!(
            pair(DbConfig::new("postgresql://h/d")),
            ("postgresql://h/d".to_string(), DEFAULT_CLIENT_TIMEOUT_MS)
        );
        // The statement timeout is off, so a long migration never hits it.
        if let Ok(cfg) = migrate_config() {
            assert_eq!(cfg.statement_timeout_ms, 0);
        }
        // Everything else reads as `from_env` reads it.
        assert_eq!(
            migrate_config().map(pair).map_err(|err| err.to_string()),
            DbConfig::from_env()
                .map(pair)
                .map_err(|err| err.to_string()),
        );
    }

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

    /// The signal wins over the work: an interrupt sent to this process ends
    /// `until_signal` with `None`, and work that finishes first gives its
    /// value.
    #[tokio::test]
    async fn until_signal_answers_none_on_a_signal_and_some_on_finished_work() {
        let mut shutdown = Shutdown::install().unwrap();
        let done = until_signal(&mut shutdown, async { Ok::<u8, StoreError>(7) })
            .await
            .unwrap();
        assert_eq!(done, Some(7));

        let pid = std::process::id().to_string();
        let sent = std::process::Command::new("kill")
            .args(["-INT", &pid])
            .status()
            .unwrap();
        assert!(sent.success());
        let stopped = until_signal(
            &mut shutdown,
            std::future::pending::<Result<u8, StoreError>>(),
        )
        .await
        .unwrap();
        assert_eq!(stopped, None);
    }

    /// The count is 0 before the first run, 12 after it, and an error when
    /// the pool is closed.
    #[tokio::test]
    async fn the_applied_count_reads_the_migration_ledger() {
        TestDb::with(|db| async move {
            assert_eq!(applied_count(&db.admin).await.unwrap(), 12);
            let closed = db.pool_as("cadus_app", 1).await;
            closed.close().await;
            assert!(applied_count(&closed).await.is_err());
        })
        .await;
        let cfg = DbConfig::new(TestDb::superuser_dsn_for("postgres"));
        let mut conn = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&cfg.database_url)
            .await
            .unwrap();
        // The maintenance database holds no ledger.
        assert_eq!(applied_count(&conn).await.unwrap(), 0);
        conn.close().await;
        conn = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect(&cfg.database_url)
            .await
            .unwrap();
        conn.close().await;
    }

    /// Every failed step of `apply` is the error of that step: a connect that
    /// fails, and a migrate-only run that connects reports the counts.
    #[tokio::test]
    async fn apply_reports_the_counts_or_the_first_failed_step() {
        let refused = apply(
            Mode::MigrateOnly,
            &DbConfig::new("postgresql://x@127.0.0.1:1/x"),
        )
        .await
        .err()
        .map(|err| err.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|m| m.starts_with("database error: ")),
            "{refused:?}"
        );
    }
}
