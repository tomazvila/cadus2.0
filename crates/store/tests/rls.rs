//! Proof tests for the database-level guarantees of M0.
//!
//! - C2: `events` is append-only for the runtime role. The grant enforces it.
//! - C3: row-level security isolates the tenants, and the boot guard refuses a
//!   role that bypasses it.
//! - D9: the grant surface, the role attributes, and the idempotency index are
//!   what the migrations say.
//!
//! Every test uses `TestDb::with`, so it gets its own database, proves one
//! property with literal expected values, and drops the database again. The
//! drop also happens when the test body panics.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use cadus_store::test_support::TestDb;
use cadus_store::{StoreError, assert_rls_enforced, begin_tenant};
use uuid::Uuid;

/// The 12 tables that carry a tenant policy (`docs/SCHEMA.md`, C3).
///
/// `diagnosis_jobs` and `email_outbox` joined the list with finding #15. The
/// worker reads both as `cadus_admin`, which holds BYPASSRLS, so the old
/// exemption bought nothing and gave `cadus_app` every tenant's payload.
///
/// The order is the `C` collation order of `pg_class.relname`, because the
/// catalog queries below order by that column.
const RLS_TABLES: [&str; 12] = [
    "anki_cards_created",
    "anki_queue",
    "diag_states",
    "diagnosis_jobs",
    "email_outbox",
    "events",
    "learner_models",
    "profiles",
    "serving_pool",
    "session_plans",
    "user_settings",
    "web_states",
];

/// The 4 tables that carry a `user_id` and stay outside row-level security.
const EXEMPT_TABLES: [&str; 4] = [
    "auth_sessions",
    "auth_tokens",
    "model_call_log",
    "oauth_accounts",
];

/// The union of the two lists above: every `public` table with a `user_id`
/// column. The literal union pins that no table sits outside both buckets
/// (finding #23).
const ALL_USER_ID_TABLES: [&str; 16] = [
    "anki_cards_created",
    "anki_queue",
    "auth_sessions",
    "auth_tokens",
    "diag_states",
    "diagnosis_jobs",
    "email_outbox",
    "events",
    "learner_models",
    "model_call_log",
    "oauth_accounts",
    "profiles",
    "serving_pool",
    "session_plans",
    "user_settings",
    "web_states",
];

/// The literal text of the `tenant_isolation` predicate, as Postgres prints it
/// from the catalog. Both `USING` and `WITH CHECK` carry this text on all 12
/// policies. The literal pins the `nullif` guard and the `true` missing-ok flag,
/// so an edit of `migrations/0006_grants_rls.sql` cannot pass in silence (C3).
const POLICY_PREDICATE: &str =
    "(user_id = (NULLIF(current_setting('app.user_id'::text, true), ''::text))::uuid)";

/// The literal text of the `users_read_self` and `users_update_self` predicate
/// (findings #4 and #11).
///
/// `users` is keyed by `id`, not by `user_id`, so it stays outside the
/// `tenant_isolation` set and carries its own per-command policies. The
/// predicate is otherwise the same shape, `nullif` guard included. Finding #11
/// put the SELECT policy on this predicate too, so a bound tenant reads its own
/// row and no other.
const USERS_SELF_PREDICATE: &str =
    "(id = (NULLIF(current_setting('app.user_id'::text, true), ''::text))::uuid)";

/// Finding #12: the live sequence privileges of `cadus_app`, sequence by
/// sequence.
///
/// The array is `(sequence_name, [USAGE, SELECT, UPDATE])`, in the byte order of
/// the sequence name. Every sequence of schema `public` is here. The blanket
/// grant of `0006_grants_rls.sql` covered `model_call_log_id_seq`, so the
/// runtime role read `last_value` and moved the ledger key with `nextval`, and
/// no test saw it: the table matrix reads relations, not sequences.
const APP_SEQUENCE_PRIVILEGES: [(&str, [bool; 3]); 1] = [
    // #12: the model-call ledger is a `cadus_admin` table, sequence included.
    ("model_call_log_id_seq", [false, false, false]),
];

