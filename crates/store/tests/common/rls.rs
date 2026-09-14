//! The literal tables of the row-level-security proofs: every list a test of
//! `rls_*.rs` compares the catalog against, and the two helpers those tests
//! share.

use cadus_store::test_support::TestDb;
use uuid::Uuid;

/// The 17 tables that carry a tenant policy (`docs/SCHEMA.md`, C3).
///
/// `diagnosis_jobs` and `email_outbox` joined the list with round-3 finding #15.
/// The worker reads both as `cadus_admin`, which holds BYPASSRLS, so the old
/// exemption bought nothing and gave `cadus_app` every tenant's payload.
///
/// `auth_sessions`, `auth_tokens`, and `oauth_accounts` joined the list with
/// round-4 finding #1. All three carried `user_id` and full DML for `cadus_app`
/// with no policy, so one INSERT into `auth_sessions` minted a live cookie for
/// any account and opened every other tenant table behind it. The pre-tenant
/// reads go through the SECURITY DEFINER functions of `0006_grants_rls.sql`.
///
/// The order is the `C` collation order of `pg_class.relname`, because the
/// catalog queries below order by that column.
pub const RLS_TABLES: [&str; 18] = [
    "anki_cards_created",
    "anki_queue",
    "auth_sessions",
    "auth_tokens",
    "diag_states",
    "diagnosis_jobs",
    "email_outbox",
    "events",
    "exposure_history_progress",
    "learner_models",
    "oauth_accounts",
    "problem_report_diagnostics",
    "problem_reports",
    "profiles",
    "serving_pool",
    "session_plans",
    "user_settings",
    "web_states",
];

/// The 1 table that carries a `user_id` and stays outside row-level security.
pub const EXEMPT_TABLES: [&str; 1] = ["model_call_log"];

/// The union of the two lists above: every `public` table with a `user_id`
/// column. The literal union pins that no table sits outside both buckets
/// (finding #23).
pub const ALL_USER_ID_TABLES: [&str; 19] = [
    "anki_cards_created",
    "anki_queue",
    "auth_sessions",
    "auth_tokens",
    "diag_states",
    "diagnosis_jobs",
    "email_outbox",
    "events",
    "exposure_history_progress",
    "learner_models",
    "model_call_log",
    "oauth_accounts",
    "problem_report_diagnostics",
    "problem_reports",
    "profiles",
    "serving_pool",
    "session_plans",
    "user_settings",
    "web_states",
];

/// The literal text of the `tenant_isolation` predicate, as Postgres prints it
/// from the catalog. Both `USING` and `WITH CHECK` carry this text on all 17
/// policies. The literal pins the `nullif` guard and the `true` missing-ok flag,
/// so an edit of `migrations/0006_grants_rls.sql` cannot pass in silence (C3).
pub const POLICY_PREDICATE: &str =
    "(user_id = (NULLIF(current_setting('app.user_id'::text, true), ''::text))::uuid)";

/// The literal text of the `users_read_self` and `users_update_self` predicate
/// (findings #4 and #11).
///
/// `users` is keyed by `id`, not by `user_id`, so it stays outside the
/// `tenant_isolation` set and carries its own per-command policies. The
/// predicate is otherwise the same shape, `nullif` guard included. Finding #11
/// put the SELECT policy on this predicate too, so a bound tenant reads its own
/// row and no other.
pub const USERS_SELF_PREDICATE: &str =
    "(id = (NULLIF(current_setting('app.user_id'::text, true), ''::text))::uuid)";

