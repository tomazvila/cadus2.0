//! Fault injections for the error paths of the store.
//!
//! Every function here changes the throwaway database or opens a pool whose
//! next statement fails, so a test reaches one `?` of the store with a real
//! database error. The throwaway database goes away with the test, so no
//! injection outlives it.

use cadus_store::test_support::TestDb;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{AssertSqlSafe, PgPool};
use uuid::Uuid;

/// The runtime role of every injection that names a role.
const APP_ROLE: &str = "cadus_app";

/// Run one statement of the test's own text on the admin pool.
async fn admin_exec(db: &TestDb, sql: String) {
    sqlx::query(AssertSqlSafe(sql))
        .execute(&db.admin)
        .await
        .expect("the fault statement runs");
}

/// before_acquire exclusively owns connection; terminate via admin with
/// bounded server-confirmed wait; hand dead connection to application wrapper.
pub async fn dead_pool(db: &TestDb) -> PgPool {
    let dsn = std::env::var("CADUS_TEST_DATABASE_URL").expect("the test DSN is set");
    let options: PgConnectOptions = dsn.parse().expect("the test DSN parses");

    // Clone the admin pool so the before_acquire closure can use it.
    let admin = db.admin.clone();

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .test_before_acquire(false)
        .before_acquire(move |conn, _meta| {
            let admin = admin.clone();
            Box::pin(async move {
                // Read this connection's backend PID through the connection
                // itself.  The connection is exclusively held by the callback.
                let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                    .fetch_one(&mut *conn)
                    .await?;
                // Terminate that backend via the superuser admin pool.
                let ended: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1, 5000)")
                    .bind(pid)
                    .fetch_one(&admin)
                    .await?;
                assert!(ended, "the backend {pid} was not terminated within 5000 ms");
                // Dead connection: hand to app wrapper.
                Ok(true)
            })
        })
        .connect_with(options.database(&db.name).username(APP_ROLE).password(""))
        .await
        .expect("the dead pool opens");

    pool
}

/// A closed app pool: every acquire fails at once with `PoolClosed`.
pub async fn closed_pool(db: &TestDb) -> PgPool {
    let pool = db.pool_as(APP_ROLE, 1).await;
    pool.close().await;
    pool
}

/// A pool of one app connection with the tenant bound at SESSION level, so
/// the binding outlives every transaction of the connection.
pub async fn bound_session_pool(db: &TestDb, user_id: Uuid) -> PgPool {
    let pool = db.pool_as(APP_ROLE, 1).await;
    sqlx::query("SELECT set_config('app.user_id', $1, false)")
        .bind(user_id.to_string())
        .execute(&pool)
        .await
        .expect("the session binding sets");
    pool
}

/// Take `privilege` on `table` away from the app role, so every statement of
/// that kind on that table fails with SQLSTATE 42501.
pub async fn revoke(db: &TestDb, privilege: &str, table: &str) {
    admin_exec(db, format!("REVOKE {privilege} ON {table} FROM {APP_ROLE}")).await;
}

/// Take EXECUTE on `set_config` away from every role but the superuser, so
/// every tenant bind of the app role fails after `BEGIN`.
pub async fn revoke_set_config(db: &TestDb) {
    admin_exec(
        db,
        "REVOKE EXECUTE ON FUNCTION pg_catalog.set_config(text, text, boolean) FROM PUBLIC"
            .to_string(),
    )
    .await;
}

/// The trigger function that raises `injected`.
async fn raise_function(db: &TestDb) {
    admin_exec(
        db,
        "CREATE OR REPLACE FUNCTION fault_raise() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected'; END $$"
            .to_string(),
    )
    .await;
}

/// Fail the COMMIT of every transaction that writes `table`.
///
/// A deferred constraint trigger runs at the commit, so every statement of
/// the transaction succeeds and the commit reports SQLSTATE P0001.
pub async fn fail_commit_on(db: &TestDb, table: &str) {
    raise_function(db).await;
    admin_exec(
        db,
        format!(
            "CREATE CONSTRAINT TRIGGER fault_commit AFTER INSERT OR UPDATE OR DELETE ON {table} \
             DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fault_raise()"
        ),
    )
    .await;
}

/// Fail every `event` (INSERT, UPDATE, or DELETE) on `table` at once.
pub async fn fail_on(db: &TestDb, event: &str, table: &str) {
    raise_function(db).await;
    admin_exec(
        db,
        format!(
            "CREATE TRIGGER fault_{event} BEFORE {event} ON {table} FOR EACH ROW EXECUTE \
             FUNCTION fault_raise()"
        ),
    )
    .await;
}

/// End the session of the caller AFTER every statement of `event` on `table`.
///
/// A statement-level trigger sends `pg_terminate_backend` to its own session.
/// The signal lands after the statement, so the statement succeeds and the
/// next command of the session fails on the closed connection.
pub async fn die_after(db: &TestDb, event: &str, table: &str) {
    admin_exec(
        db,
        "CREATE OR REPLACE FUNCTION fault_die() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RETURN CASE WHEN pg_terminate_backend(pg_backend_pid()) THEN NULL END; END $$"
            .to_string(),
    )
    .await;
    admin_exec(
        db,
        format!(
            "CREATE TRIGGER fault_die_{event} AFTER {event} ON {table} FOR EACH STATEMENT EXECUTE \
             FUNCTION fault_die()"
        ),
    )
    .await;
}

/// Make every UPDATE of `table` write no row: a BEFORE UPDATE trigger that
/// returns NULL skips the row, so the statement reports zero rows.
pub async fn skip_updates_on(db: &TestDb, table: &str) {
    admin_exec(
        db,
        "CREATE OR REPLACE FUNCTION fault_skip() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RETURN NULL; END $$"
            .to_string(),
    )
    .await;
    admin_exec(
        db,
        format!(
            "CREATE TRIGGER fault_skip BEFORE UPDATE ON {table} FOR EACH ROW EXECUTE FUNCTION \
             fault_skip()"
        ),
    )
    .await;
}

/// Drop every CHECK constraint of `table`, so a later migration's value can
/// enter a column this build gates.
pub async fn drop_checks(db: &TestDb, table: &str) {
    let names: Vec<String> = sqlx::query_scalar(
        "SELECT conname::text FROM pg_constraint WHERE conrelid = $1::regclass AND contype = 'c'",
    )
    .bind(table)
    .fetch_all(&db.admin)
    .await
    .expect("the constraint names read");
    for name in names {
        admin_exec(
            db,
            format!("ALTER TABLE {table} DROP CONSTRAINT \"{name}\""),
        )
        .await;
    }
}

/// Overwrite the payload of the event at line `seq` of `user_id` with a
/// document that is not an event of this build.
pub async fn poison_event(db: &TestDb, user_id: Uuid, seq: i64) {
    let changed = sqlx::query(
        r#"UPDATE events SET payload = '{"type": "not-an-event"}'::jsonb
           WHERE user_id = $1 AND seq = $2"#,
    )
    .bind(user_id)
    .bind(seq)
    .execute(&db.admin)
    .await
    .expect("the poison writes")
    .rows_affected();
    assert_eq!(changed, 1, "no event at line {seq}");
}
