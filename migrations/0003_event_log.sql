-- 0003_event_log: the append-only event log and the derived learner model.
-- Requirements: C2 (event sourcing, append-only), D7 (jsonb payload), D9 (lineage).
--
-- Deliberate difference from 1.0: events.payload is jsonb, not json. D7 names
-- jsonb for event payloads. 1.0 used json because its diagnostic projection read
-- object key order. The 2.0 projector must not depend on key order (M0 plan,
-- open finding for M3).
--
-- 0006_grants_rls revokes UPDATE, DELETE, and TRUNCATE on events from cadus_app.
-- That grant is the append-only guarantee of C2.

CREATE TABLE events (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,  -- RESTRICT: the log outlives the account
    seq        bigint NOT NULL,     -- dense per-user line number; ts is not a total order
    ts         timestamptz NOT NULL,  -- copied out of payload for index and scan only
    type       text NOT NULL,         -- copied out of payload for index and scan only
    session_id text,                  -- copied out of payload for index and scan only
    v          smallint NOT NULL DEFAULT 1,  -- payload schema version
    attempt_id text,                  -- set only when type = 'attempt'; NULL otherwise
    payload    jsonb NOT NULL,        -- D7: the whole event document; authoritative
    PRIMARY KEY (user_id, seq)
);

-- FR-14 idempotency in the database: a repeated attempt_id makes the INSERT a
-- no-op under ON CONFLICT DO NOTHING. The index is partial because attempt_id is
-- NULL for every non-attempt event, and NULLs must not collide.
CREATE UNIQUE INDEX events_attempt_idem ON events (user_id, attempt_id)
    WHERE attempt_id IS NOT NULL;

CREATE TABLE learner_models (
    user_id           uuid PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    model             jsonb NOT NULL,    -- the whole LearnerModel document
    through_seq       bigint NOT NULL,   -- high-water mark: last events.seq folded in
    projector_version integer NOT NULL,  -- a bump forces a full replay
    config_hash       text NOT NULL,     -- config-drift detection
    curriculum_hash   text,              -- content hash at build time; nullable
    built_at          timestamptz NOT NULL DEFAULT now()
);
