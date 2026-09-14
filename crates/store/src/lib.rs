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

use std::future::Future;
use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub mod auth;
pub mod config;
pub mod content;
pub mod diagnosis;
pub mod integrated;
pub mod pool;
pub mod shutdown;
pub mod state;
pub mod reports;

pub use config::{
    CLIENT_TIMEOUT_VAR, DEFAULT_CLIENT_TIMEOUT_MS, DEFAULT_STATEMENT_TIMEOUT_MS, DbConfig,
    STATEMENT_TIMEOUT_VAR,
};

#[cfg(feature = "test-support")]
pub mod test_support;

/// The migration set of the repository. `sqlx::migrate!` embeds the files at
/// compile time, so the binaries carry the schema and need no file access.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

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

    /// A query ran longer than the client-side bound.
    #[error("the query did not answer within {after_ms} ms")]
    Timeout { after_ms: u64 },

    /// A `serving_pool` document did not read or did not write (M4 U4).
    #[error("pool document error: {0}")]
    Body(#[from] cadus_core::pool::PoolBodyError),

    /// A `serving_pool` row carries a value this build does not know, or a
    /// claim did not take the row the pop locked.
    #[error("serving pool error: {0}")]
    PoolRow(String),

    /// A stored document did not read or did not write: an `events.payload`, a
    /// `learner_models.model`, or the config preimage (M5 U6).
    #[error("document error: {0}")]
    Document(String),

    /// The fold refused the stream (M5 U6).
    #[error("projection error: {0}")]
    Projector(#[from] cadus_core::projector::ProjectorError),

    /// An auth statement did not hold the M5 call order (`docs/SCHEMA.md`).
    #[error("auth error: {0}")]
    Auth(String),

    /// A statement addressed a row that the table does not hold (R4, C6).
    ///
    /// The M6 admin routes answer 404 to this variant, so a reviewer who
    /// approves a digest that is gone reads "not found" and never "approved"
    /// (`docs/reference/authoring-and-spa-1.0-spec.md` section 3.2).
    #[error("no {entity} row with key {key}")]
    NotFound { entity: &'static str, key: String },

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

/// How long an acquire waits for a connection before it gives up.
///
/// The sqlx default is 30 s. The readiness probe of `cadus-web` acquires from
/// this pool, so the default holds the probe open for 30 s during a database
/// outage, and a scraper with a shorter client timeout records a timeout in
/// place of the 503 that the handler promises. 5 s is longer than a normal
/// connect on a loaded host and shorter than every scrape interval in
/// `deploy/Caddyfile`.
pub const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Build the connection options of the pool.
///
/// The function puts `statement_timeout` into the startup options of the
/// connection, so the bound also holds for a connection that the pool opens
/// later. A `statement_timeout_ms` of 0 adds no option and leaves the server
/// default in place.
pub fn connect_options(cfg: &DbConfig) -> Result<PgConnectOptions, StoreError> {
    let options: PgConnectOptions = cfg.database_url.parse()?;
    if cfg.statement_timeout_ms == 0 {
        return Ok(options);
    }
    Ok(options.options([("statement_timeout", cfg.statement_timeout_ms.to_string())]))
}

/// Open a connection pool with the given configuration.
pub async fn connect(cfg: &DbConfig) -> Result<PgPool, StoreError> {
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .acquire_timeout(ACQUIRE_TIMEOUT)
        .connect_with(connect_options(cfg)?)
        .await?;
    tracing::debug!(
        statement_timeout_ms = cfg.statement_timeout_ms,
        "store: connection pool is open"
    );
    Ok(pool)
}

/// A pool and the client-side query bound that belongs to it.
///
/// `connect` still returns a bare `PgPool`, so `cadus-web` and `cadus-worker`
/// need no change. A caller that wants the client-side bound builds a `Db` and
/// gives it to `bounded`.
#[derive(Debug, Clone)]
pub struct Db {
    pool: PgPool,
    client_timeout_ms: u64,
}

impl Db {
    /// Build a `Db` from a pool and a bound in milliseconds. 0 turns the bound
    /// off.
    pub fn new(pool: PgPool, client_timeout_ms: u64) -> Self {
        Self {
            pool,
            client_timeout_ms,
        }
    }

    /// Open a pool with `connect` and keep the bound of the configuration.
    pub async fn connect(cfg: &DbConfig) -> Result<Self, StoreError> {
        Ok(Self::new(connect(cfg).await?, cfg.client_timeout_ms))
    }

    /// The pool of this handle.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// The client-side query bound, in milliseconds. 0 means no bound.
    pub fn client_timeout_ms(&self) -> u64 {
        self.client_timeout_ms
    }

    /// The client-side query bound as a `Duration`. `None` means no bound.
    pub fn client_timeout(&self) -> Option<Duration> {
        match self.client_timeout_ms {
            0 => None,
            ms => Some(Duration::from_millis(ms)),
        }
    }
}

/// Run a query future under the client-side bound of `db`.
///
/// The function returns `StoreError::Timeout` when the future does not finish
/// inside the bound. A bound of 0 runs the future without a bound.
///
/// A server that vanishes in the middle of a query leaves the caller in a read
/// that never ends: sqlx 0.9 sets no TCP keepalive, and `statement_timeout`
/// needs a live server to cancel the statement. This bound is the last resort
/// for that case.
pub async fn bounded<T, Fut>(db: &Db, fut: Fut) -> Result<T, StoreError>
where
    Fut: Future<Output = Result<T, sqlx::Error>>,
{
    let Some(bound) = db.client_timeout() else {
        return Ok(fut.await?);
    };
    match tokio::time::timeout(bound, fut).await {
        Ok(outcome) => Ok(outcome?),
        Err(_) => Err(StoreError::Timeout {
            after_ms: db.client_timeout_ms,
        }),
    }
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

#[cfg(test)]
mod tests {
    use super::{DEFAULT_CLIENT_TIMEOUT_MS, DbConfig};

    /// The timeout reaches the startup options of the connection as the literal
    /// `-c statement_timeout=<ms>`. 0 adds no option at all.
    #[test]
    fn the_timeout_becomes_a_startup_option() {
        let cfg = DbConfig {
            database_url: "postgresql://u@h:5432/d".to_string(),
            statement_timeout_ms: 250,
            client_timeout_ms: DEFAULT_CLIENT_TIMEOUT_MS,
        };
        let options = super::connect_options(&cfg).unwrap();
        assert_eq!(options.get_options(), Some("-c statement_timeout=250"));

        let off = DbConfig {
            database_url: "postgresql://u@h:5432/d".to_string(),
            statement_timeout_ms: 0,
            client_timeout_ms: DEFAULT_CLIENT_TIMEOUT_MS,
        };
        assert_eq!(super::connect_options(&off).unwrap().get_options(), None);
    }

    /// A connection string that does not parse is a store error, not a panic.
    #[test]
    fn a_connection_string_that_does_not_parse_is_an_error() {
        let err = super::connect_options(&DbConfig::new("not a url")).unwrap_err();
        assert!(err.to_string().starts_with("database error: "), "{err}");
    }
}
