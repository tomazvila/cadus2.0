-- 0002_identity: the five auth and identity tables.
-- Requirements: C3 (multi-tenant Postgres), D9 (schema lineage from 1.0).
--
-- Column definitions carry over from 1.0 (migrations/versions/0001_baseline.py)
-- without change. These tables are looked up by their own keys (email,
-- token_hash, (scope, key, window_start)) before a tenant context exists, so
-- 0006_grants_rls leaves them exempt from row-level security.

CREATE TABLE users (
    id                uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    email             citext NOT NULL UNIQUE,  -- citext: email match is case-folded
    password_hash     text,                    -- NULL for an OAuth-only account
    email_verified_at timestamptz,
    is_admin          boolean NOT NULL DEFAULT false,
    disabled_at       timestamptz,
    created_at        timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE auth_sessions (
    token_hash   text PRIMARY KEY,  -- SHA-256 of the token; the live token is never stored
    user_id      uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   timestamptz NOT NULL,
    last_seen_at timestamptz NOT NULL,  -- touched at most once per hour
    expires_at   timestamptz NOT NULL,
    ip           text,
    user_agent   text
);

CREATE TABLE oauth_accounts (
    provider            text NOT NULL,
    provider_account_id text NOT NULL,
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    email_at_link       text NOT NULL,  -- the email seen at link time; audit only
    PRIMARY KEY (provider, provider_account_id)
);

CREATE TABLE auth_tokens (
    token_hash  text PRIMARY KEY,  -- SHA-256 of a single-use reset or verify token
    user_id     uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose     text NOT NULL,
    expires_at  timestamptz NOT NULL,
    consumed_at timestamptz  -- non-NULL marks the token spent; single use
);

CREATE TABLE auth_rate_counters (
    scope        text NOT NULL,
    key          text NOT NULL,
    window_start timestamptz NOT NULL,
    count        integer NOT NULL DEFAULT 0,
    PRIMARY KEY (scope, key, window_start)  -- fixed-window counter; upsert on the PK
);

CREATE INDEX auth_sessions_user ON auth_sessions (user_id);
CREATE INDEX oauth_accounts_user ON oauth_accounts (user_id);
CREATE INDEX auth_tokens_user ON auth_tokens (user_id);
