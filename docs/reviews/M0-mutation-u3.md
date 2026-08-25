# M0 mutation check of unit U3 (row-level security proofs)

Requirement: `HANDOVER.md` §3, "Tests are not self-oracles". A test suite that stays
green on broken code is rejected.

Subject: `crates/store/tests/rls.rs` (5 tests) against `migrations/0006_grants_rls.sql`
and `crates/store/src/lib.rs`.

Method: apply one mutation, run
`CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_mut cargo test -p cadus-store`,
record the failed tests, then restore the original file and confirm that
`git diff --stat` prints nothing for that file. Each test creates and migrates its own
database, so every run reads the mutated migration.

Baseline before the first mutation: `5 passed; 0 failed`.

## Result table

| Mutation | Change | Tests that failed | Verdict |
| --- | --- | --- | --- |
| M1 | Delete `REVOKE UPDATE, DELETE, TRUNCATE ON events FROM cadus_app;` | `app_role_cannot_update_events` | KILLED |
| M2 | Delete every `FORCE ROW LEVEL SECURITY` statement (10) | `rls_coverage_is_the_literal_list` | KILLED |
| M3 | Delete every `ENABLE ROW LEVEL SECURITY` statement and every `CREATE POLICY` | `rls_coverage_is_the_literal_list`, `rls_isolates_tenants` | KILLED |
| M4 | Remove the `WITH CHECK` clause from every policy | none | **SURVIVED** |
| M5 | `assert_rls_enforced` returns `Ok` for every role | `boot_guard` | KILLED |
| M6 | `begin_tenant` skips the `set_config` call | `app_role_cannot_update_events`, `rls_isolates_tenants` | KILLED |

Score: 5 of 6 mutations killed, 1 survivor.

## Failure output per mutation

### M1 — KILLED

```
test app_role_cannot_update_events ... FAILED
thread 'app_role_cannot_update_events' (840774) panicked at crates/store/tests/rls.rs:76:10:
called `Result::unwrap_err()` on an `Ok` value: PgQueryResult { rows_affected: 1 }
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

### M2 — KILLED

```
test rls_coverage_is_the_literal_list ... FAILED
thread 'rls_coverage_is_the_literal_list' (841746) panicked at crates/store/tests/rls.rs:215:5:
assertion `left == right` failed
  left: []
 right: ["anki_cards_created", "anki_queue", "diag_states", "events", "learner_models", "profiles", "serving_pool", "session_plans", "user_settings", "web_states"]
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

Note: only the catalog-shape test sees M2. The behavior tests use the `cadus_app` role,
and `cadus_app` does not own the tables, so `FORCE` changes nothing for that role. The
suite therefore has one guard for `FORCE`, not two.

### M3 — KILLED

```
test rls_coverage_is_the_literal_list ... FAILED
test rls_isolates_tenants ... FAILED
thread 'rls_coverage_is_the_literal_list' (842837) panicked at crates/store/tests/rls.rs:215:5:
assertion `left == right` failed
  left: []
 right: ["anki_cards_created", "anki_queue", "diag_states", "events", "learner_models", "profiles", "serving_pool", "session_plans", "user_settings", "web_states"]
thread 'rls_isolates_tenants' (842838) panicked at crates/store/tests/rls.rs:135:5:
assertion `left == right` failed
  left: 2
 right: 1
test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
```

### M4 — SURVIVED

```
test boot_guard ... ok
test rls_coverage_is_the_literal_list ... ok
test app_role_cannot_update_events ... ok
test rls_isolates_tenants ... ok
test migrate_is_idempotent ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Analysis. PostgreSQL applies the `USING` expression as the write check when a policy for
`ALL` commands has no `WITH CHECK` clause. A probe on a database that carries the mutated
migration confirms this. The probe runs as `cadus_app` with `app.user_id` bound to tenant
A:

```
BEGIN
psql:<stdin>:5: ERROR:  42501: new row violates row-level security policy for table "learner_models"
ROLLBACK
BEGIN
INSERT 0 1
psql:<stdin>:11: ERROR:  42501: new row violates row-level security policy for table "learner_models"
ROLLBACK
```

The first error is a cross-tenant `INSERT`. The second is an `UPDATE` that moves an own
row to tenant B. Both stay blocked, so M4 changes no tenant-isolation behavior today.

The catalog does change. `pg_policy.polwithcheck` becomes NULL for all 10 policies:

```
     polname      | polcmd |                                    using_expr                                    | check_expr
------------------+--------+----------------------------------------------------------------------------------+------------
 tenant_isolation | *      | (user_id = (NULLIF(current_setting('app.user_id'::text, true), ''::text))::uuid) |
```

M4 stays a survivor and a finding. The suite pins the name of each policy and pins nothing
about the text of each policy. A later edit that keeps the name `tenant_isolation` and
changes the predicate passes `rls_coverage_is_the_literal_list` without a word. The
missing assertion is named in the next section.

### M5 — KILLED

```
test boot_guard ... FAILED
thread 'boot_guard' (849045) panicked at crates/store/tests/rls.rs:167:52:
called `Result::unwrap_err()` on an `Ok` value: RoleInfo { name: "test", superuser: true, bypass_rls: true }
test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

### M6 — KILLED

```
test rls_isolates_tenants ... FAILED
test app_role_cannot_update_events ... FAILED
thread 'rls_isolates_tenants' (856451) panicked at crates/store/tests/rls.rs:135:5:
assertion `left == right` failed
  left: 0
 right: 1
thread 'app_role_cannot_update_events' (856447) panicked at crates/store/tests/rls.rs:97:6:
called `Result::unwrap()` on an `Err` value: Database(PgDatabaseError { severity: Error, code: "42501", message: "new row violates row-level security policy for table \"events\"", ... })
test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
```

## Findings

### F1 (from M4): no test asserts the text of a policy

Location: `crates/store/tests/rls.rs`, test `rls_coverage_is_the_literal_list`, the second
query. It selects `c.relname` and `p.polname` only.

Add the predicate to that query and assert it against a literal string. Select
`pg_get_expr(p.polqual, p.polrelid)` and `pg_get_expr(p.polwithcheck, p.polrelid)`, then
assert that each of the 10 policies carries this exact text in both columns:

```
(user_id = (NULLIF(current_setting('app.user_id'::text, true), ''::text))::uuid)
```

The assertion also pins the `nullif` guard and the `true` missing-ok flag, so a later edit
of the predicate cannot pass in silence.

### F2 (additional gap, not a mutation survivor): the `nullif` guard has no test

`migrations/0006_grants_rls.sql` calls the `nullif(..., '')` guard load-bearing: after a
session sets `app.user_id`, the reset value of the GUC is the empty string, and a raw
`''::uuid` cast raises `22P02`. The suite proves the unset case only
(`rls_isolates_tenants`, `unbound_count == 0`). No test sets the GUC, resets it, and then
reads a table on the same connection.

Add a test that sets `app.user_id` at session level, runs `RESET app.user_id`, and asserts
a literal row count of `0` from `events` on that same connection. Without the guard, that
query raises SQLSTATE `22P02` instead.
