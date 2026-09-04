//! Proof tests for the database-level guarantees of M0, part 6: the column
//! ACL list, the public functions, and the foreign-key delete actions (D9).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use common::rls::{COLUMN_ACL_GRANTS, FOREIGN_KEY_DELETE_ACTIONS, PUBLIC_FUNCTIONS, triples};
use common::sqlstate;

/// C2, C3, round-4 finding #5: the column-level ACLs of schema `public` are the
/// literal list of `COLUMN_ACL_GRANTS`.
///
/// `has_table_privilege` reports a column grant as `false`, so the privilege
/// matrix is blind to one: `GRANT UPDATE (payload) ON events TO cadus_app`
/// rewrote the authoritative event document with every C2 test green. This test
/// reads `pg_attribute.attacl` for every column of the schema, so a column grant
/// on `events`, `content_store`, or `model_call_log` fails the literal list.
#[tokio::test]
async fn no_column_level_acl_outside_the_literal_list() {
    TestDb::with(|db| async move {
        let rows = sqlx::query!(
            r#"
            SELECT c.relname::text                  AS "table_name!",
                   a.attname::text                  AS "column_name!",
                   split_part(entry::text, '/', 1)  AS "acl_entry!"
            FROM pg_attribute a
            JOIN pg_class c ON c.oid = a.attrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            CROSS JOIN LATERAL unnest(a.attacl) AS entry
            WHERE n.nspname = 'public'
              AND a.attnum > 0
              AND NOT a.attisdropped
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        let found = triples(&rows, |row| {
            (
                row.table_name.clone(),
                row.column_name.clone(),
                row.acl_entry.clone(),
            )
        });

        let expected: Vec<(String, String, String)> = COLUMN_ACL_GRANTS
            .iter()
            .map(|(table, column, acl)| {
                (
                    (*table).to_string(),
                    (*column).to_string(),
                    (*acl).to_string(),
                )
            })
            .collect();
        assert_eq!(found, expected);
    })
    .await;
}

/// C3, round-4 findings #4 and #7: the functions of schema `public` are the
/// literal list of `PUBLIC_FUNCTIONS`, and every other function there belongs to
/// the `citext` extension.
///
/// A SECURITY DEFINER function owned by the migration runner bypasses row-level
/// security and the append-only revoke, and `EXECUTE` on a new function goes to
/// PUBLIC by default. The old suite matched the name prefix `auth_user_by_`, so a
/// new helper was invisible to every test. This test enumerates `pg_proc`.
///
/// The extension half reads the property, not the number. An earlier version
/// pinned the count of citext functions at 47, and a Postgres or citext upgrade
/// changed that one literal without changing one fact that the test guards. The
/// test now names no count: every function that `pg_depend` ties to an extension
/// belongs to `citext`, is SECURITY INVOKER, and carries the default ACL, so no
/// `cadus_app` EXECUTE grant hides inside the extension.
#[tokio::test]
async fn public_functions_are_the_literal_list() {
    TestDb::with(|db| async move {
        let rows = sqlx::query!(
            r#"
            SELECT p.proname::text        AS "name!",
                   p.prosecdef            AS "security_definer!",
                   coalesce(p.proconfig::text, '') AS "config!",
                   has_function_privilege('cadus_app', p.oid, 'EXECUTE')   AS "app_execute!",
                   has_function_privilege('cadus_admin', p.oid, 'EXECUTE') AS "admin_execute!",
                   has_function_privilege('public', p.oid, 'EXECUTE')      AS "public_execute!",
                   p.proacl IS NULL       AS "acl_is_default!",
                   (
                       SELECT e.extname::text
                       FROM pg_depend d
                       JOIN pg_extension e ON e.oid = d.refobjid
                       WHERE d.objid = p.oid
                         AND d.classid = 'pg_proc'::regclass
                         AND d.deptype = 'e'
                       LIMIT 1
                   ) AS "extension?"
            FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'public'
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        // The functions that a migration creates, one by one.
        let mut found: Vec<(String, bool, String, bool, bool, bool)> = rows
            .iter()
            .filter(|row| row.extension.is_none())
            .map(|row| {
                (
                    row.name.clone(),
                    row.security_definer,
                    row.config.clone(),
                    row.app_execute,
                    row.admin_execute,
                    row.public_execute,
                )
            })
            .collect();
        found.sort();

        let expected: Vec<(String, bool, String, bool, bool, bool)> = PUBLIC_FUNCTIONS
            .iter()
            .map(|(name, secdef, config, app, admin, public)| {
                (
                    (*name).to_string(),
                    *secdef,
                    (*config).to_string(),
                    *app,
                    *admin,
                    *public,
                )
            })
            .collect();
        assert_eq!(found, expected);

        // The rest of schema `public` belongs to one extension, `citext`.
        // `migrations/0002_identity.sql` creates that extension, so the list
        // below is never empty; an empty list means the walk lost every
        // extension row and the loop after it proves nothing.
        let extension_functions: Vec<_> = rows
            .iter()
            .filter(|row| row.extension.is_some())
            .collect();
        assert!(
            !extension_functions.is_empty(),
            "schema public holds no extension function, so citext is gone"
        );
        for row in &extension_functions {
            assert_eq!(
                row.extension.as_deref(),
                Some("citext"),
                "function {} belongs to another extension",
                row.name
            );
            assert!(
                !row.security_definer,
                "extension function {} is SECURITY DEFINER, so it bypasses row-level security",
                row.name
            );
            assert!(
                row.acl_is_default,
                "extension function {} carries an EXECUTE grant of its own; the default ACL is the only one this schema allows",
                row.name
            );
        }

        let mut extensions = sqlx::query_scalar!(
            r#"
            SELECT e.extname::text AS "name!"
            FROM pg_extension e
            JOIN pg_namespace n ON n.oid = e.extnamespace
            WHERE n.nspname = 'public'
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();
        extensions.sort();
        assert_eq!(extensions, vec!["citext".to_string()]);
    })
    .await;
}

/// C2, D9, round-4 finding #8: the `ON DELETE` action of every foreign key is
/// the literal list of `FOREIGN_KEY_DELETE_ACTIONS`.
///
/// `migrations/0003_event_log.sql` names RESTRICT as the guarantee that the event
/// log outlives the account, and round-1 finding #2 was a cascade that destroyed
/// rows through this same parent. No test read the action, so the mutation from
/// RESTRICT to CASCADE on `events` survived the whole store suite. One
/// `DELETE FROM users` then erased a learner's whole history.
#[tokio::test]
async fn foreign_key_delete_actions_are_the_literal_list() {
    TestDb::with(|db| async move {
        let rows = sqlx::query!(
            r#"
            SELECT c.relname::text       AS "table_name!",
                   con.conname::text     AS "constraint_name!",
                   con.confdeltype::text AS "delete_action!"
            FROM pg_constraint con
            JOIN pg_class c ON c.oid = con.conrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND con.contype = 'f'
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        let found = triples(&rows, |row| {
            (
                row.table_name.clone(),
                row.constraint_name.clone(),
                row.delete_action.clone(),
            )
        });

        let expected: Vec<(String, String, String)> = FOREIGN_KEY_DELETE_ACTIONS
            .iter()
            .map(|(table, constraint, action)| {
                (
                    (*table).to_string(),
                    (*constraint).to_string(),
                    (*action).to_string(),
                )
            })
            .collect();
        assert_eq!(found, expected);

        // The functional half of the pin: a `users` row with an event cannot be
        // deleted, not even by the superuser owner.
        let user = db.seed_user("restrict-guard@example.test").await;
        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();
        let delete_err = sqlx::query!("DELETE FROM users WHERE id = $1", user)
            .execute(&db.admin)
            .await
            .unwrap_err();
        // 23503 is foreign_key_violation: the RESTRICT action refused the delete.
        assert_eq!(sqlstate(&delete_err), "23503");
        let survivors = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(survivors, 1);
    })
    .await;
}
