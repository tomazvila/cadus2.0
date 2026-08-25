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

use std::ffi::OsString;
use std::future::Future;
use std::process::ExitCode;
use std::time::Duration;

use cadus_store::{DbConfig, StoreError};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};

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

/// The environment variable that holds the new password of `cadus_app`.
const APP_PASSWORD_VAR: &str = "CADUS_APP_PASSWORD";

/// The environment variable that holds the new password of `cadus_admin`.
const ADMIN_PASSWORD_VAR: &str = "CADUS_ADMIN_PASSWORD";

/// The least number of characters of a role password.
///
/// `openssl rand -hex 24` gives 48 characters, so the generator of the message
/// below stays well above this bound.
const PASSWORD_MIN_LEN: usize = 16;

/// The most characters of a role password.
const PASSWORD_MAX_LEN: usize = 128;

/// The line that a stop signal prints on stderr.
const STOPPED_BY_SIGNAL: &str = "cadus-migrate: stopped by signal";

/// The environment variable that names the maintenance database of the cluster.
const MAINTENANCE_DB_VAR: &str = "CADUS_MAINTENANCE_DB";

/// The maintenance database that applies when `CADUS_MAINTENANCE_DB` is absent.
const DEFAULT_MAINTENANCE_DB: &str = "postgres";

/// The key of the advisory lock that guards the `ALTER ROLE` statements.
///
/// 7241001 is an arbitrary but fixed number. It has one rule: every caller that
/// alters a cluster role in this repository takes this one key.
/// `crates/store/tests/migrate_bin.rs` takes the same key.
const ROLE_LOCK_KEY: i64 = 7_241_001;

/// How many times an `ALTER ROLE` statement runs before the program gives up.
const ROLE_ATTEMPT_LIMIT: u32 = 5;

/// The wait between two attempts of an `ALTER ROLE` statement.
const ROLE_RETRY_BACKOFF: Duration = Duration::from_millis(200);

