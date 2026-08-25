-- 0006_grants_rls: role grants, default privileges, the append-only revoke, and
-- row-level security.
-- Requirements: C2 (append-only events at the grant level), C3 (RLS tenant
-- isolation), D9 (schema lineage).
--
-- RLS-scoped tables (literal list, docs/plans/M0.md):
--   events, learner_models, profiles, session_plans, diag_states, user_settings,
--   web_states, anki_queue, anki_cards_created, serving_pool.
--
-- Exempt tables that carry a user_id (literal list with the reason for each,
-- copied from docs/plans/M0.md):
--   auth_sessions   -- looked up before a tenant context exists
--   oauth_accounts  -- looked up before a tenant context exists
--   auth_tokens     -- looked up before a tenant context exists
--   email_outbox    -- the worker drains it across tenants
--   diagnosis_jobs  -- the worker claims jobs across tenants
--   model_call_log  -- operator telemetry; user_id is nullable
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
-- C3: row-level security on every tenant table.
--
-- ENABLE turns the policy on. FORCE applies it to the table owner too, so a
-- migration or an owner session gets no free pass. The policy reads the
-- app.user_id GUC that the unit of work sets per transaction.
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
