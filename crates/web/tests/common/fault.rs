//! Fault injection for one throwaway database: a statement of the app role
//! fails on purpose, so a test reaches the error arm of one store call.
//!
//! Every statement here is DDL on the admin pool, which owns the database. The
//! table names are the literal names of the migrations, so no bind parameter
//! carries an identifier.

use cadus_store::state::web_state_lock_key;
use sqlx::{AssertSqlSafe, Postgres, Transaction};

use super::prelude::*;
use super::{call, parse, put_doc};

/// The role the app pool runs as.
const APP_ROLE: &str = "cadus_app";

/// Run one DDL statement on the admin pool.
async fn run(db: &TestDb, sql: String) {
    sqlx::query(AssertSqlSafe(sql.clone()))
        .execute(&db.admin)
        .await
        .unwrap_or_else(|err| panic!("{sql}: {err}"));
}

/// Install the trigger function that raises `injected fault`.
async fn install_raise(db: &TestDb) {
    run(
        db,
        "CREATE OR REPLACE FUNCTION test_fault_raise() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RAISE EXCEPTION 'injected fault'; END $$"
            .to_string(),
    )
    .await;
}

/// Make every INSERT or UPDATE of `table` whose new row satisfies `condition`
/// fail. `condition` reads the row as `NEW`.
pub async fn fail_writes(db: &TestDb, table: &str, condition: &str) {
    install_raise(db).await;
    run(
        db,
        format!(
            "CREATE TRIGGER test_fault_write BEFORE INSERT OR UPDATE ON {table} \
             FOR EACH ROW WHEN ({condition}) EXECUTE FUNCTION test_fault_raise()"
        ),
    )
    .await;
}

/// Make the COMMIT of every transaction that inserted a row of `table`
/// satisfying `condition` fail. The statement itself succeeds.
pub async fn fail_commit_after_insert(db: &TestDb, table: &str, condition: &str) {
    install_raise(db).await;
    run(
        db,
        format!(
            "CREATE CONSTRAINT TRIGGER test_fault_commit AFTER INSERT ON {table} \
             DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN ({condition}) \
             EXECUTE FUNCTION test_fault_raise()"
        ),
    )
    .await;
}

/// Make every SELECT of the app role on `table` fail when the query text holds
/// `needle`. The table must carry row-level security, and the read must find a
/// row: the policy runs per row.
pub async fn fail_reads(db: &TestDb, table: &str, needle: &str) {
    run(
        db,
        "CREATE OR REPLACE FUNCTION test_fault_query(needle text) RETURNS boolean \
         LANGUAGE plpgsql STABLE AS $$ BEGIN \
         IF position(needle in current_query()) > 0 THEN \
         RAISE EXCEPTION 'injected fault: %', needle; END IF; RETURN true; END $$"
            .to_string(),
    )
    .await;
    run(
        db,
        format!("GRANT EXECUTE ON FUNCTION test_fault_query(text) TO {APP_ROLE}"),
    )
    .await;
    run(
        db,
        format!(
            "CREATE POLICY test_fault_read ON {table} AS RESTRICTIVE FOR SELECT \
             USING (test_fault_query('{needle}'))"
        ),
    )
    .await;
}

/// Take SELECT on `table` away from the app role.
pub async fn revoke_reads(db: &TestDb, table: &str) {
    run(db, format!("REVOKE SELECT ON {table} FROM {APP_ROLE}")).await;
}

/// Make every DELETE of `table` whose old row satisfies `condition` fail.
/// `condition` reads the row as `OLD`.
pub async fn fail_deletes(db: &TestDb, table: &str, condition: &str) {
    install_raise(db).await;
    run(
        db,
        format!(
            "CREATE TRIGGER test_fault_delete BEFORE DELETE ON {table} \
             FOR EACH ROW WHEN ({condition}) EXECUTE FUNCTION test_fault_raise()"
        ),
    )
    .await;
}

/// Make every SELECT of the app role on `table` fail when it reads a row that
/// satisfies `condition`. The table must carry row-level security.
pub async fn fail_rows(db: &TestDb, table: &str, condition: &str) {
    run(
        db,
        "CREATE OR REPLACE FUNCTION test_fault_row() RETURNS boolean \
         LANGUAGE plpgsql STABLE AS $$ BEGIN \
         RAISE EXCEPTION 'injected fault: row'; END $$"
            .to_string(),
    )
    .await;
    run(
        db,
        format!("GRANT EXECUTE ON FUNCTION test_fault_row() TO {APP_ROLE}"),
    )
    .await;
    run(
        db,
        format!(
            "CREATE POLICY test_fault_row ON {table} AS RESTRICTIVE FOR SELECT \
             USING (NOT ({condition}) OR test_fault_row())"
        ),
    )
    .await;
}

/// Hold the D-S6 advisory lock of `user` on the admin pool. The route's own
/// lock wait then passes `LOCK_TIMEOUT_MS` and fails. Drop the answer to
/// release the lock.
pub async fn hold_state_lock(db: &TestDb, user: Uuid) -> Transaction<'static, Postgres> {
    let (class, key) = web_state_lock_key(user);
    let mut tx = db.admin.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock($1, $2)")
        .bind(class)
        .bind(key)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

/// Take `set_config` away from every role of the throwaway database, so the
/// tenant bind of every route fails at its first statement.
pub async fn fail_tenant_bind(db: &TestDb) {
    run(
        db,
        "REVOKE EXECUTE ON FUNCTION pg_catalog.set_config(text, text, boolean) FROM PUBLIC"
            .to_string(),
    )
    .await;
}

/// Store a D-S6 row of `user` that no version of the document reads.
pub async fn seed_unreadable_state(db: &TestDb, user: Uuid) {
    put_doc(db, user, json!({"bogus": 1})).await;
}

/// Store a diagnostic document of `user` that does not read.
pub async fn seed_unreadable_diagnostic(db: &TestDb, user: Uuid) {
    sqlx::query("INSERT INTO diag_states (user_id, state) VALUES ($1, $2)")
        .bind(user)
        .bind(json!({"balances": "not a tally"}))
        .execute(&db.admin)
        .await
        .unwrap();
}

/// Call `method` `path` as `user` with `body`, and read the
/// `500 internal_error` envelope back.
pub async fn assert_internal(
    app: &Router,
    method: Method,
    path: &str,
    user: Uuid,
    body: Option<Value>,
) -> Value {
    let (status, raw) = call(app, method, path, Some(user), body).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{raw}");
    let body = parse(&raw);
    assert_eq!(body["error"]["code"], "internal_error", "{raw}");
    body
}