/// Finding #16: the live table privileges of `cadus_app`, relation by relation.
///
/// The array is `(relation_name, [SELECT, INSERT, UPDATE, DELETE, TRUNCATE])`,
/// in the byte order of the name. Every ordinary table, partitioned table, view,
/// and materialized view of schema `public` is here, `_sqlx_migrations`
/// included, so a widened grant in a later migration fails this test instead of
/// reaching production.
///
/// Finding #6: the list holds no view, because schema `public` holds none. A
/// view runs with the rights of its owner, and the owner is the migration
/// runner, a superuser in the shipped stack, so a view over `events` bypasses
/// the tenant policies and the append-only revoke. A later view lands in this
/// matrix and fails the literal list until someone reviews it.
///
/// `has_table_privilege` reports a column-level grant as `false`, so the INSERT
/// cell and the UPDATE cell of `users` are `false` even though `cadus_app`
/// writes five of its columns.
/// `app_role_privilege_matrix_is_the_literal_table` asserts the column grants
/// separately.
const APP_TABLE_PRIVILEGES: [(&str, [bool; 5]); 20] = [
    // #8: the runtime role holds nothing on the migration ledger.
    ("_sqlx_migrations", [false, false, false, false, false]),
    ("anki_cards_created", [true, true, true, true, false]),
    ("anki_queue", [true, true, true, true, false]),
    ("auth_rate_counters", [true, true, true, true, false]),
    ("auth_sessions", [true, true, true, true, false]),
    ("auth_tokens", [true, true, true, true, false]),
    // #14: the request tier reads approved content and never writes it.
    ("content_store", [true, false, false, false, false]),
    ("diag_states", [true, true, true, true, false]),
    ("diagnosis_jobs", [true, true, true, true, false]),
    ("email_outbox", [true, true, true, true, false]),
    // C2: events is append-only for the runtime role.
    ("events", [true, true, false, false, false]),
    ("learner_models", [true, true, true, true, false]),
    // #5: only the worker spends tokens, and the worker is cadus_admin.
    ("model_call_log", [false, false, false, false, false]),
    ("oauth_accounts", [true, true, true, true, false]),
    ("profiles", [true, true, true, true, false]),
    ("serving_pool", [true, true, true, true, false]),
    ("session_plans", [true, true, true, true, false]),
    ("user_settings", [true, true, true, true, false]),
    // #2, #4, and #5: no DELETE, no TRUNCATE, and INSERT and UPDATE are
    // column-level only.
    ("users", [true, false, false, false, false]),
    ("web_states", [true, true, true, true, false]),
];

/// The line of `migrations/0001_roles.sql` that gives `cadus_app` its
/// attributes. Finding #6: the roles are cluster-scoped and `CREATE ROLE` is
/// guarded, so a long-lived cluster keeps the old attributes and the catalog
/// alone proves nothing about the migration text.
const APP_ROLE_LINE: &str =
    "CREATE ROLE cadus_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;";

/// Return the SQLSTATE of a database error, or a message that names the miss.
fn sqlstate(err: &sqlx::Error) -> String {
    match err.as_database_error().and_then(|db| db.code()) {
        Some(code) => code.into_owned(),
        None => format!("not a database error: {err}"),
    }
}

/// One row of `pg_policy`: table, policy name, command, USING, WITH CHECK.
type PolicyRow = (String, String, String, Option<String>, Option<String>);

fn to_owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// C2: the app role appends to `events` and never edits or erases a row.
#[tokio::test]
async fn app_role_cannot_update_events() {
    TestDb::with(|db| async move {
        let user = db.seed_user("append-only@example.test").await;

        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let update_err = sqlx::query!("UPDATE events SET type = 'x'")
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&update_err), "42501");
        let _ = tx.rollback().await;

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let delete_err = sqlx::query!("DELETE FROM events")
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&delete_err), "42501");
        let _ = tx.rollback().await;

        // Append-only, not read-only: the insert of a second event succeeds.
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 2, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let _ = tx.rollback().await;
    })
    .await;
}

