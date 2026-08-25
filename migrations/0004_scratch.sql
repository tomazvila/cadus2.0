-- 0004_scratch: per-user scratch documents and the integration queues.
-- Requirements: C3 (multi-tenant Postgres), D9 (schema lineage from 1.0).
--
-- Column definitions carry over from 1.0 (migrations/versions/0001_baseline.py)
-- without change. Each scratch table holds one row per user and is written by a
-- plain upsert on user_id.
--
-- 1.0's tutor_cache is NOT carried over. 2.0 replaces it with content_store
-- (0005_content), because the LLM is an offline compiler in 2.0, not a runtime
-- dependency that needs a grade cache.

CREATE TABLE profiles (
    user_id    uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    profile    jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE session_plans (
    user_id    uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    session_id text NOT NULL,  -- the row loads only when the request session matches
    plan       jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE diag_states (
    user_id    uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    state      jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE user_settings (
    user_id    uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    settings   jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE web_states (
    user_id    uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    doc        jsonb NOT NULL,  -- hot web-tier state: served problem, answer buffer, timers
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE anki_queue (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    front_hash text NOT NULL,  -- dedup key: one pending card per distinct front
    item       jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, front_hash)
);

CREATE TABLE anki_cards_created (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    front_hash text NOT NULL,  -- idempotency ledger: a second mark for a hash is a no-op
    note_id    bigint,
    card       jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, front_hash)
);

CREATE TABLE email_outbox (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),  -- the send idempotency key
    user_id         uuid REFERENCES users(id) ON DELETE SET NULL,  -- nullable: keep the row after account deletion
    to_addr         text NOT NULL,
    kind            text NOT NULL,
    payload         jsonb NOT NULL DEFAULT '{}'::jsonb,
    status          text NOT NULL DEFAULT 'pending',
    provider_msg_id text,
    attempts        integer NOT NULL DEFAULT 0,
    created_at      timestamptz NOT NULL DEFAULT now(),
    sent_at         timestamptz
);

-- The worker drains pending mail in creation order across all tenants.
CREATE INDEX email_outbox_pending ON email_outbox (created_at) WHERE status = 'pending';
