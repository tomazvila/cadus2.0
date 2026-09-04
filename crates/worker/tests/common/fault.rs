//! Fault injection: a role that lacks one privilege, and a pool that is closed.

use std::future::Future;
use std::sync::Arc;

use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use sqlx::{AssertSqlSafe, PgPool};

use super::{superuser_dsn, trace};

/// Run `body` with a `Db` handle connected as a fresh role that holds exactly
/// the privileges these statements grant, and nothing else.
///
/// Each statement runs as the superuser with `{role}` replaced by the quoted
/// role name, so a test names the one privilege it withholds and every read
/// or write that needs it fails at that statement and nowhere else. The role
/// bypasses row-level security, as `cadus_admin` does, and it is dropped after
/// the body, panic or not (`TestDb::with_role`).
pub async fn with_grants<F, Fut>(db: &Arc<TestDb>, statements: &'static [&'static str], body: F)
where
    F: FnOnce(Arc<TestDb>, Db) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    with_granted_role(db, statements, move |db, _role, pool| {
        body(db, Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS))
    })
    .await;
}

/// Run `body` with the name of a fresh role that holds exactly the privileges
/// these statements grant, and a pool connected as that role.
///
/// [`with_grants`] wraps the pool in a `Db` handle. A test that starts the
/// worker process takes the name instead, and builds the DSN of the role.
pub async fn with_granted_role<F, Fut>(
    db: &Arc<TestDb>,
    statements: &'static [&'static str],
    body: F,
) where
    F: FnOnce(Arc<TestDb>, String, PgPool) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    TestDb::with_role(
        db,
        "fault",
        "LOGIN BYPASSRLS",
        move |db, role, pool| async move {
            for statement in statements {
                let text = statement.replace("{role}", &format!("\"{role}\""));
                sqlx::query(AssertSqlSafe(text.clone()))
                    .execute(&db.admin)
                    .await
                    .unwrap_or_else(|e| panic!("{text}: {e}"));
            }
            trace();
            let outcome = tokio::spawn(body(Arc::clone(&db), role.clone(), pool)).await;
            // A role that still holds a privilege cannot be dropped, so the grants
            // go first, whatever the body did.
            sqlx::query(AssertSqlSafe(format!("DROP OWNED BY \"{role}\"")))
                .execute(&db.admin)
                .await
                .unwrap_or_else(|e| panic!("DROP OWNED BY {role}: {e}"));
            if let Err(err) = outcome
                && err.is_panic()
            {
                std::panic::resume_unwind(err.into_panic());
            }
        },
    )
    .await;
}

/// A `Db` handle whose pool is closed, so every statement fails at once.
///
/// The pool is a pool of its own, so the admin pool of the test still reads
/// the table after the failure.
pub async fn closed_handle(db: &TestDb) -> Db {
    let pool = PgPool::connect(&superuser_dsn(&db.name))
        .await
        .expect("the throwaway database accepts a connection");
    pool.close().await;
    trace();
    Db::new(pool, DEFAULT_CLIENT_TIMEOUT_MS)
}