/// C3, finding #2: the app role cannot delete a `users` row.
///
/// `users` stays outside the `tenant_isolation` set, because its key is `id`,
/// and every tenant table points at it with `ON DELETE CASCADE`. Postgres runs a
/// referential-action trigger with row-level security off, so a `DELETE` on
/// `users` erases another tenant's rows through the cascade. Account deletion is
/// an admin operation. Findings #4 and #11 narrowed SELECT, INSERT, and UPDATE;
/// `app_role_reads_only_its_own_user_row`,
/// `app_role_inserts_no_id_and_no_admin_flag`, and
/// `app_role_updates_only_its_own_user_row` prove those parts.
#[tokio::test]
async fn app_role_cannot_delete_users() {
    TestDb::with(|db| async move {
        let user = db.seed_user("cascade-guard@example.test").await;

        sqlx::query!(
            "INSERT INTO learner_models
                 (user_id, model, through_seq, projector_version, config_hash)
             VALUES ($1, '{}'::jsonb, 0, 1, 'test')",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let delete_err = sqlx::query!("DELETE FROM users")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&delete_err), "42501");

        let truncate_err = sqlx::query("TRUNCATE users CASCADE")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&truncate_err), "42501");

        // The cascade never ran: the child row of the tenant is still there.
        let children = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM learner_models"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(children, 1);

        // #11: an unbound SELECT of the runtime role reads no row of users.
        let seen = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(seen, 0);

        // #4: an UPDATE of the caller's own row still succeeds inside a tenant.
        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        let updated = sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(updated, 1);
        tx.commit().await.unwrap();
    })
    .await;
}

/// D9, finding #8: the app role holds no privilege on the migration ledger.
///
/// sqlx creates `_sqlx_migrations` before the first migration runs, so the
/// blanket grant in 0006 swept it in. A `DELETE` on the ledger makes the next
/// deploy replay 0002 and stop with an error.
#[tokio::test]
async fn app_role_cannot_touch_the_migration_ledger() {
    TestDb::with(|db| async move {
        let read_err = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM _sqlx_migrations"#)
            .fetch_one(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&read_err), "42501");

        let delete_err = sqlx::query!("DELETE FROM _sqlx_migrations")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&delete_err), "42501");
    })
    .await;
}

