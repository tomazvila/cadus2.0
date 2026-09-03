//! Throwaway test databases for the row-level-security proofs (C2, C3).
//!
//! `TestDb::with` is the only entry point. It creates a database on the cluster
//! that `CADUS_TEST_DATABASE_URL` names, migrates it, runs the test body, and
//! drops the database in every case: a panic in the body, a failed migration,
//! and a failed pool all end with the database gone. Two pools connect to that
//! database:
//!
//! - `admin`: the superuser of the test cluster. It seeds fixtures, because a
//!   superuser bypasses row-level security.
//! - `app`: the runtime role `cadus_app`. Row-level security applies to it, so a
//!   test that uses this pool proves the production behavior.
//!
//! `TestDb::with_role` adds the same contract for a cluster role: it creates the
//! role, runs the body, and drops the role in every case, a panic included. A
//! role outlives the throwaway database, so a test that needs one takes this
//! function and never a bare `CREATE ROLE` (finding #9).
//!
//! `DeafPostgres` is the third helper. It is a TCP server that speaks the
//! Postgres handshake and then answers no query. The web tests and the worker
//! tests both need it, and each one carried its own copy, so a change to one
//! copy left the other test on the old behavior (round-4 open finding). One
//! copy lives here.
//!
//! The module panics on a setup failure. A panic is correct here: a test harness
//! that cannot build its fixture must stop, not report a false result.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod deaf;

use std::env::VarError;
use std::fmt::Display;
use std::future::Future;
use std::sync::Arc;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};
use tokio::task::JoinError;
use uuid::Uuid;

pub use deaf::DeafPostgres;

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// The default runtime role of a throwaway database.
const DEFAULT_APP_ROLE: &str = "cadus_app";

/// One throwaway database with an admin pool and an app pool.
pub struct TestDb {
    pub name: String,
    pub admin: PgPool,
    pub app: PgPool,
}

/// The value of `result`, or a panic that reads `"{what}: {error}"`.
///
/// A test harness that cannot build its fixture must stop, not report a false
/// result, so every setup step of this module ends here.
fn or_stop<T, E: Display>(what: impl Display, result: Result<T, E>) -> T {
    match result {
        Ok(value) => value,
        Err(err) => panic!("{what}: {err}"),
    }
}

/// Raise the panic of a task again, or panic with `what` when the task ended
/// another way.
///
/// `resume_unwind` carries the original payload, so the test reports the
/// message and the location of the first panic.
fn resume(err: JoinError, what: impl Display) -> ! {
    if err.is_panic() {
        std::panic::resume_unwind(err.into_panic());
    }
    panic!("{what}: {err}");
}

/// The superuser DSN of the test cluster, from one read of `TEST_DSN_VAR`.
///
/// An absent or empty variable stops the run with the line that names the
/// variable and one example value.
fn dsn_from(read: Result<String, VarError>) -> String {
    let dsn = read.ok().filter(|dsn| !dsn.is_empty());
    or_stop(
        format!("{TEST_DSN_VAR} is not set"),
        dsn.ok_or(
            "point it at a superuser DSN of a throwaway Postgres, for example \
                   postgresql://test:test@127.0.0.1:55434/postgres",
        ),
    )
}

/// The superuser DSN of the test cluster.
fn test_dsn() -> String {
    dsn_from(std::env::var(TEST_DSN_VAR))
}

/// A fresh database name: `cadus2_t_` plus 8 hex characters of a UUID.
fn fresh_name() -> String {
    format!("cadus2_t_{}", &Uuid::new_v4().simple().to_string()[..8])
}

impl TestDb {
    /// The maintenance connection options of the test cluster.
    fn maintenance_options() -> PgConnectOptions {
        or_stop(
            format!("{TEST_DSN_VAR} is not a valid Postgres DSN"),
            test_dsn().parse(),
        )
    }

