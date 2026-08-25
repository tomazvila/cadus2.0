-- 0005_content: the four tables that are new in 2.0.
-- Requirements: D9 (schema lineage adds three tables), C6 (human review gate),
-- D-S4 content store, D-S5 serving pool, D-S6 queues, A4 (async diagnosis),
-- A7 (source-agnostic pool), T3/T6 (authoring cost, model-call telemetry).
--
-- These four tables replace 1.0's tutor_cache. 1.0 cached model verdicts at
-- runtime; 2.0 authors content offline, approves it once, and serves it locally.

-- D-S4: authored content, keyed by content digest. C6 binds approval to the
-- digest, so an edited body is a new row that needs its own approval.
CREATE TABLE content_store (
    digest             text PRIMARY KEY,  -- content address of body; approval binds to it (C6)
    kp_id              text NOT NULL,
    kind               text NOT NULL CHECK (kind IN ('template','teach','hint_ladder','diagnosis')),
    body               jsonb NOT NULL,
    status             text NOT NULL DEFAULT 'pending'
                       CHECK (status IN ('pending','approved','rejected')),  -- C6: 'pending' is never served
    approved_by        uuid NULL REFERENCES users(id) ON DELETE SET NULL,  -- keep the approval after the reviewer account goes
    approved_at        timestamptz NULL,
    authoring_attempts integer NOT NULL DEFAULT 1,  -- T3: alert above 3 attempts for one KP
    authoring_cost_usd numeric(12,6) NULL,  -- T3: exact money; never a float
    created_at         timestamptz NOT NULL DEFAULT now()
);

-- The serve path asks for one KP, one kind, status 'approved'.
CREATE INDEX content_store_kp_kind_status ON content_store (kp_id, kind, status);

-- D-S5: verified problem instances, one small ring per (user, kp). A7 makes the
-- pool source-agnostic, so a future generator is a third value of `source`.
CREATE TABLE serving_pool (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    kp_id           text NOT NULL,
    source          text NOT NULL CHECK (source IN ('template','exemplar','generator')),  -- A7: measure effect per source
    content_digest  text NULL REFERENCES content_store(digest),  -- NULL for an exemplar with no content row
    problem         jsonb NOT NULL,
    expected_answer jsonb NOT NULL,  -- hidden from the learner; the checker reads it
    instance_hash   text NOT NULL,   -- A5 anti-repeat key
    created_at      timestamptz NOT NULL DEFAULT now(),
    claimed_at      timestamptz NULL,  -- non-NULL marks the instance served
    UNIQUE (user_id, kp_id, instance_hash)  -- A5: the same instance never enters the pool twice
);

-- D-O1 pops from this index with SELECT ... FOR UPDATE SKIP LOCKED.
CREATE INDEX serving_pool_unclaimed ON serving_pool (user_id, kp_id) WHERE claimed_at IS NULL;

-- T6: one row per model call. Cost is a dashboard, not a surprise.
CREATE TABLE model_call_log (
    id                    bigserial PRIMARY KEY,
    ts                    timestamptz NOT NULL DEFAULT now(),
    purpose               text NOT NULL,  -- T2: 'authoring' or 'diagnosis'; nothing else spends tokens
    model_id              text NOT NULL,
    provider              text NULL,
    user_id               uuid NULL REFERENCES users(id) ON DELETE SET NULL,  -- NULL for an offline authoring call
    session_id            text NULL,
    input_tokens_cached   integer NOT NULL DEFAULT 0,  -- T5: prompt caching is on by default
    input_tokens_uncached integer NOT NULL DEFAULT 0,
    output_tokens         integer NOT NULL DEFAULT 0,
    reasoning_tokens      integer NOT NULL DEFAULT 0,  -- T5: capped by default
    latency_ms            integer NOT NULL,
    cost_usd              numeric(12,6) NULL,  -- exact money; never a float
    request_id            text NULL
);

CREATE INDEX model_call_log_ts ON model_call_log (ts);
CREATE INDEX model_call_log_user_session ON model_call_log (user_id, session_id);

-- D-S6 queue: A4 async miss diagnosis. Finding #15: this table is inside RLS
-- (see 0006_grants_rls). The worker claims a job as cadus_admin, which holds
-- BYPASSRLS, so the claim still crosses every tenant.
CREATE TABLE diagnosis_jobs (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    attempt_id  text NOT NULL,
    status      text NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending','running','done','failed','capped')),  -- 'capped' records a T4 cap hit
    attempts    integer NOT NULL DEFAULT 0,
    payload     jsonb NOT NULL,
    result      jsonb NULL,   -- the diagnosis document; NULL until status = 'done'
    created_at  timestamptz NOT NULL DEFAULT now(),
    claimed_at  timestamptz NULL,
    finished_at timestamptz NULL,
    UNIQUE (user_id, attempt_id)  -- one diagnosis job per attempt; enqueue is idempotent
);

-- The worker claims the oldest pending job with FOR UPDATE SKIP LOCKED (D-O5).
CREATE INDEX diagnosis_jobs_pending ON diagnosis_jobs (created_at) WHERE status = 'pending';
