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
| `0007_worker_liveness` | worker liveness (D-M5-6) | no tables; `diagnosis_claim_age_secs()` |
| `0008_auth_session_absolute` | the 90-day session window | no tables; `auth_session_by_token_hash` returns `created_at` |
| `0009_metrics_readers` | the new `/metrics` series (T6) | no tables; `model_call_totals()`, `diagnosis_job_totals()` |

## Roles

| Role | Login | RLS | Use |
|---|---|---|---|
| `cadus_owner` | NOLOGIN | — | Reserved for a deployment that runs the migrations as a non-superuser owner. It owns nothing in the shipped stack. |
| `cadus_app` | LOGIN | enforced (NOBYPASSRLS) | The runtime role. See the privilege table below. `crates/store/tests/rls.rs` pins the whole matrix. |
| `cadus_admin` | NOLOGIN | BYPASSRLS | Cross-tenant sweeps. Member of `cadus_app` for per-tenant drains. |

### Who runs the migrations

In the shipped compose stack the migration runner is the `postgres` superuser
(`docker-compose.yml`, the `migrate` service). That role owns every object and
bypasses row-level security, so `FORCE ROW LEVEL SECURITY` gives no protection
there. `FORCE` protects the other deployment shape: a non-superuser owner
(`cadus_owner`, when a deployment chooses it) runs the statements and stays
inside the tenant policy.

## Append-only events (C2)

`0006_grants_rls` runs `REVOKE UPDATE, DELETE, TRUNCATE ON events FROM cadus_app`
after the blanket grant. `cadus_app` keeps SELECT and INSERT. A wrong grade is
superseded by a `regraded` event. The grant enforces this, not a convention.

## The other revokes in `0006_grants_rls`

| Statement | Finding | Reason |
|---|---|---|
| `REVOKE DELETE, TRUNCATE ON users FROM cadus_app` | #2 | `users` is the parent of every tenant table, and each child references it with `ON DELETE CASCADE`. Postgres runs a referential-action trigger with row-level security off, so a `DELETE` on `users` erases another tenant's rows and no policy sees it. Account deletion is an admin operation. |
| `REVOKE UPDATE ON users FROM cadus_app` then `GRANT UPDATE (email, password_hash, email_verified_at, disabled_at, created_at) ON users TO cadus_app` | #4 | Table-wide UPDATE let a session bound to tenant A rewrite tenant B's `password_hash` and set `is_admin = true` on its own row. The column list keeps `id` and `is_admin` out of reach of an UPDATE. The `users_update_self` policy below narrows the rows. |
| `REVOKE INSERT ON users FROM cadus_app` then `GRANT INSERT (email, password_hash, email_verified_at, disabled_at, created_at) ON users TO cadus_app` | #4, #5 | The round-2 column list covered UPDATE only, so one sign-up INSERT still wrote `is_admin = true` and chose its own primary key. The two column lists together keep `id` and `is_admin` out of reach of the runtime role. Both columns come from their defaults: `gen_random_uuid()` for `id`, `false` for `is_admin`. |
| `REVOKE ALL ON _sqlx_migrations FROM cadus_app` | #8 | sqlx creates the ledger before the first migration runs, so the blanket grant swept it in. A `DELETE` on the ledger makes the next deploy replay `0002` and stop with an error; an `UPDATE` of a checksum makes every later run fail with `VersionMismatch`. `cadus_admin` keeps the ledger for an operator repair. |
| `REVOKE ALL ON model_call_log FROM cadus_app` | #5 | The table holds `user_id`, `session_id`, token counts, and `cost_usd`, and it stays outside row-level security, so a table-wide grant gave one tenant every tenant's rows and a one-statement wipe of the T6 ledger. T2 names the worker as the only unit that spends tokens, and the worker connects as `cadus_admin`. |
| `REVOKE ALL ON SEQUENCE model_call_log_id_seq FROM cadus_app` | #12 | `model_call_log.id` is `bigserial`, so the table owns a sequence, and `REVOKE ALL` on a table leaves that sequence untouched. The blanket sequence grant left `cadus_app` with USAGE and SELECT, so a tenant connection read `last_value`, the cluster-wide count of model calls, and moved the ledger key with `nextval`. The table exemption from RLS rests on the empty privilege set, so the sequence needs its own revoke. |
| `REVOKE INSERT, UPDATE, DELETE ON content_store FROM cadus_app` | #14 | C6 binds approval to the digest, so an edited body is a new row that needs its own approval. The blanket grant let the request tier rewrite an approved body in place and insert a row that already carried `status = 'approved'`. The request tier reads approved content, the worker authors it as `cadus_admin`, and approval is an admin operation. |

