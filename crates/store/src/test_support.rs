//! Throwaway test databases for the row-level-security proofs (C2, C3).
//!
//! `TestDb::with` is the only entry point. It creates a database on the cluster
//! that `CADUS_TEST_DATABASE_URL` names, migrates it, runs the test body, and
//! drops the database in every case, a panic in the body included. Two pools
//! connect to that database:
//!
//! - `admin`: the superuser of the test cluster. It seeds fixtures, because a
//!   superuser bypasses row-level security.
//! - `app`: the runtime role `cadus_app`. Row-level security applies to it, so a
//!   test that uses this pool proves the production behavior.
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

impl TestDb {
    /// Create a fresh database, migrate it, and open both pools.
    ///
    /// This method stays private. `TestDb::with` is the only entry point,
    /// because it also drops the database of a test body that panics.
    async fn create() -> TestDb {
        let dsn = match std::env::var(TEST_DSN_VAR) {
            Ok(dsn) if !dsn.is_empty() => dsn,
            _ => panic!(
                "{TEST_DSN_VAR} is not set. Point it at a superuser DSN of a throwaway Postgres, \
                 for example postgresql://test:test@127.0.0.1:55434/postgres"
            ),
        };

        let maintenance: PgConnectOptions = dsn
            .parse()
            .unwrap_or_else(|e| panic!("{TEST_DSN_VAR} is not a valid Postgres DSN: {e}"));

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
        conn.close()
            .await
            .unwrap_or_else(|e| panic!("close of the maintenance connection failed: {e}"));

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
                    .username("cadus_app")
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
        let db = Arc::new(TestDb::create().await);
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
            "DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)",
            self.name
        )))
        .execute(&mut conn)
        .await;
        let _ = conn.close().await;
    }
}
