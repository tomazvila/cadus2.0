-- 0016_ordinary_problem_handoff_identity: RLS-safe bounded exposure lookups.
--
-- The JSON expression keys from 0015 remain filters under tenant RLS. Stored
-- generated columns expose the same authoritative payload identity through
-- ordinary text equality, allowing every identity field into the index scan.

ALTER TABLE events
    ADD COLUMN ordinary_kp_id text GENERATED ALWAYS AS (
        CASE WHEN type = 'ordinary_problem_served' THEN payload->>'kp_id' END
    ) STORED,
    ADD COLUMN ordinary_finite_case_id text GENERATED ALWAYS AS (
        CASE WHEN type = 'ordinary_problem_served' THEN payload->>'finite_case_id' END
    ) STORED,
    ADD COLUMN ordinary_item_digest text GENERATED ALWAYS AS (
        CASE WHEN type = 'ordinary_problem_served' THEN payload->>'item_digest' END
    ) STORED;

CREATE INDEX events_ordinary_handoff_finite_identity
    ON events (user_id, ordinary_kp_id, ordinary_finite_case_id)
    WHERE ordinary_finite_case_id IS NOT NULL;

CREATE INDEX events_ordinary_handoff_digest_identity
    ON events (user_id, ordinary_item_digest)
    WHERE ordinary_item_digest IS NOT NULL;