/// The message that PostgreSQL reports when two sessions write one `pg_authid`
/// row at the same time. The condition is transient, so the statement runs
/// again.
const CONCURRENT_UPDATE: &str = "tuple concurrently updated";

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

        let terminate = signal(SignalKind::terminate())
            .map_err(|err| StoreError::Config(format!("the SIGTERM handler failed: {err}")))?;
        let interrupt = signal(SignalKind::interrupt())
            .map_err(|err| StoreError::Config(format!("the SIGINT handler failed: {err}")))?;
        Ok(Self {
            terminate,
            interrupt,
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

/// Check every password variable of this run against the character rule.
///
/// The function returns the message of the first variable that breaks the rule.
/// An absent variable, an empty variable, and a variable that is not valid
/// Unicode pass this check: `password_from_env` reports those three with its own
/// message, and that message names the defect better than this one.
fn check_password_rule() -> Result<(), String> {
    for var in [APP_PASSWORD_VAR, ADMIN_PASSWORD_VAR] {
        match std::env::var(var) {
            Ok(value) if value.is_empty() => (),
            Ok(value) if password_follows_rule(&value) => (),
            Ok(_) => return Err(password_rule_message(var)),
            Err(_) => (),
        }
    }
    Ok(())
}

/// Report whether a password holds allowed characters only and a length inside
/// the bounds.
///
/// The allowed set is `A-Z a-z 0-9 - _`. Every character of that set goes
/// through a `postgresql://user:password@host/db` URL unchanged, so the value
/// that reaches the role is the value that the runtime DSN carries. `@` ends the
/// user information, `#` starts a fragment, and `%` opens a percent escape, so
/// each of those three makes the DSN name another host or another password with
/// no error at all (finding #14).
///
/// The length bound is the second half of the rule: a short password is weak,
/// and a long one is a paste mistake.
fn password_follows_rule(password: &str) -> bool {
    let length = password.chars().count();
    if !(PASSWORD_MIN_LEN..=PASSWORD_MAX_LEN).contains(&length) {
        return false;
    }
    password
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Build the message that a password outside the rule prints.
///
/// The message names the variable, the allowed set, the bounds, and one command
/// that gives a value which passes.
fn password_rule_message(var: &str) -> String {
    format!(
        "{var} holds a character outside [A-Za-z0-9_-] or a length outside \
         {PASSWORD_MIN_LEN}..={PASSWORD_MAX_LEN}; generate one with: openssl rand -hex 24"
    )
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

async fn run(mode: Mode, shutdown: &mut Shutdown) -> Result<Outcome, StoreError> {
    let cfg = migrate_config()?;

    // Every await of this function runs under the stop signals. A run that the
    // signal ends returns here without a pool close: the process exits at that
    // return, so the operating system closes the sockets, and a wait for a
    // database that answers nothing only delays the stop (finding #12).
    let Some(pool) = until_signal(shutdown, cadus_store::connect(&cfg)).await? else {
        return Ok(Outcome::Stopped);
    };

    let Some(before) = until_signal(shutdown, applied_count(&pool)).await? else {
        return Ok(Outcome::Stopped);
    };
    if until_signal(shutdown, cadus_store::migrate(&pool))
        .await?
        .is_none()
    {
        return Ok(Outcome::Stopped);
    }
    let Some(after) = until_signal(shutdown, applied_count(&pool)).await? else {
        return Ok(Outcome::Stopped);
    };

    // The report goes to stdout after the pool is closed, so a failed statement
    // never prints a success line.
    let mut password_roles: Vec<&str> = Vec::new();
    if mode == Mode::AdminLogin {
        let Some(lock) = until_signal(shutdown, RoleLock::acquire(&cfg)).await? else {
            return Ok(Outcome::Stopped);
        };
        let result = until_signal(shutdown, alter_roles(&pool, &mut password_roles)).await;
        lock.release().await;
        if result?.is_none() {
            return Ok(Outcome::Stopped);
        }
    }

    pool.close().await;
    println!(
        "cadus-migrate: applied {} migrations ({} total)",
        after - before,
        after
    );
    if mode == Mode::AdminLogin {
        println!("cadus-migrate: cadus_admin LOGIN granted");
    }
    for role in password_roles {
        println!("cadus-migrate: password set for {role}");
    }
    Ok(Outcome::Done)
}

/// A cluster-wide lock on the `ALTER ROLE` statements of this program.
///
/// Roles are cluster-scoped. Two migrate runs on one cluster (two databases, or
/// a test next to a deploy) that alter one role at the same time fail with
/// "tuple concurrently updated". PostgreSQL scopes an advisory lock to the
/// database of the session, so a lock on the database of `DATABASE_URL`
/// serializes nothing between two databases (finding #2). This lock therefore
/// opens one connection to the maintenance database of the cluster, and every
/// run on the cluster shares that database.
struct RoleLock(PgConnection);

impl RoleLock {
    /// Take the lock. The call waits until every other holder gives it back.
    ///
    /// The connection carries `statement_timeout` 0, so the wait has no bound.
    async fn acquire(cfg: &DbConfig) -> Result<RoleLock, StoreError> {
        let database = maintenance_db()?;
        let options = cadus_store::connect_options(cfg)?.database(&database);
        let mut conn = PgConnection::connect_with(&options).await?;
        // The key is a constant of this program, so no input reaches the text.
        // `pg_advisory_lock` returns void, which the checked macros do not map,
        // so this statement stays outside them (R2).
        sqlx::query(AssertSqlSafe(format!(
            "SELECT pg_advisory_lock({ROLE_LOCK_KEY})"
        )))
        .execute(&mut conn)
        .await?;
        Ok(RoleLock(conn))
    }

    /// Give the lock back and close the connection.
    ///
    /// PostgreSQL releases the advisory locks of a session that ends, so the
    /// close alone gives the lock back. A failed unlock therefore stops nothing
    /// and this program ignores it.
    async fn release(self) {
        let mut conn = self.0;
        let _ = sqlx::query(AssertSqlSafe(format!(
            "SELECT pg_advisory_unlock({ROLE_LOCK_KEY})"
        )))
        .execute(&mut conn)
        .await;
        let _ = conn.close().await;
    }
}

/// Read the name of the maintenance database.
///
/// The name must be a database of the cluster that every run reaches.
/// `postgres` is the name that a default PostgreSQL cluster carries. A cluster
/// without that database gives the operator `CADUS_MAINTENANCE_DB`.
fn maintenance_db() -> Result<String, StoreError> {
    match std::env::var(MAINTENANCE_DB_VAR) {
        Ok(value) if value.is_empty() => {
            Err(StoreError::Config(format!("{MAINTENANCE_DB_VAR} is empty")))
        }
        Ok(value) => Ok(value),
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_MAINTENANCE_DB.to_string()),
        Err(std::env::VarError::NotUnicode(_)) => Err(StoreError::Config(format!(
            "{MAINTENANCE_DB_VAR} is not valid Unicode"
        ))),
    }
}

/// Grant the admin login and set the role passwords that the environment
/// names. The caller holds the cluster-wide role lock.
async fn alter_roles(
    pool: &sqlx::PgPool,
    password_roles: &mut Vec<&'static str>,
) -> Result<(), StoreError> {
    grant_admin_login(pool).await?;
    for (role, var) in [
        ("cadus_app", APP_PASSWORD_VAR),
        ("cadus_admin", ADMIN_PASSWORD_VAR),
    ] {
        if let Some(password) = password_from_env(var)? {
            set_role_password(pool, role, &password).await?;
            password_roles.push(role);
        }
    }
    Ok(())
}

/// Read a password variable.
///
/// The function returns `None` when the variable is absent, and
/// `StoreError::Config` when the variable is empty or not valid Unicode. An
/// empty password is a configuration mistake, not a request to clear the
/// password, so the program stops instead of guessing.
fn password_from_env(var: &str) -> Result<Option<String>, StoreError> {
    match std::env::var(var) {
        Ok(value) if value.is_empty() => Err(StoreError::Config(format!("{var} is empty"))),
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(StoreError::Config(format!("{var} is not valid Unicode")))
        }
    }
}

/// Build the `ALTER ROLE ... PASSWORD` statement of a role.
///
/// `ALTER ROLE` is a utility statement. PostgreSQL parses it before it binds
/// parameters, so `$1` is impossible here and the password goes into the
/// statement text. The text becomes an SQL string literal with every single
/// quote doubled. `standard_conforming_strings` is on by default, so a
/// backslash carries no escape meaning and the doubled quote is the only escape
/// that the literal needs. Without the doubled quote a password of the form
/// `x' SUPERUSER --` ends the literal and adds role options to a statement that
/// a superuser runs (finding #8). The role name is a constant of this program
/// and never comes from input.
fn alter_role_password_statement(role: &str, password: &str) -> String {
    let literal = password.replace('\'', "''");
    format!("ALTER ROLE {role} PASSWORD '{literal}'")
}

/// Set the password of a role.
///
/// The deployment needs this step because the runtime image carries no `psql`,
/// and because a password lets the database refuse `trust` authentication.
///
/// NOTE: the statement text holds the password. Keep `log_statement` off on the
/// production cluster.
async fn set_role_password(pool: &PgPool, role: &str, password: &str) -> Result<(), StoreError> {
    run_role_statement(pool, alter_role_password_statement(role, password)).await
}

/// Give the `cadus_admin` role a login.
///
/// `ALTER ROLE` is a utility statement with no result columns and no bind
/// parameters, so it stays outside the compile-time checked macros (R2). The
/// text is a constant, so no input reaches the statement.
async fn grant_admin_login(pool: &PgPool) -> Result<(), StoreError> {
    run_role_statement(pool, "ALTER ROLE cadus_admin LOGIN".to_string()).await
}

/// Run one `ALTER ROLE` statement. Run it again after a
/// "tuple concurrently updated" error.
///
/// The role lock serializes every run that takes it. The retry is the second
/// guard: it covers a writer that the lock does not reach, for example a `psql`
/// session of an operator, or a run of an older version of this program.
async fn run_role_statement(pool: &PgPool, sql: String) -> Result<(), StoreError> {
    let mut attempt: u32 = 1;
    loop {
        let outcome = sqlx::query(AssertSqlSafe(sql.clone())).execute(pool).await;
        let Err(err) = outcome else {
            return Ok(());
        };
        if attempt >= ROLE_ATTEMPT_LIMIT || !is_concurrent_update(&err) {
            return Err(StoreError::Db(err));
        }
        attempt += 1;
        tokio::time::sleep(ROLE_RETRY_BACKOFF).await;
    }
}

/// Report whether the error is the transient "tuple concurrently updated"
/// error. An error of another kind stops the run at the first attempt.
fn is_concurrent_update(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db_err) => message_is_concurrent_update(db_err.message()),
        _ => false,
    }
}