    /// Create the database `name` on the cluster of `maintenance`.
    ///
    /// The name is an identifier, and an identifier cannot be a bind
    /// parameter, so this one statement stays outside the compile-time checked
    /// macros (R2). Every panic of this method happens before the database
    /// exists, so a failure here leaves nothing on the cluster.
    async fn create_database(maintenance: &PgConnectOptions, name: &str) {
        let mut conn = or_stop(
            "connect to the test cluster failed",
            PgConnection::connect_with(maintenance).await,
        );
        or_stop(
            format!("CREATE DATABASE {name} failed"),
            sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
                .execute(&mut conn)
                .await,
        );
        let _ = conn.close().await;
    }

    /// Migrate the database that `name` names and open both pools.
    ///
    /// The caller drops the database when this method panics. `TestDb::with` is
    /// the only caller.
    async fn open(name: String, app_role: &str) -> TestDb {
        let maintenance = Self::maintenance_options();

        let admin = or_stop(
            format!("admin pool for {name} failed"),
            PgPoolOptions::new()
                .max_connections(4)
                .connect_with(maintenance.clone().database(&name))
                .await,
        );

        or_stop(
            format!("migrate of {name} failed"),
            crate::migrate(&admin).await,
        );

        // Trust authentication accepts the empty password of the app role.
        let app = or_stop(
            format!("app pool for {name} failed"),
            PgPoolOptions::new()
                .max_connections(4)
                .connect_with(
                    maintenance
                        .clone()
                        .database(&name)
                        .username(app_role)
                        .password(""),
                )
                .await,
        );

        TestDb { name, admin, app }
    }

