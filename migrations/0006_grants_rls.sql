-- 0006_grants_rls: role grants, default privileges, the append-only revoke, the
-- privilege revokes on users and on the migration ledger, and row-level security.
-- Requirements: C2 (append-only events at the grant level), C3 (RLS tenant
-- isolation), D9 (schema lineage).
--
-- RLS-scoped tables (literal list, docs/SCHEMA.md):
--   events, learner_models, profiles, session_plans, diag_states, user_settings,
--   web_states, anki_queue, anki_cards_created, serving_pool, diagnosis_jobs,
--   email_outbox.
--
-- Exempt tables that carry a user_id (literal list with the reason for each):
--   auth_sessions   -- looked up before a tenant context exists
--   oauth_accounts  -- looked up before a tenant context exists
--   auth_tokens     -- looked up before a tenant context exists
--   model_call_log  -- finding #5: cadus_app holds no privilege on it
--
-- Finding #15: diagnosis_jobs and email_outbox left the exempt list. The old
-- reason was "the worker claims (or drains) across tenants". The worker connects
-- as cadus_admin, which holds BYPASSRLS, so the cross-tenant scan never depended
-- on the missing policy. The exemption only gave cadus_app every tenant's
-- attempt payload, diagnosis result, and email address.
--
-- Tables with no user_id stay outside the user_id-keyed derivation above:
--   users               -- finding #4: the key is id, not user_id. users carries
--                          its own per-command policies (users_read_self,
--                          users_insert, users_update_self) further down.
--   auth_rate_counters  -- fixed-window counters; the table has no tenant column.
--   content_store       -- curriculum content, not learner data. Finding #14
--                          takes INSERT, UPDATE, and DELETE away from cadus_app.
--
-- The list is literal on purpose. A migration describes the schema as of its own
-- revision. A later table gets its own ENABLE, FORCE, and policy in a later
-- migration.

-- --------------------------------------------------------------------------
-- Grants
-- --------------------------------------------------------------------------
GRANT USAGE ON SCHEMA public TO cadus_app, cadus_admin;

GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cadus_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cadus_admin;

-- bigserial columns need the sequence too (model_call_log.id). Finding #5: only
-- cadus_admin writes that table now, so the grant matters for that role.
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO cadus_app, cadus_admin;

-- Default privileges: a table or sequence that a LATER migration creates becomes
-- grantable without a manual GRANT. Without this, a new table is DML-denied for
-- cadus_app until someone notices at runtime. The default applies to objects that
-- the migration-running role creates. RLS on a new tenant table still needs its
-- own ENABLE, FORCE, and policy in that migration.
--
-- Finding #6: the TABLES object type of ALTER DEFAULT PRIVILEGES also covers a
-- view and a materialized view. A view runs with the rights of its owner, and
-- the owner is the migration runner, a superuser in the shipped compose stack.
-- Such a view bypasses row-level security and the append-only revoke on events,
-- so an auto-updatable view over events gives cadus_app the UPDATE and the
-- DELETE that the append-only revoke below takes away. Schema public therefore holds no view and no
-- materialized view. rls_coverage_is_the_literal_list asserts that count as 0,
-- and app_role_privilege_matrix_is_the_literal_table enumerates pg_class, not
-- pg_tables, so a later view lands in the matrix and fails the literal list.
-- A migration that adds a view revokes the default grant on it and states why.
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO cadus_app, cadus_admin;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
    GRANT USAGE, SELECT ON SEQUENCES TO cadus_app, cadus_admin;

-- --------------------------------------------------------------------------
-- C2: events is append-only for the runtime role. The grant enforces it, so a
-- code defect cannot edit or erase history. A wrong grade is superseded by a
-- 'regraded' event. cadus_admin keeps full DML for an operator repair.
-- --------------------------------------------------------------------------
REVOKE UPDATE, DELETE, TRUNCATE ON events FROM cadus_app;