### The privilege matrix of `cadus_app`

`has_table_privilege` for every relation of schema `public`. The test
`app_role_privilege_matrix_is_the_literal_table` (finding #16) asserts this whole
matrix, so a widened grant in a later migration fails the suite.

| Table | SELECT | INSERT | UPDATE | DELETE | TRUNCATE |
|---|---|---|---|---|---|
| `_sqlx_migrations` | no | no | no | no | no |
| `content_store` | yes | no | no | no | no |
| `events` | yes | yes | no | no | no |
| `model_call_log` | no | no | no | no | no |
| `users` | yes | column-level | column-level | no | no |
| every other table | yes | yes | yes | yes | no |

`has_table_privilege` reports a column-level grant as `false`, so the INSERT cell
and the UPDATE cell of `users` read `false` in the catalog.
`has_column_privilege('cadus_app', 'users', 'password_hash', 'UPDATE')` is `true`
and the same call for `is_admin` is `false`. The same pair holds for INSERT, and
`has_column_privilege('cadus_app', 'users', 'id', 'INSERT')` is `false` too.

The matrix test enumerates `pg_class` with `relkind IN ('r','p','v','m')`, not
`pg_tables` (finding #6). `ALTER DEFAULT PRIVILEGES ... ON TABLES` also covers a
view and a materialized view, and a view runs with the rights of its owner. In
the shipped stack that owner is the `postgres` superuser, so an auto-updatable
view over `events` reads and writes every tenant's rows and skips the append-only
revoke. Schema `public` therefore holds no view and no materialized view:
`rls_coverage_is_the_literal_list` asserts that count as `0`, and a later view
lands in the matrix above. A migration that adds a view revokes the default grant
on it and states why.

### The sequence privileges of `cadus_app`

`has_sequence_privilege` for every sequence of schema `public`. The test
`app_role_sequence_privileges_are_the_literal_table` (finding #12) asserts this
whole matrix.

| Sequence | USAGE | SELECT | UPDATE |
|---|---|---|---|
| `model_call_log_id_seq` | no | no | no |

`cadus_admin` keeps USAGE and SELECT on that sequence, because the worker writes
the ledger.

## Row-level security (C3)

Each scoped table gets `ENABLE ROW LEVEL SECURITY`, `FORCE ROW LEVEL SECURITY`,
and one policy `tenant_isolation`:

```
USING      (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
WITH CHECK (user_id = nullif(current_setting('app.user_id', true), '')::uuid)
```

`FORCE` applies the policy to the table owner too, but only when the owner is not a
superuser. See "Who runs the migrations" above. The `true` argument (`missing_ok`)
makes an unset GUC resolve to NULL, so an unscoped query returns zero rows instead
of an error. The failure mode is closed.

`nullif(..., '')` is a 2.0 addition to the 1.0 policy text. After a session runs
`SET app.user_id`, the reset value of that GUC is `''`, not NULL, so a later `RESET
app.user_id` leaves `''` and `''::uuid` raises SQLSTATE `22P02`. `nullif` maps `''`
to NULL, so a cleared context returns zero rows like an unset one.

**RLS-scoped (15 tables):** `events`, `learner_models`, `profiles`, `session_plans`,
`diag_states`, `user_settings`, `web_states`, `anki_queue`, `anki_cards_created`,
`serving_pool`, `diagnosis_jobs`, `email_outbox`, `auth_sessions`, `auth_tokens`,
`oauth_accounts`.

### `users` carries its own per-command policies (finding #4)

`users` is keyed by `id`, not by `user_id`, so it stays outside the
`tenant_isolation` set. It still gets `ENABLE` and `FORCE ROW LEVEL SECURITY`,
and three policies:

| Policy | Command | Predicate |
|---|---|---|
| `users_read_self` | SELECT | `USING (id = nullif(current_setting('app.user_id', true), '')::uuid)` — a SELECT reaches the caller's own row only, and an unbound session reads nothing. |
| `users_insert` | INSERT | `WITH CHECK (true)` — sign-up inserts the row that becomes the tenant. |
| `users_update_self` | UPDATE | `USING` and `WITH CHECK` on the same predicate as `users_read_self` — an UPDATE reaches the caller's own row only, and an unbound session matches no row. |

There is no DELETE policy, because `0006_grants_rls` already revokes DELETE and
TRUNCATE on `users` from the runtime role.

### The pre-tenant lookup functions (findings #11, #1, #2, #4)

Every policy above gives an unbound caller zero rows, and the auth paths are
unbound by definition: each one reads a key to learn which `user_id` to bind.
`0006_grants_rls` serves those reads with five SECURITY DEFINER functions and
nothing else.

| Function | Argument | Returns | Use |
|---|---|---|---|
| `auth_user_by_email(citext)` | the submitted email | `(id, password_hash, email_verified_at, disabled_at, is_admin)` | Password login, sign-up read-back, the OAuth link by email. |
| `auth_user_by_id(uuid)` | a `user_id` | the same five columns | The account status behind a session cookie. |
| `auth_session_by_token_hash(text)` | SHA-256 of the cookie | `(user_id, expires_at, last_seen_at)` | The session cookie. |
| `auth_token_by_hash(text)` | SHA-256 of the token | `(user_id, purpose, expires_at, consumed_at)` | Password reset and email verification. |
| `oauth_account_lookup(text, text)` | `(provider, provider_account_id)` | `(user_id)` | The OAuth callback. |

Three properties hold for all five.

1. **An unbound caller only.** Each body carries
   `nullif(current_setting('app.user_id', true), '') IS NULL`, so a bound tenant
   gets zero rows from every function, its own account included. Round-4 finding
   #2: a SECURITY DEFINER body runs with the rights of the owner, so an unguarded
   function let a tenant bound to A read B's `password_hash` and `is_admin` while
   `users_read_self` saw nothing. The guard tests the caller, not the argument.
2. **One account, and only the columns a decision needs.**
3. **`SET search_path = public, pg_temp`.** Round-4 finding #4: Postgres searches
   the temporary schema BEFORE every schema that `search_path` names whenever
   `pg_temp` is not written out, so `SET search_path = public` resolved to the
   effective list `pg_temp, public`. A caller ran `CREATE TEMP TABLE users` plus
   one INSERT, and both login functions returned the attacker's row with
   `is_admin = true`. Naming `pg_temp` last puts the temporary schema after
   `public`.

`REVOKE ALL ... FROM PUBLIC` takes the automatic EXECUTE away from each function,
and `GRANT EXECUTE` names `cadus_app` and `cadus_admin`.
`ALTER DEFAULT PRIVILEGES REVOKE EXECUTE ON FUNCTIONS FROM PUBLIC` closes a
function of a later migration by default (finding #7). That statement carries no
`IN SCHEMA` clause: a schema-scoped default ACL is a delta that Postgres adds to
the hard-wired default, so the PUBLIC entry survives a schema-scoped REVOKE. The
global form replaces the hard-wired default instead.

SECURITY DEFINER runs the body with the rights of the function owner, which is
the migration runner. In the shipped stack that role is the `postgres` superuser
and bypasses row-level security. A deployment that runs the migrations as a
non-superuser owner gives that owner BYPASSRLS, or every lookup returns zero rows.

### The M5 auth contract: the call order

This is the binding contract for M5. A step that says "unbound" runs on a
`cadus_app` connection with no `app.user_id` set. A step that says "bound" runs
inside a unit of work that has set `app.user_id` to the `user_id` of that step.

**Password login**

1. Unbound: `SELECT * FROM auth_user_by_email($email)`. Zero rows means "unknown
   account". Verify the password against `password_hash`. Refuse a NULL
   `password_hash` (an OAuth-only account), a NULL `email_verified_at` if the
   deployment demands verification, and a non-NULL `disabled_at`.
2. Bind `app.user_id` to the returned `id`.
3. Bound: `INSERT INTO auth_sessions (token_hash, user_id, created_at,
   last_seen_at, expires_at) VALUES (...)` with `user_id` equal to the bound id.
   The `tenant_isolation` WITH CHECK clause refuses any other `user_id`.

**Sign-up**

1. Unbound: `INSERT INTO users (email, password_hash) VALUES (...)` with **no**
   `RETURNING` clause. Postgres applies the SELECT policy to the returned row,
   and an unbound session sees no row, so `RETURNING` fails. The INSERT names
   neither `id` nor `is_admin`: the column grant holds neither.
2. Unbound: `SELECT * FROM auth_user_by_email($email)` for the new `id`.
3. Bind, then write the session as in step 3 of password login.

**Session cookie**

1. Unbound: `SELECT * FROM auth_session_by_token_hash($sha256_of_cookie)`. Zero
   rows means "no session". Refuse an `expires_at` in the past.
2. Unbound: `SELECT * FROM auth_user_by_id($user_id)` for the account status.
   Refuse a non-NULL `disabled_at`.
3. Bind `app.user_id` to that `user_id`.
4. Bound, at most once per hour: `UPDATE auth_sessions SET last_seen_at = now()
   WHERE token_hash = $1`. The policy admits the row, because it belongs to the
   bound tenant.
5. Bound: sign-out is `DELETE FROM auth_sessions WHERE token_hash = $1`, and
   "sign out everywhere" is `DELETE FROM auth_sessions`. The policy holds both
   statements to the bound tenant.

**Password reset and email verification**

1. Unbound: `SELECT * FROM auth_token_by_hash($sha256_of_token)`. Zero rows means
   "unknown token". Refuse a non-NULL `consumed_at` (single use) and an
   `expires_at` in the past. Check `purpose` against the endpoint.
2. Bind `app.user_id` to the returned `user_id`.
3. Bound, in one transaction: `UPDATE auth_tokens SET consumed_at = now() WHERE
   token_hash = $1 AND consumed_at IS NULL`, then the write the purpose calls for
   (`UPDATE users SET password_hash = $2 WHERE id = $bound`, or `UPDATE users SET
   email_verified_at = now() WHERE id = $bound`). A zero row count on the token
   UPDATE means another request spent it first: stop and roll back.
4. Bound: a password change ends every other session with
   `DELETE FROM auth_sessions WHERE token_hash <> $current`.

Token creation belongs to the same shape: the request that asks for a reset is
unbound, so it calls `auth_user_by_email` first, binds, and then inserts the
`auth_tokens` row for the bound `user_id`.

**OAuth callback**

1. Unbound: `SELECT * FROM oauth_account_lookup($provider, $provider_account_id)`.
   One row gives the linked `user_id`: bind it and write the session.
2. Zero rows means the provider account is not linked yet. Unbound:
   `SELECT * FROM auth_user_by_email($email_from_provider)`.
   - One row: the account exists. Bind it, then bound:
     `INSERT INTO oauth_accounts (provider, provider_account_id, user_id,
     email_at_link) VALUES (...)` with the bound id. Link only an email that the
     provider states as verified.
   - Zero rows: run the sign-up steps above with a NULL `password_hash`, bind,
     and then insert the `oauth_accounts` row.

**After the bind.** A plain `SELECT` on `users` reads the caller's own row, and a
plain `SELECT` on `auth_sessions`, `auth_tokens`, and `oauth_accounts` reads the
caller's own rows. The profile page, the settings page, and the session list need
no function. Calling a function after the bind returns zero rows, so the handler
must not use one there.

**Rate limiting.** `auth_rate_counters` carries no `user_id` and no policy, so an
unbound `INSERT ... ON CONFLICT (scope, key, window_start) DO UPDATE` works at any
point of the flow.

**Exempt tables that carry `user_id` (1 table), with the reason:**

| Table | Reason |
|---|---|
| `model_call_log` | Findings #5 and #12: `cadus_app` holds no privilege on the table and none on its identity sequence, so no runtime statement reaches either. Only `cadus_admin` reads and writes them. Migration 0009 adds `model_call_totals()`, a SECURITY DEFINER aggregate that `/metrics` calls: it returns one line per `purpose` with token sums, a latency sum and a call count, and no row of the table. The privilege set of `cadus_app` on the table and on the sequence stays empty, so the exemption stands. |

`auth_sessions`, `auth_tokens`, and `oauth_accounts` left the exempt list with
round-4 finding #1. The old reason was "looked up before a tenant context
exists", which round 3 had already rejected for `users`. All three carry
`user_id` and full DML for `cadus_app`, so one INSERT into `auth_sessions` minted
a live cookie for any account and the web tier then bound `app.user_id` to the
victim, one SELECT read every live session, and one DELETE logged out the whole
deployment. The pre-tenant reads go through the functions above.

`diagnosis_jobs` and `email_outbox` left the exempt list. The old reason was "the
worker claims (or drains) across tenants". The worker connects as `cadus_admin`,
which holds BYPASSRLS, so the cross-tenant scan never depended on the missing
policy. The exemption only gave `cadus_app` every tenant's attempt payload,
diagnosis result, and email address. `email_outbox.user_id` is nullable
(`ON DELETE SET NULL`), and a NULL `user_id` matches no tenant, so an orphaned
row stays visible to `cadus_admin` only.

Tables with no `user_id` stay outside that derivation: `users`,
`auth_rate_counters`, and `content_store`. `users` carries the three per-command
policies above. `auth_rate_counters` holds fixed-window counters and has no tenant
column at all. `content_store` holds curriculum content, not learner data, and
finding #14 protects it with a revoke instead of a policy.

The lists are literal in the migration. A migration describes the schema as of its
own revision. A later tenant table needs its own `ENABLE`, `FORCE`, and policy in
a later migration.

## The other literal pins in `crates/store/tests/rls.rs`

Round 4 found three catalog surfaces that no test read, so a later migration
changed each one with the whole gate green.

| Test | Surface | Finding |
|---|---|---|
| `no_column_level_acl_outside_the_literal_list` | `pg_attribute.attacl` for every column of schema `public`. `has_table_privilege` reports a column grant as `false`, so `GRANT UPDATE (payload) ON events TO cadus_app` rewrote the authoritative event document with every C2 test green. The literal list holds the five `users` columns and nothing else. | #5 |
| `public_functions_are_the_literal_list` | `pg_proc` for every function of schema `public`: name, `prosecdef`, `proconfig`, and the EXECUTE bits of `cadus_app`, `cadus_admin`, and PUBLIC. The old test matched the name prefix `auth_user_by_`, so a new SECURITY DEFINER helper was invisible. Every other function of the schema belongs to the `citext` extension. | #7, #4 |
| `foreign_key_delete_actions_are_the_literal_list` | `pg_constraint.confdeltype` for every foreign key of schema `public`. `events_user_id_fkey` must stay `r` (RESTRICT): C2 says the event log outlives the account, and a flip to CASCADE erased a learner's whole history on one `DELETE FROM users`. | #8 |

## The `serving_pool` pop rule (D-O1, C6, A5)

The pop is `cadus_store::pool::pop_with_ring_tx`. It reads at most 8 rows of one
`(user_id, kp_id)` pair, gives them to the D5 anti-repeat rule, and claims the
row that rule chooses — all inside the caller's transaction (D-O1).

A row is a candidate when all three hold:

1. `claimed_at IS NULL`. The claim is what retires a row. A claimed row stays in
   the table as the served-instance log of A5; a retention job of M5 owns the
   delete.
2. `user_id` and `kp_id` name the pair the serve asks for.
3. The row carries NO `content_digest`, or the `content_store` row of that
   digest carries `status = 'approved'`.

Rule 3 is the C6 gate on the serve path. The pop therefore joins:

```sql
FROM serving_pool AS sp
LEFT JOIN content_store AS cs ON cs.digest = sp.content_digest
WHERE sp.user_id = $1
  AND sp.kp_id = $2
  AND sp.claimed_at IS NULL
  AND (sp.content_digest IS NULL OR cs.status = 'approved')
ORDER BY sp.created_at, sp.id
FOR UPDATE OF sp SKIP LOCKED
LIMIT $3
```

Three details of that statement are load-bearing:

- **`FOR UPDATE OF sp`**, not a bare `FOR UPDATE`. The lock belongs on the pool
  row. Two learners of one template share one `content_store` row, and a lock on
  that row makes the second serve wait for the first.
- **`SKIP LOCKED`** (D7). Two serves of one pair walk past each other's locked
  rows, so neither one waits and neither one reads the row the other claims.
- **`ORDER BY sp.created_at, sp.id`**. One refill batch is one statement, so
  every row of a batch shares `created_at`. The pool serves batch by batch in age
  order, and the `id` breaks the tie inside a batch.

### Why the approval is read on every serve

`content_store.status` is the only lever C6 gives an operator. Before M4 review 2
the pop read `serving_pool` alone, so a revocation stopped the next refill and
nothing else: the up-to-`target_depth` unclaimed rows the digest had already
written kept reaching learners, one per serve, each with the answer the operator
rejected (finding #4).

A row with no digest is an exemplar rotation (A6). Its authority is the
curriculum file, `content_store` holds no row for it, and the pop serves it.

### The refill retires what the pop walks past

The pop does not write to the rows it refuses; the serve path stays one
transaction with no extra write. `cadus_store::pool::retire_unapproved` is the
other half, and the D-O4 refill job runs it at the start of every pass:

```sql
UPDATE serving_pool AS sp
SET claimed_at = now()
FROM content_store AS cs
WHERE cs.digest = sp.content_digest
  AND sp.claimed_at IS NULL
  AND cs.status <> 'approved'
RETURNING ...
```

Each returned row reaches the log with its pair, its digest, and the new status.
The retire runs BEFORE the target query of the pass, so a pair the retire emptied
falls under its target depth in that same pass and refills from the source that
IS approved.

## Two deliberate differences from 1.0

1. **`events.payload` is `jsonb`, not `json` (D7).** 1.0 used `json` because its
   diagnostic projection read object key order. `jsonb` reorders keys, so the 2.0
   projector must not depend on key order. M3 carries this as an open finding.
2. **`tutor_cache` is not created.** 1.0 cached model verdicts and templates at
   runtime. In 2.0 the LLM is an offline compiler (A1, A2), so `content_store`
   replaces the cache with digest-keyed, human-approved content (C6).
