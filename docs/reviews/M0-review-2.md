# M0 adversarial review — round 2 (2026-08-25)

Run on the tree after FIX1–FIX5 (commit ce65602). Two find/refute rounds, major+ only: 32 raised, 16 confirmed. Assigned to fix units FIX7a (SQL grants + RLS tests), FIX7b (store crate), FIX7c (bins, CI, ops, docs).

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `.github/workflows/ci.yml:25` | FIX7c | CI gate can never pass: the CI Postgres requires a password and cadus_app has none, so every database-backed test fails to authenticate |
| 2 | blocker | `.github/workflows/ci.yml:25` | FIX7c (dup of #1) | CI gate can never pass: the FIX5 removal of trust auth locks the cadus_app test pool out, so the whole C2/C3 proof suite fails on every push |
| 3 | blocker | `crates/store/src/test_support.rs:83` | FIX7c (dup of #1) | The test harness only works under trust auth, so `cargo test` — and therefore the whole CI gate — fails on the `postgres:16` service that ci.yml declares |
| 4 | major | `migrations/0006_grants_rls.sql:68` | FIX7a | cadus_app rewrites another tenant's password_hash and grants itself is_admin: the #2 fix took DELETE and TRUNCATE off users but left UPDATE |
| 5 | major | `migrations/0006_grants_rls.sql:15` | FIX7a | model_call_log carries user_id and stays outside RLS, so any tenant's connection reads and erases the whole T6 ledger |
| 6 | major | `crates/web/src/bin/cadus-web.rs:148` | FIX7c | cadus-web hangs forever in pool.close() after the drain deadline, so the bounded shutdown of finding #9 is defeated |
| 7 | major | `crates/web/src/bin/cadus-web.rs:105` | FIX7c | cadus-web ignores SIGTERM and SIGINT while the C3 boot-guard query stalls |
| 8 | major | `crates/worker/src/bin/cadus-worker.rs:67` | FIX7c | cadus-worker ignores SIGTERM while the role-report query stalls |
| 9 | major | `crates/store/src/lib.rs:163` | FIX7b | The superuser half of the C3 boot guard is a self-oracle: deleting it keeps all 50 tests green and lets cadus-web boot on a role that reads every tenant |
| 10 | major | `crates/store/src/test_support.rs:106` | FIX7b | `TestDb::with` still leaks a throwaway database whenever setup panics, because `TestDb::create()` runs outside the guarded task |
| 11 | major | `crates/store/tests/migrate_bin.rs:31` | FIX7b | `migrate_bin.rs` mutates cluster-scoped roles under a process-local mutex, so two gate runs on the shared cluster fail with `tuple concurrently updated` |
| 12 | major | `scripts/check_ops.sh:50` | FIX7c | scripts/check_ops.sh passes on a compose file whose Dockerfile path and service command are both broken |
| 13 | major | `docker-compose.yml:32` | FIX7c | Rotating POSTGRES_PASSWORD in .env takes the running site down and cannot bring it back |
| 14 | major | `migrations/0006_grants_rls.sql:35` | FIX7a | The runtime role can rewrite an approved content_store body in place, so the C6 digest-to-approval binding that migrations/0005_content.sql claims does not exist |
| 15 | major | `crates/store/src/lib.rs:119` | FIX7b | The pool bounds only the acquire, so a database that answers no query wedges /api/ready and the worker loop without any bound |
| 16 | major | `crates/store/tests/rls.rs:270` | FIX7a | No test pins the live privilege ACL of cadus_app, so a TRUNCATE grant destroys every tenant and the suite stays green |

## FIX7a

### #4 [major] cadus_app rewrites another tenant's password_hash and grants itself is_admin: the #2 fix took DELETE and TRUNCATE off users but left UPDATE

File: `migrations/0006_grants_rls.sql:68` — IDs: C3

**Claim.** `users` is the one RLS-exempt table that keys every tenant, and the round-1 fix for blocker #2 revoked only DELETE and TRUNCATE, so the runtime role still holds table-wide UPDATE on it and a session bound to tenant A writes every column of tenant B's row — including `password_hash` and `is_admin` — with no policy in the path.

**Evidence.**

```
migrations/0006_grants_rls.sql:35 `GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cadus_app;` and :68 `REVOKE DELETE, TRUNCATE ON users FROM cadus_app;` — UPDATE is not revoked and no column list narrows it. docs/SCHEMA.md:45 states the rationale: "cadus_app keeps SELECT, INSERT, and UPDATE, because sign-up and sign-in touch `users` before a tenant context exists", which covers the caller's own row, not every row and not `is_admin`.

Live proof on a fresh database with migrations 0001-0006 (users a@x.test = tenant A, b@x.test = tenant B):
  $ psql -U cadus_app -d cadus2_rv_c2c3
  BEGIN;
  SELECT set_config('app.user_id','b79f5c80-...-137d6a820f52',true);   -- bound to tenant A
  UPDATE users SET password_hash='PWNED', is_admin=true WHERE id='73b1b801-...-82ab84637ff4';
  UPDATE 1
  COMMIT;
Superuser view afterwards:
  a@x.test|hashA|f
  b@x.test|PWNED|t
The same session is correctly stopped on the RLS tables: `INSERT INTO events (user_id,...) VALUES ('<tenant B>',...)` -> `ERROR: new row violates row-level security policy for table "events"`. Catalog: `has_table_privilege('cadus_app','users','UPDATE')` = t, `relrowsecurity`/`relforcerowsecurity` on users = f/f.
```

**Failure scenario.** M5 adds the password-reset handler. A defect passes the wrong id (the reset token's row is looked up before a tenant context exists, exactly the case the grant is justified by), or an injection reaches the same statement. The cadus_app connection runs `UPDATE users SET password_hash = $1 WHERE id = <another learner>` and succeeds: the victim's credential is replaced and the attacker signs in as that learner. The same grant sets `is_admin = true` on the attacker's own row, so one tenant escalates to the admin flag the schema defines. FORCE ROW LEVEL SECURITY on the twelve child tables gives no protection, because `users` carries no policy at all. The fix Postgres offers is column-level: `REVOKE UPDATE ON users FROM cadus_app;` then `GRANT UPDATE (password_hash, email_verified_at, disabled_at) ON users TO cadus_app;`, which at minimum takes `id` and `is_admin` out of reach.

**Refuter.** The claim is demonstrable and I reproduced it end to end on a fresh database with migrations 0001-0006. Three facts hold together, and each one is necessary and sufficient for the defect.

1. `users` carries no row-level security. `migrations/0006_grants_rls.sql:23` puts `users` outside the RLS set ("Tables with no user_id never enter the RLS set: users, auth_rate_counters, content_store"). A grep of all six migrations finds no `ENABLE ROW LEVEL SECURITY` and no `CREATE POLICY` for `users`. The catalog agrees: `relrowsecurity`/`relforcerowsecurity` = f/f, and `pg_policies` has 0 rows for `users`.

2. The runtime role keeps table-wide UPDATE. Line 35 grants `SELECT, INSERT, UPDATE, DELETE ON ALL TABLES` to `cadus_app`. Line 68 revokes `DELETE, TRUNCATE` only, with no column list on UPDATE. `has_table_privilege('cadus_app','users','UPDATE')` = t, `...,'DELETE')` = f. `cadus_app` is NOBYPAS

### #5 [major] model_call_log carries user_id and stays outside RLS, so any tenant's connection reads and erases the whole T6 ledger

File: `migrations/0006_grants_rls.sql:15` — IDs: C3, T6

**Claim.** `model_call_log` is the last table with a `user_id` column that has neither ENABLE/FORCE ROW LEVEL SECURITY nor a tenant_isolation policy, and its stated exemption reason — "operator telemetry; user_id is nullable" — is the same reason finding #15 rejected for `email_outbox`, which joined the RLS set with a nullable user_id; cadus_app therefore reads every tenant's per-learner model-call rows and deletes the entire cost ledger.

**Evidence.**

```
migrations/0006_grants_rls.sql:15 `--   model_call_log  -- operator telemetry; user_id is nullable`, repeated at docs/SCHEMA.md:79. Compare :185-189 of the same file, where email_outbox joins RLS despite a nullable user_id: "email_outbox.user_id is nullable (ON DELETE SET NULL), and a NULL user_id matches no tenant, so an orphaned row stays visible to cadus_admin only." Catalog on a fresh 0001-0006 database: `model_call_log|f|f|0` (relrowsecurity | relforcerowsecurity | policy count) and `model_call_log|DELETE,INSERT,SELECT,UPDATE` for grantee cadus_app.

Live proof, two rows seeded for tenant A and tenant B, then as cadus_app bound to tenant A:
  BEGIN; SELECT set_config('app.user_id','b79f5c80-...',true);
  SELECT user_id, session_id, output_tokens, cost_usd FROM model_call_log ORDER BY id;
  b79f5c80-...|sessA|600|0.004000
  73b1b801-...|sessB|600|0.004000      <-- the other tenant
  DELETE FROM model_call_log;
  DELETE 2
  COMMIT;
Superuser count afterwards: 0.
```

**Failure scenario.** M5 turns on A4 async diagnosis, so model_call_log fills with one row per diagnosis call carrying user_id, session_id, token counts, and cost_usd (0005_content.sql:52-66). Any read on the cadus_app connection that forgets a `WHERE user_id = ...` clause — a per-learner cost panel, a T4 cap query counting calls per session — returns every learner's rows, because no policy narrows it and the boot guard has nothing to say about a table with RLS off. A stray or injected `DELETE FROM model_call_log` on the same connection erases the whole T6 ledger for all tenants in one statement; the table is the only record of token spend, so the cost dashboard that T6 requires silently reports zero and the T3 per-KP authoring-cost alert loses its input.

**Refuter.** The claim is demonstrable. On a fresh 0001-0006 database, model_call_log carries no ENABLE/FORCE ROW LEVEL SECURITY and no policy, and cadus_app holds SELECT, INSERT, UPDATE, DELETE on it. As cadus_app inside a tenant-bound transaction, I read both tenants' rows and then deleted the whole table in one statement. The stated exemption (migrations/0006_grants_rls.sql:15, docs/SCHEMA.md:79) rests on "user_id is nullable", which is the same rationale the same file rejects at :185-189 for email_outbox: a NULL user_id matches no tenant, so an orphan row stays visible to cadus_admin only. That reason therefore does not hold model_call_log outside RLS, and the table holds per-learner data (user_id, session_id, token counts, cost_usd; 0005_content.sql:48-66), not operator-only data. This breaks C3 for the table and leaves the T6 ledger open to a one-statement wipe. Three refinements to the claim, 

### #14 [major] The runtime role can rewrite an approved content_store body in place, so the C6 digest-to-approval binding that migrations/0005_content.sql claims does not exist

File: `migrations/0006_grants_rls.sql:35` — IDs: C6, C5, D-S4

**Claim.** The blanket grant gives cadus_app INSERT, UPDATE and DELETE on content_store, and content_store is outside RLS, so the request-tier role can change the `body` of a human-approved digest in place and can insert new rows already marked status='approved' - the exact opposite of what migrations/0005_content.sql:9-12 states ("C6 binds approval to the digest, so an edited body is a new row that needs its own approval").

**Evidence.**

```
ACL after a full migrate (pg_class.relacl):
  content_store | test=arwdDxt/test | cadus_app=arwd/test | cadus_admin=arwd/test
(a=INSERT, r=SELECT, w=UPDATE, d=DELETE). No REVOKE follows for content_store, unlike events (line 56), users (line 68) and _sqlx_migrations (line 79).
Live proof, database built from migrations 0001-0006. Seeded as superuser:
  seeded: sha256:deadbeef status=approved body={"statement": "human-reviewed problem"}
Then as cadus_app on a tenant-bound transaction:
  BEGIN;
  SELECT set_config('app.user_id', <tenant A id>, true);
  UPDATE content_store SET body='{"statement":"UNREVIEWED text injected at runtime"}' WHERE digest='sha256:deadbeef';   -> UPDATE 1
  INSERT INTO content_store (digest,kp_id,kind,body,status) VALUES ('sha256:newone','kp.x','template','{"statement":"never reviewed"}','approved'); -> INSERT 0 1
  COMMIT;
State after, read as superuser:
  sha256:deadbeef | status=approved | approved_by_is_null=false | {"statement": "UNREVIEWED text injected at runtime"}
  sha256:newone   | status=approved | approved_by_is_null=true  | {"statement": "never reviewed"}
No test in the workspace reads or writes content_store at all: `grep -rn content_store crates/` matches nothing under crates/.
```

**Failure scenario.** M5/M6 add the serve path and the authoring pipeline. A defect or an injection on any request handler running as cadus_app executes `UPDATE content_store SET body = <attacker text> WHERE digest = <an approved template digest>`. The digest, the status 'approved', approved_by and approved_at are all unchanged, so every C6 check that looks at the approval record still passes, and the serve path hands unreviewed content to every learner on the instance. The same connection can also insert a brand-new row with status='approved' and approved_by NULL - no human ever saw it - or DELETE approved content and silently drop a knowledge point back to the A6 exemplar fallback.

**Refuter.** I could not refute the claim. I reproduced it twice, end to end, on a fresh database built from migrations 0001-0006 on the throwaway cluster. Every step of the reviewer's chain holds.

Fact 1 — the grant. /home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql:35 grants SELECT, INSERT, UPDATE, DELETE on ALL TABLES IN SCHEMA public to cadus_app. content_store exists from 0005_content.sql, so the grant covers it. The file revokes for three other tables (line 56 events, line 68 users, line 79 _sqlx_migrations) and never for content_store. The measured ACL is cadus_app=arwd/test.

Fact 2 — no second control exists. content_store has relrowsecurity=f, relforcerowsecurity=f, zero policies, and zero non-internal triggers. Its only constraints are the kind CHECK, the status CHECK, the digest PRIMARY KEY, and the approved_by FOREIGN KEY. No constraint ties digest to a hash of body. So an in-pla

### #16 [major] No test pins the live privilege ACL of cadus_app, so a TRUNCATE grant destroys every tenant and the suite stays green

File: `crates/store/tests/rls.rs:270` — IDs: C2, C3, U3

**Claim.** The suite asserts only four hand-picked negative privileges (UPDATE/DELETE on events, DELETE/TRUNCATE on users, ALL on _sqlx_migrations) and the ALTER DEFAULT PRIVILEGES entries; it never pins the actual ACL that migrations/0006_grants_rls.sql:35 hands cadus_app, so a widened grant of TRUNCATE — which row-level security does not cover at all — passes all 50 tests.

**Evidence.**

```
Mutation applied to migrations/0006_grants_rls.sql only:
  35: GRANT SELECT, INSERT, UPDATE, DELETE, TRUNCATE ON ALL TABLES IN SCHEMA public TO cadus_app;
  56: REVOKE UPDATE, DELETE ON events FROM cadus_app;
Result of `cargo test --workspace`:
  MU11_truncate_hole_on_events: *** SURVIVED ***   (50 passed, 0 failed)
Live proof on a database carrying that migration, connected as cadus_app with no tenant bound:
   current_user 
  --------------
   cadus_app
   rows_this_tenant_may_read 
  ---------------------------
                           0
  TRUNCATE TABLE
  TRUNCATE TABLE
           what         | count 
  ----------------------+-------
   after events         |     0
   after learner_models |     0
The only ACL-shaped assertion in the file is `default_privileges_are_the_literal_grants` (rls.rs:270), which reads pg_default_acl — future objects — and never reads has_table_privilege / relacl for the tables that exist.
```

**Failure scenario.** A later migration widens the blanket grant in 0006 (or a new migration adds TRUNCATE for a bulk-delete job). cargo test stays 50/50 green and the gate passes. In production cadus_app — the role cadus-web connects as, and the role the C3 boot guard certifies as RLS-bound — executes `TRUNCATE events; TRUNCATE learner_models;` and erases every tenant's append-only event log and derived state, even though the same connection reads 0 rows through the tenant policy. C2 (append-only events) and C3 (tenant isolation) are both lost with no test signal.

**Refuter.** The claim is demonstrable and I could not refute it. The test suite contains no assertion of the live ACL that migrations/0006_grants_rls.sql:35 hands cadus_app. A repo-wide grep finds no has_table_privilege, relacl, aclexplode, or information_schema privilege query in crates/*/tests/, scripts/gate.sh, or scripts/check_migrations.sh. The one ACL-shaped test, default_privileges_are_the_literal_grants at crates/store/tests/rls.rs:270, reads pg_default_acl, which covers objects a LATER migration creates, not the tables that exist. The other privilege assertions are four hand-picked negatives: UPDATE/DELETE on events, DELETE/TRUNCATE on users, and ALL on _sqlx_migrations. I reproduced the hole live on Postgres 16.14 with a mutation smaller than the reviewer's: I changed line 35 only and left line 56 (REVOKE UPDATE, DELETE, TRUNCATE ON events) intact. cadus_app then holds TRUNCATE (the D bit)

## FIX7b

### #9 [major] The superuser half of the C3 boot guard is a self-oracle: deleting it keeps all 50 tests green and lets cadus-web boot on a role that reads every tenant

File: `crates/store/src/lib.rs:163` — IDs: C3, U3, U4

**Claim.** `assert_rls_enforced` rejects on `info.superuser || info.bypass_rls`, but every test feeds it a role whose `rolbypassrls` is already true, so the `info.superuser ||` term is never the deciding one; a role created with `CREATE ROLE ... SUPERUSER` has `rolbypassrls = false` yet still bypasses RLS, and with that term removed the whole suite still passes while the guard admits it.

**Evidence.**

```
Only three roles ever reach the guard: `db.admin` (the bootstrap superuser `test`, `rolsuper=t rolbypassrls=t`) in rls.rs:426 and http.rs:202/232; the generated `cadus2_t_bypass_*` role (`rolsuper=f rolbypassrls=t`) in store_api.rs:74; and `cadus_app` (both false). The combination `rolsuper=t rolbypassrls=f` is tested nowhere. That combination is reachable:
```
$ psql -c "CREATE ROLE rv_su LOGIN SUPERUSER;" -c "SELECT rolname, rolsuper, rolbypassrls FROM pg_roles WHERE rolname IN ('test','rv_su')"
 rolname | rolsuper | rolbypassrls
---------+----------+--------------
 test    | t        | t
 rv_su   | t        | f
```
Mutation, applied to a copy of the tree — `if info.superuser || info.bypass_rls {` -> `if info.bypass_rls {`:
```
$ cargo test --workspace
test result: ok. 5 passed  (rls.rs -> 13 passed) ... every target ok, 0 failed
```
SURVIVED. The consequence, using the mutated `cadus-web` binary against a migrated database:
```
$ DATABASE_URL=postgresql://rv_su@127.0.0.1:55434/cadus2_rv_tq BIND_ADDR=127.0.0.1:19311 ./target/debug/cadus-web
INFO cadus_web: cadus-web: the boot guard passed role=rv_su
INFO cadus_web: cadus-web: listening address=127.0.0.1:19311
```
and that role does bypass the tenant policy:
```
as rv_su, no app.user_id:   events_visible = 2
as cadus_app, no app.user_id: events_visible = 0
```
Round 1 closed the mirror gap (#7/#11, BYPASSRLS without superuser) with `boot_guard_rejects_bypassrls_non_superuser`; the superuser-without-BYPASSRLS direction was left open.
```

**Failure scenario.** A later refactor of `assert_rls_enforced` (for example, folding the two flags into a single `rolbypassrls` lookup, or reordering the condition and dropping a term) removes the superuser test. `cargo test --workspace` stays fully green and the change merges. An operator then points `DATABASE_URL` at a role created with `CREATE ROLE cadus_app_admin LOGIN SUPERUSER` — common when someone works around a permission problem. `cadus-web` logs `the boot guard passed`, starts, and serves every request outside row-level security: one learner's session reads and writes every other learner's `events`, `learner_models`, and `serving_pool` rows. C3's stated invariant, "The app refuses to start if its DB role bypasses RLS", is silently gone.

**Refuter.** The claim is demonstrable and I confirmed every step of it independently. The guard text at crates/store/src/lib.rs:163 is correct, so there is no live bug in the shipped binary. The defect is a test gap: the `info.superuser ||` term of the C3 boot guard has no test that pins it. Three roles reach `assert_rls_enforced`: the bootstrap superuser `test` (rolsuper=t, rolbypassrls=t) in crates/store/tests/rls.rs:426 and crates/web/tests/http.rs:202 and :232, the generated `cadus2_t_bypass_*` role (f/t) in crates/store/tests/store_api.rs:74, and `cadus_app` (f/f). No test uses the combination rolsuper=t, rolbypassrls=f. Every rejecting test therefore rejects on `bypass_rls` alone, and the two tests that assert `superuser` assert the field inside the returned `StoreError::RlsBypass`, not the branch that fired. I removed the `info.superuser ||` term and the whole workspace suite stayed green, so

### #10 [major] `TestDb::with` still leaks a throwaway database whenever setup panics, because `TestDb::create()` runs outside the guarded task

File: `crates/store/src/test_support.rs:106` — IDs: U3

**Claim.** `TestDb::with` awaits `TestDb::create()` before it spawns the guarded task, so the `CREATE DATABASE` happens outside the region whose panic is converted into a `JoinError`; any panic inside `create` after the database exists (migration failure, app-pool failure) unwinds through `with` and the database is never dropped.

**Evidence.**

```
crates/store/src/test_support.rs:100-116 —
```
let db = Arc::new(TestDb::create().await);          // line 106: outside the guard
let outcome = tokio::spawn(body(Arc::clone(&db))).await;
db.drop_database().await;
```
`create` runs `CREATE DATABASE` at line 57 and then panics at line 73 (`migrate of {name} failed`) or line 86 (`app pool for {name} failed`). Demonstrated on a copy of the tree with one broken migration (`SELECT 1/0;` appended to 0006), running only `store_api.rs`, every test of which already uses `TestDb::with`:
```
thread 'boot_guard_rejects_bypassrls_non_superuser' panicked at crates/store/src/test_support.rs:73:33:
migrate of cadus2_t_b9f2ca50 failed: migration error: while executing migration 6: division by zero
...
test result: FAILED. 2 passed; 4 failed
--- leaked databases ---
cadus2_t_20e8bff0
cadus2_t_3c14d3b5
cadus2_t_735af03b
cadus2_t_b9f2ca50
```
Four databases from one binary. The same leak reproduces on an auth failure — the CI run in finding 1 left `cadus2_t_9a0781ea` behind. Converting rls.rs/http.rs/run.rs to `TestDb::with` (the fix unit already in flight) does not close this path.
```

**Failure scenario.** A developer edits a migration and introduces a SQL error, then runs `scripts/gate.sh`. `cargo test --workspace` panics inside `TestDb::create` for every database-backed test across four test binaries; roughly 25 throwaway databases are created and none is dropped. The developer fixes the SQL and re-runs the gate, which leaks another batch. On the shared cluster at 127.0.0.1:55434 the orphans accumulate until `CREATE DATABASE` and `DROP DATABASE` start contending — the state I observed during this review, with ~20 `DROP DATABASE ... WITH (FORCE)` statements stuck on `ProcSignalBarrier` and a single `migrate_bin` test binary reporting `finished in 973.31s`.

**Refuter.** The claim is correct and I reproduced it. In crates/store/src/test_support.rs, `TestDb::with` awaits `TestDb::create()` at line 106, outside the `tokio::spawn` region whose panic becomes a `JoinError`. `create` executes `CREATE DATABASE` at line 57 and then panics at line 69 (admin pool), line 73 (migrate) or line 86 (app pool). `TestDb` has no `Drop` impl, and `drop_database()` runs only at line 108 after `create` returns a value, so any panic inside `create` after the database exists unwinds through `with` and leaves the database on the shared cluster. The doc comment at lines 91-96, which promises a drop "in every case", does not hold for that window. I confirmed the leak empirically on a scratchpad copy of the tree with one broken migration. I did not verify the reviewer's severity numbers (~25 databases, the ProcSignalBarrier contention, the 973 s migrate_bin run), but those are fra

### #11 [major] `migrate_bin.rs` mutates cluster-scoped roles under a process-local mutex, so two gate runs on the shared cluster fail with `tuple concurrently updated`

File: `crates/store/tests/migrate_bin.rs:31` — IDs: U3

**Claim.** `ROLE_LOCK` is a `static` inside one test binary and serializes only the threads of that process, but `admin_login_flag_grants_the_login` and `admin_login_sets_the_app_password` run `ALTER ROLE cadus_admin NOLOGIN` and `ALTER ROLE cadus_app PASSWORD NULL` on roles that are cluster-scoped and shared by every concurrent run, so two `cargo test` runs on one cluster collide.

**Evidence.**

```
crates/store/tests/migrate_bin.rs:28-31 states the hazard and scopes the fix to one process: "Roles are cluster-scoped, and two `ALTER ROLE` statements on one role at the same time give \"tuple concurrently updated\". The tests of this file run in parallel, so every test that alters a role takes this lock first." The lock is `static ROLE_LOCK: tokio::sync::Mutex<()>`. Reproduced on the first attempt by starting the same test binary twice against one cluster:
```
$ timeout 120 $B admin_login > A.log 2>&1 & timeout 120 $B admin_login > B.log 2>&1 & wait
=== A
test result: ok. 2 passed; 0 failed
=== B
---- admin_login_sets_the_app_password stdout ----
thread 'admin_login_sets_the_app_password' panicked at crates/store/tests/migrate_bin.rs:168:9:
assertion `left == right` failed: stdout: stderr: cadus-migrate: database error: error returned from database: tuple concurrently updated
  left: Some(2)
 right: Some(0)
test result: FAILED. 1 passed; 1 failed
```
The preconditions at lines 99 and 144 (`assert!(!admin_can_login(&db))`, `assert!(!app_has_password(&db))`) fail the same way when the other run's `cadus-migrate` wins the race between the reset and the check. Concurrent runs on one cluster are the designed case elsewhere in the repo: migrations/0001_roles.sql:6-9 absorbs the lost `CREATE ROLE` race "because two databases on one cluster run this migration at the same time in tests", and scripts/check_migrations.sh:112-114 derives `<base>_migcheck_$$` so that "two gate runs on one cluster then never drop each other's database".
```

**Failure scenario.** Two developers — or, as docs/plans/M0.md prescribes, two units in parallel worktrees on this box — each run `scripts/gate.sh` against `postgresql://test:test@127.0.0.1:55434/...`. One run's `cadus-migrate --admin-login` issues `ALTER ROLE cadus_app PASSWORD ...` while the other has just issued `ALTER ROLE cadus_app PASSWORD NULL`; PostgreSQL raises `tuple concurrently updated`, the binary exits 2, and the test asserts exit code 0. The gate fails on a change that is correct, the developer re-runs it, and it passes — a red gate that carries no information, which is the exact failure mode HANDOVER.md §3 rejects.

**Refuter.** The claim is demonstrable and I reproduced it on the shared cluster. `static ROLE_LOCK: tokio::sync::Mutex<()>` at crates/store/tests/migrate_bin.rs:31 is process-local, but the objects it protects live in `pg_authid`, which is cluster-scoped. Two concurrent runs of the same test binary against postgresql://test:test@127.0.0.1:55434 failed in 4 of 5 rounds with "tuple concurrently updated" from `simple_heap_update` (heapam.c:4312). Both failure paths the reviewer named occurred: the exit-code assertion got Some(2) instead of Some(0) at lines 105 and 168, and the reset statement itself panicked at line 143. I looked for a contract that puts concurrent runs out of scope and found none that covers this. HANDOVER.md section 3 serializes gate runs, but docs/plans/M0.md:52-53 runs U3-U6 in parallel worktrees, and a unit's own `cargo test --workspace` is not a gate run. The repo elsewhere desig

### #15 [major] The pool bounds only the acquire, so a database that answers no query wedges /api/ready and the worker loop without any bound

File: `crates/store/src/lib.rs:119` — IDs: R4, M0-U4, M0-U5

**Claim.** cadus_store::connect() sets acquire_timeout and nothing else — no statement_timeout, no socket timeout, no TCP keepalive — so once a connection is checked out of the pool every query waits without bound; /api/ready therefore never delivers the 503 it promises, and the worker tick loop stops for good, both while the process stays alive and reports nothing.

**Evidence.**

```
crates/store/src/lib.rs:116-124 is the whole bound:

    pub async fn connect(cfg: &DbConfig) -> Result<PgPool, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect(&cfg.database_url)

The doc comment on ACQUIRE_TIMEOUT (crates/store/src/lib.rs:105-113, the round-1 fix for finding #30) claims this covers the outage case: "the default holds the probe open for 30 s during a database outage, and a scraper with a shorter client timeout records a timeout in place of the 503 that the handler promises. 5 s is longer than a normal connect ...". `grep -rn "statement_timeout|connect_timeout|keepalive|options=" crates/ migrations/ docker-compose.yml .env.example` returns nothing.

Demonstration. I put a freezable TCP proxy on 127.0.0.1:55499 in front of the test cluster (it keeps every socket open and relays no bytes — a hung backend or a stateful firewall that drops packets), migrated cadus2_rv_unsafe, and ran the real binaries.

(a) The boot path IS bounded, which isolates the defect: frozen before start, cadus-web exits in 6 s —
    cadus-web: database error: pool timed out while waiting for an open connection ; exit=2 after 6s

(b) The request path is NOT. Frozen after the boot guard warmed the pool:
    warmup: {"ready":true}
    --- proxy frozen (DB reachable, never answers)
    http_code=000 total=45.002638      (curl -m 45 gave up; the handler never answered)
    WARN sqlx::query: slow statement ... summary="SELECT 1 AS \"one!\"" elapsed=45.001315675s
Repeated with a 30 s client limit, cold and warm pool: http_code=000 total=30.002185 and total=30.002399, sqlx logging elapsed=30.001133481s and elapsed=30.001424315s. /api/health kept answering {"ok":true} the whole time, so the container looks healthy.

(c) crates/worker/src/lib.rs:161-169 has the same unbounded await. Frozen mid-run, WORKER_TICK_SECS=1, left alone 40 s:
    heartbeat tick=1 / tick=2 / tick=3   <- last line ever written
    --- alive? yes ; tick lines total: 3

The test that is supposed to pin this (crates/web/tests/http.rs:289-311, ready_returns_503_on_a_closed_pool) uses a closed pool, which errors instantly, so it passes identically whether the probe is bounded or not.
```

**Failure scenario.** The compose stack runs; the `db` container hangs (a stuck checkpoint, an OOM-throttled container, or a stateful firewall between web and db that silently drops packets while both ends keep the TCP connection open). cadus-web's pool already holds warm connections, so acquire_timeout never fires. Every GET /api/ready then blocks forever instead of returning the documented 503 with {"ready":false}: I measured 30 s and 45 s waits ended only by the client giving up (http_code=000), while sqlx logged the same SELECT 1 running 30.0 s and 45.0 s. An operator scraping web:8080/api/ready per docs/SELF_HOST.md records client timeouts, not a 503, so the outage reads as a monitoring fault. Because each stalled probe holds one of the 16 pool connections, roughly 16 scrapes wedge the whole pool. In the same outage cadus-worker stops silently: after tick=3 it wrote no further line for 40 s (indefinitely), kept the container in the `up` state, exited nothing, and logged no error — the R4 async layer is dead with no signal anywhere. Nothing in the process recovers when the network heals mid-query, and no test in the gate distinguishes this from correct behavior.

**Refuter.** I reproduced the defect independently and could not refute it. crates/store/src/lib.rs:116-124 sets only max_connections(16) and acquire_timeout(5s); no statement_timeout, no connect/socket timeout, and no TCP keepalive exists anywhere in crates/, migrations/, docker-compose.yml, or .env.example. acquire_timeout bounds the checkout only. After checkout, both crates/web/src/lib.rs:60-73 and crates/worker/src/lib.rs:161-169 await the query with no deadline, so a database that accepts the socket and never answers holds the probe forever. With a warm pool and a frozen TCP proxy, three consecutive GET /api/ready calls each ran the full 20 s client limit and returned no status, while /api/health kept answering 200. With the response path dropped mid-query, cadus-worker wrote heartbeat tick=3 and then stayed alive and silent for 45 s with no error and no exit. The pinning test crates/web/tests/

## FIX7c

### #1 [blocker] CI gate can never pass: the CI Postgres requires a password and cadus_app has none, so every database-backed test fails to authenticate

File: `.github/workflows/ci.yml:25` — IDs: R2, C3, D9

**Claim.** The CI `postgres:16` service starts with `POSTGRES_PASSWORD` set and no `POSTGRES_HOST_AUTH_METHOD`, so its generated `pg_hba.conf` ends with `host all all all scram-sha-256`; the test harness connects as `cadus_app` with an empty password (`crates/store/src/test_support.rs:82-83`) and migration `0001_roles.sql:35` creates that role with no password, so `TestDb::create()` panics with `password authentication failed for user "cadus_app"` and `cargo test --workspace` inside `scripts/gate.sh` fails on every push and pull request.

**Evidence.**

```
ci.yml:19 comment states the premise that is false: "# password, so the container needs no `trust` auth." — the DSN carries a password for `test`, but the tests connect as `cadus_app`.
migrations/0001_roles.sql:35  `CREATE ROLE cadus_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;`  (no PASSWORD clause anywhere in the repo)
crates/store/src/test_support.rs:82-83  `.username("cadus_app")` / `.password("")`

Reproduced with a container started exactly as the CI service block does:
$ docker run -d --name cadus2rvci -e POSTGRES_USER=test -e POSTGRES_PASSWORD=test -p 127.0.0.1:55501:5432 postgres:16
$ docker exec cadus2rvci cat /var/lib/postgresql/data/pg_hba.conf | grep -v '^#' | grep -v '^$'
local   all             all                                     trust
host    all             all             127.0.0.1/32            trust
host    all             all             ::1/128                 trust
local   replication     all                                     trust
host    replication     all             127.0.0.1/32            trust
host    replication     all             ::1/128                 trust
host all all all scram-sha-256          <-- every connection that is not loopback-inside-the-container

$ CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55501/postgres cargo test -p cadus-store --test rls
thread 'rls_isolates_tenants' panicked at crates/store/src/test_support.rs:86:33:
app pool for cadus2_t_172540b8 failed: error returned from database: password authentication failed for user "cadus_app" at line 331
...
failures:
    app_role_cannot_delete_users
    app_role_cannot_touch_the_migration_ledger
    app_role_cannot_update_events
    app_role_inserts_into_model_call_log
    boot_guard
    default_privileges_are_the_literal_grants
    events_attempt_idem_is_unique
    migrate_is_idempotent
    reset_guc_yields_zero_rows
    rls_coverage_is_the_literal_list
    rls_covers_the_worker_queues
    rls_isolates_tenants
    role_attributes_match_the_migration
test result: FAILED. 0 passed; 13 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.05s

The identical command against the local trust-auth cluster passes:
$ CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/postgres cargo test --workspace
test result: ok. 13 passed; 0 failed  (rls.rs), ok. 6 passed (store_api.rs), ok. 5 passed (worker run.rs), ...

The scram requirement is not an artifact of my client: a peer container gets the same answer.
$ docker run --
```

**Failure scenario.** A developer pushes any commit. The `gate` job installs the toolchain and sqlx-cli, creates `cadus2_ci`, migrates it (both as the `test` superuser, which does carry a password, so those steps pass), then runs `scripts/gate.sh`. Step 1 (fmt) and step 2 (clippy) pass because they compile offline from `.sqlx`. Step 3, `cargo test --workspace` (gate.sh:41), reaches the first `TestDb::create()` and dies: all 13 tests in `crates/store/tests/rls.rs`, the 5 database tests in `store_api.rs`, the 6 in `migrate_bin.rs`, the 5 database tests in `crates/web/tests/http.rs`, and 3 in `crates/worker/tests/run.rs` panic with `password authentication failed for user "cadus_app"`. The job exits non-zero, so the merge gate that HANDOVER.md §2 stage 3 calls non-negotiable is red on 100% of pushes and pull requests and gives no signal about the code — the same class of failure as round-1 blockers #1/#3/#4, which fixed the missing migrate step but left the auth method. Each failed test also leaks its `cadus2_t_*` database because `TestDb::create` panics before any drop. The one-line fix is `POSTGRES_HOST_AUTH_METHOD: trust` in the service `env:` block (matching the local throwaway cluster the harness documents at test_support.rs:41), or giving `cadus_app` a password in CI and threading it through `TestDb` and `dsn_for`.

**Refuter.** The claim is correct and I reproduced it in full. The CI service block at /home/deploy/dev/cadus2.0/.github/workflows/ci.yml:21-26 starts `postgres:16` with POSTGRES_USER/POSTGRES_PASSWORD and no POSTGRES_HOST_AUTH_METHOD. The image entrypoint then appends `host all all all scram-sha-256` to pg_hba.conf. A GitHub runner job (`runs-on: ubuntu-latest`, no `container:`) reaches the service through the published port, so the server sees the connection from the docker bridge gateway, not from 127.0.0.1 inside the container. Only the appended scram line matches. /home/deploy/dev/cadus2.0/migrations/0001_roles.sql:35 creates `cadus_app` with no PASSWORD clause, and `crates/store/src/test_support.rs:78-86` opens the app pool with `.username("cadus_app").password("")`. `crates/store/src/lib.rs:129` (`migrate`) only runs the migrator; it sets no password. The password path exists only in `crates/s

### #2 [blocker] CI gate can never pass: the FIX5 removal of trust auth locks the cadus_app test pool out, so the whole C2/C3 proof suite fails on every push

File: `.github/workflows/ci.yml:25` — IDs: C2, C3 — duplicate of #1

**Claim.** Commit c738ced (FIX5) deleted `POSTGRES_HOST_AUTH_METHOD: trust` from the CI Postgres service, but `TestDb::create` still opens the second pool as `cadus_app` with an empty password and migration 0001 never gives that role a password, so every test that builds a TestDb dies at setup with `password authentication failed for user "cadus_app"` and the entire append-only / RLS / boot-guard proof suite never runs in CI.

**Evidence.**

```
ci.yml:18-27 — comment `# A throwable Postgres 16 ... The DSN carries the password, so the container needs no \`trust\` auth.` with `POSTGRES_USER: test` / `POSTGRES_PASSWORD: test` and no `POSTGRES_HOST_AUTH_METHOD`. `git log -p .github/workflows/ci.yml` shows commit c738ced ("M0 FIX5") removing the line `-          POSTGRES_HOST_AUTH_METHOD: trust`.

crates/store/src/test_support.rs:75-86 still assumes trust: `// Trust authentication accepts the empty password of the app role.` ... `.username("cadus_app").password("")`. migrations/0001_roles.sql:34 creates the role with no password: `CREATE ROLE cadus_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;`.

Reproduction of the CI service on this box (`docker run -e POSTGRES_USER=test -e POSTGRES_PASSWORD=test -p 127.0.0.1:55521:5432 postgres:16`), pg_hba.conf generated by the image:
  local   all  all                        trust
  host    all  all   127.0.0.1/32         trust
  host all all all scram-sha-256          <-- the line a runner-to-service-container connection matches

Migrations applied as the `test` superuser (Applied 1..6), then:
  $ SQLX_OFFLINE=true CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55521/cadus2_ci cargo test -p cadus-store --test rls -- boot_guard
  test boot_guard ... FAILED
  thread 'boot_guard' panicked at crates/store/src/test_support.rs:86:33:
  app pool for cadus2_t_6bee2fa1 failed: error returned from database: password authentication failed for user "cadus_app"
  $ SQLX_OFFLINE=true CADUS_TEST_DATABASE_URL=... cargo test -p cadus-store
  test result: FAILED. 0 passed; 5 failed   (migrate_bin binary; the run aborts there)
```

**Failure scenario.** A developer pushes any commit. The gate job starts the postgres service with password auth, creates and migrates `cadus2_ci` as the `test` superuser (both steps pass, because that DSN carries a password), then runs `scripts/gate.sh`. fmt and clippy pass. `cargo test --workspace` reaches the first `TestDb::create()` and panics with `password authentication failed for user "cadus_app"`, so gate.sh exits non-zero at step 3. Every M0 test that proves a C2 or C3 invariant — app_role_cannot_update_events, app_role_cannot_delete_users, app_role_cannot_touch_the_migration_ledger, rls_isolates_tenants, rls_covers_the_worker_queues, rls_coverage_is_the_literal_list, boot_guard, boot_guard_rejects_bypassrls_non_superuser, reset_guc_yields_zero_rows — never reports a verdict in CI. The merge gate is red on every push and pull request, so it gives no signal at all, and each failed run leaks a `cadus2_t_*` database on the service cluster.

**Refuter.** The defect is real and demonstrable. Commit c738ced removed `POSTGRES_HOST_AUTH_METHOD: trust` from the CI Postgres service, but no other change gave the `cadus_app` role a password, and the test harness still connects as `cadus_app` with an empty password. The postgres:16 image writes `host all all all scram-sha-256` as the last pg_hba rule. A connection from the runner to a published port arrives through the docker bridge gateway, not 127.0.0.1, so it matches that rule and not the `127.0.0.1/32 trust` rule. Migration 0001 creates `cadus_app` with `LOGIN` and no password, so `rolpassword` is NULL and scram authentication always fails. `cadus-migrate --admin-login` is the only code that sets the password (ALTER ROLE cadus_app PASSWORD ...), and the compose stack calls it; the CI job does not. gate.sh runs `cargo test --workspace` as step 3, before `sqlx prepare --check`, `check_migration

### #3 [blocker] The test harness only works under trust auth, so `cargo test` — and therefore the whole CI gate — fails on the `postgres:16` service that ci.yml declares

File: `crates/store/src/test_support.rs:83` — IDs: C2, C3, U5 — duplicate of #1

**Claim.** `TestDb::create` opens the `cadus_app` pool with a hard-coded empty password, but `.github/workflows/ci.yml` starts `postgres:16` with `POSTGRES_PASSWORD` and no `POSTGRES_HOST_AUTH_METHOD`, which makes the image write `host all all all scram-sha-256`; `cadus_app` is created with no password by `migrations/0001_roles.sql`, so every test that builds a `TestDb` dies in setup and the C2/C3 proof suite never runs in CI.

**Evidence.**

```
crates/store/src/test_support.rs:76-86 — `let app = PgPoolOptions::new().max_connections(4).connect_with(maintenance.clone().database(&name).username("cadus_app").password(""))`.

Started the exact CI service: `docker run -d --name cadus2-ciauth -e POSTGRES_USER=test -e POSTGRES_PASSWORD=test -p 127.0.0.1:55499:5432 postgres:16`. Its generated pg_hba.conf ends with the rule that covers every non-loopback-inside-container client:
```
host all all all scram-sha-256
```
Then ran the real test binary against it with the same DSN shape ci.yml uses (`postgresql://test:test@host:port/...`):
```
$ CADUS_TEST_DATABASE_URL="postgresql://test:test@127.0.0.1:55499/postgres" ./target/debug/deps/rls-b1589c07c6fc0e6c boot_guard
test boot_guard ... FAILED
thread 'boot_guard' panicked at crates/store/src/test_support.rs:86:33:
app pool for cadus2_t_9a0781ea failed: error returned from database: password authentication failed for user "cadus_app"
test result: FAILED. 0 passed; 1 failed
```
Nothing in ci.yml, README.md, or docs/SELF_HOST.md sets `POSTGRES_HOST_AUTH_METHOD: trust` or gives `cadus_app` a password before `cargo test` runs. The suite passes locally only because `cadus2-testdb` on this box happens to run trust auth.
```

**Failure scenario.** A push or pull request runs the `gate` job. `Create the gate database` and `Migrate the gate database` succeed as the superuser `test`. `scripts/gate.sh` then reaches `cargo test --workspace`; the first test that calls `TestDb::create` panics with `password authentication failed for user "cadus_app"`, every RLS, append-only, boot-guard, HTTP, and worker test that needs a database fails the same way, and the gate exits non-zero on every commit. The C2 append-only proof and the C3 tenant-isolation proof are never executed in CI at all.

**Refuter.** I tried to refute the claim and failed. The claim is demonstrable on this machine. Three facts hold together. First, migrations/0001_roles.sql line 35 creates cadus_app with LOGIN and no password, and no other file gives that role a password before the tests run. Only crates/store/src/bin/cadus-migrate.rs sets a password, through --admin-login plus CADUS_APP_PASSWORD, and .github/workflows/ci.yml never calls it. Second, .github/workflows/ci.yml starts postgres:16 with POSTGRES_USER and POSTGRES_PASSWORD and no POSTGRES_HOST_AUTH_METHOD. I started the same container and read its generated pg_hba.conf: the last rule is "host all all all scram-sha-256". The same image on this box with POSTGRES_HOST_AUTH_METHOD=trust writes "host all all all trust", which explains why the suite is green locally. Third, in a GitHub Actions host job the client is on the runner, so the server sees the docker br

### #6 [major] cadus-web hangs forever in pool.close() after the drain deadline, so the bounded shutdown of finding #9 is defeated

File: `crates/web/src/bin/cadus-web.rs:148` — IDs: C3

**Claim.** The round-1 fix for finding #9 bounds only the axum::serve future; the pool.close().await that follows it waits for every checked-out pool connection and has no bound, so a database that stops answering makes cadus-web run forever after it has already logged that the shutdown deadline was reached.

**Evidence.**

```
sqlx-core-0.9.0/src/pool/inner.rs:110 `let _permits = self.semaphore.acquire(permits_to_acquire).await;` -- close() blocks until all 16 permits (max_connections) come back, i.e. until every checked-out connection is released.

Live run, SHUTDOWN_DEADLINE_SECS=1, 40 concurrent /api/ready clients, the TCP link to Postgres blackholed at the moment of SIGTERM:
  13:18:04.324 INFO cadus_web: cadus-web: SIGTERM received
  13:18:04.324 INFO cadus_web: cadus-web: graceful shutdown starts
  13:18:05.325 INFO cadus_web: shutdown deadline reached; closing
  <nothing after this line>
  $ ps -o etime= -p $W   ->  00:55        (still running)
  $ grep -c "cadus-web: stopped" w4.err  ->  0
A second run (backends SIGSTOPped instead of a partition) stayed alive 2 min 36 s and ignored a second SIGTERM and a SIGINT:
  alive after first SIGTERM+5s? yes / alive after second SIGTERM? yes / alive after SIGINT? yes
Control, identical load with no database fault: `exited after 1s` and `cadus-web: stopped`.
After that log line the only remaining await in run() is `pool.close().await` (line 148); line 149 has none.
```

**Failure scenario.** Operator runs `docker compose down` or `docker compose up -d` for an upgrade while the db container is restarting or the backend network is partitioned, with a few /api/ready or future M5 requests in flight. cadus-web logs `shutdown deadline reached; closing` at SHUTDOWN_DEADLINE_SECS (default 10 s) and then blocks in pool.close(). docker-compose.yml:92-94 sets `stop_grace_period: 20s` with the comment "the process exits by itself and Docker never has to send SIGKILL (exit 137)"; instead Docker SIGKILLs the container at 20 s and the service reports exit 137 on every such restart. The operator cannot shorten this: a second SIGTERM and a SIGINT are both swallowed, because tokio's signal handler is installed but the Shutdown struct that listened for it was consumed by the graceful-shutdown block.

**Refuter.** The defect is real and I reproduced it twice. I built the current source into a private target directory, ran the binary against a real Postgres 16 through a TCP proxy, and blackholed the server-to-client direction while queries were in flight. In both runs cadus-web logged "shutdown deadline reached; closing" one second after SIGTERM and then printed nothing more. The process stayed alive past 120 s, ignored a second SIGTERM and a SIGINT, and never printed "cadus-web: stopped". Only an external SIGKILL ended it. The mechanism is exactly the one in the claim. After line 142 of /home/deploy/dev/cadus2.0/crates/web/src/bin/cadus-web.rs the sole remaining await in run() is pool.close().await at line 148; line 149 holds only a map_err, and main() prints "cadus-web: stopped" the moment run() returns. The absence of that line proves that the process waits inside pool.close(). sqlx-core 0.9.0 c

### #7 [major] cadus-web ignores SIGTERM and SIGINT while the C3 boot-guard query stalls

File: `crates/web/src/bin/cadus-web.rs:105` — IDs: C3

**Claim.** boot_check(&pool) is awaited outside the shutdown select of lines 93-102, and neither sqlx nor the store sets any statement timeout, so a database that accepts the connection but never answers the pg_roles query leaves cadus-web unkillable by SIGTERM or SIGINT for as long as the stall lasts.

**Evidence.**

```
A TCP proxy that forwards the first 600 bytes DB->app (enough for the startup handshake) and then blackholes the link:
  $ ss -tlnp | grep 18130   ->  (nothing)   # TcpListener::bind at line 111 never reached
  $ cat c.err               ->  (empty)     # the `cadus-web: the boot guard passed` log at line 106 never printed
  $ ss -tanp | grep pid=... ->  ESTAB 127.0.0.1:36108 -> 127.0.0.1:55601 fd=9
  SIGTERM sent 4 s after start  ->  "STILL ALIVE 25 s after SIGTERM";  ps elapsed 00:54
Control, same binary and same proxy with a 50-byte limit so the stall lands inside connect() instead:
  exit=0, log: "cadus-web: SIGTERM received" / "cadus-web: the stop signal came before the database connect" / "cadus-web: stopped"
The control proves the signal handlers are installed and the connect-time race works; the only await between that select and the missing log line is boot_check at line 105.
```

**Failure scenario.** A `docker compose up -d` lands while Postgres accepts connections but is not answering (crash recovery, a stuck checkpoint, or a `backend` network partition after the TCP handshake). cadus-web passes connect(), then blocks in the C3 boot guard with no listener bound and no log line. `docker compose down`, `docker compose restart web`, and a manual `kill -TERM` are all ignored, so every stop of the container costs the full stop_grace_period (20 s) and ends in SIGKILL/exit 137. The module header at lines 8-9 promises the opposite: "The handlers exist before the pool opens, so a signal during the connect gives a clean stop."

**Refuter.** The claim is correct and I reproduced it end to end. In /home/deploy/dev/cadus2.0/crates/web/src/bin/cadus-web.rs the shutdown select covers only cadus_store::connect (lines 93-102). Line 105 awaits cadus_web::boot_check(&pool) bare, with no select arm on shutdown.wait() and no tokio::time::timeout. Nothing in the path bounds that await: cadus_store::connect at crates/store/src/lib.rs:114-123 sets only max_connections(16) and acquire_timeout(ACQUIRE_TIMEOUT) — no connect options, no after_connect, no statement_timeout; DbConfig::from_env (crates/store/src/lib.rs:44-58) passes DATABASE_URL through untouched; and a grep for statement_timeout and for "ALTER ROLE ... SET" over migrations, scripts, docs, Dockerfile, and docker-compose.yml finds no server-side timeout for cadus_app either. boot_check calls cadus_store::assert_rls_enforced, which calls current_role, a plain fetch_one of the pg_

### #8 [major] cadus-worker ignores SIGTERM while the role-report query stalls

File: `crates/worker/src/bin/cadus-worker.rs:67` — IDs: R4, C3

**Claim.** cadus_store::current_role(&pool) is awaited between the connect select (lines 52-59) and the tick loop that owns the shutdown future, so a database that answers no query leaves cadus-worker deaf to SIGTERM -- the same defect that finding #10 fixed inside the loop, one statement earlier.

**Evidence.**

```
Same 600-byte proxy (handshake forwarded, every query response blackholed), DATABASE_URL pointing at it, WORKER_TICK_SECS=1, RUST_LOG=info:
  $ cat e.err   ->  (empty)   # the `cadus-worker: database role` log at line 68 never printed, so run() at line 75 was never reached
  SIGTERM sent 4 s after start  ->  "WORKER STILL ALIVE 20 s after SIGTERM";  ps elapsed 00:56
By contrast, with the same proxy frozen only after the loop had started, the worker stopped in 0 s: "cadus-worker: SIGTERM received" / "worker: loop stops after 3 ticks" / "cadus-worker: stop after 3 ticks".
```

**Failure scenario.** The db container is restarting or the `backend` network drops packets when `docker compose up -d` starts the worker. cadus-worker blocks on the pg_roles query and logs nothing at all. docker-compose.yml gives the `worker` service no stop_grace_period, so Docker's default of 10 s elapses on the next stop and the container is SIGKILLed with exit 137; a `docker compose restart worker` during the outage hangs for that whole grace period. crates/worker/tests/run.rs test (5) pins only the connect-time signal path, so no test covers this window.

**Refuter.** The claim holds; I could not refute it. crates/worker/src/bin/cadus-worker.rs:67 awaits cadus_store::current_role(&pool) bare, between the shutdown-guarded connect select (lines 52-59) and the tick loop that owns the shutdown future (line 75). In that window nothing polls Shutdown::wait, and tokio's installed SIGTERM handler replaced the default disposition, so the process neither stops nor dies. I reproduced the reported behavior and the contrast case. connect() (crates/store/src/lib.rs:115-123) sets acquire_timeout(ACQUIRE_TIMEOUT = 5 s) and sets no statement timeout; that 5 s bound covers the pool acquire only, not the query. One sub-case self-limits: if the blackhole starts right after the handshake, the acquire liveness ping is lost and the acquire fails after 5 s with "pool timed out while waiting for an open connection" (exit 2). The other sub-case does not: if the acquire succeed

### #12 [major] scripts/check_ops.sh passes on a compose file whose Dockerfile path and service command are both broken

File: `scripts/check_ops.sh:50` — IDs: U6, D9

**Claim.** check_ops.sh builds with `docker build` and never runs `docker compose build`, and `docker compose config` validates neither the `dockerfile:` path nor the `command:` binary name, so the gate stays green on exactly the two mistakes the script's own header says it catches.

**Evidence.**

```
scripts/check_ops.sh:5-8 states "a renamed binary target or a broken compose key first appears on the operator's server. This script runs both in the gate instead." README.md:31-33 and docs/SELF_HOST.md:100-103 repeat the claim. I edited a scratch copy of the tree: docker-compose.yml:16 `dockerfile: Dockerfile` -> `dockerfile: Dockerfil`, and docker-compose.yml:85 `command: ["cadus-web"]` -> `command: ["cadus-webb"]`. Then:
  $ bash scripts/check_ops.sh
  PASS: compose -- docker compose config resolves docker-compose.yml
  PASS: build   -- docker build tagged cadus2:gate
  check_ops rc=0
The operator's command on the same tree:
  $ docker compose build web
  unable to prepare context: unable to evaluate symlinks in Dockerfile path: lstat .../Dockerfil: no such file or directory
`docker build -t cadus2:gate .` passes because it reads ./Dockerfile directly and never consults docker-compose.yml.
```

**Failure scenario.** A developer renames the Dockerfile, mistypes the `dockerfile:` key, or renames a binary target and updates the Dockerfile COPY but not the compose `command:`. scripts/gate.sh prints GATE OK, CI is green, and the change merges. The operator then runs the documented `docker compose up -d --build` and gets either `unable to prepare context ... no such file or directory` at build time or an `exec: "cadus-webb": executable file not found in $PATH` crash loop at run time. The gate that exists to move this failure off the operator's server does not detect it.

**Refuter.** The defect is demonstrable, so I cannot refute it. scripts/check_ops.sh runs only `docker compose config` (line 37) and `docker build -t cadus2:gate .` (line 50). I reproduced the hole with a minimal compose file on this machine (docker 29.6.2, compose v5.3.1): with `dockerfile: Dockerfil` (typo, ./Dockerfile present) and `command: ["cadus-webb"]`, `docker compose config` returns 0 and `docker build .` returns 0, while `docker compose build web` fails with `unable to prepare context: unable to evaluate symlinks in Dockerfile path`. The reason is mechanical: `docker compose config` interpolates variables and validates keys, but it does not stat `build.dockerfile` and treats `command:` as opaque strings; `docker build .` reads ./Dockerfile directly and never opens docker-compose.yml. The gate therefore stays green on the documented operator path `docker compose up -d --build` (docker-compo

### #13 [major] Rotating POSTGRES_PASSWORD in .env takes the running site down and cannot bring it back

File: `docker-compose.yml:32` — IDs: C3, U6

**Claim.** `POSTGRES_PASSWORD` only reaches the database at initdb time. On an existing `db-data` volume a changed value is ignored by the `db` container but is written into the `migrate` DSN, so `docker compose up -d` recreates and destroys the running `web` and `worker` containers, then leaves them stopped forever because `migrate` cannot authenticate.

**Evidence.**

```
Live run on the shipped compose file (services db, migrate, web, worker; project rvops). Before rotation:
  db Up 3 minutes (healthy) / migrate Exited (0) / web Up 3 minutes / worker Up
Then POSTGRES_PASSWORD, CADUS_APP_PASSWORD and CADUS_ADMIN_PASSWORD were changed in the environment and `docker compose up -d` was re-run:
  Container rvops-web-1 Recreated ; Container rvops-worker-1 Recreated
  Container rvops-migrate-1 Error service "migrate" didn't complete successfully: exit 2
  db Up 6 seconds (healthy) / migrate Exited (2) / web Created / worker Created
  migrate-1 | cadus-migrate: database error: error returned from database: password authentication failed for user "postgres"
The `postgres` superuser password lives only in the volume, and cadus-migrate (crates/store/src/bin/cadus-migrate.rs:151-160) can only ALTER ROLE for cadus_app and cadus_admin after it authenticates as `postgres`, so the stack cannot self-heal. .env.example:24-36 and docs/SELF_HOST.md:15-21 present all three passwords as one set the operator generates and sets in .env, and docs/SELF_HOST.md:67-69 states "a changed password in `.env` reaches the database on the next `docker compose up -d`". No file warns that POSTGRES_PASSWORD is write-once.
```

**Failure scenario.** An operator does a routine credential rotation: `openssl rand -hex 24` three times, new values into .env, `docker compose up -d`. Compose destroys the healthy `web` and `worker` containers, `migrate` exits 2 on an authentication error, and the site is down. Nothing in .env.example, README.md, or docs/SELF_HOST.md tells the operator that the superuser password is fixed at first initdb, so the obvious recovery (`docker compose down -v`) destroys every learner's data.

**Refuter.** The claim is demonstrable and I reproduced it exactly on the shipped compose file. POSTGRES_PASSWORD reaches the database only at initdb. On an existing db-data volume a rotated value is ignored by the db container but is interpolated into the migrate DSN at docker-compose.yml:63, so `docker compose up -d` recreates web and worker, migrate exits 2 on `password authentication failed for user "postgres"`, and web and worker stay in `Created` across repeated `up -d` runs. The site stays down until manual intervention. No shipped file warns that POSTGRES_PASSWORD is write-once. Two parts of the claim are overstated and need correction, but they do not save the defect. (1) Recovery does not need `docker compose down -v` and loses no data: the postgres:16 image writes `local all all trust` into pg_hba.conf, so one `docker compose exec -T db psql -U postgres -d cadus -c "ALTER USER postgres PAS
