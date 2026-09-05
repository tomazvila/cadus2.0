//! Process tests of `cadus-migrate` under a fault at one step: the database
//! URL, the connect, the ledger count before and after the migrations, the
//! migrations, the maintenance database of the role lock, and the two
//! `ALTER ROLE` statements.
//!
//! Every test starts the real binary and reads the literal exit code and the
//! literal error line. `migrate_bin.rs` and `migrate_admin.rs` hold the runs
//! that succeed.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::process::Output;

use cadus_store::test_support::TestDb;
use common::migrate::{
    HEX_PASSWORD, RoleLock, create_database, drop_database, exec_on, expect_exit_two,
    migrate_command, role_dsn, run_migrate,
};
use sqlx::{AssertSqlSafe, Connection, PgConnection};

/// The ledger table of sqlx, as `sqlx-postgres` creates it.
const LEDGER_TABLE: &str = "CREATE TABLE public._sqlx_migrations (\
     version BIGINT PRIMARY KEY, description TEXT NOT NULL, \
     installed_on TIMESTAMPTZ NOT NULL DEFAULT now(), success BOOLEAN NOT NULL, \
     checksum BYTEA NOT NULL, execution_time BIGINT NOT NULL)";

/// A function that raises `injected`, and a relation named `_sqlx_migrations`
/// in `schema` that reads through it.
fn raising_ledger(schema: &str) -> Vec<String> {
    vec![
        format!("CREATE SCHEMA IF NOT EXISTS {schema}"),
        format!(
            "CREATE FUNCTION {schema}.fault_raise() RETURNS boolean LANGUAGE plpgsql AS $$ \
             BEGIN RAISE EXCEPTION 'injected'; END $$"
        ),
        format!(
            "CREATE VIEW {schema}._sqlx_migrations AS SELECT 1::bigint AS version WHERE \
             {schema}.fault_raise()"
        ),
    ]
}

/// The stdout of a run, which a failed run leaves empty.
fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The row count of the ledger of the database `name`.
async fn ledger_rows(name: &str) -> i64 {
    let mut conn = PgConnection::connect(&TestDb::superuser_dsn_for(name))
        .await
        .unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM public._sqlx_migrations")
        .fetch_one(&mut conn)
        .await
        .unwrap();
    let _ = conn.close().await;
    rows
}

/// An empty `DATABASE_URL` stops the run before the signals and the connect.
#[test]
fn an_empty_database_url_exits_two() {
    let output = run_migrate("", None, &[]);
    expect_exit_two(
        &output,
        "cadus-migrate: configuration error: DATABASE_URL is empty",
    );
}

/// A cluster that does not answer stops the run at the connect.
#[test]
fn a_refused_connect_exits_two() {
    let output = run_migrate("postgresql://x@127.0.0.1:1/x", None, &[]);
    expect_exit_two(&output, "cadus-migrate: database error: ");
    assert_eq!(stdout_of(&output), "");
}

/// A ledger that does not read stops the run at the count before the
/// migrations, so the database keeps its state.
#[tokio::test]
async fn a_ledger_that_does_not_read_stops_the_run_before_the_migrations() {
    TestDb::with(|db| async move {
        let fresh = create_database(&db, "ledger").await;
        exec_on(&fresh, &raising_ledger("public")).await;

        let output = run_migrate(&TestDb::superuser_dsn_for(&fresh), None, &[]);

        let dropped = drop_database(&db, &fresh).await;
        expect_exit_two(&output, "cadus-migrate: database error: ");
        expect_exit_two(&output, "injected");
        assert_eq!(stdout_of(&output), "");
        dropped.unwrap();
    })
    .await;
}

/// A ledger of another shape stops the run inside the migrations: sqlx reads
/// a column the table does not hold.
#[tokio::test]
async fn a_ledger_of_another_shape_stops_the_migrations() {
    TestDb::with(|db| async move {
        let fresh = create_database(&db, "shape").await;
        exec_on(
            &fresh,
            &["CREATE TABLE public._sqlx_migrations (version bigint)".to_string()],
        )
        .await;

        let output = run_migrate(&TestDb::superuser_dsn_for(&fresh), None, &[]);

        let dropped = drop_database(&db, &fresh).await;
        expect_exit_two(&output, "cadus-migrate: migration error: ");
        expect_exit_two(&output, "column \"success\" does not exist");
        assert_eq!(stdout_of(&output), "");
        dropped.unwrap();
    })
    .await;
}

