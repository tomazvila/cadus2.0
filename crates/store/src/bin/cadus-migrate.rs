//! Apply the migrations of the repository to the database that `DATABASE_URL`
//! names.
//!
//! Usage:
//!
//! ```text
//! cadus-migrate                 apply every pending migration
//! cadus-migrate --admin-login   apply, then ALTER ROLE cadus_admin LOGIN
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
use sqlx::PgPool;

/// The usage text. An unknown argument prints it on stderr.
const USAGE: &str = "\
usage: cadus-migrate [--admin-login]

Apply every pending migration to the database that DATABASE_URL names.

  --admin-login   After the migrations, run ALTER ROLE cadus_admin LOGIN.
                  Migration 0001 creates cadus_admin as NOLOGIN. The worker
                  DSN needs the login, so the deployment grants it here.
                  The statement is idempotent.";

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

    if mode == Mode::AdminLogin {
        grant_admin_login(&pool).await?;
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