/// T6, findings #24 and #5: `cadus_admin` writes `model_call_log`, and the
/// runtime role reaches it with no statement at all.
///
/// Finding #24: `model_call_log.id` is the only bigserial column of the schema,
/// so the insert needs `USAGE` on `model_call_log_id_seq`. `ALTER DEFAULT
/// PRIVILEGES` does not cover that sequence: 0005 creates it before 0006 sets
/// the defaults. The insert here runs under `SET ROLE cadus_admin`, so the
/// sequence grant of that role is still under test.
///
/// Finding #5: the table carries `user_id`, `session_id`, token counts, and
/// `cost_usd`, and it stays outside row-level security, so a table-wide grant
/// gave one tenant every tenant's rows and a one-statement wipe of the T6
/// ledger. T2 names the worker as the only unit that spends tokens, and the
/// worker connects as `cadus_admin`.
#[tokio::test]
async fn admin_role_inserts_into_model_call_log() {
    TestDb::with(|db| async move {
        // One fixed connection: SET ROLE outlives a statement, so the test
        // returns the connection to the pool with RESET ROLE.
        let mut conn = db.admin.acquire().await.unwrap();
        sqlx::query("SET ROLE cadus_admin")
            .execute(&mut *conn)
            .await
            .unwrap();

        let id = sqlx::query_scalar!(
            "INSERT INTO model_call_log (purpose, model_id, latency_ms)
             VALUES ('test', 'none', 1)
             RETURNING id"
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        assert_eq!(id, 1);

        sqlx::query("RESET ROLE").execute(&mut *conn).await.unwrap();
        drop(conn);

        // #5: the runtime role writes no row and reads no row.
        let insert_err = sqlx::query!(
            "INSERT INTO model_call_log (purpose, model_id, latency_ms)
             VALUES ('test', 'none', 1)"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&insert_err), "42501");

        let select_err = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM model_call_log"#)
            .fetch_one(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&select_err), "42501");
    })
    .await;
}

/// C6, finding #14: the runtime role reads `content_store` and never writes it.
///
/// `content_store` binds approval to the digest (`migrations/0005_content.sql`),
/// so an edited body is a new row that needs its own approval. Table-wide
/// INSERT, UPDATE, and DELETE let the request tier rewrite an approved body in
/// place and insert a row that already carried `status = 'approved'`.
#[tokio::test]
async fn app_role_reads_content_store_and_never_writes_it() {
    TestDb::with(|db| async move {
        sqlx::query!(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ('sha256:seed', 'kp.x', 'template',
                     '{\"statement\": \"reviewed\"}'::jsonb, 'approved')"
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // The serve path reads approved content.
        let seen = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM content_store"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(seen, 1);

        let update_err = sqlx::query!("UPDATE content_store SET body = '{}'::jsonb")
            .execute(&db.app)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&update_err), "42501");

        let insert_err = sqlx::query!(
            "INSERT INTO content_store (digest, kp_id, kind, body, status)
             VALUES ('sha256:new', 'kp.x', 'template', '{}'::jsonb, 'approved')"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&insert_err), "42501");

        // The approved body is still the one the admin seeded.
        let statement = sqlx::query_scalar!(
            r#"SELECT body ->> 'statement' AS "statement!"
               FROM content_store WHERE digest = 'sha256:seed'"#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(statement, "reviewed");
    })
    .await;
}

/// C3, finding #4: the runtime role updates its own `users` row and nothing else.
///
/// `users` is keyed by `id`, so it stays outside the `tenant_isolation` set and
/// carries three per-command policies instead. Table-wide UPDATE let a session
/// bound to tenant A rewrite tenant B's `password_hash`. A column list keeps
/// `is_admin` out of reach of the runtime role in every case.
#[tokio::test]
async fn app_role_updates_only_its_own_user_row() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("self-a@example.test").await;
        let user_b = db.seed_user("self-b@example.test").await;

        // Both catalog flags are on, so a non-superuser owner stays inside the
        // policies too.
        let flags = sqlx::query!(
            r#"
            SELECT c.relrowsecurity      AS "rls_enabled!",
                   c.relforcerowsecurity AS "rls_forced!"
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relname = 'users'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(flags.rls_enabled, "relrowsecurity must be true on users");
        assert!(
            flags.rls_forced,
            "relforcerowsecurity must be true on users"
        );

        // Bound to A, an UPDATE of B's row matches no row.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let cross_tenant = sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(cross_tenant, 0);
        tx.commit().await.unwrap();

        // B is untouched.
        let b_unverified = sqlx::query_scalar!(
            r#"SELECT (email_verified_at IS NULL) AS "unverified!" FROM users WHERE id = $1"#,
            user_b
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(b_unverified, "tenant B keeps email_verified_at NULL");

        // Bound to A, an UPDATE of A's own row matches exactly one row.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let own_row = sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user_a
        )
        .execute(&mut *tx)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(own_row, 1);
        tx.commit().await.unwrap();

        // #4: the column list stops the admin flag before any policy runs.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let admin_err = sqlx::query!("UPDATE users SET is_admin = true WHERE id = $1", user_a)
            .execute(&mut *tx)
            .await
            .unwrap_err();
        assert_eq!(sqlstate(&admin_err), "42501");
        let _ = tx.rollback().await;
    })
    .await;
}