    /// Create a database, run `body`, and drop the database in every case.
    ///
    /// `body` runs in its own task, so a panic in the test body becomes a
    /// `JoinError` instead of an unwind through this function. The database goes
    /// away first, and this function then raises the original panic again. A red
    /// test therefore leaves no database on the shared cluster.
    ///
    /// The body receives an `Arc<TestDb>`, because the task needs an owned
    /// handle and this function keeps one for the cleanup.
    pub async fn with<F, Fut, T>(body: F) -> T
    where
        F: FnOnce(Arc<TestDb>) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        Self::with_app_role(DEFAULT_APP_ROLE, body).await
    }

    /// `TestDb::with` with a different runtime role for the `app` pool.
    ///
    /// The setup after `CREATE DATABASE` runs in a task of its own too, so a
    /// panic of the migration step or of the app pool also becomes a
    /// `JoinError`. This function drops the database first and then raises that
    /// panic again, so a broken migration and a wrong role name leave no
    /// database on the shared cluster (finding #10).
    ///
    /// A test gives a role that does not exist to prove that contract.
    pub async fn with_app_role<F, Fut, T>(app_role: &'static str, body: F) -> T
    where
        F: FnOnce(Arc<TestDb>) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let name = fresh_name();
        Self::create_database(&Self::maintenance_options(), &name).await;

        let setup = {
            let name = name.clone();
            tokio::spawn(async move { TestDb::open(name, app_role).await })
        }
        .await;

        let db = match setup {
            Ok(db) => Arc::new(db),
            Err(err) => {
                Self::drop_database_named(&name).await;
                resume(err, format!("the setup of {name} did not finish"));
            }
        };

        let outcome = tokio::spawn(body(Arc::clone(&db))).await;
        db.drop_database().await;
        outcome.unwrap_or_else(|err| resume(err, "the test body did not finish"))
    }

    /// Create a cluster role, run `body`, and drop the role in every case.
    ///
    /// A role is cluster-scoped, so `TestDb::with` does not clean it up: the
    /// drop of the throwaway database leaves a role behind. A test body that
    /// creates a role and drops it at the end therefore leaks the role on every
    /// panic before that drop, and the test cluster uses `trust`
    /// authentication, so a leaked `LOGIN SUPERUSER` role is a login for every
    /// user of the host (finding #9).
    ///
    /// The role name is `<name_prefix>_<8 hex>`, so two runs on one cluster
    /// never collide. `attributes` is the option list of `CREATE ROLE`, for
    /// example `LOGIN SUPERUSER`. Both strings are literals of the test file:
    /// the statement is an identifier plus an option list, and neither is a
    /// bind parameter, so this statement stays outside the compile-time checked
    /// macros (R2).
    ///
    /// The body runs in a task of its own, and the pool opens in a task of its
    /// own, so a panic of either one becomes a `JoinError`. This function drops
    /// the role first and then raises that panic again.
    ///
    /// The body receives the `TestDb`, the role name, and a pool of one
    /// connection that is connected as the role.
    pub async fn with_role<F, Fut, T>(
        db: &Arc<TestDb>,
        name_prefix: &str,
        attributes: &str,
        body: F,
    ) -> T
    where
        F: FnOnce(Arc<TestDb>, String, PgPool) -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let role = format!(
            "{name_prefix}_{}",
            &Uuid::new_v4().simple().to_string()[..8]
        );
        or_stop(
            format!("CREATE ROLE {role} failed"),
            sqlx::query(AssertSqlSafe(format!(
                "CREATE ROLE \"{role}\" {attributes}"
            )))
            .execute(&db.admin)
            .await,
        );

        let opened = {
            let db = Arc::clone(db);
            let role = role.clone();
            tokio::spawn(async move { db.pool_as(&role, 1).await })
        }
        .await;

        let pool = match opened {
            Ok(pool) => pool,
            Err(err) => {
                let _ = Self::drop_role(&db.admin, &role).await;
                resume(err, format!("the pool of {role} did not open"));
            }
        };

        let outcome = tokio::spawn(body(Arc::clone(db), role.clone(), pool.clone())).await;
        pool.close().await;
        let dropped = Self::drop_role(&db.admin, &role).await;

        let value =
            outcome.unwrap_or_else(|err| resume(err, format!("the body of {role} did not finish")));
        // The body gave its verdict, so a failed drop is the only news left. A
        // leaked role is the defect that this function exists to stop, so the
        // failure stops the test.
        or_stop(format!("DROP ROLE {role} failed"), dropped);
        value
    }

    /// Drop a cluster role. The caller decides what a failure means.
    async fn drop_role(admin: &PgPool, role: &str) -> Result<(), sqlx::Error> {
        sqlx::query(AssertSqlSafe(format!("DROP ROLE IF EXISTS \"{role}\"")))
            .execute(admin)
            .await
            .map(|_| ())
    }

    /// The superuser DSN of this database.
    ///
    /// A test that spawns a binary gives it this string in `DATABASE_URL`.
    pub fn superuser_dsn(&self) -> String {
        Self::superuser_dsn_for(&self.name)
    }

    /// The superuser DSN of any database on the test cluster.
    ///
    /// `CADUS_TEST_DATABASE_URL` has the form
    /// `postgresql://user:password@host:port/dbname` and carries no query
    /// string, so a replacement of the last path segment names the database.
    pub fn superuser_dsn_for(name: &str) -> String {
        dsn_for(&test_dsn(), name)
    }

    /// Open a pool on this database as `role` with `max_connections`.
    ///
    /// The test cluster uses trust authentication, so the empty password is
    /// enough for every role. `max_connections(1)` gives a test one fixed
    /// session, which makes a setting that outlives a transaction visible.
    pub async fn pool_as(&self, role: &str, max_connections: u32) -> PgPool {
        let options = Self::maintenance_options();
        or_stop(
            format!("pool as {role} for {} failed", self.name),
            PgPoolOptions::new()
                .max_connections(max_connections)
                .connect_with(options.database(&self.name).username(role).password(""))
                .await,
        )
    }

    /// Insert one user and return its id. The admin pool does the insert,
    /// because `users` carries no tenant column and the app role has no tenant
    /// context yet.
    pub async fn seed_user(&self, email: &str) -> Uuid {
        or_stop(
            format!("seed_user({email}) failed"),
            sqlx::query_scalar!(
                "INSERT INTO users (email) VALUES ($1::text::citext) RETURNING id",
                email
            )
            .fetch_one(&self.admin)
            .await,
        )
    }

    /// Close both pools and drop the database. The drop is best effort: a failed
    /// cleanup must not fail a test that already gave its verdict.
    async fn drop_database(&self) {
        self.app.close().await;
        self.admin.close().await;
        Self::drop_database_named(&self.name).await;
    }

    /// Drop the database that `name` names. The drop is best effort.
    ///
    /// `TestDb::with_app_role` calls this after a failed setup, when no `TestDb`
    /// value exists.
    async fn drop_database_named(name: &str) {
        let _ = drop_database_at(&Self::maintenance_options(), name).await;
    }
}

