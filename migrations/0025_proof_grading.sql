-- Amendment K point 6: background grading of a written proof or a free
-- explanation (an attempt the deterministic checker leaves UNGRADED because
-- the item has no checkable key: answer kind `proof`, or the contract `none`).
--
-- One table, proof_grading_jobs: one row per (user, attempt). The answer
-- route writes it inside the grade transaction (the A4 pattern: a grade that
-- rolls back leaves no row). The worker claims it with FOR UPDATE SKIP LOCKED,
-- asks the hosted model for a per-check grading, and settles the row with the
-- result document. A pass also appends a `regraded` event (outcome correct)
-- and refolds the learner model in the same transaction as the settle.
--
-- No cache: two learners rarely write the same proof, and a proof verdict is
-- per text, not per value.
--
-- The row carries user_id, so it joins the row-level-security set (the 0006
-- rule): ENABLE, FORCE, and the tenant_isolation policy. cadus_app holds
-- SELECT plus an input-column INSERT; every worker-owned column (status,
-- attempts, result, the timestamps) stays out of its reach.
CREATE TABLE proof_grading_jobs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    attempt_id  text NOT NULL,
    status      text NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending','running','done','failed','capped')),
    attempts    integer NOT NULL DEFAULT 0,
    payload     jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object' AND octet_length(payload::text) <= 98304),
    result      jsonb NULL,   -- {v, verdict, checks, feedback, model}; NULL until status = 'done'
    created_at  timestamptz NOT NULL DEFAULT now(),
    claimed_at  timestamptz NULL,
    finished_at timestamptz NULL,
    UNIQUE (user_id, attempt_id)  -- one job per attempt; the enqueue is idempotent
);
CREATE INDEX proof_grading_jobs_pending ON proof_grading_jobs (created_at) WHERE status = 'pending';
ALTER TABLE proof_grading_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE proof_grading_jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON proof_grading_jobs
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
REVOKE ALL ON proof_grading_jobs FROM PUBLIC, cadus_app;
GRANT SELECT ON proof_grading_jobs TO cadus_app;
GRANT INSERT (user_id, attempt_id, payload) ON proof_grading_jobs TO cadus_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON proof_grading_jobs TO cadus_admin;
