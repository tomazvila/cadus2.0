-- Only current case identities can complete a finite semantic reconciliation.
ALTER TABLE finite_exposure_contexts
    ADD COLUMN current_case_ids text[] NOT NULL DEFAULT '{}';

-- Metadata belongs to the immutable handoff, while Attempt JSON stays unchanged.
ALTER TABLE events ADD COLUMN handoff_problem_sha256 text;
ALTER TABLE events ADD COLUMN attempt_handoff_seq bigint;
ALTER TABLE events ADD CONSTRAINT events_handoff_text_shape CHECK (
    handoff_problem_sha256 IS NULL OR
    (type = 'ordinary_problem_served' AND handoff_problem_sha256 ~ '^[0-9a-f]{64}$')
);
ALTER TABLE events ADD CONSTRAINT events_attempt_handoff_shape CHECK (
    attempt_handoff_seq IS NULL OR (type = 'attempt' AND attempt_handoff_seq < seq)
);
ALTER TABLE events ADD CONSTRAINT events_attempt_handoff_fk
    FOREIGN KEY (user_id, attempt_handoff_seq) REFERENCES events (user_id, seq);
-- A linked Attempt repeats the exact recorded handoff and adds no new exposure.
ALTER TABLE events ADD COLUMN exposure_requires_reconciliation boolean
    GENERATED ALWAYS AS (
        type = 'ordinary_problem_served' OR
        (type = 'attempt' AND attempt_handoff_seq IS NULL)
    ) STORED;
CREATE INDEX events_unclassified_exposure_tail ON events (user_id, seq)
    WHERE exposure_requires_reconciliation;
-- The runtime may advance a complete checkpoint over its own known handoff.
-- It cannot mark an unresolved history complete or replace its review context.
GRANT UPDATE (target_seq, through_seq, updated_at)
    ON exposure_history_progress TO cadus_app;