/// C3, findings #4 and #5: the runtime role inserts a `users` row and decides
/// neither `id` nor `is_admin`.
///
/// The round-2 fix narrowed UPDATE only. INSERT stayed table-wide over every
/// column, so one sign-up statement minted an account with `is_admin = true` and
/// a chosen primary key. A column list on INSERT closes that door.
#[tokio::test]
async fn app_role_inserts_no_id_and_no_admin_flag() {
    TestDb::with(|db| async move {
        // #5: an INSERT that names is_admin stops at the grant, before any policy.
        let admin_err = sqlx::query!(
            "INSERT INTO users (email, password_hash, is_admin)
             VALUES ($1::text::citext, $2, true)",
            "mint-admin@example.test",
            "MINT-HASH"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&admin_err), "42501");

        // #4: an INSERT that names id stops at the grant too.
        let chosen = Uuid::parse_str("00000000-0000-0000-0000-0000000000ff").unwrap();
        let id_err = sqlx::query!(
            "INSERT INTO users (id, email, password_hash) VALUES ($1, $2::text::citext, $3)",
            chosen,
            "chosen-id@example.test",
            "CHOSEN-HASH"
        )
        .execute(&db.app)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&id_err), "42501");

        // The sign-up shape succeeds. `users_insert` allows the row, and the
        // grant covers both named columns.
        let inserted = sqlx::query!(
            "INSERT INTO users (email, password_hash) VALUES ($1::text::citext, $2)",
            "signup@example.test",
            "SIGNUP-HASH"
        )
        .execute(&db.app)
        .await
        .unwrap()
        .rows_affected();
        assert_eq!(inserted, 1);

        // Both withheld columns come from their defaults.
        let row = sqlx::query!(
            r#"
            SELECT id AS "id!", is_admin AS "is_admin!"
            FROM users WHERE email = $1::text::citext
            "#,
            "signup@example.test"
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!row.is_admin, "a sign-up row carries is_admin = false");
        assert_ne!(row.id, chosen);

        // Neither denied row exists.
        let denied = sqlx::query_scalar!(
            r#"
            SELECT count(*) AS "count!"
            FROM users WHERE email IN ($1::text::citext, $2::text::citext)
            "#,
            "mint-admin@example.test",
            "chosen-id@example.test"
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(denied, 0);

        // The column ACL is the reason, and it is pinned here.
        let columns = sqlx::query!(
            r#"
            SELECT has_column_privilege('cadus_app', 'users', 'is_admin', 'INSERT')
                       AS "may_insert_is_admin!",
                   has_column_privilege('cadus_app', 'users', 'id', 'INSERT')
                       AS "may_insert_id!",
                   has_column_privilege('cadus_app', 'users', 'email', 'INSERT')
                       AS "may_insert_email!"
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(
            !columns.may_insert_is_admin,
            "cadus_app must not insert users.is_admin"
        );
        assert!(!columns.may_insert_id, "cadus_app must not insert users.id");
        assert!(
            columns.may_insert_email,
            "cadus_app must insert users.email"
        );
    })
    .await;
}

/// C3, finding #11: the runtime role reads its own `users` row and no other, and
/// the login path goes through the two SECURITY DEFINER functions.
///
/// `users_read` was `USING (true)` over a table-wide SELECT grant, so a bound
/// tenant read every account's `email`, `password_hash`, `is_admin`, and
/// `disabled_at`. The SELECT policy now carries the same `id` predicate as the
/// UPDATE policy. `auth_user_by_email` and `auth_user_by_id` serve the one read
/// that happens before a tenant is bound.
#[tokio::test]
async fn app_role_reads_only_its_own_user_row() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("read-a@example.test").await;
        let user_b = db.seed_user("read-b@example.test").await;
        sqlx::query!(
            "UPDATE users SET password_hash = $1 WHERE id = $2",
            "VICTIM-HASH",
            user_b
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // Bound to A, exactly one row of users is visible, and it is A's row.
        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let bound_rows = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bound_rows, 1);
        let bound_id = sqlx::query_scalar!(r#"SELECT id AS "id!" FROM users"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bound_id, user_a);
        tx.commit().await.unwrap();

        // Unbound, no row of users is visible at all.
        let unbound_rows = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM users"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(unbound_rows, 0);

        // A plain SELECT reaches no other account's password hash.
        let hashes = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM users WHERE password_hash = $1"#,
            "VICTIM-HASH"
        )
        .fetch_one(&db.app)
        .await
        .unwrap();
        assert_eq!(hashes, 0);

        // The password-login lookup by email reaches B and returns B's id.
        let by_email = sqlx::query_scalar!(
            r#"SELECT id AS "id!" FROM auth_user_by_email($1::text::citext)"#,
            "read-b@example.test"
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(by_email, vec![user_b]);

        // The session-cookie lookup by id reaches B and returns one row with the
        // columns that an account-status decision needs.
        let by_id = sqlx::query!(
            r#"
            SELECT id AS "id!", password_hash AS "password_hash?", is_admin AS "is_admin!"
            FROM auth_user_by_id($1)
            "#,
            user_b
        )
        .fetch_all(&db.app)
        .await
        .unwrap();
        assert_eq!(by_id.len(), 1);
        assert_eq!(by_id[0].id, user_b);
        assert_eq!(by_id[0].password_hash.as_deref(), Some("VICTIM-HASH"));
        assert!(!by_id[0].is_admin);

        // The two functions are SECURITY DEFINER with a pinned search_path, and
        // EXECUTE belongs to the two login roles, not to PUBLIC.
        let mut definitions = sqlx::query!(
            r#"
            SELECT p.proname::text     AS "name!",
                   p.prosecdef         AS "security_definer!",
                   p.proconfig::text   AS "config?",
                   has_function_privilege('cadus_app', p.oid, 'EXECUTE')   AS "app_execute!",
                   has_function_privilege('cadus_admin', p.oid, 'EXECUTE') AS "admin_execute!",
                   has_function_privilege('public', p.oid, 'EXECUTE')      AS "public_execute!"
            FROM pg_proc p
            JOIN pg_namespace n ON n.oid = p.pronamespace
            WHERE n.nspname = 'public' AND p.proname LIKE 'auth\_user\_by\_%'
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap()
        .iter()
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
        .collect::<Vec<_>>();
        definitions.sort();
        assert_eq!(
            definitions,
            vec![
                (
                    "auth_user_by_email".to_string(),
                    true,
                    Some("{search_path=public}".to_string()),
                    true,
                    true,
                    false,
                ),
                (
                    "auth_user_by_id".to_string(),
                    true,
                    Some("{search_path=public}".to_string()),
                    true,
                    true,
                    false,
                ),
            ]
        );
    })
    .await;
}

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

