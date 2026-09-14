-- Preserve complete diagnostic universes and exact run boundaries for verified repairs.
CREATE TABLE problem_report_diagnostics (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    run_id uuid PRIMARY KEY,
    start_seq bigint NOT NULL CHECK (start_seq >= 0),
    end_seq bigint CHECK (end_seq >= start_seq),
    completed boolean NOT NULL DEFAULT false,
    state jsonb NOT NULL CHECK (jsonb_typeof(state) = 'object'),
    config jsonb NOT NULL CHECK (jsonb_typeof(config) = 'object'),
    curriculum_digest text NOT NULL,
    CHECK (NOT completed OR end_seq IS NOT NULL)
);
CREATE UNIQUE INDEX problem_report_diagnostics_active ON problem_report_diagnostics(user_id) WHERE end_seq IS NULL;
CREATE INDEX problem_report_diagnostics_answer ON problem_report_diagnostics(user_id,start_seq,end_seq);
ALTER TABLE problem_report_diagnostics ENABLE ROW LEVEL SECURITY;
ALTER TABLE problem_report_diagnostics FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON problem_report_diagnostics
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
REVOKE ALL ON problem_report_diagnostics FROM PUBLIC, cadus_app, cadus_admin;
GRANT SELECT,INSERT,UPDATE ON problem_report_diagnostics TO cadus_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON problem_report_diagnostics TO cadus_admin;
