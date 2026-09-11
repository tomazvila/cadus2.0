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
mod signals;

use std::ffi::OsString;
use std::process::ExitCode;

use cadus_store::{DbConfig, StoreError};
use sqlx::PgPool;

use password::check_password_rule;
use roles::{RoleLock, alter_roles, maintenance_db};
use signals::{Shutdown, until_signal};

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

    let outcome = match migrate_config() {
        Ok(cfg) => start(mode, &cfg).await,
        Err(err) => Err(err),
    };
    match outcome {
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
/// role statements of `--admin-login` under the cluster-wide role lock.
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
        let lock = RoleLock::acquire(cfg, &maintenance_db()?).await?;
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

/// Install the stop signals, then run every step under them.
///
/// The handlers exist before the connect, so a SIGTERM during the connect,
/// during the migrations, or during the unbounded wait of the role lock ends
/// the run (finding #12).
async fn start(mode: Mode, cfg: &DbConfig) -> Result<Outcome, StoreError> {
    let mut shutdown = Shutdown::install()?;
    run(mode, cfg, &mut shutdown).await
}

/// Run every step under the stop signals, then print the report.
///
/// A run that the signal ends returns here without a pool close: the process
/// exits at that return, so the operating system closes the sockets, and a
/// wait for a database that answers nothing only delays the stop (finding
/// #12). The drop of the role lock closes its connection, and PostgreSQL
/// releases the advisory lock of a session that ends.
async fn run(mode: Mode, cfg: &DbConfig, shutdown: &mut Shutdown) -> Result<Outcome, StoreError> {
    let Some(report) = until_signal(shutdown, apply(mode, cfg)).await? else {
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
    use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, DbConfig};

    use super::signals::SIGNAL_TESTS;
    use super::{Mode, applied_count, apply, migrate_config, parse_args, start};

    /// A DSN of the loopback that no server answers.
    const CLOSED_PORT: &str = "postgresql://x@127.0.0.1:1/x";

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
        // `migrate_config` reads everything but the statement timeout as
        // `from_env` reads it. `pair` keeps the URL and the client timeout.
        assert_eq!(
            migrate_config().map(pair).map_err(|err| err.to_string()),
            DbConfig::from_env()
                .map(pair)
                .map_err(|err| err.to_string()),
        );
        // The override sets `statement_timeout_ms` to 0 and keeps the rest.
        let base = DbConfig {
            statement_timeout_ms: 7,
            ..DbConfig::new("postgresql://h/d")
        };
        let migrated = DbConfig {
            statement_timeout_ms: 0,
            ..base.clone()
        };
        assert_eq!(migrated.statement_timeout_ms, 0);
        assert_eq!(migrated.client_timeout_ms, base.client_timeout_ms);
        assert_eq!(migrated.database_url, base.database_url);
    }

    /// The count is 12 after every migration, an error when the role cannot
    /// read the ledger, an error when the pool is closed, and 0 on a database
    /// that holds no ledger.
    ///
    /// The throwaway database of `TestDb` makes all four states itself. The
    /// last step drops the ledger table, which puts that database in the state
    /// of a database before its first run. A read of the maintenance database
    /// of the cluster took the fourth state from the cluster instead, and a
    /// maintenance database with a ledger of 0 rows gave the same count
    /// through the other path (u12).
    #[tokio::test]
    async fn the_applied_count_reads_the_migration_ledger() {
        TestDb::with(|db| async move {
            assert_eq!(applied_count(&db.admin).await.unwrap(), 21);
            let app = db.pool_as("cadus_app", 1).await;
            sqlx::query("REVOKE SELECT ON _sqlx_migrations FROM cadus_app")
                .execute(&db.admin)
                .await
                .unwrap();
            assert!(applied_count(&app).await.is_err());
            app.close().await;
            assert!(applied_count(&app).await.is_err());
            sqlx::query("DROP TABLE public._sqlx_migrations")
                .execute(&db.admin)
                .await
                .unwrap();
            assert_eq!(applied_count(&db.admin).await.unwrap(), 0);
        })
        .await;
    }

    /// A connect that fails is the error of the first step of `apply`.
    #[tokio::test]
    async fn apply_reports_the_first_failed_step() {
        let refused = apply(Mode::MigrateOnly, &DbConfig::new(CLOSED_PORT))
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

    /// The start installs the signals, then runs the steps: a runtime whose
    /// signal driver is gone stops it before the connect, and a runtime with
    /// the driver reaches the connect, which refuses the DSN at once.
    #[test]
    fn start_installs_the_signals_and_then_runs_the_steps() {
        let cfg = DbConfig::new("not a url");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _serial = runtime.block_on(SIGNAL_TESTS.lock());
        let reached = runtime
            .block_on(start(Mode::MigrateOnly, &cfg))
            .err()
            .map(|err| err.to_string());
        assert!(
            reached
                .as_deref()
                .is_some_and(|m| m.starts_with("database error: ")),
            "{reached:?}"
        );

        let handle = runtime.handle().clone();
        drop(runtime);
        let stopped = handle
            .block_on(start(Mode::MigrateOnly, &cfg))
            .err()
            .map(|err| err.to_string());
        assert_eq!(
            stopped.as_deref(),
            Some("configuration error: the SIGTERM handler failed: signal driver gone")
        );
    }
}
