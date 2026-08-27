-- Cadus 2.0 migration 0008: the session lookup returns created_at.
--
-- Requirements: C3 (row-level security and the tenant bind).
-- Specification: docs/reference/web-service-1.0-spec.md section 3.3, "Cookie
-- check", step 1, and section 10, row "Lifetimes".
--
-- The binding call order puts BOTH session refusals in the UNBOUND step 1:
-- "refuse expires_at <= now and now - created_at >= 90 days". Migration 0006
-- gave `auth_session_by_token_hash` three output columns — user_id, expires_at,
-- last_seen_at — so the handler saw no created_at and the 90-day absolute
-- window had no value to test. A bound read of created_at is the alternative,
-- and it costs one transaction on EVERY guarded request, which the L1 budget of
-- docs/reference/l1-budget.md does not carry.
--
-- The function therefore gains a fourth output column. `CREATE OR REPLACE`
-- cannot change the output columns of a set-returning function, so the file
-- drops the old function first and grants EXECUTE again on the new one.
--
-- The guard inside the body does not change: an unbound caller only.

DROP FUNCTION auth_session_by_token_hash(text);

CREATE FUNCTION auth_session_by_token_hash(p_token_hash text)
RETURNS TABLE (
    user_id      uuid,
    created_at   timestamptz,
    expires_at   timestamptz,
    last_seen_at timestamptz
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
STABLE
AS $$
    SELECT s.user_id, s.created_at, s.expires_at, s.last_seen_at
    FROM auth_sessions s
    WHERE s.token_hash = p_token_hash
      -- The caller must be unbound. The guard tests the CALLER, not the
      -- argument, so a bound caller reads zero rows here.
      AND nullif(current_setting('app.user_id', true), '') IS NULL
$$;
REVOKE ALL ON FUNCTION auth_session_by_token_hash(text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION auth_session_by_token_hash(text)
    TO cadus_app, cadus_admin;
