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
//! The module panics on a setup failure. A panic is correct here: a test harness
//! that cannot build its fixture must stop, not report a false result.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use std::future::Future;
use std::sync::Arc;

use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, Connection, PgConnection, PgPool};
use uuid::Uuid;

/// The environment variable that holds the superuser DSN of the test cluster.
const TEST_DSN_VAR: &str = "CADUS_TEST_DATABASE_URL";

/// One throwaway database with an admin pool and an app pool.
pub struct TestDb {
    pub name: String,
    pub admin: PgPool,
    pub app: PgPool,
}

/// The default runtime role of a throwaway database.
const DEFAULT_APP_ROLE: &str = "cadus_app";

impl TestDb {
    /// The maintenance connection options of the test cluster.
    fn maintenance_options() -> PgConnectOptions {
        let dsn = match std::env::var(TEST_DSN_VAR) {
            Ok(dsn) if !dsn.is_empty() => dsn,
            _ => panic!(
                "{TEST_DSN_VAR} is not set. Point it at a superuser DSN of a throwaway Postgres, \
                 for example postgresql://test:test@127.0.0.1:55434/postgres"
            ),
        };
        dsn.parse()
            .unwrap_or_else(|e| panic!("{TEST_DSN_VAR} is not a valid Postgres DSN: {e}"))
    }

    /// Create a fresh database and return its name.
    ///
    /// Every panic of this method happens before `CREATE DATABASE`, so a
    /// failure here leaves nothing on the cluster.
    async fn create_database() -> String {
        let maintenance = Self::maintenance_options();

        // A UUID gives the 8 hex characters of the name. The name is an
        // identifier, and an identifier cannot be a bind parameter, so this one
        // statement stays outside the compile-time checked macros (R2).
        let name = format!("cadus2_t_{}", &Uuid::new_v4().simple().to_string()[..8]);

        let mut conn = PgConnection::connect_with(&maintenance)
            .await
            .unwrap_or_else(|e| panic!("connect to the test cluster failed: {e}"));
        sqlx::query(AssertSqlSafe(format!("CREATE DATABASE \"{name}\"")))
            .execute(&mut conn)
            .await
            .unwrap_or_else(|e| panic!("CREATE DATABASE {name} failed: {e}"));
        let _ = conn.close().await;
        name
    }

    /// Migrate the database that `name` names and open both pools.
    ///
    /// The caller drops the database when this method panics. `TestDb::with` is
    /// the only caller.
    async fn open(name: String, app_role: &str) -> TestDb {
        let maintenance = Self::maintenance_options();

        let admin = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(maintenance.clone().database(&name))
            .await
            .unwrap_or_else(|e| panic!("admin pool for {name} failed: {e}"));

        crate::migrate(&admin)
            .await
            .unwrap_or_else(|e| panic!("migrate of {name} failed: {e}"));

        // Trust authentication accepts the empty password of the app role.
        let app = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(
                maintenance
                    .clone()
                    .database(&name)
                    .username(app_role)
                    .password(""),
            )
            .await
            .unwrap_or_else(|e| panic!("app pool for {name} failed: {e}"));

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
        let name = Self::create_database().await;

        let setup = {
            let name = name.clone();
            tokio::spawn(async move { TestDb::open(name, app_role).await })
        }
        .await;

        let db = match setup {
            Ok(db) => Arc::new(db),
            Err(err) => {
                Self::drop_database_named(&name).await;
                if err.is_panic() {
                    std::panic::resume_unwind(err.into_panic());
                }
                panic!("the setup of {name} did not finish: {err}");
            }
        };

        let outcome = tokio::spawn(body(Arc::clone(&db))).await;
        db.drop_database().await;
        match outcome {
            Ok(value) => value,
            // `resume_unwind` carries the original payload, so the test reports
            // the message and the location of the first panic.
            Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
            Err(err) => panic!("the test body did not finish: {err}"),
        }
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
        sqlx::query(AssertSqlSafe(format!(
            "CREATE ROLE \"{role}\" {attributes}"
        )))
        .execute(&db.admin)
        .await
        .unwrap_or_else(|e| panic!("CREATE ROLE {role} failed: {e}"));

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
                if err.is_panic() {
                    std::panic::resume_unwind(err.into_panic());
                }
                panic!("the pool of {role} did not open: {err}");
            }
        };

        let outcome = tokio::spawn(body(Arc::clone(db), role.clone(), pool.clone())).await;
        pool.close().await;
        let dropped = Self::drop_role(&db.admin, &role).await;

        match outcome {
            Ok(value) => {
                // The body gave its verdict, so a failed drop is the only news
                // left. A leaked role is the defect that this function exists
                // to stop, so the failure stops the test.
                dropped.unwrap_or_else(|e| panic!("DROP ROLE {role} failed: {e}"));
                value
            }
            // `resume_unwind` carries the original payload, so the test reports
            // the message and the location of the first panic.
            Err(err) if err.is_panic() => std::panic::resume_unwind(err.into_panic()),
            Err(err) => panic!("the body of {role} did not finish: {err}"),
        }
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
        let dsn =
            std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
        let (base, _) = dsn
            .rsplit_once('/')
            .unwrap_or_else(|| panic!("{TEST_DSN_VAR} carries no database path segment"));
        format!("{base}/{name}")
    }

    /// Open a pool on this database as `role` with `max_connections`.
    ///
    /// The test cluster uses trust authentication, so the empty password is
    /// enough for every role. `max_connections(1)` gives a test one fixed
    /// session, which makes a setting that outlives a transaction visible.
    pub async fn pool_as(&self, role: &str, max_connections: u32) -> PgPool {
        let dsn =
            std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
        let options: PgConnectOptions = dsn
            .parse()
            .unwrap_or_else(|e| panic!("{TEST_DSN_VAR} is not a valid Postgres DSN: {e}"));
        PgPoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options.database(&self.name).username(role).password(""))
            .await
            .unwrap_or_else(|e| panic!("pool as {role} for {} failed: {e}", self.name))
    }

    /// Insert one user and return its id. The admin pool does the insert,
    /// because `users` carries no tenant column and the app role has no tenant
    /// context yet.
    pub async fn seed_user(&self, email: &str) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO users (email) VALUES ($1::text::citext) RETURNING id",
            email
        )
        .fetch_one(&self.admin)
        .await
        .unwrap_or_else(|e| panic!("seed_user({email}) failed: {e}"))
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
        let Ok(dsn) = std::env::var(TEST_DSN_VAR) else {
            return;
        };
        let Ok(maintenance) = dsn.parse::<PgConnectOptions>() else {
            return;
        };
        let Ok(mut conn) = PgConnection::connect_with(&maintenance).await else {
            return;
        };
        // FORCE ends a connection that a leaked pool still holds.
        let _ = sqlx::query(AssertSqlSafe(format!(
            "DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"
        )))
        .execute(&mut conn)
        .await;
        let _ = conn.close().await;
    }
}