/// A ledger that stops reading after the last migration stops the run at the
/// count after the migrations. Every migration is applied, and the report
/// line is not printed.
///
/// The fault: a trigger on the ledger row of the last migration moves the
/// session to a schema whose `_sqlx_migrations` raises, at the last write of
/// sqlx. The count reads the ledger by its bare name and meets that relation.
#[tokio::test]
async fn a_ledger_that_stops_reading_after_the_migrations_stops_the_report() {
    TestDb::with(|db| async move {
        let last: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
            .fetch_one(&db.admin)
            .await
            .unwrap();
        let fresh = create_database(&db, "after").await;
        let mut statements = vec![LEDGER_TABLE.to_string()];
        statements.extend(raising_ledger("shadow"));
        statements.push(
            "CREATE FUNCTION public.fault_flip() RETURNS trigger LANGUAGE plpgsql AS $$ \
             BEGIN PERFORM set_config('search_path', 'shadow', false); RETURN NULL; END $$"
                .to_string(),
        );
        statements.push(format!(
            "CREATE TRIGGER fault_flip AFTER UPDATE ON public._sqlx_migrations FOR EACH ROW \
             WHEN (NEW.version = {last}) EXECUTE FUNCTION public.fault_flip()"
        ));
        exec_on(&fresh, &statements).await;

        let output = run_migrate(&TestDb::superuser_dsn_for(&fresh), None, &[]);

        let applied = ledger_rows(&fresh).await;
        let dropped = drop_database(&db, &fresh).await;
        expect_exit_two(&output, "cadus-migrate: database error: ");
        expect_exit_two(&output, "injected");
        assert_eq!(stdout_of(&output), "");
        assert_eq!(applied, 12);
        dropped.unwrap();
    })
    .await;
}

/// An empty `CADUS_MAINTENANCE_DB` stops the run after the migrations, before
/// the role lock.
#[tokio::test]
async fn an_empty_maintenance_database_variable_exits_two() {
    TestDb::with(|db| async move {
        let output = migrate_command(&db.superuser_dsn(), Some(""))
            .arg("--admin-login")
            .output()
            .unwrap();
        expect_exit_two(
            &output,
            "cadus-migrate: configuration error: CADUS_MAINTENANCE_DB is empty",
        );
        assert_eq!(stdout_of(&output), "");
    })
    .await;
}

/// A maintenance database that the cluster does not hold stops the run at
/// the connect of the role lock.
#[tokio::test]
async fn a_maintenance_database_that_does_not_exist_exits_two() {
    TestDb::with(|db| async move {
        let output = migrate_command(&db.superuser_dsn(), Some("cadus2_t_no_such_db"))
            .arg("--admin-login")
            .output()
            .unwrap();
        expect_exit_two(&output, "cadus-migrate: database error: ");
        expect_exit_two(&output, "database \"cadus2_t_no_such_db\" does not exist");
        assert_eq!(stdout_of(&output), "");
    })
    .await;
}

/// Run `--admin-login` as `role` against the database of `db`, with `env`,
/// under the cluster-wide role lock of the test. The binary locks in the
/// throwaway database, so the two holders never wait on each other.
///
/// The run needs two rights on the migrated database: the ledger read of the
/// count, and the schema right of the `CREATE TABLE IF NOT EXISTS` of sqlx.
/// Both go away after the run, so the role drops with no dependent object.
async fn admin_login_as(db: &TestDb, role: &str, env: &[(&str, &str)]) -> Output {
    for sql in [
        format!("GRANT SELECT ON _sqlx_migrations TO \"{role}\""),
        format!("GRANT CREATE ON SCHEMA public TO \"{role}\""),
    ] {
        sqlx::query(AssertSqlSafe(sql))
            .execute(&db.admin)
            .await
            .unwrap();
    }
    let role_lock = RoleLock::acquire().await;
    let mut command = migrate_command(&role_dsn(&db.name, role), Some(&db.name));
    command.arg("--admin-login");
    for (name, value) in env {
        command.env(name, value);
    }
    let output = command.output().unwrap();
    role_lock.release().await;
    for sql in [
        format!("REVOKE SELECT ON _sqlx_migrations FROM \"{role}\""),
        format!("REVOKE CREATE ON SCHEMA public FROM \"{role}\""),
    ] {
        sqlx::query(AssertSqlSafe(sql))
            .execute(&db.admin)
            .await
            .unwrap();
    }
    output
}

/// A role without `CREATEROLE` reaches the `ALTER ROLE cadus_admin LOGIN`
/// statement, and that statement is refused.
#[tokio::test]
async fn a_role_that_cannot_alter_the_admin_role_exits_two() {
    TestDb::with(|db| async move {
        TestDb::with_role(&db, "cadus2_t_plain", "LOGIN", |db, role, _| async move {
            let output = admin_login_as(&db, &role, &[]).await;
            expect_exit_two(&output, "cadus-migrate: database error: ");
            expect_exit_two(&output, "permission denied");
            assert_eq!(stdout_of(&output), "");
        })
        .await;
    })
    .await;
}

/// A role with `CREATEROLE` and the admin option on `cadus_admin` grants the
/// login, and the password statement on `cadus_app` is refused.
#[tokio::test]
async fn a_role_that_cannot_alter_the_app_role_exits_two() {
    TestDb::with(|db| async move {
        TestDb::with_role(
            &db,
            "cadus2_t_admin",
            "LOGIN CREATEROLE",
            |db, role, _| async move {
                sqlx::query(AssertSqlSafe(format!(
                    "GRANT cadus_admin TO \"{role}\" WITH ADMIN OPTION"
                )))
                .execute(&db.admin)
                .await
                .unwrap();
                let output =
                    admin_login_as(&db, &role, &[("CADUS_APP_PASSWORD", HEX_PASSWORD)]).await;
                expect_exit_two(&output, "cadus-migrate: database error: ");
                expect_exit_two(&output, "permission denied");
                assert_eq!(stdout_of(&output), "");
            },
        )
        .await;
    })
    .await;
}
