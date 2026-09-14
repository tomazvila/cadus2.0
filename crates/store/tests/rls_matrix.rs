//! Proof tests for the database-level guarantees of M0, part 3: the literal
//! privilege matrix, the default privileges, the RLS coverage list, and the
//! sequence privileges (D9).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use common::rls::{
    ALL_USER_ID_TABLES, APP_SEQUENCE_PRIVILEGES, APP_TABLE_PRIVILEGES, EXEMPT_TABLES,
    POLICY_PREDICATE, PolicyRow, RLS_TABLES, USERS_SELF_PREDICATE, to_owned,
};
use common::sqlstate;

/// C2, C3, U3, finding #16: the table privileges of `cadus_app` are the literal
/// matrix of `APP_TABLE_PRIVILEGES`.
///
/// The hand-picked negative assertions elsewhere in this file leave the rest of
/// the ACL unpinned, so a widened blanket grant of TRUNCATE, which row-level
/// security does not cover at all, passed the whole suite. This test reads every
/// relation of schema `public` and compares the whole matrix.
///
/// Finding #6: the enumeration reads `pg_class` with `relkind IN ('r','p','v','m')`,
/// not `pg_tables`. `ALTER DEFAULT PRIVILEGES ... ON TABLES` covers a view and a
/// materialized view too, so a view of a later migration arrives with `arwd` for
/// `cadus_app`. `pg_tables` never showed it. `pg_class` puts it in the matrix,
/// where the literal list fails until someone reviews the view.
#[tokio::test]
async fn app_role_privilege_matrix_is_the_literal_table() {
    TestDb::with(|db| async move {
        let rows = sqlx::query!(
            r#"
            SELECT c.relname::text AS "table_name!",
                   has_table_privilege('cadus_app', c.oid, 'SELECT')   AS "may_select!",
                   has_table_privilege('cadus_app', c.oid, 'INSERT')   AS "may_insert!",
                   has_table_privilege('cadus_app', c.oid, 'UPDATE')   AS "may_update!",
                   has_table_privilege('cadus_app', c.oid, 'DELETE')   AS "may_delete!",
                   has_table_privilege('cadus_app', c.oid, 'TRUNCATE') AS "may_truncate!"
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relkind IN ('r', 'p', 'v', 'm')
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        // Sort in Rust. A SQL `ORDER BY` on text follows the database collation,
        // and the order of `user_settings` against `users` differs between
        // collations.
        let mut found: Vec<(String, [bool; 5])> = rows
            .iter()
            .map(|row| {
                (
                    row.table_name.clone(),
                    [
                        row.may_select,
                        row.may_insert,
                        row.may_update,
                        row.may_delete,
                        row.may_truncate,
                    ],
                )
            })
            .collect();
        found.sort();

        let expected: Vec<(String, [bool; 5])> = APP_TABLE_PRIVILEGES
            .iter()
            .map(|(table, privileges)| ((*table).to_string(), *privileges))
            .collect();
        assert_eq!(found, expected);

        // #4 and #5: `has_table_privilege` reports a column-level grant as false,
        // so the UPDATE cell and the INSERT cell of `users` need a second,
        // column-level assertion.
        let columns = sqlx::query!(
            r#"
            SELECT has_column_privilege('cadus_app', 'users', 'is_admin', 'UPDATE')
                       AS "may_update_is_admin!",
                   has_column_privilege('cadus_app', 'users', 'password_hash', 'UPDATE')
                       AS "may_update_password_hash!",
                   has_column_privilege('cadus_app', 'users', 'is_admin', 'INSERT')
                       AS "may_insert_is_admin!",
                   has_column_privilege('cadus_app', 'users', 'password_hash', 'INSERT')
                       AS "may_insert_password_hash!"
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(
            !columns.may_update_is_admin,
            "cadus_app must not update users.is_admin"
        );
        assert!(
            columns.may_update_password_hash,
            "cadus_app must update users.password_hash"
        );
        assert!(
            !columns.may_insert_is_admin,
            "cadus_app must not insert users.is_admin"
        );
        assert!(
            columns.may_insert_password_hash,
            "cadus_app must insert users.password_hash"
        );
    })
    .await;
}

/// D9, findings #20 (round 2) and #7 (round 4): `ALTER DEFAULT PRIVILEGES` holds
/// the literal grant surface.
///
/// The two schema-scoped statements exist so that a table or a sequence of a
/// later migration is grantable without a manual GRANT. The entries carry the
/// grantor after a slash, so the test compares the part before it.
///
/// Round-4 finding #7: the third statement takes the automatic PUBLIC EXECUTE
/// away from a function of a later migration, so a SECURITY DEFINER helper is
/// closed until a migration grants it. That statement carries no `IN SCHEMA`
/// clause: a schema-scoped default ACL is a delta that Postgres adds to the
/// hard-wired default, so the PUBLIC entry survives a schema-scoped REVOKE. The
/// global form replaces the hard-wired default instead. The global entry has
/// `defaclnamespace = 0`, so this test reads every row of `pg_default_acl`.
#[tokio::test]
async fn default_privileges_are_the_literal_grants() {
    TestDb::with(|db| async move {
        // The owner of a default-privilege entry is the migration runner, and
        // that role name differs between deployments: `postgres` in the shipped
        // compose stack, the test superuser here. `db.admin` runs the migrations,
        // so `current_user` on that pool names the same role.
        let owner = sqlx::query_scalar!(r#"SELECT current_user AS "owner!""#)
            .fetch_one(&db.admin)
            .await
            .unwrap();

        let rows = sqlx::query!(
            r#"
            SELECT coalesce(n.nspname, '')::text  AS "schema_name!",
                   d.defaclobjtype::text          AS "obj_type!",
                   split_part(entry::text, '/', 1) AS "acl_entry!"
            FROM pg_default_acl d
            LEFT JOIN pg_namespace n ON n.oid = d.defaclnamespace
            CROSS JOIN LATERAL unnest(d.defaclacl) AS entry
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        // Sort in Rust. A SQL `ORDER BY` on text follows the database collation, and
        // the case order of 'S' against 'r' differs between collations.
        let mut found: Vec<(String, String, String)> = rows
            .iter()
            .map(|row| {
                (
                    row.schema_name.clone(),
                    row.obj_type.clone(),
                    row.acl_entry.clone(),
                )
            })
            .collect();
        found.sort();
        let mut expected: Vec<(String, String, String)> = vec![
            // #7: the global function default. The owner keeps EXECUTE and
            // nobody else holds it, so PUBLIC has no entry here. A PUBLIC entry
            // prints with an empty grantee, as `=X`.
            (String::new(), "f".to_string(), format!("{owner}=X")),
            (
                "public".to_string(),
                "S".to_string(),
                "cadus_admin=rU".to_string(),
            ),
            (
                "public".to_string(),
                "S".to_string(),
                "cadus_app=rU".to_string(),
            ),
            (
                "public".to_string(),
                "r".to_string(),
                "cadus_admin=arwd".to_string(),
            ),
            (
                "public".to_string(),
                "r".to_string(),
                "cadus_app=arwd".to_string(),
            ),
        ];
        expected.sort();
        assert_eq!(found, expected);

        // The three object types are exactly tables ('r'), sequences ('S'), and
        // functions ('f').
        let mut obj_types: Vec<String> = rows.iter().map(|row| row.obj_type.clone()).collect();
        obj_types.sort();
        obj_types.dedup();
        assert_eq!(
            obj_types,
            vec!["S".to_string(), "f".to_string(), "r".to_string()]
        );
    })
    .await;
}

/// C3, finding #23: the set of protected tables is the literal list of
/// `docs/SCHEMA.md`, and both catalog flags are asserted one by one.
///
/// A new tenant table without its own policy fails this test. An exempt table
/// that gains `ENABLE` without `FORCE` fails it too.
#[tokio::test]
async fn rls_coverage_is_the_literal_list() {
    TestDb::with(|db| async move {
        let rows = sqlx::query!(
            r#"
            SELECT c.relname::text        AS "table_name!",
                   c.relrowsecurity       AS "rls_enabled!",
                   c.relforcerowsecurity  AS "rls_forced!"
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            JOIN pg_attribute a ON a.attrelid = c.oid
                               AND a.attname = 'user_id'
                               AND a.attnum > 0
                               AND NOT a.attisdropped
            WHERE n.nspname = 'public' AND c.relkind = 'r'
            ORDER BY c.relname
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        // The union of the two buckets is the whole set. No table sits outside both.
        let all: Vec<String> = rows.iter().map(|row| row.table_name.clone()).collect();
        assert_eq!(all, to_owned(&ALL_USER_ID_TABLES));

        for row in &rows {
            let name = row.table_name.as_str();
            if RLS_TABLES.contains(&name) {
                assert!(row.rls_enabled, "relrowsecurity must be true on {name}");
                assert!(row.rls_forced, "relforcerowsecurity must be true on {name}");
            } else {
                assert!(
                    EXEMPT_TABLES.contains(&name),
                    "{name} is in neither literal list"
                );
                assert!(!row.rls_enabled, "relrowsecurity must be false on {name}");
                assert!(
                    !row.rls_forced,
                    "relforcerowsecurity must be false on {name}"
                );
            }
        }

        let policies = sqlx::query!(
            r#"
            SELECT c.relname::text AS "table_name!",
                   p.polname::text AS "policy_name!",
                   p.polcmd::text  AS "command!",
                   pg_get_expr(p.polqual, p.polrelid)      AS "using_expr?",
                   pg_get_expr(p.polwithcheck, p.polrelid) AS "with_check_expr?"
            FROM pg_policy p
            JOIN pg_class c ON c.oid = p.polrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public'
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        // Pin the name, the command, and both expressions of every policy. A
        // migration that keeps the name `tenant_isolation` and drops the WITH
        // CHECK clause, or that drops the nullif guard, fails here. Sort in Rust:
        // a SQL `ORDER BY` on text follows the database collation, and the order
        // of `user_settings` against `users` differs between collations.
        let mut found: Vec<PolicyRow> = policies
            .iter()
            .map(|row| {
                (
                    row.table_name.clone(),
                    row.policy_name.clone(),
                    row.command.clone(),
                    row.using_expr.clone(),
                    row.with_check_expr.clone(),
                )
            })
            .collect();
        found.sort();

        let mut expected: Vec<PolicyRow> = RLS_TABLES
            .iter()
            .map(|table| {
                (
                    (*table).to_string(),
                    if *table == "problem_reports" {
                        "tenant_problem_reports".to_string()
                    } else {
                        "tenant_isolation".to_string()
                    },
                    // '*' is the polcmd of a policy that covers every command.
                    "*".to_string(),
                    Some(POLICY_PREDICATE.to_string()),
                    Some(POLICY_PREDICATE.to_string()),
                )
            })
            .collect();
        // #4 and #11: the three per-command policies of `users`. 'a' is INSERT,
        // 'r' is SELECT, and 'w' is UPDATE. SELECT and UPDATE carry the same
        // `id` predicate. There is no DELETE policy, because 0006 revokes DELETE
        // on `users` from the runtime role.
        expected.push((
            "users".to_string(),
            "users_insert".to_string(),
            "a".to_string(),
            None,
            Some("true".to_string()),
        ));
        expected.push((
            "users".to_string(),
            "users_read_self".to_string(),
            "r".to_string(),
            Some(USERS_SELF_PREDICATE.to_string()),
            None,
        ));
        expected.push((
            "users".to_string(),
            "users_update_self".to_string(),
            "w".to_string(),
            Some(USERS_SELF_PREDICATE.to_string()),
            Some(USERS_SELF_PREDICATE.to_string()),
        ));
        expected.sort();

        assert_eq!(found.len(), 21);
        assert_eq!(found, expected);

        // #6: schema public holds no view and no materialized view. A view runs
        // with the rights of its owner, and the owner is the migration runner, a
        // superuser in the shipped stack. Such a view reads and writes `events`
        // outside the tenant policy and outside the append-only revoke, and the
        // default privileges of 0006 hand `cadus_app` all four DML privileges on
        // it. The literal count is 0: a later view fails this test until someone
        // reviews it and writes its own revoke.
        let views = sqlx::query_scalar!(
            r#"
            SELECT count(*) AS "count!"
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relkind IN ('v', 'm')
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(views, 0);
    })
    .await;
}

/// C3, T6, finding #12: the sequence privileges of `cadus_app` are the literal
/// matrix of `APP_SEQUENCE_PRIVILEGES`.
///
/// `REVOKE ALL ON model_call_log` leaves the identity sequence of that table
/// untouched, and the blanket grant of 0006 gave `cadus_app` USAGE and SELECT on
/// it. The runtime role therefore read `last_value`, the cluster-wide count of
/// model calls, and moved the ledger key with `nextval` — under a comment that
/// names the empty table ACL as the reason `model_call_log` needs no policy.
/// This test reads every sequence of schema `public`, so a later `bigserial`
/// column also lands in the literal list.
#[tokio::test]
async fn app_role_sequence_privileges_are_the_literal_table() {
    TestDb::with(|db| async move {
        let rows = sqlx::query!(
            r#"
            SELECT c.relname::text AS "sequence_name!",
                   has_sequence_privilege('cadus_app', c.oid, 'USAGE')  AS "may_use!",
                   has_sequence_privilege('cadus_app', c.oid, 'SELECT') AS "may_select!",
                   has_sequence_privilege('cadus_app', c.oid, 'UPDATE') AS "may_update!"
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relkind = 'S'
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        // Sort in Rust. A SQL `ORDER BY` on text follows the database collation.
        let mut found: Vec<(String, [bool; 3])> = rows
            .iter()
            .map(|row| {
                (
                    row.sequence_name.clone(),
                    [row.may_use, row.may_select, row.may_update],
                )
            })
            .collect();
        found.sort();

        let expected: Vec<(String, [bool; 3])> = APP_SEQUENCE_PRIVILEGES
            .iter()
            .map(|(sequence, privileges)| ((*sequence).to_string(), *privileges))
            .collect();
        assert_eq!(found, expected);

        // The live statements fail too, not only the catalog view of them.
        let read_err =
            sqlx::query!(r#"SELECT last_value AS "last_value!" FROM model_call_log_id_seq"#)
                .fetch_all(&db.app)
                .await
                .unwrap_err();
        assert_eq!(sqlstate(&read_err), "42501");
        let advance_err = sqlx::query!(r#"SELECT nextval('model_call_log_id_seq') AS "next!""#)
            .fetch_all(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&advance_err), "42501");

        // The worker writes the ledger as cadus_admin, so that role keeps both.
        let admin_privileges = sqlx::query!(
            r#"
            SELECT has_sequence_privilege('cadus_admin', 'model_call_log_id_seq', 'USAGE')
                       AS "may_use!",
                   has_sequence_privilege('cadus_admin', 'model_call_log_id_seq', 'SELECT')
                       AS "may_select!"
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(admin_privileges.may_use, "cadus_admin keeps USAGE");
        assert!(admin_privileges.may_select, "cadus_admin keeps SELECT");
    })
    .await;
}

/// Report results are mutable only in the worker queue; evidence and corrections append.
#[tokio::test]
async fn report_admin_grants_and_non_tenant_evidence_tables_are_exact() {
    TestDb::with(|db| async move {
        type ReportPrivileges = (String, bool, bool, bool, bool, bool, bool, bool);
        let mut rows: Vec<ReportPrivileges> = sqlx::query_as(
            "SELECT c.relname::text,
                    has_table_privilege('cadus_admin', c.oid, 'SELECT'),
                    has_table_privilege('cadus_admin', c.oid, 'INSERT'),
                    has_table_privilege('cadus_admin', c.oid, 'UPDATE'),
                    has_table_privilege('cadus_admin', c.oid, 'DELETE'),
                    has_table_privilege('cadus_admin', c.oid, 'TRUNCATE'),
                    c.relrowsecurity, c.relforcerowsecurity
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
             WHERE n.nspname = 'public'
               AND c.relname IN ('problem_reports','problem_report_steps','problem_corrections')
             ORDER BY c.relname",
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        rows.sort();
        assert_eq!(rows, vec![
            ("problem_corrections".to_string(), true, true, false, false, false, false, false),
            ("problem_report_steps".to_string(), true, true, false, false, false, false, false),
            ("problem_reports".to_string(), true, true, true, true, false, true, true),
        ]);
    }).await;
}
