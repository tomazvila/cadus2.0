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

/// Install the trigger function that makes an UPDATE a no-op: it returns
/// `NULL`, so the row is skipped and the statement reports zero rows.
async fn install_skip(db: &TestDb) {
    run(
        db,
        "CREATE OR REPLACE FUNCTION test_fault_skip() RETURNS trigger LANGUAGE plpgsql AS $$ \
         BEGIN RETURN NULL; END $$"
            .to_string(),
    )
    .await;
}

/// Make the COMMIT of every transaction that ran `event` on a row of `table`
/// satisfying `condition` fail. The statement itself succeeds. `condition`
/// reads the row as `NEW` for an insert or an update, and as `OLD` for a
/// delete.
pub async fn fail_commit_after(db: &TestDb, table: &str, event: &str, condition: &str) {
    install_raise(db).await;
    run(
        db,
        format!(
            "CREATE CONSTRAINT TRIGGER test_fault_commit_{} AFTER {event} ON {table} \
             DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN ({condition}) \
             EXECUTE FUNCTION test_fault_raise()",
            event
                .split_whitespace()
                .next()
                .unwrap_or("x")
                .to_lowercase()
        ),
    )
    .await;
}

/// Make every UPDATE of `table` whose old row satisfies `condition` a no-op:
/// the statement succeeds and reports zero rows.
pub async fn skip_updates(db: &TestDb, table: &str, condition: &str) {
    install_skip(db).await;
    run(
        db,
        format!(
            "CREATE TRIGGER test_fault_skip BEFORE UPDATE ON {table} \
             FOR EACH ROW WHEN ({condition}) EXECUTE FUNCTION test_fault_skip()"
        ),
    )
    .await;
}

/// Install the policy function that hides a row from a query whose text holds
/// `needle` on its first `hidden` evaluations, and shows it after them. The
/// counter is one sequence of the database, so one test installs one hide.
async fn install_hide(db: &TestDb) {
    run(
        db,
        "CREATE OR REPLACE FUNCTION test_fault_hide(needle text, hidden bigint) RETURNS boolean \
         LANGUAGE plpgsql STABLE AS $$ BEGIN \
         IF position(needle in current_query()) = 0 THEN RETURN true; END IF; \
         RETURN nextval('test_hide_calls') > hidden; END $$"
            .to_string(),
    )
    .await;
    run(
        db,
        format!("GRANT EXECUTE ON FUNCTION test_fault_hide(text, bigint) TO {APP_ROLE}"),
    )
    .await;
    run(
        db,
        "CREATE SEQUENCE IF NOT EXISTS test_hide_calls".to_string(),
    )
    .await;
    run(
        db,
        format!("GRANT USAGE ON SEQUENCE test_hide_calls TO {APP_ROLE}"),
    )
    .await;
}

/// The SQL string literal of `text`.
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Hide every row of `table` from the app role on the first `hidden` reads
/// whose query text holds `needle`, and show it after them. The read raises no
/// error: it sees no row. The table must carry row-level security.
pub async fn hide_rows_first(db: &TestDb, table: &str, needle: &str, hidden: i64) {
    install_hide(db).await;
    run(
        db,
        format!(
            "CREATE POLICY test_fault_hide ON {table} AS RESTRICTIVE FOR SELECT \
             USING (test_fault_hide({}, {hidden}))",
            quote(needle)
        ),
    )
    .await;
}

/// Hide every row of `table` from the app role when the query text holds
/// `needle`. See [`hide_rows_first`].
pub async fn hide_rows(db: &TestDb, table: &str, needle: &str) {
    hide_rows_first(db, table, needle, i64::MAX).await;
}

/// Drop one function of the schema, so every call of it fails.
pub async fn drop_function(db: &TestDb, signature: &str) {
    run(db, format!("DROP FUNCTION {signature}")).await;
}

/// Make every UPDATE of `table` whose rows satisfy `condition` fail.
/// `condition` reads the rows as `OLD` and `NEW`.
pub async fn fail_updates(db: &TestDb, table: &str, condition: &str) {
    install_raise(db).await;
    run(
        db,
        format!(
            "CREATE TRIGGER test_fault_update BEFORE UPDATE ON {table} \
             FOR EACH ROW WHEN ({condition}) EXECUTE FUNCTION test_fault_raise()"
        ),
    )
    .await;
}

/// Replace the body of the SECURITY DEFINER account lookup `name` with one
/// that reads `users u` under `where_clause`. The lookups run as their owner,
/// so no policy of the table reaches them; the body is the one seam.
async fn replace_user_lookup(db: &TestDb, name: &str, param: &str, where_clause: &str) {
    run(
        db,
        format!(
            "CREATE OR REPLACE FUNCTION {name}({param}) RETURNS TABLE (id uuid, \
             password_hash text, email_verified_at timestamptz, disabled_at timestamptz, \
             is_admin boolean) LANGUAGE sql SECURITY DEFINER SET search_path = public, pg_temp \
             AS $$ SELECT u.id, u.password_hash, u.email_verified_at, u.disabled_at, u.is_admin \
             FROM users u WHERE {where_clause} $$"
        ),
    )
    .await;
}

/// Make the account lookup by id find no row.
pub async fn hide_user_by_id(db: &TestDb) {
    replace_user_lookup(db, "auth_user_by_id", "p_id uuid", "false").await;
}

/// Make the account lookup by address find no row on its first `hidden`
/// calls, and the row after them.
pub async fn hide_user_by_email_first(db: &TestDb, hidden: i64) {
    run(
        db,
        "CREATE SEQUENCE IF NOT EXISTS test_hide_calls".to_string(),
    )
    .await;
    replace_user_lookup(
        db,
        "auth_user_by_email",
        "p_email citext",
        &format!("u.email = p_email AND nextval('test_hide_calls') > {hidden}"),
    )
    .await;
}

/// Rename `column` of `table`, so every statement that reads the column fails
/// and every statement that does not read it still runs.
pub async fn hide_column(db: &TestDb, table: &str, column: &str) {
    run(
        db,
        format!("ALTER TABLE {table} RENAME COLUMN {column} TO {column}_hidden"),
    )
    .await;
}