-- --------------------------------------------------------------------------
-- C3, finding #2: users is the parent of every tenant table, and each child
-- references it with ON DELETE CASCADE. Postgres runs a referential-action
-- trigger with row-level security off, so a DELETE on users erases another
-- tenant's rows through the cascade and no policy sees it. Account deletion is an
-- admin operation, so the runtime role loses DELETE and TRUNCATE here.
--
-- C3, finding #4: table-wide UPDATE stayed with the runtime role, so a session
-- bound to tenant A rewrote tenant B's password_hash and set is_admin = true on
-- its own row.
--
-- Round-3 findings #4, #5, and #11 show that the round-2 fix narrowed UPDATE
-- only. INSERT stayed table-wide, so cadus_app wrote is_admin = true and picked
-- its own id on a new row. SELECT stayed USING (true), so a bound tenant read
-- every other account's email, password_hash, and is_admin. Three controls
-- replace the blanket grant.
--
-- 1. Per-command policies. users keeps the key id, not user_id, so it stays out
--    of the tenant_isolation set. SELECT and UPDATE reach the caller's own row
--    only, and an unbound session matches no row at all. INSERT stays open,
--    because sign-up creates the row that becomes the tenant. There is no DELETE
--    policy, because the REVOKE above already stops a DELETE.
-- 2. Two column lists, one on INSERT and one on UPDATE. Neither list holds id or
--    is_admin, so the runtime role never writes the admin flag and never picks a
--    primary key. Both columns come from the defaults of 0002_identity.sql:
--    gen_random_uuid() for id, false for is_admin.
-- 3. Two SECURITY DEFINER functions for the login path, which reads users before
--    a tenant context exists. The block after this one holds them.
-- --------------------------------------------------------------------------
-- #2: take DELETE and TRUNCATE on users away from the runtime role.
REVOKE DELETE, TRUNCATE ON users FROM cadus_app;

-- #4: turn the policy layer on for users.
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
-- #4: FORCE keeps a non-superuser owner inside the policies.
ALTER TABLE users FORCE ROW LEVEL SECURITY;
-- #11: a SELECT reaches the caller's own row only. USING (true) plus the
-- table-wide SELECT grant gave every bound tenant every other account's email,
-- password_hash, is_admin, and disabled_at. The login path, which runs before a
-- tenant context exists, goes through auth_user_by_email and auth_user_by_id.
CREATE POLICY users_read_self ON users FOR SELECT
    USING (id = nullif(current_setting('app.user_id', true), '')::uuid);