/// D9, finding #20: `ALTER DEFAULT PRIVILEGES` holds the literal grant surface.
///
/// The two statements exist so that a table or a sequence of a later migration
/// is grantable without a manual GRANT. The entries carry the grantor after a
/// slash, so the test compares the part before it.
#[tokio::test]
async fn default_privileges_are_the_literal_grants() {
    TestDb::with(|db| async move {
        let rows = sqlx::query!(
            r#"
            SELECT d.defaclobjtype::text        AS "obj_type!",
                   split_part(entry::text, '/', 1) AS "acl_entry!"
            FROM pg_default_acl d
            JOIN pg_namespace n ON n.oid = d.defaclnamespace
            CROSS JOIN LATERAL unnest(d.defaclacl) AS entry
            WHERE n.nspname = 'public'
            "#
        )
        .fetch_all(&db.admin)
        .await
        .unwrap();

        // Sort in Rust. A SQL `ORDER BY` on text follows the database collation, and
        // the case order of 'S' against 'r' differs between collations.
        let mut found: Vec<(String, String)> = rows
            .iter()
            .map(|row| (row.obj_type.clone(), row.acl_entry.clone()))
            .collect();
        found.sort();
        let expected: Vec<(String, String)> = vec![
            ("S".to_string(), "cadus_admin=rU".to_string()),
            ("S".to_string(), "cadus_app=rU".to_string()),
            ("r".to_string(), "cadus_admin=arwd".to_string()),
            ("r".to_string(), "cadus_app=arwd".to_string()),
        ];
        assert_eq!(found, expected);

        // The two object types are exactly tables ('r') and sequences ('S').
        let mut obj_types: Vec<String> = rows.iter().map(|row| row.obj_type.clone()).collect();
        obj_types.sort();
        obj_types.dedup();
        assert_eq!(obj_types, vec!["S".to_string(), "r".to_string()]);
    })
    .await;
}

/// C3: a tenant reads only its own rows, an unbound connection reads nothing,
/// and a write into another tenant fails.
#[tokio::test]
async fn rls_isolates_tenants() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("tenant-a@example.test").await;
        let user_b = db.seed_user("tenant-b@example.test").await;

        for user in [user_a, user_b] {
            sqlx::query!(
                "INSERT INTO events (user_id, seq, ts, type, payload)
                 VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
                user
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let bound_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bound_count, 1);
        let _ = tx.rollback().await;

        // No tenant context: the policy resolves to NULL and the query returns
        // nothing. The failure mode is closed.
        let unbound_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(unbound_count, 0);

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let cross_tenant_err = sqlx::query!(
            "INSERT INTO learner_models
                 (user_id, model, through_seq, projector_version, config_hash)
             VALUES ($1, '{}'::jsonb, 0, 1, 'test')",
            user_b
        )
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert_eq!(sqlstate(&cross_tenant_err), "42501");
        let _ = tx.rollback().await;
    })
    .await;
}

