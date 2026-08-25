//! Apply the migrations of the repository to the database that `DATABASE_URL`
//! names.
//!
//! The program prints the number of migrations that this run applied and exits
//! 0. On an error it prints the error on stderr and exits 2.

use std::process::ExitCode;

use cadus_store::{DbConfig, StoreError};
use sqlx::PgPool;

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("cadus-migrate: {err}");
            ExitCode::from(2)
        }
    }
}

async fn run() -> Result<(), StoreError> {
    let cfg = DbConfig::from_env()?;
    let pool = cadus_store::connect(&cfg).await?;

    let before = applied_count(&pool).await?;
    cadus_store::migrate(&pool).await?;
    let after = applied_count(&pool).await?;

    pool.close().await;
    println!(
        "cadus-migrate: applied {} migrations ({} total)",
        after - before,
        after
    );
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
