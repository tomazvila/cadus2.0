-- Amendment K (steer note 114, decided by the owner 24 Sep): the background
-- equivalence check of a deterministic "wrong" or unparseable answer.
--
-- Two tables:
--
--   * equivalence_cache -- one row per (item digest, normalized learner text),
--     with the verdict, the model's one-line reason, and the model that
--     produced it. A repeat of the same answer on the same item is answered
--     from this table at once and never reaches a model. Only cadus_admin
--     writes it; cadus_app reads it (the lookup of the answer route).
--   * equivalence_jobs -- one row per unanswered (user, attempt) pair. The
--     answer route writes it inside the grade transaction (the A4 pattern:
--     a grade that rolls back leaves no row). The worker claims it with the
--     FOR UPDATE SKIP LOCKED pattern of diagnosis_jobs, calls the owner's
--     local model, writes the result into the cache, and settles the row.
--
-- Both carry user_id, so both join the row-level-security set (the 0006
-- rule): ENABLE, FORCE, and the tenant_isolation policy. The default
-- privileges of 0006 grant cadus_app the full DML on new tables, so each
-- grant here is explicit: cadus_app holds SELECT on the cache and SELECT plus
-- an input-column INSERT on the jobs; every worker-owned column (status,
-- attempts, result, the timestamps) stays out of its reach.
CREATE TABLE equivalence_cache (
    item_digest text NOT NULL CHECK (item_digest ~ '^[0-9a-f]{12,64}$'),
    answer_key  text NOT NULL CHECK (length(answer_key) <= 4200),
    equivalent  boolean NOT NULL,
    reason      text NOT NULL,
    model       text NOT NULL,
    checked_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (item_digest, answer_key)
);
REVOKE ALL ON equivalence_cache FROM PUBLIC, cadus_app;
GRANT SELECT ON equivalence_cache TO cadus_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON equivalence_cache TO cadus_admin;

CREATE TABLE equivalence_jobs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    attempt_id  text NOT NULL,
    status      text NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending','running','done','failed','capped')),
    attempts    integer NOT NULL DEFAULT 0,
    payload     jsonb NOT NULL CHECK (jsonb_typeof(payload) = 'object' AND octet_length(payload::text) <= 98304),
    result      jsonb NULL,   -- {equivalent, reason, model}; NULL until status = 'done'
    created_at  timestamptz NOT NULL DEFAULT now(),
    claimed_at  timestamptz NULL,
    finished_at timestamptz NULL,
    UNIQUE (user_id, attempt_id)  -- one job per attempt; the enqueue is idempotent
);
CREATE INDEX equivalence_jobs_pending ON equivalence_jobs (created_at) WHERE status = 'pending';
ALTER TABLE equivalence_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE equivalence_jobs FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON equivalence_jobs
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
REVOKE ALL ON equivalence_jobs FROM PUBLIC, cadus_app;
GRANT SELECT ON equivalence_jobs TO cadus_app;
GRANT INSERT (user_id, attempt_id, payload) ON equivalence_jobs TO cadus_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON equivalence_jobs TO cadus_admin;
