-- The written proof a lesson owes (D-PR1): the decided items of a
-- proof-gated knowledge point passed, and its proof is served next.
--
-- The fact outlives the D-S6 row, which a session end and the day rollover
-- clear: a learner who leaves between the decided pass and the first draft
-- keeps the proof owed, and the plan carries its lesson first. The row goes
-- when the first draft opens the proof's revision chain.
--
-- The row carries user_id, so it joins the row-level-security set (the 0006
-- rule). cadus_app reads, writes and deletes its own rows; it never updates.
CREATE TABLE proof_owed (
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    topic      text NOT NULL,
    kp         text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, topic, kp)
);
ALTER TABLE proof_owed ENABLE ROW LEVEL SECURITY;
ALTER TABLE proof_owed FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON proof_owed
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
REVOKE ALL ON proof_owed FROM PUBLIC, cadus_app;
GRANT SELECT, INSERT, DELETE ON proof_owed TO cadus_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON proof_owed TO cadus_admin;