-- #4: sign-up inserts the row that becomes the tenant.
CREATE POLICY users_insert ON users FOR INSERT WITH CHECK (true);
-- #4: an UPDATE reaches the caller's own row only. The nullif guard is the same
-- one that the tenant_isolation policies use; the note below explains it.
CREATE POLICY users_update_self ON users FOR UPDATE
    USING (id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (id = nullif(current_setting('app.user_id', true), '')::uuid);
-- #4: drop the table-wide UPDATE of the runtime role.
REVOKE UPDATE ON users FROM cadus_app;
-- #4: give back every column of users except id and is_admin.
GRANT UPDATE (email, password_hash, email_verified_at, disabled_at, created_at)
    ON users TO cadus_app;
-- #4 and #5: drop the table-wide INSERT of the runtime role. The round-2 column
-- list covered UPDATE only, so a sign-up statement still wrote is_admin = true.
REVOKE INSERT ON users FROM cadus_app;
-- #4 and #5: give back every column of users except id and is_admin. Both come
-- from their defaults on an INSERT that names neither column.
GRANT INSERT (email, password_hash, email_verified_at, disabled_at, created_at)
    ON users TO cadus_app;

-- --------------------------------------------------------------------------
-- C3, finding #11: the two login functions.
--
-- users_read_self closes the plain SELECT for an unbound session, and the login
-- path is unbound by definition: it reads users to learn which id to bind. Two
-- SECURITY DEFINER functions serve that one read and nothing else.
--
-- auth_user_by_email serves the password login and the OAuth link by email.
-- auth_user_by_id serves the session-cookie path: auth_sessions gives a
-- user_id, and the account status decides the bind. Each function returns one
-- row of the five columns that an account-status decision needs, for the one
-- account that the caller names. A caller reads no other account, and a caller
-- with no argument reads nothing.
--
-- SECURITY DEFINER runs the body with the rights of the function owner. The
-- owner is the migration runner. In the shipped compose stack that role is the
-- postgres superuser, which bypasses row-level security, so the body sees the
-- row. A deployment that runs the migrations as a non-superuser owner gives that
-- owner BYPASSRLS, or the login lookup returns zero rows.
--
-- SET search_path = public pins the name resolution of the body, so a caller
-- with its own search_path cannot point the body at a different users table.
-- EXECUTE goes to PUBLIC by default, so the REVOKE runs first and the GRANT then
-- names the two roles that log a user in.
--
-- REQUIREMENT NOTE for M5 (docs/SCHEMA.md repeats it): the auth layer calls
-- these two functions for every read of users that happens before the bind. A
-- plain SELECT on users returns zero rows there. An INSERT with a RETURNING
-- clause also fails, because Postgres applies the SELECT policy to the new row,
-- so sign-up inserts without RETURNING and then calls auth_user_by_email.
-- --------------------------------------------------------------------------
CREATE FUNCTION auth_user_by_email(p_email citext)
RETURNS TABLE (
    id                uuid,
    password_hash     text,
    email_verified_at timestamptz,
    disabled_at       timestamptz,
    is_admin          boolean
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
STABLE
AS $$
    SELECT u.id, u.password_hash, u.email_verified_at, u.disabled_at, u.is_admin
    FROM users u
    WHERE u.email = p_email
$$;
REVOKE ALL ON FUNCTION auth_user_by_email(citext) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION auth_user_by_email(citext) TO cadus_app, cadus_admin;

CREATE FUNCTION auth_user_by_id(p_id uuid)
RETURNS TABLE (
    id                uuid,
    password_hash     text,
    email_verified_at timestamptz,
    disabled_at       timestamptz,
    is_admin          boolean
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
STABLE
AS $$
    SELECT u.id, u.password_hash, u.email_verified_at, u.disabled_at, u.is_admin
    FROM users u
    WHERE u.id = p_id
$$;
REVOKE ALL ON FUNCTION auth_user_by_id(uuid) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION auth_user_by_id(uuid) TO cadus_app, cadus_admin;

-- --------------------------------------------------------------------------
-- D9, finding #8: sqlx creates public._sqlx_migrations before it applies the
-- first migration, so the blanket GRANT above sweeps the migration ledger in.
-- The runtime role must not read, rewrite, or erase the schema history. A DELETE
-- on the ledger makes the next deploy replay 0002 and stop with an error. An
-- UPDATE of a checksum makes every later migration run fail with VersionMismatch.
-- cadus_admin keeps the ledger for an operator repair.
-- --------------------------------------------------------------------------
-- #8: take every privilege on the migration ledger away from the runtime role.
REVOKE ALL ON _sqlx_migrations FROM cadus_app;

-- --------------------------------------------------------------------------
-- T2, T6, finding #5: model_call_log holds one row per model call, with user_id,
-- session_id, token counts, and cost_usd. The table stayed outside row-level
-- security, so a tenant connection read every learner's rows and erased the whole
-- cost ledger in one statement. T2 names the worker as the only unit that spends
-- tokens, and the worker connects as cadus_admin. The runtime role therefore
-- keeps no privilege here. The table stays outside the RLS set for a new reason:
-- cadus_app reaches it with no statement at all.
-- --------------------------------------------------------------------------
-- #5: take every privilege on the model-call ledger away from the runtime role.
REVOKE ALL ON model_call_log FROM cadus_app;
-- #12: model_call_log.id is bigserial, so the table owns a sequence, and the
-- blanket sequence grant above gave cadus_app USAGE and SELECT on it. A tenant
-- connection read last_value, which counts the model calls of every tenant, and
-- ran nextval, which moved the primary key of the ledger. REVOKE ALL on the
-- table leaves a sequence untouched, so the sequence needs its own statement.
-- app_role_sequence_privileges_are_the_literal_table pins the result.
REVOKE ALL ON SEQUENCE model_call_log_id_seq FROM cadus_app;

-- --------------------------------------------------------------------------
-- C6, finding #14: content_store binds approval to the digest, so an edited body
-- is a new row that needs its own approval (0005_content.sql). The blanket grant
-- let the runtime role rewrite the body of an approved row in place and insert a
-- row that already carried status = 'approved'. The request tier reads approved
-- content, the worker authors it as cadus_admin, and approval is an admin
-- operation, so the runtime role keeps SELECT and nothing else.
-- --------------------------------------------------------------------------
-- #14: leave the runtime role with SELECT on content_store.
REVOKE INSERT, UPDATE, DELETE ON content_store FROM cadus_app;

-- --------------------------------------------------------------------------
-- C3: row-level security on every tenant table.
--
-- ENABLE turns the policy on. FORCE also applies the policy to the table owner.
--
-- Findings #25 and #36: FORCE gives no protection in the shipped compose stack.
-- There the migration runner is the postgres superuser, so it owns every object,
-- and Postgres skips row-level security for a superuser whether FORCE is set or
-- not. FORCE protects the other deployment shape: a non-superuser owner
-- (cadus_owner, when a deployment chooses it) runs the statements and stays
-- inside the policy. cadus_owner is reserved for that deployment shape. It owns
-- nothing in the shipped stack.
--
-- The policy reads the app.user_id GUC that the unit of work sets per
-- transaction.
--
-- current_setting('app.user_id', true) returns NULL when the GUC is unset, and
-- NULL = user_id is never true, so an unset GUC yields zero rows instead of an
-- error. The failure mode is closed: a query that escapes a unit of work leaks
-- nothing. WITH CHECK repeats the test so a write cannot place a row in another
-- tenant.
--
-- The nullif(..., '') guard is a 2.0 addition to the 1.0 policy text, and it is
-- load-bearing. After a session does SET app.user_id, the reset value of the GUC
-- is the empty string, not NULL. RESET app.user_id therefore leaves '' behind,
-- and ''::uuid raises 22P02 invalid_text_representation. The raw cast turns a
-- cleared tenant context into a query error instead of an empty result, which
-- breaks a pooled connection that a unit of work released. nullif maps '' to
-- NULL, so a cleared context yields zero rows exactly like an unset one.
-- --------------------------------------------------------------------------

ALTER TABLE events ENABLE ROW LEVEL SECURITY;
ALTER TABLE events FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON events
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE learner_models ENABLE ROW LEVEL SECURITY;
ALTER TABLE learner_models FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON learner_models
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE profiles FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON profiles
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE session_plans ENABLE ROW LEVEL SECURITY;
ALTER TABLE session_plans FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON session_plans
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE diag_states ENABLE ROW LEVEL SECURITY;
ALTER TABLE diag_states FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON diag_states
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE user_settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_settings FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON user_settings
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE web_states ENABLE ROW LEVEL SECURITY;
ALTER TABLE web_states FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON web_states
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE anki_queue ENABLE ROW LEVEL SECURITY;
ALTER TABLE anki_queue FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON anki_queue
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE anki_cards_created ENABLE ROW LEVEL SECURITY;
ALTER TABLE anki_cards_created FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON anki_cards_created
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

ALTER TABLE serving_pool ENABLE ROW LEVEL SECURITY;
ALTER TABLE serving_pool FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON serving_pool
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

-- #15: diagnosis_jobs joins the RLS set. payload holds the learner's attempt
-- document and result holds the diagnosis prose. The worker claims a job as
-- cadus_admin, which holds BYPASSRLS, so the claim still crosses every tenant.
ALTER TABLE diagnosis_jobs ENABLE ROW LEVEL SECURITY;
-- #15: FORCE keeps a non-superuser owner inside the policy.
ALTER TABLE diagnosis_jobs FORCE ROW LEVEL SECURITY;
-- #15: the same tenant predicate as every other scoped table.
CREATE POLICY tenant_isolation ON diagnosis_jobs
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);

-- #15: email_outbox joins the RLS set. to_addr holds a registered user's email
-- address. The worker drains the outbox as cadus_admin, which holds BYPASSRLS.
-- email_outbox.user_id is nullable (ON DELETE SET NULL), and a NULL user_id
-- matches no tenant, so an orphaned row stays visible to cadus_admin only.
ALTER TABLE email_outbox ENABLE ROW LEVEL SECURITY;
-- #15: FORCE keeps a non-superuser owner inside the policy.
ALTER TABLE email_outbox FORCE ROW LEVEL SECURITY;
-- #15: the same tenant predicate as every other scoped table.
CREATE POLICY tenant_isolation ON email_outbox
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