/// Finding #12: the live sequence privileges of `cadus_app`, sequence by
/// sequence.
///
/// The array is `(sequence_name, [USAGE, SELECT, UPDATE])`, in the byte order of
/// the sequence name. Every sequence of schema `public` is here. The blanket
/// grant of `0006_grants_rls.sql` covered `model_call_log_id_seq`, so the
/// runtime role read `last_value` and moved the ledger key with `nextval`, and
/// no test saw it: the table matrix reads relations, not sequences.
pub const APP_SEQUENCE_PRIVILEGES: [(&str, [bool; 3]); 1] = [
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
pub const APP_TABLE_PRIVILEGES: [(&str, [bool; 5]); 27] = [
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
    (
        "exposure_history_progress",
        [true, true, false, false, false],
    ),
    (
        "finite_exposure_aliases",
        [true, false, false, false, false],
    ),
    (
        "finite_exposure_contexts",
        [true, false, false, false, false],
    ),
    ("learner_models", [true, true, true, true, false]),
    // #5: only the worker spends tokens, and the worker is cadus_admin.
    ("model_call_log", [false, false, false, false, false]),
    ("oauth_accounts", [true, true, true, true, false]),
    // Reports permit only the seven input-column INSERT grants listed below.
    ("problem_corrections", [true, false, false, false, false]),
    ("problem_report_diagnostics", [true, true, true, false, false]),
    ("problem_report_steps", [false, false, false, false, false]),
    ("problem_reports", [true, false, false, false, false]),
    ("profiles", [true, true, true, true, false]),
    ("serving_pool", [true, true, true, true, false]),
    ("session_plans", [true, true, true, true, false]),
    ("user_settings", [true, true, true, true, false]),
    // #2, #4, and #5: no DELETE, no TRUNCATE, and INSERT and UPDATE are
    // column-level only.
    ("users", [true, false, false, false, false]),
    ("web_states", [true, true, true, true, false]),
];

/// Round-4 finding #7: every function of schema `public` that a migration
/// creates, as `(name, prosecdef, proconfig, cadus_app EXECUTE, cadus_admin
/// EXECUTE, PUBLIC EXECUTE)`.
///
/// A SECURITY DEFINER function runs with the rights of its owner, and the owner
/// is the migration runner, a superuser in the shipped stack. Such a function
/// reads every tenant's rows outside the policies and outside the append-only
/// revoke, and `EXECUTE` on a new function goes to PUBLIC by default. The old
/// suite pinned two functions by a name prefix, so a new one was invisible.
///
/// Round-4 finding #4: `proconfig` carries `pg_temp` in every entry. Postgres
/// searches the temporary schema BEFORE every schema that `search_path` names
/// whenever `pg_temp` is not written out, so `SET search_path = public` let a
/// caller point the body at its own `pg_temp.users`. Naming `pg_temp` last puts
/// the temporary schema after `public`.
///
/// The list holds no function of an extension.
/// `public_functions_are_the_literal_list` pins those separately.
pub const PUBLIC_FUNCTIONS: [(&str, bool, &str, bool, bool, bool); 11] = [
    // #1: the session cookie, read before the tenant bind.
    (
        "auth_session_by_token_hash",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
    // #1: the password-reset and email-verification token.
    (
        "auth_token_by_hash",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
    // #11: the password login and the OAuth link by email.
    (
        "auth_user_by_email",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
    // #11: the account status behind a session cookie.
    (
        "auth_user_by_id",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
    // Instruction context helpers preserve the caller's table privileges.
    ("cadus_template_bank", false, "", true, true, false),
    ("cadus_template_context", false, "", true, true, false),
    // migration 0019: the template bank context after a new candidate is
    // inserted, for re-gating the approval before the insert commits.
    (
        "cadus_template_context_after",
        false,
        r#""#,
        true,
        true,
        false,
    ),
    // M5 U1, migration 0007: the worker-liveness read of `/api/ready`
    // (D-M5-6). `diagnosis_jobs` carries a FORCEd tenant policy and the
    // readiness probe runs unbound, so a plain SELECT reads zero rows on every
    // deployment. This function is the ONE read that crosses that policy, and
    // it gives back one aggregate number and no tenant row: no id, no attempt,
    // no payload, no prose.
    (
        "diagnosis_claim_age_secs",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
    // M5 U11, migration 0009: the `cadus_diagnosis_jobs_total` counts of
    // `/metrics` (T6). `/metrics` runs unbound on a `cadus_app` connection and
    // `diagnosis_jobs` carries a FORCEd tenant policy, so a plain SELECT reads
    // zero rows. This function gives back one line per job STATUS and no row of
    // the queue: no id, no attempt, no payload, no prose.
    (
        "diagnosis_job_totals",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
    // M5 U11, migration 0009: the `cadus_model_call_tokens_total` and
    // `cadus_model_call_latency_seconds` totals of `/metrics` (T6). Findings #5
    // and #12 leave `cadus_app` with no privilege on `model_call_log` and none
    // on its sequence, and that stays true: what this function returns is one
    // line per `purpose` with token sums, a latency sum and a call count. The
    // money column and every per-learner column stay inside the ledger, which
    // only `cadus_admin` reads.
    (
        "model_call_totals",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
    // #1: the OAuth callback.
    (
        "oauth_account_lookup",
        true,
        r#"{"search_path=public, pg_temp"}"#,
        true,
        true,
        false,
    ),
];

/// Round-4 finding #5: every column-level ACL of schema `public`, as
/// `(table, column, acl entry without the grantor)`.
///
/// `has_table_privilege` reports a column grant as `false`, so the privilege
/// matrix is blind to one. `GRANT UPDATE (payload) ON events TO cadus_app`
/// therefore rewrote the authoritative event document with every C2 test green.
/// `aw` is INSERT plus UPDATE: the two column lists of `users` in
/// `0006_grants_rls.sql`. Neither list holds `id` or `is_admin`.
pub const COLUMN_ACL_GRANTS: [(&str, &str, &str); 15] = [
    // migration 0020: the exposure backfill can write the legacy cursor.
    ("exposure_history_progress", "target_seq", "cadus_app=w"),
    ("exposure_history_progress", "through_seq", "cadus_app=w"),
    ("exposure_history_progress", "updated_at", "cadus_app=w"),
    ("problem_reports", "attempt_id", "cadus_app=a"),
    ("problem_reports", "input", "cadus_app=a"),
    ("problem_reports", "problem_id", "cadus_app=a"),
    ("problem_reports", "request_id", "cadus_app=a"),
    ("problem_reports", "source_hash", "cadus_app=a"),
    ("problem_reports", "task_id", "cadus_app=a"),
    ("problem_reports", "user_id", "cadus_app=a"),
    ("users", "created_at", "cadus_app=aw"),
    ("users", "disabled_at", "cadus_app=aw"),
    ("users", "email", "cadus_app=aw"),
    ("users", "email_verified_at", "cadus_app=aw"),
    ("users", "password_hash", "cadus_app=aw"),
];

/// Round-4 finding #8: every foreign key of schema `public`, as
/// `(table, constraint, confdeltype)`.
///
/// `confdeltype` is the `ON DELETE` action: `c` is CASCADE, `r` is RESTRICT,
/// `n` is SET NULL, and `a` is NO ACTION. `events` must stay `r`: C2 says the
/// event log outlives the account, and a flip to CASCADE erases a learner's
/// whole history on one `DELETE FROM users` with the store suite green.
pub const FOREIGN_KEY_DELETE_ACTIONS: [(&str, &str, &str); 24] = [
    ("anki_cards_created", "anki_cards_created_user_id_fkey", "c"),
    ("anki_queue", "anki_queue_user_id_fkey", "c"),
    ("auth_sessions", "auth_sessions_user_id_fkey", "c"),
    ("auth_tokens", "auth_tokens_user_id_fkey", "c"),
    ("content_store", "content_store_approved_by_fkey", "n"),
    ("diag_states", "diag_states_user_id_fkey", "c"),
    ("diagnosis_jobs", "diagnosis_jobs_user_id_fkey", "c"),
    ("email_outbox", "email_outbox_user_id_fkey", "n"),
    // C2: the log outlives the account.
    ("events", "events_attempt_handoff_fk", "a"),
    ("events", "events_user_id_fkey", "r"),
    (
        "exposure_history_progress",
        "exposure_history_progress_user_id_fkey",
        "c",
    ),
    ("learner_models", "learner_models_user_id_fkey", "c"),
    ("model_call_log", "model_call_log_user_id_fkey", "n"),
    ("oauth_accounts", "oauth_accounts_user_id_fkey", "c"),
    (
        "problem_corrections",
        "problem_corrections_report_id_fkey",
        "a",
    ),
    ("problem_report_diagnostics", "problem_report_diagnostics_user_id_fkey", "c"),
    (
        "problem_report_steps",
        "problem_report_steps_report_id_fkey",
        "c",
    ),
    ("problem_reports", "problem_reports_user_id_fkey", "c"),
    ("profiles", "profiles_user_id_fkey", "c"),
    ("serving_pool", "serving_pool_content_digest_fkey", "a"),
    ("serving_pool", "serving_pool_user_id_fkey", "c"),
    ("session_plans", "session_plans_user_id_fkey", "c"),
    ("user_settings", "user_settings_user_id_fkey", "c"),
    ("web_states", "web_states_user_id_fkey", "c"),
];

/// The line of `migrations/0001_roles.sql` that gives `cadus_app` its
/// attributes. Finding #6: the roles are cluster-scoped and `CREATE ROLE` is
/// guarded, so a long-lived cluster keeps the old attributes and the catalog
/// alone proves nothing about the migration text.
pub const APP_ROLE_LINE: &str =
    "CREATE ROLE cadus_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;";

/// One row of `pg_policy`: table, policy name, command, USING, WITH CHECK.
pub type PolicyRow = (String, String, String, Option<String>, Option<String>);

pub fn to_owned(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

/// Seed the three auth rows of `user`: one live session with `session_hash`,
/// one reset token with `token_hash`, and one linked Google account under
/// `email`, all with the admin pool.
pub async fn seed_auth_rows(
    db: &TestDb,
    user: Uuid,
    session_hash: &str,
    token_hash: &str,
    email: &str,
) {
    sqlx::query(
        "INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at, expires_at) \
         VALUES ($1, $2, now(), now(), now() + interval '1 day')",
    )
    .bind(session_hash)
    .bind(user)
    .execute(&db.admin)
    .await
    .expect("the session row inserts");
    sqlx::query(
        "INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at) \
         VALUES ($1, $2, 'password_reset', now() + interval '1 day')",
    )
    .bind(token_hash)
    .bind(user)
    .execute(&db.admin)
    .await
    .expect("the token row inserts");
    sqlx::query(
        "INSERT INTO oauth_accounts (provider, provider_account_id, user_id, email_at_link) \
         VALUES ('google', 'provider-id-of-b', $1, $2)",
    )
    .bind(user)
    .bind(email)
    .execute(&db.admin)
    .await
    .expect("the provider link inserts");
}

/// The triples `pick` reads out of `rows`, sorted in Rust: a SQL `ORDER BY`
/// on text follows the database collation.
pub fn triples<T>(
    rows: &[T],
    pick: impl Fn(&T) -> (String, String, String),
) -> Vec<(String, String, String)> {
    let mut found: Vec<(String, String, String)> = rows.iter().map(pick).collect();
    found.sort();
    found
}