/// C3, finding #15: the policy also covers the two worker queues.
///
/// One tenant reads its own `diagnosis_jobs` and `email_outbox` rows and nothing
/// of the other tenant. The worker role keeps the cross-tenant view, because it
/// holds BYPASSRLS.
#[tokio::test]
async fn rls_covers_the_worker_queues() {
    TestDb::with(|db| async move {
        let user_a = db.seed_user("queue-a@example.test").await;
        let user_b = db.seed_user("queue-b@example.test").await;

        for user in [user_a, user_b] {
            sqlx::query!(
                "INSERT INTO diagnosis_jobs (user_id, attempt_id, payload)
                 VALUES ($1, 'attempt-1', '{\"secret\": \"answer\"}'::jsonb)",
                user
            )
            .execute(&db.admin)
            .await
            .unwrap();

            sqlx::query!(
                "INSERT INTO email_outbox (user_id, to_addr, kind)
                 VALUES ($1, 'someone@example.test', 'verify')",
                user
            )
            .execute(&db.admin)
            .await
            .unwrap();
        }

        let mut tx = begin_tenant(&db.app, user_a).await.unwrap();
        let jobs = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM diagnosis_jobs"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(jobs, 1);
        let mail = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM email_outbox"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(mail, 1);
        let _ = tx.rollback().await;

        // No tenant context: both queues read nothing.
        let unbound_jobs =
            sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM diagnosis_jobs"#)
                .fetch_one(&db.app)
                .await
                .unwrap();
        assert_eq!(unbound_jobs, 0);
        let unbound_mail = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM email_outbox"#)
            .fetch_one(&db.app)
            .await
            .unwrap();
        assert_eq!(unbound_mail, 0);
    })
    .await;
}

/// C3 boot guard: a role that bypasses row-level security is rejected.
#[tokio::test]
async fn boot_guard() {
    TestDb::with(|db| async move {
        let err = assert_rls_enforced(&db.admin).await.unwrap_err();
        let StoreError::RlsBypass { superuser, .. } = err else {
            panic!("the admin pool must be rejected with StoreError::RlsBypass");
        };
        assert!(superuser, "the test cluster admin is a superuser");

        let info = assert_rls_enforced(&db.app).await.unwrap();
        assert_eq!(info.name, "cadus_app");
        assert!(!info.superuser);
        assert!(!info.bypass_rls);
    })
    .await;
}

