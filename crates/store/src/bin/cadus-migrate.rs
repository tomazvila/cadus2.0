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
//! argument prints the usage on stderr and exits 2.
//!
//! `--admin-login` keeps `psql` out of the runtime image. Migration 0001
//! creates `cadus_admin` as `NOLOGIN`, because a `BYPASSRLS` login is a
//! deployment decision, not a schema fact (C3). The worker DSN needs that
//! login, so the deployment grants it through this flag. The statement is
//! idempotent.
//!
//! The program runs with `statement_timeout` off. A migration waits on the
//! migration lock of sqlx and on the DDL locks of the statements it applies. A
//! cancel there is worse than a wait (finding #3).

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
                  as it is. An empty variable is an error.";

/// The environment variable that holds the new password of `cadus_app`.
const APP_PASSWORD_VAR: &str = "CADUS_APP_PASSWORD";

/// The environment variable that holds the new password of `cadus_admin`.
const ADMIN_PASSWORD_VAR: &str = "CADUS_ADMIN_PASSWORD";

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

    match run(mode).await {
        Ok(()) => ExitCode::SUCCESS,
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

async fn run(mode: Mode) -> Result<(), StoreError> {
    let cfg = migrate_config()?;
    let pool = cadus_store::connect(&cfg).await?;

    let before = applied_count(&pool).await?;
    cadus_store::migrate(&pool).await?;
    let after = applied_count(&pool).await?;

    // The report goes to stdout after the pool is closed, so a failed statement
    // never prints a success line.
    let mut password_roles: Vec<&str> = Vec::new();
    if mode == Mode::AdminLogin {
        let lock = RoleLock::acquire(&cfg).await?;
        let result = alter_roles(&pool, &mut password_roles).await;
        lock.release().await;
        result?;
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
    Ok(())
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

    use super::{Mode, alter_role_password_statement, message_is_concurrent_update, parse_args};

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
