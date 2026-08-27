-- Cadus 2.0 migration 0009: the two aggregate readers behind the new /metrics
-- series (T6, spec section 7).
--
-- `GET /metrics` runs in `cadus-web`, on a `cadus_app` connection, with no
-- tenant bound. The two tables the new series count are out of its reach:
--
--   * `model_call_log` -- finding #5 leaves `cadus_app` with no privilege at
--     all on the table and finding #12 none on its sequence;
--   * `diagnosis_jobs` -- a FORCEd tenant policy, so an unbound SELECT reads
--     zero rows on every deployment.
--
-- Migration 0007 met the same problem for the worker-liveness field of
-- `/api/ready` and answered it with one SECURITY DEFINER function that returns
-- an aggregate and no tenant row. These two follow that pattern.
--
-- Why this does NOT reopen finding #5: the ledger's exemption from row-level
-- security rests on the empty privilege set of `cadus_app` on the TABLE, and
-- that set stays empty. `SELECT`, `INSERT`, `UPDATE`, `DELETE`, `last_value`
-- and `nextval` all still fail with 42501. What leaves this function is one row
-- per `purpose` -- two values, 'diagnosis' and 'authoring' -- carrying token
-- sums, a latency sum and a call count. No `user_id`, no `session_id`, no
-- `request_id`, no `cost_usd`: the money column and every per-learner column
-- stay inside the ledger, where only `cadus_admin` reads them.

-- --------------------------------------------------------------------------
-- T6: the token and latency totals of `cadus_model_call_tokens_total` and
-- `cadus_model_call_latency_seconds`. One row per purpose.
--
-- The sums are `bigint`: an `integer` sum overflows after about 2 billion
-- tokens, which one busy month reaches.
-- --------------------------------------------------------------------------
CREATE FUNCTION model_call_totals()
RETURNS TABLE (
    purpose          text,
    input_cached     bigint,
    input_uncached   bigint,
    output_tokens    bigint,
    reasoning_tokens bigint,
    latency_ms       bigint,
    calls            bigint
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
STABLE
AS $$
    SELECT m.purpose,
           sum(m.input_tokens_cached)::bigint,
           sum(m.input_tokens_uncached)::bigint,
           sum(m.output_tokens)::bigint,
           sum(m.reasoning_tokens)::bigint,
           sum(m.latency_ms)::bigint,
           count(*)::bigint
      FROM model_call_log m
     GROUP BY m.purpose
$$;

-- --------------------------------------------------------------------------
-- The `cadus_diagnosis_jobs_total` counts. One row per status, across every
-- tenant. The queue holds the job id, the attempt id and the payload; this
-- function returns neither, only the status name and how many rows carry it.
-- --------------------------------------------------------------------------
CREATE FUNCTION diagnosis_job_totals()
RETURNS TABLE (
    status text,
    jobs   bigint
)
LANGUAGE sql
SECURITY DEFINER
SET search_path = public, pg_temp
STABLE
AS $$
    SELECT j.status, count(*)::bigint
      FROM diagnosis_jobs j
     GROUP BY j.status
$$;

-- EXECUTE on a new function goes to PUBLIC by default. Migration 0006 closes
-- that door with ALTER DEFAULT PRIVILEGES, and these REVOKEs repeat it, so the
-- grant surface of every SECURITY DEFINER function stays explicit at its own
-- definition (migration 0007 does the same).
REVOKE ALL ON FUNCTION model_call_totals() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION model_call_totals() TO cadus_app, cadus_admin;
REVOKE ALL ON FUNCTION diagnosis_job_totals() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION diagnosis_job_totals() TO cadus_app, cadus_admin;
