-- Cadus 2.0 migration 0007: the worker-liveness read of /api/ready (D-M5-6).
--
-- Ruling D-M5-6 (docs/plans/M5.md) drops the 1.0 `redis` dependency of the
-- readiness probe and takes worker liveness from the `diagnosis_jobs` claim
-- age instead. `/api/ready` is public and unauthenticated, so the web tier
-- calls it on a `cadus_app` connection with NO `app.user_id` set.
--
-- `diagnosis_jobs` carries the `tenant_isolation` policy of migration 0006, and
-- the policy is FORCEd. An unbound `cadus_app` SELECT on the table therefore
-- reads zero rows and reports "no backlog" on every deployment, whatever the
-- worker does. The function below is the one read that crosses the policy.
--
-- It is SECURITY DEFINER, so the body runs as the owner. The owner is the role
-- that applies the migrations: the postgres superuser, or a `cadus_admin` with
-- BYPASSRLS. Migration 0006 documents the same requirement for the five
-- `auth_*` lookups.
--
-- It returns ONE aggregate number and no row, so a caller learns the age of the
-- backlog and nothing about any tenant: no id, no attempt, no payload, no
-- prose. That is the whole reason the readiness probe is allowed to cross the
-- policy here.
--
-- The number is the age in seconds of the OLDEST job that still waits for a
-- claim (`status = 'pending'`). NULL means no job waits. A live worker claims
-- the oldest pending job with FOR UPDATE SKIP LOCKED (D-O5), so a backlog that
-- gets old is the signal that no worker claims. A "time since the last claim"
-- reading cannot serve here: an idle deployment with no work at all reports an
-- age that grows without end, and every idle probe then reads as stale.
--
-- The function takes no `app.user_id` clause. The five `auth_*` lookups of
-- migration 0006 admit an unbound caller only, because each one returns a row
-- of an account. This one returns an aggregate, and a bound caller that asks
-- for it must get the same true number, not a silent NULL.
CREATE FUNCTION diagnosis_claim_age_secs()
RETURNS double precision
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
STABLE
AS $$
    SELECT EXTRACT(EPOCH FROM (now() - min(j.created_at)))::double precision
    FROM diagnosis_jobs j
    WHERE j.status = 'pending'
$$;

-- EXECUTE on a new function goes to PUBLIC by default. Migration 0006 closes
-- that door with ALTER DEFAULT PRIVILEGES, and this REVOKE repeats it, so the
-- grant surface of every SECURITY DEFINER function stays explicit at its own
-- definition.
REVOKE ALL ON FUNCTION diagnosis_claim_age_secs() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION diagnosis_claim_age_secs() TO cadus_app, cadus_admin;
