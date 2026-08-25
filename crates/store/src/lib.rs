//! Postgres adapter for the core: connection pool, migrations, and tenant scoping.
//!
//! Requirements: C2 (append-only `events`), C3 (row-level security and the boot
//! guard), R2 (sqlx with compile-time checked queries), D9 (schema lineage).
//!
//! Every statement with a fixed shape goes through `sqlx::query!` or
//! `sqlx::query_scalar!`, so the compiler checks it against the schema (R2). The
//! generated data in `.sqlx/` makes an offline build possible.

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

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[cfg(feature = "test-support")]
pub mod test_support;

/// The migration set of the repository. `sqlx::migrate!` embeds the files at
/// compile time, so the binaries carry the schema and need no file access.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// The connection configuration of the store.
#[derive(Clone)]
pub struct DbConfig {
    pub database_url: String,
}

impl DbConfig {
    /// Read `DATABASE_URL` from the environment.
    ///
    /// The function returns `StoreError::Config` when the variable is absent,
    /// empty, or not valid Unicode.
    pub fn from_env() -> Result<Self, StoreError> {
        match std::env::var("DATABASE_URL") {
            Ok(url) if url.is_empty() => {
                Err(StoreError::Config("DATABASE_URL is empty".to_string()))
            }
            Ok(url) => Ok(Self { database_url: url }),
            Err(std::env::VarError::NotPresent) => {
                Err(StoreError::Config("DATABASE_URL is not set".to_string()))
            }
            Err(std::env::VarError::NotUnicode(_)) => Err(StoreError::Config(
                "DATABASE_URL is not valid Unicode".to_string(),
            )),
        }
    }
}

/// The connection string holds the database password. Keep it out of every log
/// line and every panic message.
impl std::fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("database_url", &"<redacted>")
            .finish()
    }
}

/// The identity and the row-level-security status of the connected role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleInfo {
    pub name: String,
    pub superuser: bool,
    pub bypass_rls: bool,
}

/// The error type of the store.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The configuration is absent or malformed.
    #[error("configuration error: {0}")]
    Config(String),

    /// The database rejected a statement or the connection failed.
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),

    /// A migration failed to apply.
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    /// C3 boot guard: the connected role escapes row-level security.
    #[error(
        "role {role} bypasses row-level security (superuser={superuser}, bypassrls={bypass_rls})"
    )]
    RlsBypass {
        role: String,
        superuser: bool,
        bypass_rls: bool,
    },
}

/// Open a connection pool with the given configuration.
pub async fn connect(cfg: &DbConfig) -> Result<PgPool, StoreError> {
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .connect(&cfg.database_url)
        .await?;
    tracing::debug!("store: connection pool is open");
    Ok(pool)
}

/// Apply every migration that the database does not have yet.
///
/// The call is idempotent: a second run applies nothing and returns `Ok`.
pub async fn migrate(pool: &PgPool) -> Result<(), StoreError> {
    MIGRATOR.run(pool).await?;
    tracing::info!("store: migrations are up to date");
    Ok(())
}

/// Read the identity and the row-level-security status of the connected role.
pub async fn current_role(pool: &PgPool) -> Result<RoleInfo, StoreError> {
    let row = sqlx::query!(
        r#"
        SELECT current_user::text AS "name!",
               r.rolsuper        AS "superuser!",
               r.rolbypassrls    AS "bypass_rls!"
        FROM pg_roles r
        WHERE r.rolname = current_user
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(RoleInfo {
        name: row.name,
        superuser: row.superuser,
        bypass_rls: row.bypass_rls,
    })
}

/// C3 boot guard. Return `Ok` only when row-level security applies to the
/// connected role.
///
/// A superuser and a `BYPASSRLS` role both read every tenant, so the application
/// refuses to start with such a role.
pub async fn assert_rls_enforced(pool: &PgPool) -> Result<RoleInfo, StoreError> {
    let info = current_role(pool).await?;
    if info.superuser || info.bypass_rls {
        tracing::error!(
            role = %info.name,
            superuser = info.superuser,
            bypass_rls = info.bypass_rls,
            "store: the database role bypasses row-level security"
        );
        return Err(StoreError::RlsBypass {
            role: info.name,
            superuser: info.superuser,
            bypass_rls: info.bypass_rls,
        });
    }
    Ok(info)
}

/// Start a transaction and bind the tenant to it.
///
/// `set_config(..., true)` makes the setting local to the transaction, so the
/// tenant context goes away when the transaction ends and a pooled connection
/// carries no tenant into the next unit of work.
pub async fn begin_tenant(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Transaction<'static, Postgres>, StoreError> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "SELECT set_config('app.user_id', $1, true)",
        user_id.to_string()
    )
    .fetch_one(&mut *tx)
    .await?;
    Ok(tx)
}