/// Report whether a database message names the transient condition.
///
/// PostgreSQL reports "tuple concurrently updated" with SQLSTATE XX000, the
/// code of every internal error, so the message is the one part that names this
/// condition and the code separates nothing.
fn message_is_concurrent_update(message: &str) -> bool {
    message.contains(CONCURRENT_UPDATE)
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

    use super::{
        Mode, alter_role_password_statement, message_is_concurrent_update, parse_args,
        password_follows_rule, password_rule_message,
    };

    /// Finding #8: a single quote in the password becomes two single quotes, so
    /// the password stays inside the SQL string literal.
    #[test]
    fn a_quote_in_the_password_becomes_two_quotes() {
        assert_eq!(
            alter_role_password_statement("cadus_app", "a'b"),
            "ALTER ROLE cadus_app PASSWORD 'a''b'"
        );
    }

    /// Finding #8: the injection form of the review closes no literal.
    #[test]
    fn the_injection_form_stays_inside_the_literal() {
        assert_eq!(
            alter_role_password_statement("cadus_app", "s3cret' SUPERUSER --"),
            "ALTER ROLE cadus_app PASSWORD 's3cret'' SUPERUSER --'"
        );
    }

    /// A password without a quote passes through unchanged.
    #[test]
    fn a_password_without_a_quote_passes_through() {
        assert_eq!(
            alter_role_password_statement("cadus_admin", "pw-0123abcd"),
            "ALTER ROLE cadus_admin PASSWORD 'pw-0123abcd'"
        );
    }

    /// Finding #13: an argument that is not valid Unicode is an unknown
    /// argument, not a panic.
    #[test]
    fn a_non_unicode_argument_is_unknown() {
        let bad = OsString::from_vec(vec![0xff]);
        assert_eq!(parse_args(&[bad]), None);
    }

    /// Finding #2, second guard: the retry runs for the transient condition
    /// and for no other error.
    #[test]
    fn the_retry_reads_the_concurrent_update_message() {
        assert!(message_is_concurrent_update(
            "tuple concurrently updated at line 1258"
        ));
        assert!(!message_is_concurrent_update(
            "permission denied for table users"
        ));
        assert!(!message_is_concurrent_update(
            "role \"cadus_app\" does not exist"
        ));
    }

    /// Finding #14: a character outside `[A-Za-z0-9_-]` breaks the rule. Each
    /// character below changes the meaning of a DSN.
    #[test]
    fn a_character_outside_the_set_breaks_the_rule() {
        for password in [
            "corr@horse#battery1",
            "0123456789abcdef@",
            "0123456789abcdef#",
            "0123456789abcdef%",
            "0123456789abcdef/",
            "0123456789abcdef:",
            "0123456789abcdef?",
            "0123456789 abcdef",
            "0123456789abcdef'",
            "0123456789abcdéf",
        ] {
            assert!(
                !password_follows_rule(password),
                "{password:?} must break the rule"
            );
        }
    }

    /// Finding #14: the length bound is 16..=128, and both ends are inclusive.
    #[test]
    fn the_length_bounds_are_sixteen_and_one_hundred_twenty_eight() {
        assert!(!password_follows_rule(&"a".repeat(15)));
        assert!(password_follows_rule(&"a".repeat(16)));
        assert!(password_follows_rule(&"a".repeat(128)));
        assert!(!password_follows_rule(&"a".repeat(129)));
        assert!(!password_follows_rule(""));
    }

    /// Finding #14: the output of the generator that the message names passes
    /// the rule, and so does every character of the allowed set.
    #[test]
    fn the_generated_password_follows_the_rule() {
        // 48 characters, the length of `openssl rand -hex 24`.
        assert!(password_follows_rule(
            "9f2c1d4b7a6e0358cf91d24e7b60a5c38d1f4e29b70c6a55"
        ));
        assert!(password_follows_rule(
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
        ));
    }

    /// Finding #14: the message names the variable, the set, the bounds, and
    /// the generator.
    #[test]
    fn the_rule_message_is_the_literal_line() {
        assert_eq!(
            password_rule_message("CADUS_APP_PASSWORD"),
            "CADUS_APP_PASSWORD holds a character outside [A-Za-z0-9_-] or a length outside \
             16..=128; generate one with: openssl rand -hex 24"
        );
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
}
