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

use std::process::ExitCode;

use cadus_store::{DbConfig, StoreError};
use sqlx::{AssertSqlSafe, PgPool};

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
    let args: Vec<String> = std::env::args().skip(1).collect();
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
/// does not know.
fn parse_args(args: &[String]) -> Option<Mode> {
    match args {
        [] => Some(Mode::MigrateOnly),
        [flag] if flag == "--admin-login" => Some(Mode::AdminLogin),
        _ => None,
    }
}

async fn run(mode: Mode) -> Result<(), StoreError> {
    let cfg = DbConfig::from_env()?;
    let pool = cadus_store::connect(&cfg).await?;

    let before = applied_count(&pool).await?;
    cadus_store::migrate(&pool).await?;
    let after = applied_count(&pool).await?;

    // The report goes to stdout after the pool is closed, so a failed statement
    // never prints a success line.
    let mut password_roles: Vec<&str> = Vec::new();
    if mode == Mode::AdminLogin {
        grant_admin_login(&pool).await?;
        for (role, var) in [
            ("cadus_app", APP_PASSWORD_VAR),
            ("cadus_admin", ADMIN_PASSWORD_VAR),
        ] {
            if let Some(password) = password_from_env(var)? {
                set_role_password(&pool, role, &password).await?;
                password_roles.push(role);
            }
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

/// Set the password of a role.
///
/// The deployment needs this step because the runtime image carries no `psql`,
/// and because a password lets the database refuse `trust` authentication.
///
/// `ALTER ROLE` is a utility statement. PostgreSQL parses it before it binds
/// parameters, so `$1` is impossible here and the password goes into the
/// statement text. The text becomes an SQL string literal with every single
/// quote doubled. `standard_conforming_strings` is on by default, so a
/// backslash carries no escape meaning and the doubled quote is the only
/// escape that the literal needs. The role name is a constant of this program
/// and never comes from input.
///
/// NOTE: the statement text holds the password. Keep `log_statement` off on the
/// production cluster.
async fn set_role_password(pool: &PgPool, role: &str, password: &str) -> Result<(), StoreError> {
    let literal = password.replace('\'', "''");
    sqlx::query(AssertSqlSafe(format!(
        "ALTER ROLE {role} PASSWORD '{literal}'"
    )))
    .execute(pool)
    .await?;
    Ok(())
}

/// Give the `cadus_admin` role a login.
///
/// `ALTER ROLE` is a utility statement with no result columns and no bind
/// parameters, so it stays outside the compile-time checked macros (R2). The
/// text is a constant, so no input reaches the statement.
async fn grant_admin_login(pool: &PgPool) -> Result<(), StoreError> {
    sqlx::query("ALTER ROLE cadus_admin LOGIN")
        .execute(pool)
        .await?;
    Ok(())
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
