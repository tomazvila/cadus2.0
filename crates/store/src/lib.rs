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

use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[cfg(feature = "test-support")]
pub mod test_support;

/// The migration set of the repository. `sqlx::migrate!` embeds the files at
/// compile time, so the binaries carry the schema and need no file access.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");

/// The environment variable that holds the statement timeout, in milliseconds.
pub const STATEMENT_TIMEOUT_VAR: &str = "DB_STATEMENT_TIMEOUT_MS";

/// The statement timeout that applies when `DB_STATEMENT_TIMEOUT_MS` is absent.
///
/// `ACQUIRE_TIMEOUT` bounds the checkout of a connection and nothing after it.
/// A database that accepts the socket and answers no query therefore holds the
/// readiness probe of `cadus-web` and the tick of `cadus-worker` open without a
/// bound. `statement_timeout` adds the server-side bound: the backend cancels
/// the statement and reports SQLSTATE 57014. 5000 ms is longer than every M0
/// query and shorter than every scrape interval in `deploy/Caddyfile`.
pub const DEFAULT_STATEMENT_TIMEOUT_MS: u64 = 5000;

/// The connection configuration of the store.
#[derive(Clone)]
pub struct DbConfig {
    pub database_url: String,
    /// The `statement_timeout` of every connection of the pool, in
    /// milliseconds. 0 turns the timeout off.
    pub statement_timeout_ms: u64,
}

impl DbConfig {
    /// Build a configuration from a connection string with the default
    /// statement timeout.
    pub fn new(database_url: impl Into<String>) -> Self {
        Self {
            database_url: database_url.into(),
            statement_timeout_ms: DEFAULT_STATEMENT_TIMEOUT_MS,
        }
    }

    /// Read `DATABASE_URL` and `DB_STATEMENT_TIMEOUT_MS` from the environment.
    ///
    /// The function returns `StoreError::Config` when `DATABASE_URL` is absent,
    /// empty, or not valid Unicode, and when `DB_STATEMENT_TIMEOUT_MS` holds
    /// anything other than a whole number of milliseconds. An absent
    /// `DB_STATEMENT_TIMEOUT_MS` gives `DEFAULT_STATEMENT_TIMEOUT_MS`.
    pub fn from_env() -> Result<Self, StoreError> {
        let database_url = match std::env::var("DATABASE_URL") {
            Ok(url) if url.is_empty() => {
                return Err(StoreError::Config("DATABASE_URL is empty".to_string()));
            }
            Ok(url) => url,
            Err(std::env::VarError::NotPresent) => {
                return Err(StoreError::Config("DATABASE_URL is not set".to_string()));
            }
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(StoreError::Config(
                    "DATABASE_URL is not valid Unicode".to_string(),
                ));
            }
        };

        let raw = match std::env::var(STATEMENT_TIMEOUT_VAR) {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(std::env::VarError::NotUnicode(_)) => {
                return Err(StoreError::Config(format!(
                    "{STATEMENT_TIMEOUT_VAR} is not valid Unicode"
                )));
            }
        };

        Ok(Self {
            database_url,
            statement_timeout_ms: parse_statement_timeout(raw.as_deref())?,
        })
    }
}

/// Read the statement timeout from the raw value of the variable.
///
/// `None` means the variable is absent, so the default applies. Every other
/// value must be a whole number of milliseconds. A value that is not a whole
/// number is a configuration error: the store never guesses a bound that an
/// operator wrote by hand.
fn parse_statement_timeout(raw: Option<&str>) -> Result<u64, StoreError> {
    match raw {
        None => Ok(DEFAULT_STATEMENT_TIMEOUT_MS),
        Some(value) => value.parse::<u64>().map_err(|_| {
            StoreError::Config(format!(
                "{STATEMENT_TIMEOUT_VAR} must be a whole number of milliseconds, not {value:?}"
            ))
        }),
    }
}

/// The connection string holds the database password. Keep it out of every log
/// line and every panic message.
impl std::fmt::Debug for DbConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbConfig")
            .field("database_url", &"<redacted>")
            .field("statement_timeout_ms", &self.statement_timeout_ms)
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
    use super::{DEFAULT_STATEMENT_TIMEOUT_MS, DbConfig, StoreError, parse_statement_timeout};

    /// R4: an absent variable gives the documented default of 5000 ms.
    #[test]
    fn an_absent_statement_timeout_gives_the_default() {
        assert_eq!(parse_statement_timeout(None).unwrap(), 5000);
        assert_eq!(DEFAULT_STATEMENT_TIMEOUT_MS, 5000);
        assert_eq!(DbConfig::new("postgresql://h/d").statement_timeout_ms, 5000);
    }

    /// A whole number passes through unchanged. 0 turns the timeout off.
    #[test]
    fn a_whole_number_passes_through() {
        assert_eq!(parse_statement_timeout(Some("200")).unwrap(), 200);
        assert_eq!(parse_statement_timeout(Some("0")).unwrap(), 0);
    }

    /// A value that is not a whole number is a configuration error. The store
    /// stops instead of a silent fall back to the default.
    #[test]
    fn a_value_that_is_not_a_whole_number_is_a_configuration_error() {
        for raw in ["", "5s", "-1", "2.5", "5000ms"] {
            let err = parse_statement_timeout(Some(raw))
                .expect_err("a value that is not a whole number must be an error");
            let StoreError::Config(message) = err else {
                panic!("expected StoreError::Config for {raw:?}, got {err}");
            };
            assert_eq!(
                message,
                format!(
                    "DB_STATEMENT_TIMEOUT_MS must be a whole number of milliseconds, not {raw:?}"
                )
            );
        }
    }

    /// The timeout reaches the startup options of the connection as the literal
    /// `-c statement_timeout=<ms>`. 0 adds no option at all.
    #[test]
    fn the_timeout_becomes_a_startup_option() {
        let cfg = DbConfig {
            database_url: "postgresql://u@h:5432/d".to_string(),
            statement_timeout_ms: 250,
        };
        let options = super::connect_options(&cfg).unwrap();
        assert_eq!(options.get_options(), Some("-c statement_timeout=250"));

        let off = DbConfig {
            database_url: "postgresql://u@h:5432/d".to_string(),
            statement_timeout_ms: 0,
        };
        assert_eq!(super::connect_options(&off).unwrap().get_options(), None);
    }
}
