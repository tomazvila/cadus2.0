-- 0015_ordinary_problem_handoff: bounded lifetime exposure lookups.
--
-- The event remains authoritative. These partial expression indexes let the
-- ordinary serve path decide exposure without scanning a learner's lifetime log.

CREATE INDEX events_ordinary_handoff_finite
    ON events (user_id, (payload->>'kp_id'), (payload->>'finite_case_id'))
    WHERE type = 'ordinary_problem_served' AND payload ? 'finite_case_id';

CREATE INDEX events_ordinary_handoff_digest
    ON events (user_id, (payload->>'item_digest'))
    WHERE type = 'ordinary_problem_served';
