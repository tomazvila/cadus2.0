-- Derived exposure identity preserves immutable event payloads and grades.
ALTER TABLE events ADD COLUMN attempt_problem_digest text;
ALTER TABLE events ADD CONSTRAINT events_attempt_digest_shape CHECK (
    attempt_problem_digest IS NULL OR
    (type = 'attempt' AND attempt_problem_digest ~ '^[0-9a-f]{12}$')
);
CREATE INDEX events_attempt_problem_digest_identity
    ON events (user_id, attempt_problem_digest)
    WHERE attempt_problem_digest IS NOT NULL;
CREATE INDEX events_attempt_digest_missing
    ON events (user_id, seq)
    WHERE type = 'attempt' AND attempt_problem_digest IS NULL;
CREATE INDEX events_item_exposure_history
    ON events (user_id, seq)
    WHERE type IN ('attempt', 'ordinary_problem_served');

-- Retired renderings remain exposure evidence without becoming serving variants.
CREATE TABLE finite_exposure_aliases (
    kp_id text NOT NULL,
    case_id text NOT NULL,
    item_digest text NOT NULL CHECK (item_digest ~ '^[0-9a-f]{12}$'),
    problem_sha256 text NOT NULL CHECK (problem_sha256 ~ '^[0-9a-f]{64}$'),
    problem_text text NOT NULL CHECK (length(problem_text) > 0),
    first_review_ref text NOT NULL CHECK (length(first_review_ref) > 0),
    source_policy_digest text,
    source_kind text NOT NULL CHECK (source_kind IN ('active_variant', 'retired_variant', 'exposure_only')),
    registered_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (kp_id, case_id, item_digest, problem_sha256)
);
REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON finite_exposure_aliases FROM cadus_app;
GRANT SELECT ON finite_exposure_aliases TO cadus_app;

-- One published reviewed alias context per current finite knowledge point.
CREATE TABLE finite_exposure_contexts (
    kp_id text PRIMARY KEY,
    policy_digest text NOT NULL CHECK (length(policy_digest) > 0),
    context_digest text NOT NULL CHECK (context_digest ~ '^[0-9a-f]{64}$'),
    review_ref text NOT NULL CHECK (length(review_ref) > 0),
    published_at timestamptz NOT NULL DEFAULT now()
);
REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON finite_exposure_contexts FROM cadus_app;
GRANT SELECT ON finite_exposure_contexts TO cadus_app;

-- Digest ingestion and finite semantic reconciliation have distinct scopes.
CREATE TABLE exposure_history_progress (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    scope_key text NOT NULL CHECK (length(scope_key) > 0),
    algorithm_version integer NOT NULL CHECK (algorithm_version > 0),
    context_digest text,
    target_seq bigint NOT NULL CHECK (target_seq >= 0),
    through_seq bigint NOT NULL CHECK (through_seq >= 0 AND through_seq <= target_seq),
    status text NOT NULL CHECK (status IN ('pending', 'complete', 'unresolved')),
    unresolved_count bigint NOT NULL DEFAULT 0 CHECK (unresolved_count >= 0),
    review_ref text,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, scope_key),
    CHECK (status <> 'complete' OR (through_seq = target_seq AND unresolved_count = 0)),
    CHECK ((scope_key = '*' AND context_digest IS NULL) OR
           (scope_key <> '*' AND context_digest IS NOT NULL AND context_digest ~ '^[0-9a-f]{64}$'))
);
ALTER TABLE exposure_history_progress ENABLE ROW LEVEL SECURITY;
ALTER TABLE exposure_history_progress FORCE ROW LEVEL SECURITY;
CREATE POLICY tenant_isolation ON exposure_history_progress
    USING (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
    WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid);
-- The app inserts only a proved empty-history checkpoint in its tenant lock.
-- Backfill and reconciliation updates use the admin connection.
REVOKE UPDATE, DELETE, TRUNCATE ON exposure_history_progress FROM cadus_app;
GRANT SELECT, INSERT ON exposure_history_progress TO cadus_app;
