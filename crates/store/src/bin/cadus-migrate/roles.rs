//! The cluster-wide role lock and the `ALTER ROLE` statements of
//! `--admin-login`.

use std::env::VarError;

use cadus_store::{DbConfig, StoreError};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};

use crate::password::{ADMIN_PASSWORD_VAR, APP_PASSWORD_VAR, password_from_env};
use crate::retry::retry_concurrent_update;

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

/// A cluster-wide lock on the `ALTER ROLE` statements of this program.
///
/// Roles are cluster-scoped. Two migrate runs on one cluster (two databases, or
/// a test next to a deploy) that alter one role at the same time fail with
/// "tuple concurrently updated". PostgreSQL scopes an advisory lock to the
/// database of the session, so a lock on the database of `DATABASE_URL`
/// serializes nothing between two databases (finding #2). This lock therefore
/// opens one connection to the maintenance database of the cluster, and every
/// run on the cluster shares that database.
pub struct RoleLock(PgConnection);

impl RoleLock {
    /// Take the lock in `database`, the maintenance database of the cluster.
    /// The call waits until every other holder gives it back.
    ///
    /// The connection carries `statement_timeout` 0, so the wait has no bound.
    pub async fn acquire(cfg: &DbConfig, database: &str) -> Result<RoleLock, StoreError> {
        let options = cadus_store::connect_options(cfg)?.database(database);
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
    pub async fn release(self) {
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
pub fn maintenance_db() -> Result<String, StoreError> {
    maintenance_db_from(std::env::var(MAINTENANCE_DB_VAR))
}

/// The maintenance database of one variable read.
fn maintenance_db_from(read: Result<String, VarError>) -> Result<String, StoreError> {
    match read {
        Ok(value) if value.is_empty() => {
            Err(StoreError::Config(format!("{MAINTENANCE_DB_VAR} is empty")))
        }
        Ok(value) => Ok(value),
        Err(VarError::NotPresent) => Ok(DEFAULT_MAINTENANCE_DB.to_string()),
        Err(VarError::NotUnicode(_)) => Err(StoreError::Config(format!(
            "{MAINTENANCE_DB_VAR} is not valid Unicode"
        ))),
    }
}

/// Grant the admin login and set the role passwords that the environment
/// names. The caller holds the cluster-wide role lock.
pub async fn alter_roles(
    pool: &PgPool,
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
    retry_concurrent_update(|| {
        let sql = sql.clone();
        async move {
            sqlx::query(AssertSqlSafe(sql)).execute(pool).await?;
            Ok(())
        }
    })
    .await
}

#[cfg(test)]
mod tests {
    use std::env::VarError;
    use std::ffi::OsString;

    use cadus_store::DbConfig;
    use cadus_store::test_support::TestDb;
    use sqlx::AssertSqlSafe;

    use super::{
        RoleLock, alter_role_password_statement, alter_roles, maintenance_db_from,
        run_role_statement,
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

    /// The maintenance database is the variable, `postgres` when it is absent,
    /// and an error when it is empty or not valid Unicode.
    #[test]
    fn the_maintenance_database_read_maps_its_four_shapes() {
        assert_eq!(
            maintenance_db_from(Ok("cluster".to_string())).unwrap(),
            "cluster"
        );
        assert_eq!(
            maintenance_db_from(Err(VarError::NotPresent)).unwrap(),
            "postgres"
        );
        assert_eq!(
            maintenance_db_from(Ok(String::new()))
                .unwrap_err()
                .to_string(),
            "configuration error: CADUS_MAINTENANCE_DB is empty"
        );
        assert_eq!(
            maintenance_db_from(Err(VarError::NotUnicode(OsString::from("x"))))
                .unwrap_err()
                .to_string(),
            "configuration error: CADUS_MAINTENANCE_DB is not valid Unicode"
        );
    }

    /// A role statement on a closed pool is the error of the statement, and
    /// the whole `alter_roles` step reports it. A statement on an open pool
    /// runs.
    #[tokio::test]
    async fn a_role_statement_on_a_closed_pool_is_an_error() {
        TestDb::with(|db| async move {
            run_role_statement(&db.admin, "SELECT 1".to_string())
                .await
                .unwrap();
            let pool = db.pool_as("cadus_app", 1).await;
            pool.close().await;
            let err = run_role_statement(&pool, "SELECT 1".to_string())
                .await
                .unwrap_err();
            assert_eq!(
                err.to_string(),
                "database error: attempted to acquire a connection on a closed pool"
            );
            let mut roles = Vec::new();
            assert!(alter_roles(&pool, &mut roles).await.is_err());
            assert!(roles.is_empty());
        })
        .await;
    }

    /// The lock connects to the maintenance database of the cluster. A DSN
    /// that does not parse and a cluster that does not answer both fail before
    /// the lock statement.
    #[tokio::test]
    async fn the_lock_fails_before_the_statement_on_a_bad_configuration() {
        let unparsed = RoleLock::acquire(&DbConfig::new("not a url"), "postgres")
            .await
            .err();
        assert!(unparsed.is_some());
        let refused = RoleLock::acquire(&DbConfig::new("postgresql://x@127.0.0.1:1/x"), "postgres")
            .await
            .err();
        assert!(refused.is_some());
        let lock = RoleLock::acquire(
            &DbConfig::new(TestDb::superuser_dsn_for("postgres")),
            "postgres",
        )
        .await
        .unwrap();
        lock.release().await;
    }

    /// The lock statement itself fails when the maintenance database resolves
    /// `pg_advisory_lock` to a function that raises: the database puts a
    /// schema of its own before `pg_catalog` on the search path.
    #[tokio::test]
    async fn a_refused_lock_statement_is_the_error_of_the_statement() {
        TestDb::with(|db| async move {
            let statements = [
                "CREATE SCHEMA shadow".to_string(),
                "CREATE FUNCTION shadow.pg_advisory_lock(bigint) RETURNS void LANGUAGE plpgsql \
                 AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$"
                    .to_string(),
                format!(
                    "ALTER DATABASE \"{}\" SET search_path = shadow, pg_catalog",
                    db.name
                ),
            ];
            for sql in statements {
                sqlx::query(AssertSqlSafe(sql))
                    .execute(&db.admin)
                    .await
                    .unwrap();
            }
            let refused = RoleLock::acquire(&DbConfig::new(db.superuser_dsn()), &db.name)
                .await
                .err()
                .map(|err| err.to_string());
            assert!(
                refused.as_deref().is_some_and(|m| m.contains("injected")),
                "{refused:?}"
            );
        })
        .await;
    }
}
