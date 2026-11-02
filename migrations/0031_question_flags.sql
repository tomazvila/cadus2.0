-- A learner says a question is unclear or wrong (D-RP2). The report needs no model
-- review, so it does not enter `problem_reports`. One row here does three jobs:
--   * it is the report the owner reads (status `waiting_owner`);
--   * it sets the question aside for this learner while the status stays `waiting_owner`;
--   * it records the owner decision: `kept` clears the set-aside, `retired` ends the
--     service of the question to everyone (see `retired_items`).
-- problem_hash is `problem_text_hash` of the question text, the same hash the serve
-- path computes for a drawn problem.
CREATE TABLE question_flags (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_id   uuid NOT NULL,
    task_id      text NOT NULL,
    problem_id   text NOT NULL,
    problem_hash text NOT NULL,
    topic        text NOT NULL DEFAULT '',
    kp           text NOT NULL DEFAULT '',
    question     text NOT NULL CHECK (octet_length(question) <= 20000),
    note         text NOT NULL DEFAULT '' CHECK (octet_length(note) <= 8000),
    status       text NOT NULL DEFAULT 'waiting_owner'
                 CHECK (status IN ('waiting_owner', 'kept', 'retired')),
    created_at   timestamptz NOT NULL DEFAULT now(),
    decided_at   timestamptz,
    UNIQUE (user_id, request_id)
);
CREATE INDEX question_flags_set_aside
    ON question_flags (user_id, problem_hash) WHERE status = 'waiting_owner';
CREATE INDEX question_flags_owner ON question_flags (created_at) WHERE status = 'waiting_owner';
ALTER TABLE question_flags ENABLE ROW LEVEL SECURITY;
ALTER TABLE question_flags FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON question_flags
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
REVOKE ALL ON question_flags FROM PUBLIC, cadus_app;
GRANT SELECT, INSERT ON question_flags TO cadus_app;
GRANT SELECT, INSERT, UPDATE ON question_flags TO cadus_admin;

-- A question the owner retired. The serve path skips it for every learner.
CREATE TABLE retired_items (
    problem_hash text PRIMARY KEY,
    reason       text NOT NULL DEFAULT '',
    retired_by   uuid,
    created_at   timestamptz NOT NULL DEFAULT now()
);
REVOKE ALL ON retired_items FROM PUBLIC, cadus_app;
GRANT SELECT ON retired_items TO cadus_app;
GRANT SELECT, INSERT ON retired_items TO cadus_admin;