/// The DSN of the database `name` on the cluster of `dsn`.
fn dsn_for(dsn: &str, name: &str) -> String {
    let (base, _) = or_stop(
        format!("{TEST_DSN_VAR} carries no database path segment"),
        dsn.rsplit_once('/').ok_or(dsn),
    );
    format!("{base}/{name}")
}

/// Drop the database `name` through a maintenance connection of `options`.
///
/// `FORCE` ends a connection that a leaked pool still holds.
async fn drop_database_at(options: &PgConnectOptions, name: &str) -> Result<(), sqlx::Error> {
    let mut conn = PgConnection::connect_with(options).await?;
    let dropped = sqlx::query(AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
    )))
    .execute(&mut conn)
    .await;
    let _ = conn.close().await;
    dropped.map(|_| ())
}

#[cfg(test)]
mod tests {
    use std::env::VarError;

    use super::{dsn_for, dsn_from, or_stop, resume};

    /// `or_stop` gives the value back, or panics with the two parts.
    #[test]
    fn or_stop_gives_the_value_or_the_two_part_message() {
        assert_eq!(or_stop("x", Ok::<u8, &str>(7)), 7);
        let outcome =
            std::panic::catch_unwind(|| or_stop("CREATE ROLE r failed", Err::<u8, _>("boom")));
        let payload = outcome.unwrap_err();
        assert_eq!(
            payload.downcast_ref::<String>().map(String::as_str),
            Some("CREATE ROLE r failed: boom")
        );
    }

    /// A task that panicked raises its own payload again; a task that was
    /// cancelled panics with the given words.
    #[tokio::test]
    async fn resume_raises_the_panic_or_names_the_other_end() {
        let panicked = tokio::spawn(async { panic!("inside the body") })
            .await
            .unwrap_err();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resume(panicked, "the test body did not finish")
        }));
        assert_eq!(
            outcome.unwrap_err().downcast_ref::<&str>().copied(),
            Some("inside the body")
        );

        let handle = tokio::spawn(std::future::pending::<()>());
        handle.abort();
        let cancelled = handle.await.unwrap_err();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resume(cancelled, "the test body did not finish")
        }));
        let message = outcome.unwrap_err();
        let message = message.downcast_ref::<String>().unwrap();
        assert!(
            message.starts_with("the test body did not finish: "),
            "{message}"
        );
    }

    /// The DSN read refuses an absent and an empty variable with the line that
    /// names the variable and the example, and passes a value through.
    #[test]
    fn the_dsn_read_refuses_an_absent_or_empty_variable() {
        assert_eq!(dsn_from(Ok("postgresql://x".to_string())), "postgresql://x");
        for read in [Err(VarError::NotPresent), Ok(String::new())] {
            let outcome = std::panic::catch_unwind(|| dsn_from(read));
            let message = outcome.unwrap_err();
            let message = message.downcast_ref::<String>().unwrap();
            assert_eq!(
                message,
                "CADUS_TEST_DATABASE_URL is not set: point it at a superuser DSN of a \
                 throwaway Postgres, for example postgresql://test:test@127.0.0.1:55434/postgres"
            );
        }
    }

    /// The last path segment of the DSN is the database, and a DSN with no
    /// path segment stops the run.
    #[test]
    fn the_database_of_a_dsn_is_its_last_path_segment() {
        assert_eq!(
            dsn_for("postgresql://u:p@h:1/postgres", "cadus2_t_1"),
            "postgresql://u:p@h:1/cadus2_t_1"
        );
        let outcome = std::panic::catch_unwind(|| dsn_for("no-slash", "x"));
        let message = outcome.unwrap_err();
        assert_eq!(
            message.downcast_ref::<String>().map(String::as_str),
            Some("CADUS_TEST_DATABASE_URL carries no database path segment: no-slash")
        );
    }
}