/// C3, finding #6: the role attributes match `migrations/0001_roles.sql`.
///
/// The roles are cluster-scoped and each `CREATE ROLE` is guarded by a `pg_roles`
/// test, so a long-lived test cluster keeps the attributes it was built with. A
/// catalog assertion alone therefore proves nothing about the migration text.
/// This test reads both: the live attributes and the literal line of the file.
#[tokio::test]
async fn role_attributes_match_the_migration() {
    TestDb::with(|db| async move {
        let app = sqlx::query!(
            r#"
            SELECT rolsuper      AS "superuser!",
                   rolbypassrls  AS "bypass_rls!",
                   rolcanlogin   AS "can_login!",
                   rolcreatedb   AS "create_db!",
                   rolcreaterole AS "create_role!"
            FROM pg_roles WHERE rolname = 'cadus_app'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!app.superuser, "cadus_app must not be a superuser");
        assert!(
            !app.bypass_rls,
            "cadus_app must not bypass row-level security"
        );
        assert!(app.can_login, "cadus_app must log in");
        assert!(!app.create_db, "cadus_app must not create a database");
        assert!(!app.create_role, "cadus_app must not create a role");

        // cadus_admin holds BYPASSRLS for the cross-tenant sweep. Its LOGIN flag
        // stays out of this test: another suite toggles it.
        let admin = sqlx::query!(
            r#"
            SELECT rolsuper     AS "superuser!",
                   rolbypassrls AS "bypass_rls!"
            FROM pg_roles WHERE rolname = 'cadus_admin'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!admin.superuser, "cadus_admin must not be a superuser");
        assert!(
            admin.bypass_rls,
            "cadus_admin must bypass row-level security"
        );

        let owner = sqlx::query!(
            r#"
            SELECT rolsuper    AS "superuser!",
                   rolcanlogin AS "can_login!"
            FROM pg_roles WHERE rolname = 'cadus_owner'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(!owner.superuser, "cadus_owner must not be a superuser");
        assert!(!owner.can_login, "cadus_owner must not log in");

        // The migration text itself. `CARGO_MANIFEST_DIR` is `crates/store`.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../migrations/0001_roles.sql");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read of {} failed: {e}", path.display()));
        assert!(
            text.contains(APP_ROLE_LINE),
            "migrations/0001_roles.sql must contain the line: {APP_ROLE_LINE}"
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
                    "tenant_isolation".to_string(),
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

        assert_eq!(found.len(), 15);
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

/// C2, finding #41: `events_attempt_idem` is a UNIQUE index.
///
/// FR-14 idempotency lives in the database: a repeated `attempt_id` makes the
/// INSERT a no-op under `ON CONFLICT ... DO NOTHING`. A plain index makes the
/// targeted `ON CONFLICT` raise SQLSTATE `42P10` instead.
#[tokio::test]
async fn events_attempt_idem_is_unique() {
    TestDb::with(|db| async move {
        let user = db.seed_user("attempt-idem@example.test").await;

        let is_unique = sqlx::query_scalar!(
            r#"
            SELECT i.indisunique AS "is_unique!"
            FROM pg_index i
            JOIN pg_class c ON c.oid = i.indexrelid
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'public' AND c.relname = 'events_attempt_idem'
            "#
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert!(is_unique, "events_attempt_idem must be a UNIQUE index");

        let mut tx = begin_tenant(&db.app, user).await.unwrap();
        for seq in [1_i64, 2_i64] {
            sqlx::query!(
                "INSERT INTO events (user_id, seq, ts, type, attempt_id, payload)
                 VALUES ($1, $2, now(), 'attempt', 'attempt-1', '{}'::jsonb)
                 ON CONFLICT (user_id, attempt_id) WHERE attempt_id IS NOT NULL DO NOTHING",
                user,
                seq
            )
            .execute(&mut *tx)
            .await
            .unwrap();
        }

        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let _ = tx.rollback().await;
    })
    .await;
}

/// D9: a second migration run applies nothing and leaves the six rows.
#[tokio::test]
async fn migrate_is_idempotent() {
    TestDb::with(|db| async move {
        cadus_store::migrate(&db.admin).await.unwrap();

        let count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM _sqlx_migrations"#)
            .fetch_one(&db.admin)
            .await
            .unwrap();
        assert_eq!(count, 6);
    })
    .await;
}

/// C3: a reset tenant context yields zero rows and raises no error.
///
/// The `nullif(..., '')` guard in the policy is load-bearing. `RESET` leaves the
/// empty string in the GUC, not NULL, and a raw `''::uuid` cast raises SQLSTATE
/// `22P02`. That error breaks a pooled connection that a unit of work released.
/// The guard maps `''` to NULL, so a cleared context reads nothing.
#[tokio::test]
async fn reset_guc_yields_zero_rows() {
    TestDb::with(|db| async move {
        let user = db.seed_user("reset-guc@example.test").await;

        sqlx::query!(
            "INSERT INTO events (user_id, seq, ts, type, payload)
             VALUES ($1, 1, now(), 'attempt', '{}'::jsonb)",
            user
        )
        .execute(&db.admin)
        .await
        .unwrap();

        // One connection for the whole test. A session-level set_config outlives a
        // transaction, so the reset case needs the same connection throughout.
        let mut conn = db.app.acquire().await.unwrap();

        sqlx::query!(
            "SELECT set_config('app.user_id', $1, false)",
            user.to_string()
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();

        let bound_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(bound_count, 1);

        sqlx::query("RESET app.user_id")
            .execute(&mut *conn)
            .await
            .unwrap();

        let reset_count = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM events"#)
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(reset_count, 0);

        drop(conn);
    })
    .await;
}
