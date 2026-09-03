//! Fault injection for one throwaway database: a statement of the app role
//! fails on purpose, so a test reaches the error arm of one store call.
//!
//! Every statement here is DDL on the admin pool, which owns the database. The
//! table names are the literal names of the migrations, so no bind parameter
//! carries an identifier.

use cadus_store::test_support::TestDb;
use sqlx::AssertSqlSafe;

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
