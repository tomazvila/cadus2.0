# Schema v1

Requirements: C2, C3, D7, D9. Source: `migrations/0001_roles.sql` through
`0006_grants_rls.sql` — forward-only plain SQL, run by `sqlx::migrate!()`. The
lineage is 1.0's `docs/DATA_MODEL.md` §9.

## Table groups

| Migration | Group | Tables |
|---|---|---|
| `0001_roles` | roles, extensions | `citext`; `cadus_owner`, `cadus_app`, `cadus_admin` (roles are cluster-scoped, so each `CREATE ROLE` has a `pg_roles` guard) |
| `0002_identity` | auth and identity | `users`, `auth_sessions`, `oauth_accounts`, `auth_tokens`, `auth_rate_counters` |
| `0003_event_log` | source of truth | `events`, `learner_models` |
| `0004_scratch` | per-user scratch and queues | `profiles`, `session_plans`, `diag_states`, `user_settings`, `web_states`, `anki_queue`, `anki_cards_created`, `email_outbox` |
| `0005_content` | new in 2.0 | `content_store`, `serving_pool`, `model_call_log`, `diagnosis_jobs` |
| `0006_grants_rls` | grants and RLS | no tables |

## Roles

| Role | Login | RLS | Use |
|---|---|---|---|
| `cadus_owner` | NOLOGIN | — | Owns the schema. The migration runner connects as it. |
| `cadus_app` | LOGIN | enforced (NOBYPASSRLS) | The runtime role. No UPDATE, DELETE, or TRUNCATE on `events`. |
| `cadus_admin` | NOLOGIN | BYPASSRLS | Cross-tenant sweeps. Member of `cadus_app` for per-tenant drains. |

## Append-only events (C2)

`0006_grants_rls` runs `REVOKE UPDATE, DELETE, TRUNCATE ON events FROM cadus_app`
after the blanket grant. `cadus_app` keeps SELECT and INSERT. A wrong grade is
superseded by a `regraded` event. The grant enforces this, not a convention.

## Row-level security (C3)

Each scoped table gets `ENABLE ROW LEVEL SECURITY`, `FORCE ROW LEVEL SECURITY`,
and one policy `tenant_isolation`:

```
USING      (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
```

`FORCE` applies the policy to the table owner too. The `true` argument (`missing_ok`)
makes an unset GUC resolve to NULL, so an unscoped query returns zero rows instead
of an error. The failure mode is closed.

`nullif(..., '')` is a 2.0 addition to the 1.0 policy text. After a session runs
`SET app.user_id`, the reset value of that GUC is `''`, not NULL, so a later `RESET
app.user_id` leaves `''` and `''::uuid` raises SQLSTATE `22P02`. `nullif` maps `''`
to NULL, so a cleared context returns zero rows like an unset one.

**RLS-scoped (10 tables):** `events`, `learner_models`, `profiles`, `session_plans`,
`diag_states`, `user_settings`, `web_states`, `anki_queue`, `anki_cards_created`,
`serving_pool`.

**Exempt tables that carry `user_id` (6 tables), with the reason for each:**

| Table | Reason |
|---|---|
| `auth_sessions` | Looked up before a tenant context exists. |
| `oauth_accounts` | Looked up before a tenant context exists. |
| `auth_tokens` | Looked up before a tenant context exists. |
| `email_outbox` | The worker drains it across tenants. |
| `diagnosis_jobs` | The worker claims jobs across tenants. |
| `model_call_log` | Operator telemetry. `user_id` is nullable. |

Tables with no `user_id` never enter the RLS set: `users`, `auth_rate_counters`,
`content_store` (curriculum content, not learner data).

The lists are literal in the migration. A migration describes the schema as of its
own revision. A later tenant table needs its own `ENABLE`, `FORCE`, and policy in
a later migration.

## Two deliberate differences from 1.0

1. **`events.payload` is `jsonb`, not `json` (D7).** 1.0 used `json` because its
   diagnostic projection read object key order. `jsonb` reorders keys, so the 2.0
   projector must not depend on key order. M3 carries this as an open finding.
2. **`tutor_cache` is not created.** 1.0 cached model verdicts and templates at
   runtime. In 2.0 the LLM is an offline compiler (A1, A2), so `content_store`
   replaces the cache with digest-keyed, human-approved content (C6).
