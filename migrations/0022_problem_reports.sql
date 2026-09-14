-- Reports retain the original attempt and append evidence and correction versions.
CREATE TABLE problem_reports (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_id uuid NOT NULL,
    task_id text NOT NULL,
    problem_id text NOT NULL,
    attempt_id text NOT NULL,
    source_hash text NOT NULL CHECK (source_hash ~ '^[0-9a-f]{64}$'),
    input jsonb NOT NULL CHECK (jsonb_typeof(input) = 'object' AND octet_length(input::text) <= 98304),
    status text NOT NULL DEFAULT 'queued' CHECK (status IN ('queued','running','completed','unresolved','failed')),
    stage text NOT NULL DEFAULT 'queued',
    attempt integer NOT NULL DEFAULT 0 CHECK (attempt BETWEEN 0 AND 3),
    lease uuid,
    lease_until timestamptz,
    result jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (user_id, request_id)
);
CREATE UNIQUE INDEX problem_reports_active_attempt
    ON problem_reports(user_id, attempt_id) WHERE status IN ('queued','running');
CREATE INDEX problem_reports_queue ON problem_reports(created_at) WHERE status IN ('queued','running');
CREATE INDEX problem_reports_tenant_created ON problem_reports(user_id, created_at);
ALTER TABLE problem_reports ENABLE ROW LEVEL SECURITY;
ALTER TABLE problem_reports FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_problem_reports ON problem_reports
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
REVOKE ALL ON problem_reports FROM PUBLIC, cadus_app;
GRANT SELECT ON problem_reports TO cadus_app;
GRANT INSERT (user_id,request_id,task_id,problem_id,attempt_id,source_hash,input)
    ON problem_reports TO cadus_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON problem_reports TO cadus_admin;

CREATE TABLE problem_report_steps (
    id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    report_id uuid NOT NULL REFERENCES problem_reports(id) ON DELETE CASCADE,
    stage text NOT NULL,
    evidence jsonb NOT NULL CHECK (octet_length(evidence::text) <= 262144),
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX problem_report_steps_report ON problem_report_steps(report_id, created_at);
REVOKE ALL ON problem_report_steps FROM PUBLIC, cadus_app, cadus_admin;
GRANT SELECT,INSERT ON problem_report_steps TO cadus_admin;

CREATE TABLE problem_corrections (
    source_hash text NOT NULL CHECK (source_hash ~ '^[0-9a-f]{64}$'),
    version bigint NOT NULL CHECK (version > 0),
    report_id uuid NOT NULL UNIQUE REFERENCES problem_reports(id),
    body jsonb NOT NULL CHECK (jsonb_typeof(body) = 'object' AND octet_length(body::text) <= 262144),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (source_hash,version)
);
REVOKE ALL ON problem_corrections FROM PUBLIC, cadus_app, cadus_admin;
GRANT SELECT ON problem_corrections TO cadus_app;
GRANT SELECT,INSERT ON problem_corrections TO cadus_admin;
