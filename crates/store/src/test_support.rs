//! Throwaway test databases for the row-level-security proofs (C2, C3).
//!
//! Each test creates its own database on the cluster that
//! `CADUS_TEST_DATABASE_URL` names, migrates it, and drops it at the end. Two
//! pools connect to that database:
//!
//! - `admin`: the superuser of the test cluster. It seeds fixtures, because a
//!   superuser bypasses row-level security.
//! - `app`: the runtime role `cadus_app`. Row-level security applies to it, so a
//!   test that uses this pool proves the production behavior.
//!
//! The module panics on a setup failure. A panic is correct here: a test harness
//! that cannot build its fixture must stop, not report a false result.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

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
    pub async fn create() -> TestDb {
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

    /// The superuser DSN of this database.
    ///
    /// A test that spawns a binary gives it this string in `DATABASE_URL`.
    /// `CADUS_TEST_DATABASE_URL` has the form
    /// `postgresql://user:password@host:port/dbname` and carries no query
    /// string, so a replacement of the last path segment names this database.
    pub fn superuser_dsn(&self) -> String {
        let dsn =
            std::env::var(TEST_DSN_VAR).unwrap_or_else(|_| panic!("{TEST_DSN_VAR} is not set"));
        let (base, _) = dsn
            .rsplit_once('/')
            .unwrap_or_else(|| panic!("{TEST_DSN_VAR} carries no database path segment"));
        format!("{base}/{}", self.name)
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
    pub async fn drop(self) {
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
