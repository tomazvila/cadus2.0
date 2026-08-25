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
--   model_call_log  -- operator telemetry; user_id is nullable
--
-- Finding #15: diagnosis_jobs and email_outbox left the exempt list. The old
-- reason was "the worker claims (or drains) across tenants". The worker connects
-- as cadus_admin, which holds BYPASSRLS, so the cross-tenant scan never depended
-- on the missing policy. The exemption only gave cadus_app every tenant's
-- attempt payload, diagnosis result, and email address.
--
-- Tables with no user_id never enter the RLS set: users, auth_rate_counters,
-- content_store. content_store holds curriculum content, not learner data.
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

-- bigserial columns need the sequence too (model_call_log.id).
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO cadus_app, cadus_admin;

-- Default privileges: a table or sequence that a LATER migration creates becomes
-- grantable without a manual GRANT. Without this, a new table is DML-denied for
-- cadus_app until someone notices at runtime. The default applies to objects that
-- the migration-running role creates. RLS on a new tenant table still needs its
-- own ENABLE, FORCE, and policy in that migration.
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
-- C3, finding #2: users is the RLS-exempt parent of every tenant table, and each
-- child references it with ON DELETE CASCADE. Postgres runs a referential-action
-- trigger with row-level security off, so a DELETE on users erases another
-- tenant's rows through the cascade and no policy sees it. Account deletion is an
-- admin operation, so the runtime role loses DELETE and TRUNCATE here. cadus_app
-- keeps SELECT, INSERT, and UPDATE: sign-up and sign-in read and write users
-- before a tenant context exists.
-- --------------------------------------------------------------------------
-- #2: take DELETE and TRUNCATE on users away from the runtime role.
REVOKE DELETE, TRUNCATE ON users FROM cadus_app;

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
