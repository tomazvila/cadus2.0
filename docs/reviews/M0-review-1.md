# M0 adversarial review — round 1 (2026-08-25)

Source: HANDOVER.md stage 4. Six lenses, four find/refute rounds, 104 findings raised, 46 confirmed by an independent refuter. Each confirmed finding is assigned to one fix unit. Duplicates point at their primary.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `.github/workflows/ci.yml:55` | FIX5 | CI gate can never pass: the gate database is created but never migrated |
| 2 | blocker | `migrations/0006_grants_rls.sql:31` | FIX2 | cadus_app destroys another tenant's RLS-protected rows through the users FK cascade |
| 3 | blocker | `.github/workflows/ci.yml:54` | FIX5 (dup of #1) | CI never migrates cadus2_ci, so `cargo sqlx prepare --check` fails on every gate run |
| 4 | blocker | `.github/workflows/ci.yml:55` | FIX5 (dup of #1) | CI creates the gate database but never migrates it, so the gate job fails on every push and PR |
| 5 | blocker | `crates/store/tests/rls.rs:144` | FIX3 | The transaction-local scope of the tenant GUC is never asserted: set_config(..., true) → false survives the whole suite |
| 6 | blocker | `crates/store/src/test_support.rs:68` | FIX2 | The RLS proof suite reads cluster roles, not the migration: giving cadus_app BYPASSRLS in 0001 keeps all 15 tests green |
| 7 | major | `crates/store/src/lib.rs:150` | FIX3 | The C3 boot guard's BYPASSRLS branch has no test; deleting it keeps the whole suite green |
| 8 | major | `migrations/0006_grants_rls.sql:31` | FIX2 | The blanket grant gives cadus_app INSERT, UPDATE, and DELETE on _sqlx_migrations |
| 9 | major | `crates/web/src/bin/cadus-web.rs:86` | FIX4 | cadus-web graceful shutdown never completes while a client holds a half-sent request |
| 10 | major | `crates/worker/src/lib.rs:129` | FIX4 | cadus-worker ignores SIGTERM for as long as a heartbeat query is in flight |
| 11 | major | `crates/store/tests/rls.rs:171` | FIX3 (dup of #7) | The BYPASSRLS half of the boot guard is never exercised by any test |
| 12 | major | `crates/web/tests/http.rs:211` | FIX4 | `binary_exits_3_with_a_superuser_dsn` fails when the shell already sets RUST_LOG |
| 13 | major | `Dockerfile:19` | FIX5 | The builder stage downloads a second complete Rust toolchain, so the image build needs static.rust-lang.org |
| 14 | major | `crates/store/tests/rls.rs:144` | FIX3 (dup of #5) | The transaction-local scope of the tenant GUC is asserted nowhere; flipping set_config's is_local flag leaks a tenant across a pooled connection and the whole suite stays green |
| 15 | major | `migrations/0006_grants_rls.sql:16` | FIX2 | diagnosis_jobs is exempted from RLS for a reason the BYPASSRLS worker role disproves |
| 16 | major | `crates/store/src/test_support.rs:103` | FIX3 | A failing test leaks its throwaway database: TestDb has no Drop impl |
| 17 | major | `scripts/check_migrations.sh:118` | FIX5 | check_migrations.sh passes on an edited already-shipped migration, and the failure lands in production instead |
| 18 | major | `crates/store/src/bin/cadus-migrate.rs:18` | FIX3 | cadus-migrate is executed by no test; its exit-code contract survives inversion |
| 19 | major | `crates/worker/src/lib.rs:148` | FIX4 | The worker heartbeat query is asserted nowhere; deleting the database round trip keeps both worker tests green |
| 20 | major | `crates/store/tests/rls.rs:184` | FIX2 | ALTER DEFAULT PRIVILEGES in 0006 has no test; deleting both statements leaves the suite green |
| 21 | major | `docker-compose.yml:19` | FIX3 | trust auth plus an unpublished port still exposes superuser Postgres to every local user on the host |
| 22 | major | `docs/SELF_HOST.md:64` | FIX4 | The gate enforces R3 core purity mechanically but enforces R4 nowhere, while SELF_HOST.md tells the operator it does |
| 23 | major | `crates/store/tests/rls.rs:209` | FIX2 | RLS coverage test collapses (relrowsecurity, relforcerowsecurity) into one bucket, so ENABLE-without-FORCE on an exempt table passes unnoticed |
| 24 | major | `migrations/0006_grants_rls.sql:35` | FIX2 | The sequence grant in 0006 is covered by no test; deleting it keeps all 15 tests green and breaks every model_call_log insert |
| 25 | minor | `migrations/0006_grants_rls.sql:57` | FIX2 | FORCE ROW LEVEL SECURITY gives no owner protection, because the production owner is a superuser |
| 26 | minor | `migrations/0006_grants_rls.sql:31` | FIX2 (dup of #8) | The blanket GRANT gives the runtime role UPDATE and DELETE on the sqlx migration ledger |
| 27 | minor | `Dockerfile:61` | FIX5 | Dockerfile claims cadus-migrate reads /app/migrations at runtime; sqlx embeds the files at compile time |
| 28 | minor | `docs/SELF_HOST.md:17` | FIX5 | The documented bring-up check calls curl, which the runtime image does not install |
| 29 | minor | `docs/SELF_HOST.md:66` | FIX5 | /api/ready is documented as reporting worker state; the handler only runs SELECT 1 |
| 30 | minor | `crates/store/src/lib.rs:105` | FIX3 | /api/ready blocks 30 s before it reports unready |
| 31 | minor | `crates/web/src/bin/cadus-web.rs:95` | FIX4 | A non-Unicode BIND_ADDR is silently discarded and the server binds 0.0.0.0:8080 |
| 32 | minor | `scripts/gate.sh:44` | FIX5 | gate.sh prints GATE OK when the migration check script is missing |
| 33 | minor | `docs/SELF_HOST.md:17` | FIX5 (dup of #28) | docs/SELF_HOST.md bring-up check runs curl inside an image that has no curl |
| 34 | minor | `Dockerfile:62` | FIX5 (dup of #27) | The image copies migrations to /app/migrations but cadus-migrate never reads that directory |
| 35 | minor | `.env.example:14` | FIX5 | SITE_ADDRESS ships as a placeholder domain, so the documented http-only bring-up cannot work |
| 36 | minor | `migrations/0001_roles.sql:18` | FIX2 (dup of #25) | Migration comments describe a cadus_owner migration runner and owner-level RLS that the shipped stack does not use |
| 37 | minor | `README.md:9` | FIX5 | README presents CADUS_TEST_DATABASE_URL as optional, but gate.sh refuses to run any check without it |
| 38 | minor | `docs/SELF_HOST.md:47` | FIX5 | SELF_HOST.md tells the operator the BYPASSRLS worker re-enters RLS through SET ROLE, and no such code exists |
| 39 | minor | `crates/web/src/bin/cadus-web.rs:86` | FIX4 | Signal handlers install only after the pool opens, so SIGTERM during the 30 s connect kills the process by signal |
| 40 | minor | `crates/worker/src/lib.rs:62` | FIX4 | An empty WORKER_TICK_SECS silently falls back to 5 s, against the documented contract |
| 41 | minor | `migrations/0003_event_log.sql:27` | FIX2 | The uniqueness of events_attempt_idem is asserted nowhere; downgrading it to a plain index keeps the suite green |
| 42 | minor | `crates/web/tests/http.rs:194` | FIX4 | cadus-web's documented exit code 2 and its default BIND_ADDR are untested; both can be changed silently |
| 43 | minor | `crates/worker/tests/run.rs:58` | FIX4 | The worker tick test asserts a wall-clock tick count with no upper bound on how slow the heartbeat query may be |
| 44 | minor | `scripts/check_migrations.sh:49` | FIX5 | check_migrations.sh uses a fixed database name, so two runs on one cluster silently destroy each other's database |
| 45 | minor | `crates/store/src/lib.rs:61` | FIX3 | The redacting Debug impl for DbConfig is asserted nowhere; replacing it with a derive leaks the database password and the suite stays green |
| 46 | minor | `.github/workflows/ci.yml:59` | FIX5 | The gate and CI never build the image or validate the compose file, though M0.md makes both U6's acceptance check |

## FIX2 — migrations + crates/store/tests/rls.rs + docs/SCHEMA.md

### #2 [blocker] cadus_app destroys another tenant's RLS-protected rows through the users FK cascade

File: `migrations/0006_grants_rls.sql:31` — IDs: C3

**Claim.** The runtime role cadus_app holds DELETE on the RLS-exempt parent table users, and every tenant table references users with ON DELETE CASCADE, so one tenant deletes another tenant's learner_models, profiles, serving_pool, diag_states, session_plans, user_settings, web_states, anki_queue, and anki_cards_created rows without any tenant_isolation check.

**Evidence.**

```
migrations/0006_grants_rls.sql:19-20 keeps users out of the RLS set ("Tables with no user_id never enter the RLS set: users, auth_rate_counters, content_store.") and line 31 grants it full DML: "GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cadus_app;". Live proof, connected as cadus_app with app.user_id bound to tenant A:
  select count(*) from learner_models  -> 1   (RLS hides tenant B)
  delete from users where id='<tenant B>'  -> success, no error
After the delete, as superuser: learner_models 2->1, profiles 2->1, serving_pool 2->1, users 2->1. PostgreSQL runs referential-action triggers with RLS off, so FORCE ROW LEVEL SECURITY on the child tables gives no protection.
```

**Failure scenario.** A code defect or SQL injection in the web tier (or the future account-deletion handler with a wrong id) runs DELETE FROM users WHERE id = <other tenant>. Every RLS-protected row of that tenant that has no events row is erased across the tenant boundary. events itself survives only because migrations/0003_event_log.sql:13 uses ON DELETE RESTRICT; a tenant with no events yet is fully destroyed.

**Refuter.** The claim is demonstrable, and I reproduced it end to end on a fresh database built from migrations 0001-0006. Three facts hold together, and each one is verified.

First, cadus_app holds DELETE on users. /home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql:31 grants SELECT, INSERT, UPDATE, DELETE on ALL TABLES IN SCHEMA public. users exists from 0002_identity.sql, so the grant covers it. Line 19-20 of the same file keeps users out of the RLS set, with "no user_id" as the only stated reason. The reason is a mechanical rule, not a risk decision. Neither docs/plans/M0.md nor docs/SCHEMA.md records an accepted decision about the DELETE grant on users.

Second, every tenant table except events points at users with ON DELETE CASCADE. I confirmed the referential actions in the migration text: learner_models (0003:31), profiles, session_plans, diag_states, user_settings, web_states, anki_qu

### #6 [blocker] The RLS proof suite reads cluster roles, not the migration: giving cadus_app BYPASSRLS in 0001 keeps all 15 tests green

File: `crates/store/src/test_support.rs:68` — IDs: C3, M0-U3, M0-U4

**Claim.** TestDb::create() makes a fresh database but the three roles are cluster-scoped and migration 0001 skips CREATE ROLE when the role already exists, so every assertion about cadus_app's RLS status validates whatever the developer's long-lived cluster happens to hold instead of what the migration says.

**Evidence.**

```
migrations/0001_roles.sql:29-34
  DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'cadus_app') THEN
      CREATE ROLE cadus_app LOGIN NOSUPERUSER NOBYPASSRLS NOCREATEDB NOCREATEROLE;
    END IF;
  ...
No migration ever ALTERs an existing role.

Mutation run (copy of the tree, CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/postgres):
  sed -i "s/CREATE ROLE cadus_app LOGIN NOSUPERUSER NOBYPASSRLS .../CREATE ROLE cadus_app LOGIN NOSUPERUSER BYPASSRLS .../" migrations/0001_roles.sql
  cargo test --workspace
  -> test boot_guard ... ok
  -> test rls_isolates_tenants ... ok
  -> test app_role_cannot_update_events ... ok
  -> test boot_check_rejects_a_role_that_bypasses_rls ... ok
  -> 5 passed / 6 passed / 2 passed / 2 passed; 0 failed (15/15 green)

Cluster state before and after the mutated run is unchanged:
  docker exec cadus2-testdb psql -U test -Atc "select rolname,rolbypassrls from pg_roles where rolname like 'cadus%'"
  cadus_app|f
  cadus_admin|t
  cadus_owner|f

The assertions that should have caught it:
  crates/store/tests/rls.rs:176   assert!(!info.bypass_rls);
  crates/web/tests/http.rs:179-186  assert_eq!(info, RoleInfo { name: "cadus_app", superuser: false, bypass_rls: false });
```

**Failure scenario.** A later unit edits migration 0001 (or the role attributes drift on the production cluster, where 0001 also skips role creation) so that cadus_app carries BYPASSRLS. Every developer runs scripts/gate.sh against the reused cadus2-testdb cluster, where cadus_app was created NOBYPASSRLS long ago; all 15 tests pass and the gate reports GATE OK. C3 tenant isolation is dead in the schema and the suite that exists to prove C3 says nothing. The same blindness invalidates the mutation report docs/reviews/M0-mutation-u3.md, which was produced against this same long-lived cluster: no mutation of a role attribute could ever have been killed there.

**Refuter.** The claim is demonstrable, and I reproduced its mechanism directly. I could not refute it.

Confirmed facts:

1. `TestDb::create()` at /home/deploy/dev/cadus2.0/crates/store/src/test_support.rs:68 calls `crate::migrate(&admin)` on a fresh database, but roles live in the cluster catalog `pg_authid`, not in the database. `current_role()` in /home/deploy/dev/cadus2.0/crates/store/src/lib.rs:124-131 reads `pg_roles`. A fresh database therefore gives no isolation for a role attribute.

2. Migration 0001 guards each `CREATE ROLE` with `IF NOT EXISTS (SELECT 1 FROM pg_roles ...)`, and no migration ever runs `ALTER ROLE`. On a cluster that already holds `cadus_app`, the whole `CREATE ROLE` text is dead code.

3. I ran the exact mutation on both cluster types. On the long-lived `cadus2-testdb` cluster the mutated migration left `cadus_app` at `rolbypassrls = f`, so `assert!(!info.bypass_rls)` (cr

### #8 [major] The blanket grant gives cadus_app INSERT, UPDATE, and DELETE on _sqlx_migrations

File: `migrations/0006_grants_rls.sql:31` — IDs: D9

**Claim.** sqlx creates public._sqlx_migrations before it applies 0006, so GRANT ... ON ALL TABLES IN SCHEMA public sweeps the migration ledger in and the runtime role rewrites or erases the schema history.

**Evidence.**

```
migrations/0006_grants_rls.sql:31 `GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cadus_app;`. ACL dump after a full migrate: `_sqlx_migrations | r | test=arwdDxt/test  cadus_app=arwd/test  cadus_admin=arwd/test` (a=INSERT, r=SELECT, w=UPDATE, d=DELETE). Live proof as cadus_app: `update _sqlx_migrations set checksum='\x00'::bytea, success=false where version=6` succeeds, and `delete from _sqlx_migrations where version=6` takes the row count from 6 to 5. No REVOKE follows the grant, unlike the events REVOKE at line 52.
```

**Failure scenario.** A defect or injection on the request path runs DELETE FROM _sqlx_migrations. The next deploy runs cadus-migrate, which re-applies 0001 and then aborts at 0002 with 'relation "users" already exists', so the stack never reaches a new schema version. An UPDATE of the checksum column instead makes sqlx fail every future run with VersionMismatch. crates/store/tests/rls.rs:253 only counts the rows, so no test notices the widened grant.

**Refuter.** The claim is correct, and I reproduced each step on a live Postgres 16 cluster. I tried four refutation paths and all four failed.

1. Timing. sqlx creates public._sqlx_migrations before it applies the first migration. The ledger table is therefore present in schema public when 0006 runs. GRANT ... ON ALL TABLES IN SCHEMA public applies to every table that exists at that moment, so the ledger is included.
2. A later REVOKE. There is none. grep over the repo (worktrees excluded) finds only one REVOKE: migrations/0006_grants_rls.sql:52, `REVOKE UPDATE, DELETE, TRUNCATE ON events FROM cadus_app`. No file revokes anything on _sqlx_migrations. crates/store/src/lib.rs:116 `migrate()` calls `MIGRATOR.run(pool)` and adds no post-step.
3. A different runtime role or a schema move. migrations/0001_roles.sql names cadus_app the runtime role, and it is the role that gets the sweep at line 31. The le

### #15 [major] diagnosis_jobs is exempted from RLS for a reason the BYPASSRLS worker role disproves

File: `migrations/0006_grants_rls.sql:16` — IDs: C3, D9

**Claim.** `diagnosis_jobs` stays outside row-level security because "the worker claims jobs across tenants", but the worker connects as `cadus_admin`, which holds BYPASSRLS and therefore sees every row with the policy in place; the exemption buys nothing and leaks every tenant's attempt payload and diagnosis result to `cadus_app`.

**Evidence.**

```
migrations/0006_grants_rls.sql:16 — `--   diagnosis_jobs  -- the worker claims jobs across tenants`

Leak, as cadus_app bound to tenant A:
```
diagnosis_jobs rows visible: 2
leaked payload: {"secret_answer": "tenant a@x.test"}
leaked payload: {"secret_answer": "tenant b@x.test"}
leaked outbox: a@x.test
leaked outbox: b@x.test
events rows visible (RLS on): 0
```

The justification, disproved — after ENABLE + FORCE + the same `tenant_isolation` policy on diagnosis_jobs:
```
worker (cadus_admin) still sees: 2
cadus_app with no tenant now sees: 0
```
```

**Failure scenario.** Milestone M5 adds the A4 diagnosis path. The web tier enqueues a job and later reads its `result` to push the diagnosis to the client. Because `diagnosis_jobs` carries no policy, a query whose predicate is wrong or caller-influenced returns rows for every tenant: `payload` holds the learner's attempt document and `result` holds the tailored diagnosis prose. `events` on the same connection returns 0 rows, so the leak is invisible in a side-by-side smoke test. The same blanket exemption also exposes `email_outbox.to_addr`, which is every registered user's email address, to any tenant's request. Turning the policy on costs the worker nothing, as the command output above shows.

**Refuter.** The claim is demonstrable and I could not refute it. The stated reason for the exemption at migrations/0006_grants_rls.sql:16 ("the worker claims jobs across tenants") is void, because the worker connects as cadus_admin, which migrations/0001_roles.sql creates with BYPASSRLS and docker-compose.yml:92 wires into the worker DSN. BYPASSRLS is evaluated on the current role and is not affected by FORCE ROW LEVEL SECURITY, so the cross-tenant claim never depended on the missing policy. I reproduced both halves on a live Postgres 16 (throwable DSN, database cadus2_rv_rlsdj, dropped after): with the migrations unchanged, a cadus_app session bound to tenant A reads all tenants' diagnosis_jobs rows with payload, while events on the same connection returns only its own row; after adding ENABLE + FORCE + the same tenant_isolation policy, the worker as cadus_admin still sees both rows, still claims w

### #20 [major] ALTER DEFAULT PRIVILEGES in 0006 has no test; deleting both statements leaves the suite green

File: `crates/store/tests/rls.rs:184` — IDs: D9, C3, M0-U2

**Claim.** rls_coverage_is_the_literal_list pins ENABLE/FORCE/policy names but nothing pins the grant surface, so the two ALTER DEFAULT PRIVILEGES statements that exist specifically to stop a runtime-only DML failure can be removed without a single test failing.

**Evidence.**

```
migrations/0006_grants_rls.sql:37-45 states the purpose in its own comment: "Without this, a new table is DML-denied for cadus_app until someone notices at runtime."

Mutation run (both statements deleted):
  -ALTER DEFAULT PRIVILEGES IN SCHEMA public
  -    GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO cadus_app, cadus_admin;
  -ALTER DEFAULT PRIVILEGES IN SCHEMA public
  -    GRANT USAGE, SELECT ON SEQUENCES TO cadus_app, cadus_admin;

  cargo test --workspace
  -> 15/15 green, SURVIVED

Every M0 table is created before 0006, so `GRANT ... ON ALL TABLES` covers them all and the defaults are load-bearing only for later migrations, which no test creates.
```

**Failure scenario.** M1 adds migrations/0007 with a new table. The default privileges are gone (or were never re-checked), so cadus_app receives no DML on it. Every M0 and M1 test still passes because they only touch tables 0006 granted explicitly. The first request that reads the new table returns 42501 in production.

**Refuter.** The claim stands. I reproduced the mutation on Postgres 16 and confirmed both halves: the two ALTER DEFAULT PRIVILEGES statements are load-bearing for any table a later migration creates, and no test, script, or CI step observes them. With the statements present, a table created after 0006 gives cadus_app INSERT/SELECT and sequence USAGE; with them deleted, all three are false and pg_default_acl in schema public is empty, so the first production read of an 0007 table returns 42501. No file in the repo (tests, scripts/check_migrations.sh, scripts/gate.sh, .github/workflows/ci.yml) references pg_default_acl, defacl, has_table_privilege, has_sequence_privilege, aclexplode, or relacl, and no .rs file contains CREATE TABLE or CREATE SEQUENCE, so no test creates an object after 0006 that would feel the loss. migrate_is_idempotent and check_migrations.sh both start from a fresh database, so an 

### #23 [major] RLS coverage test collapses (relrowsecurity, relforcerowsecurity) into one bucket, so ENABLE-without-FORCE on an exempt table passes unnoticed

File: `crates/store/tests/rls.rs:209` — IDs: C3, D9

**Claim.** The test partitions tables by the single derived predicate `row.rls_enabled && row.rls_forced`, so a table that gains `ENABLE ROW LEVEL SECURITY` but no `FORCE` and no policy stays in the `not_forced` bucket and both literal assertions still hold, while `cadus_app` loses every row of that table.

**Evidence.**

```
crates/store/tests/rls.rs:209-213:

        if row.rls_enabled && row.rls_forced {
            forced.push(row.table_name.clone());
        } else {
            not_forced.push(row.table_name.clone());
        }

Catalog before the mutation: `auth_sessions|f|f`. After `ALTER TABLE auth_sessions ENABLE ROW LEVEL SECURITY;` it reads `auth_sessions|t|f`, and the two vectors the test compares are unchanged:

  f|auth_sessions,auth_tokens,diagnosis_jobs,email_outbox,model_call_log,oauth_accounts
  t|anki_cards_created,anki_queue,diag_states,events,learner_models,profiles,serving_pool,session_plans,user_settings,web_states
  policy list the test asserts: 10

Runtime effect on the same database (one seeded session row):

  as superuser: 1
  as cadus_app: 0
  -- after ALTER TABLE auth_sessions DISABLE ROW LEVEL SECURITY: 1
```

**Failure scenario.** Someone adds `ALTER TABLE auth_sessions ENABLE ROW LEVEL SECURITY;` to a later migration but omits `FORCE` and the policy (or adds ENABLE first and the policy in a follow-up commit that never lands). The whole suite passes, so it merges. `cadus_app` then reads zero rows from `auth_sessions`, so every session-cookie lookup fails and no user can stay logged in. The test whose docstring promises `A new tenant table without its own policy fails this test` does not fail.

**Refuter.** I tried to refute the claim and failed. The claim is demonstrable on a fresh database built from migrations/0001-0006.

The test at /home/deploy/dev/cadus2.0/crates/store/tests/rls.rs:196-234 builds two vectors of table NAMES only. The predicate `row.rls_enabled && row.rls_forced` collapses three distinct catalog states — (f,f), (t,f), and (f,t) — into one `not_forced` bucket, and the assertion `assert_eq!(not_forced, to_owned(&EXEMPT_TABLES))` compares names, never the flags. An exempt table that gains `ENABLE ROW LEVEL SECURITY` without `FORCE` therefore keeps its name and its position in `not_forced`, and the test still passes. The policy assertion also stays green, because ENABLE creates no pg_policy row.

The runtime effect is the closed failure mode of Postgres: RLS enabled plus no policy plus a non-owner, non-BYPASSRLS role equals zero rows. `cadus_app` is created NOSUPERUSER NOBY

### #24 [major] The sequence grant in 0006 is covered by no test; deleting it keeps all 15 tests green and breaks every model_call_log insert

File: `migrations/0006_grants_rls.sql:35` — IDs: C3, T6, D9

**Claim.** `GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO cadus_app, cadus_admin` is load-bearing for the only bigserial table in the schema (`model_call_log.id`), and no test in the workspace ever writes to `model_call_log` as `cadus_app`, so removing the statement is a silent mutation survivor.

**Evidence.**

```
No test file mentions `model_call_log` except as a literal name inside `EXEMPT_TABLES` (crates/store/tests/rls.rs:41). Proof that the statement is load-bearing, on a database carrying migrations 0001-0006:

  === with sequence grant (current migration) ===
  INSERT 0 1
  === mutation: revoke the sequence grant ===
  ERROR:  permission denied for sequence model_call_log_id_seq

`ALTER DEFAULT PRIVILEGES ... ON SEQUENCES` (lines 44-45) does not cover it: `model_call_log_id_seq` is created in 0005, before the default privileges exist.
```

**Failure scenario.** An edit to 0006 drops or narrows line 35 (for example a cleanup that assumes ALTER DEFAULT PRIVILEGES already covers sequences). `cargo test --workspace` reports 15 passed and the gate is green. At runtime the first `INSERT INTO model_call_log ...` from cadus-web or cadus-worker raises SQLSTATE 42501 `permission denied for sequence model_call_log_id_seq`, so every model call fails to record its token spend — the T6 cost ledger stops, and it stops on the request path.

**Refuter.** I tried to refute the claim and failed. Every factual part of it reproduces on a fresh database that carries migrations 0001-0006.

1. `model_call_log_id_seq` is the only sequence in schema `public`. `SELECT sequencename FROM pg_sequences WHERE schemaname='public'` returns exactly one row. `migrations/0005_content.sql:49` holds the only `serial`/`bigserial` column in the workspace.
2. Line 35 of `/home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql` is load-bearing. With the line present, an INSERT as `cadus_app` returns `INSERT 0 1`. With the line removed, the same INSERT returns `ERROR: permission denied for sequence model_call_log_id_seq` (SQLSTATE 42501).
3. `ALTER DEFAULT PRIVILEGES ... ON SEQUENCES` (lines 44-45) does not cover it. The sequence exists before 0006 runs, so the default privileges never apply to it. My mutated run proves this: lines 44-45 stayed in place and the I

### #25 [minor] FORCE ROW LEVEL SECURITY gives no owner protection, because the production owner is a superuser

File: `migrations/0006_grants_rls.sql:57` — IDs: C3

**Claim.** The comment says FORCE means "a migration or an owner session gets no free pass", but docker-compose.yml runs the migrations as the `postgres` superuser, so `postgres` owns every table and Postgres bypasses RLS for a superuser whether FORCE is set or not; the documented `cadus_owner` never owns anything.

**Evidence.**

```
migrations/0006_grants_rls.sql:57-58 `-- ENABLE turns the policy on. FORCE applies it to the table owner too, so a` / `-- migration or an owner session gets no free pass.`
docker-compose.yml:58 `DATABASE_URL: postgresql://postgres@db:5432/cadus` for the `migrate` service.
docs/SCHEMA.md:22 `| cadus_owner | NOLOGIN | — | Owns the schema. The migration runner connects as it. |` — contradicted by docker-compose.yml:58 and docs/SELF_HOST.md:33 (`migrate | postgres (superuser)`).
Probe on the migrated schema, owner session, no tenant context, ENABLE+FORCE both on:
 SELECT tableowner FROM pg_tables WHERE tablename='events';  -> test
 SELECT rolsuper FROM pg_roles WHERE rolname='test';         -> t
 SELECT count(*) FROM events;                                -> 1
```

**Failure scenario.** An operator opens `docker compose exec db psql -U postgres -d cadus` and runs `SELECT * FROM events` with no `app.user_id` set. The session returns every tenant's rows and can UPDATE or DELETE them, although the migration comment and docs/SCHEMA.md state that FORCE closes that path. Any future job that connects as the table owner inherits the same free pass.

**Refuter.** The claim holds. I tried to refute it and failed on both of its parts.

Part 1 — the FORCE comment overstates what the shipped stack does. The first clause of migrations/0006_grants_rls.sql:57 is correct Postgres semantics: FORCE removes the table owner's exemption. The second clause, "so a migration or an owner session gets no free pass", is false for the deployment in docker-compose.yml. Postgres skips row-level security for a superuser unconditionally, and FORCE does not change that. docker-compose.yml:58 gives the `migrate` service `postgresql://postgres@db:5432/cadus`, so the superuser `postgres` creates and therefore owns every table. The owner session is a superuser session, and it reads and writes every tenant's rows with no `app.user_id` set.

Part 2 — `cadus_owner` never owns anything. migrations/0001_roles.sql:18-25 creates the role, but no file in the repo runs `ALTER TABLE .

### #26 [minor] The blanket GRANT gives the runtime role UPDATE and DELETE on the sqlx migration ledger

File: `migrations/0006_grants_rls.sql:31` — IDs: C2, C3 — duplicate of #8

**Claim.** `GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cadus_app` also hits `_sqlx_migrations`, which sqlx creates before the first migration runs, so the least-privileged runtime role can rewrite or erase the migration history.

**Evidence.**

```
migrations/0006_grants_rls.sql:31 `GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO cadus_app;`
$ docker exec cadus2-testdb psql -U test -d cadus2_gate -c "SELECT table_name, privilege_type FROM information_schema.table_privileges WHERE grantee='cadus_app' AND table_name IN ('_sqlx_migrations','events','users')"
 _sqlx_migrations | DELETE
 _sqlx_migrations | INSERT
 _sqlx_migrations | SELECT
 _sqlx_migrations | UPDATE
 events           | INSERT
 events           | SELECT
The REVOKE on line 52 covers `events` only.
```

**Failure scenario.** A SQL-injection defect or a stray statement in the web tier runs `DELETE FROM _sqlx_migrations` on the `cadus_app` connection. The next `cadus-migrate` run finds an empty ledger, replays 0002 from the start, and aborts on `CREATE TABLE users` (SQLSTATE 42P07). The compose `migrate` one-shot then exits 2, and `web` and `worker` never start because they depend on `service_completed_successfully`.

**Refuter.** The claim is demonstrable in full. I reproduced every step on a throwaway database.

1. Line 31 of /home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql grants DML on ALL TABLES IN SCHEMA public. sqlx creates `_sqlx_migrations` in schema public before it applies migration 0001, so the table is present when 0006 runs and it receives the grant. After a clean `cargo sqlx migrate run`, pg_class.relacl for `_sqlx_migrations` is `{test=arwdDxt/test,cadus_app=arwd/test,cadus_admin=arwd/test}`. The `arwd` bits for cadus_app are exactly SELECT, INSERT, UPDATE, DELETE.

2. The only REVOKE in the file, line 52, names `events` alone. No other migration revokes anything. `ALTER DEFAULT PRIVILEGES` on lines 43-46 also keeps the same DML set for future tables, so the pattern is systemic, not a one-off.

3. I connected as cadus_app, the same role that docker-compose.yml line 75 gives the `web` servic

### #36 [minor] Migration comments describe a cadus_owner migration runner and owner-level RLS that the shipped stack does not use

File: `migrations/0001_roles.sql:18` — IDs: C3, D9 — duplicate of #25

**Claim.** `0001_roles.sql:18-19` says "cadus_owner owns the schema. The migration runner connects as it in production", and `0006_grants_rls.sql:56-58` justifies FORCE ROW LEVEL SECURITY as leaving "a migration or an owner session" with "no free pass"; in the shipped stack the migration runner is the `postgres` superuser, which owns every object and bypasses RLS regardless of FORCE, while `cadus_owner` is NOLOGIN and owns nothing.

**Evidence.**

```
migrations/0001_roles.sql:18-22 `-- cadus_owner owns the schema. The migration runner connects as it in production.` / `CREATE ROLE cadus_owner NOLOGIN NOSUPERUSER;`
migrations/0006_grants_rls.sql:56-58 `-- ENABLE turns the policy on. FORCE applies it to the table owner too, so a migration or an owner session gets no free pass.`
docker-compose.yml:58 `DATABASE_URL: postgresql://postgres@db:5432/cadus`
docs/SELF_HOST.md:33 `| \`migrate\` | \`postgres\` (superuser) | DDL, extensions, role creation |`
```

**Failure scenario.** An operator runs a data fixup through the migrate service DSN (`docker compose run --rm migrate psql "$DATABASE_URL" -c "UPDATE web_states SET doc = ..."`), trusting the 0006 comment that FORCE ROW LEVEL SECURITY keeps even a migration inside the tenant policy. Because that DSN is a superuser, RLS is not applied at all and the statement rewrites every tenant's rows. Separately, an operator who follows the 0001 comment and points the migrate DSN at `cadus_owner` cannot connect at all (NOLOGIN) and, if given LOGIN, has no CREATE on schema `public` in Postgres 15+ and no right to `CREATE EXTENSION citext`.

**Refuter.** The claim is demonstrable and I could not refute it. Three separate assertions in the repo are false for the shipped stack. (1) migrations/0001_roles.sql:18 and docs/SCHEMA.md:22 say cadus_owner owns the schema and the migration runner connects as it. No migration ever assigns ownership: after applying 0001-0006, cadus_owner owns 0 objects, all 19 public tables are owned by the migration-running superuser, and schema public is owned by pg_database_owner. cadus_owner is a dead role, referenced nowhere except its own CREATE ROLE and the docs. (2) migrations/0006_grants_rls.sql:56-58 justifies FORCE ROW LEVEL SECURITY as leaving "a migration or an owner session" with "no free pass". Because docker-compose.yml:58 points migrate at postgresql://postgres@db:5432/cadus (documented as superuser at docs/SELF_HOST.md:33), FORCE has no effect there. I reproduced the exact failure scenario: as the s

### #41 [minor] The uniqueness of events_attempt_idem is asserted nowhere; downgrading it to a plain index keeps the suite green

File: `migrations/0003_event_log.sql:27` — IDs: C2, D9

**Claim.** The partial index that migration 0003 calls the database-level attempt idempotency guard is only correct because it is UNIQUE, but no test reads `pg_index.indisunique` (or exercises an `ON CONFLICT` insert), so changing `CREATE UNIQUE INDEX` to `CREATE INDEX` passes all 15 tests.

**Evidence.**

```
The claim the SQL is supposed to carry (migrations/0003_event_log.sql lines 24-26): "FR-14 idempotency in the database: a repeated attempt_id makes the INSERT a no-op under ON CONFLICT DO NOTHING."

Mutation applied in a scratch copy: `CREATE UNIQUE INDEX events_attempt_idem` -> `CREATE INDEX events_attempt_idem`. `cargo test --workspace`:
  rls.rs: 5 passed (app_role_cannot_update_events, rls_isolates_tenants, boot_guard, rls_coverage_is_the_literal_list, migrate_is_idempotent)
  test result: ok. 15 passed; 0 failed across the workspace.

The only catalog test, `rls_coverage_is_the_literal_list` (crates/store/tests/rls.rs:187-241), queries `pg_class` and `pg_policy` and never touches `pg_index`.

Both downstream failure modes reproduced on Postgres 16 with the two index shapes side by side:
  -- UNIQUE partial index
  INSERT ... ON CONFLICT (user_id, attempt_id) WHERE attempt_id IS NOT NULL DO NOTHING;  -> INSERT 0 0 ; count = 1
  -- plain partial index
  INSERT ... ON CONFLICT (user_id, attempt_id) WHERE attempt_id IS NOT NULL DO NOTHING;  -> ERROR:  there is no unique or exclusion constraint matching the ON CONFLICT specification
  INSERT ... ON CONFLICT DO NOTHING;                                                     -> INSERT 0 1 ; count = 2
```

**Failure scenario.** M5 writes the grade path against the contract this migration comment states. If the index has silently lost UNIQUE by then, a targeted `ON CONFLICT (user_id, attempt_id) ... DO NOTHING` raises SQLSTATE 42P10 on every attempt insert (the L2 grade path returns 500 instead of a verdict), and the natural workaround — a bare `ON CONFLICT DO NOTHING` — inserts the duplicate instead of suppressing it. A retried or double-submitted attempt then lands twice in the append-only log, the incremental fold (D4) folds the same attempt twice into `learner_models`, and because `events` is append-only by grant (C2) the duplicate cannot be deleted: the wrong XP/repNum can only be superseded by a hand-authored `regraded` event.

**Refuter.** The claim is demonstrable exactly as written and I could not refute it. Changing `CREATE UNIQUE INDEX events_attempt_idem` to `CREATE INDEX events_attempt_idem` at migrations/0003_event_log.sql:27 leaves the whole test suite green: 15 passed, 0 failed on `cargo test --workspace`, and `scripts/check_migrations.sh` prints all four PASS lines. No test in the workspace reads `pg_index`/`indisunique`, issues an `ON CONFLICT` insert, or inserts a duplicate `attempt_id`. Every `INSERT INTO events` in the only schema-touching test file omits `attempt_id`, so the column is always NULL and the rows fall outside the partial index entirely. The two downstream behaviors reproduce on Postgres 16: a targeted `ON CONFLICT (user_id, attempt_id) WHERE attempt_id IS NOT NULL DO NOTHING` raises 42P10 against the plain index, and the bare `ON CONFLICT DO NOTHING` workaround inserts the duplicate. Uniqueness 

## FIX3 — crates/store/src/** (lib, test_support, bin cadus-migrate) + new store test files; #21 = password support in cadus-migrate

### #5 [blocker] The transaction-local scope of the tenant GUC is never asserted: set_config(..., true) → false survives the whole suite

File: `crates/store/tests/rls.rs:144` — IDs: C3, M0-U3, HANDOVER-3

**Claim.** No test pins the third argument of set_config in begin_tenant, so changing the tenant binding from transaction-local to session-level leaves all 15 tests green while a pooled connection carries a tenant into the next unit of work.

**Evidence.**

```
Mutation applied to a copy of the workspace: crates/store/src/lib.rs:177 "SELECT set_config('app.user_id', $1, true)" → "... $1, false)". Full suite result: purity 2 passed, rls 5 passed, http 6 passed, run 2 passed; 0 failed. The reason it survives is rls.rs:130-136 — every tenant transaction in rls_isolates_tenants ends in `tx.rollback()`, and ROLLBACK undoes a session-level SET as well, so the guard at rls.rs:140-144 (`let unbound_count = ... fetch_one(&db.app)` / `assert_eq!(unbound_count, 0);`) can never observe the difference. The one COMMIT in the suite (rls.rs:98) is never followed by an unscoped query. Effect proven on a live database as cadus_app: `BEGIN; SELECT set_config('app.user_id', <A>, false); INSERT ...; COMMIT;` then on the same session `SELECT current_setting('app.user_id', true)` → f7753f67-24b2-483f-8fc4-5430a1c49397 and `SELECT count(*) FROM events` → 2 rows, instead of the 0 rows the doc comment at crates/store/src/lib.rs:167-170 promises ("a pooled connection carries no tenant into the next unit of work"). The M0 mutation report docs/reviews/M0-mutation-u3.md only tested M6 ("begin_tenant skips the set_config call"); it never tested the scope flag.
```

**Failure scenario.** A later edit or refactor of crates/store/src/lib.rs:177 drops the `true` (or an M5 handler adds a `SET app.user_id` without LOCAL). The gate stays green. In production, request 1 for tenant A commits a grade transaction; the connection returns to the pool with app.user_id still bound to A. Request 2 for tenant B runs any query outside begin_tenant on that same pooled connection — a readiness probe, a future admin or projection query — and reads or writes tenant A's RLS-protected rows. C3's closed failure mode is gone and no test in M0 reports it.

**Refuter.** The claim is demonstrable and I confirmed it end to end. `begin_tenant` (crates/store/src/lib.rs:171-182) is exercised only by crates/store/tests/rls.rs. Changing the third argument of set_config from `true` (transaction-local) to `false` (session-level) leaves the whole workspace green, because every tenant transaction in `rls_isolates_tenants` ends in `tx.rollback()` (rls.rs:136) and ROLLBACK also undoes a session-level SET, so the unbound guard at rls.rs:140-144 cannot observe the change; the single COMMIT (rls.rs:98) is followed only by another bound `begin_tenant`, never by an unscoped query. A probe test I added proves the mutant is a real defect, not an equivalent mutation: after COMMIT a pooled connection still carries app.user_id and an unscoped SELECT reads a tenant row. docs/reviews/M0-mutation-u3.md only covered M6 ("begin_tenant skips the set_config call"), never the scope f

### #7 [major] The C3 boot guard's BYPASSRLS branch has no test; deleting it keeps the whole suite green

File: `crates/store/src/lib.rs:150` — IDs: C3, M0-U4

**Claim.** Every test of `assert_rls_enforced` uses either the cluster superuser or `cadus_app`, so the `info.bypass_rls` half of the guard is never exercised; a mutation that drops it passes all 15 tests, and a plain `BYPASSRLS NOSUPERUSER` role would boot `cadus-web`.

**Evidence.**

```
crates/store/src/lib.rs:150 `if info.superuser || info.bypass_rls {`
Mutation applied to a copy of the tree: `if info.superuser {`
$ cargo test --workspace
test boot_guard ... ok
test boot_check_rejects_a_role_that_bypasses_rls ... ok
test binary_exits_3_with_a_superuser_dsn ... ok
test result: ok. 5 passed; 0 failed (cadus-store)
test result: ok. 6 passed; 0 failed (cadus-web)
The reject paths only destructure `superuser`: rls.rs:168-171 and http.rs:169-171 both assert `superuser` is true on `db.admin`, which is a superuser. The accept path uses `db.app` (`cadus_app`), which is neither superuser nor BYPASSRLS. No test builds a LOGIN NOSUPERUSER BYPASSRLS role.
```

**Failure scenario.** A later edit or refactor drops the `|| info.bypass_rls` disjunct. The gate stays green. An operator then points `web` at `postgresql://cadus_admin@db:5432/cadus` (the role that migration 0001 creates BYPASSRLS and that docker-compose.yml already gives a LOGIN). `cadus-web` starts, the tenant policies never apply to it, and every request reads and writes every tenant's rows — the exact failure C3 exists to stop.

**Refuter.** The claim is demonstrable, and I reproduced both halves of it on a copy of the tree. The shipped guard at /home/deploy/dev/cadus2.0/crates/store/src/lib.rs:150 is correct; the test suite is what fails. No test in the workspace builds a LOGIN NOSUPERUSER BYPASSRLS role, so no test separates `info.superuser` from `info.bypass_rls`. The two reject-path tests (crates/store/tests/rls.rs:167-171 `boot_guard` and crates/web/tests/http.rs:168-174 `boot_check_rejects_a_role_that_bypasses_rls`) both use `db.admin`, which /home/deploy/dev/cadus2.0/crates/store/src/test_support.rs:62-66 opens with the cluster superuser credentials of CADUS_TEST_DATABASE_URL, and both assert `superuser` is true. The binary test crates/web/tests/http.rs:194-217 passes `dsn_for(&db.name, None)`, which is the same superuser DSN, so it proves only the superuser half of the guard. The accept path uses `db.app` (`cadus_app

### #11 [major] The BYPASSRLS half of the boot guard is never exercised by any test

File: `crates/store/tests/rls.rs:171` — IDs: C3, U3, U4 — duplicate of #7

**Claim.** `assert_rls_enforced` rejects a role when `info.superuser || info.bypass_rls`, but every test feeds it either the cluster superuser or `cadus_app`; no test ever supplies a NOSUPERUSER + BYPASSRLS role, so the `bypass_rls` half of the condition is a mutation survivor that the mutation report does not list.

**Evidence.**

```
crates/store/src/lib.rs:150 `if info.superuser || info.bypass_rls {`. The two guard tests pin the superuser flag only: rls.rs:167-171 `let err = assert_rls_enforced(&db.admin).await.unwrap_err(); ... assert!(superuser, "the test cluster admin is a superuser");` and http.rs:168-174 `Err(StoreError::RlsBypass { superuser, .. }) => { assert!(superuser, ...) }`. `TestDb` opens exactly two pools — the superuser of `CADUS_TEST_DATABASE_URL` and `cadus_app` (test_support.rs:62-83). `grep -rn "cadus_admin\|cadus_owner" crates/ scripts/` returns only a comment in `crates/worker/src/bin/cadus-worker.rs:44` and no test at all.
```

**Failure scenario.** Mutate crates/store/src/lib.rs:150 to `if info.superuser {`. All 13 tests still pass, including `boot_guard`, `boot_check_rejects_a_role_that_bypasses_rls`, and `binary_exits_3_with_a_superuser_dsn`. An operator then points the `web` service at the worker DSN (`postgresql://cadus_admin@db:5432/cadus`, docker-compose.yml:92) — a NOSUPERUSER BYPASSRLS role. The C3 boot guard accepts it, the process serves traffic, and every tenant reads every other tenant's rows. This is the exact role shape the deployment already creates and grants LOGIN to.

**Refuter.** The claim holds; I reproduced it end to end. (1) Test gap: no test gives assert_rls_enforced / boot_check a NOSUPERUSER + BYPASSRLS role. TestDb opens exactly two pools, the cluster superuser and cadus_app (/home/deploy/dev/cadus2.0/crates/store/src/test_support.rs:62-83); no test file contains CREATE ROLE, SET ROLE, or ALTER ROLE (grep over crates/ returns nothing). The two guard tests pin the superuser flag only (crates/store/tests/rls.rs:167-176, crates/web/tests/http.rs:168-186). (2) Mutation survives: in a scratchpad copy I changed lib.rs:150 from `if info.superuser || info.bypass_rls {` to `if info.superuser {` and ran the full workspace against a private Postgres 16 -- 5 passed (store rls), 6 passed (web http), 2 passed (worker), 2 passed (core), 0 failed. boot_guard, boot_check_rejects_a_role_that_bypasses_rls, and binary_exits_3_with_a_superuser_dsn all stayed green. (3) /home/d

### #14 [major] The transaction-local scope of the tenant GUC is asserted nowhere; flipping set_config's is_local flag leaks a tenant across a pooled connection and the whole suite stays green

File: `crates/store/tests/rls.rs:144` — IDs: C3, R2 — duplicate of #5

**Claim.** `assert_eq!(unbound_count, 0)` is presented as the proof that a pooled connection carries no tenant into the next unit of work, but it can only ever observe the *unset* GUC case, so `begin_tenant`'s load-bearing `is_local = true` argument is unprotected and a mutation to `false` produces a silent cross-tenant read while all 15 tests pass.

**Evidence.**

```
crates/store/src/lib.rs:167-171 states the guarantee: "`set_config(..., true)` makes the setting local to the transaction, so the tenant context goes away when the transaction ends and a pooled connection carries no tenant into the next unit of work."

Every `begin_tenant` transaction that precedes the line-144 assertion in `rls_isolates_tenants` is rolled back (rls.rs:136), and ROLLBACK reverts the GUC whether it was set with `true` or `false`. No test ever COMMITs a tenant transaction and then reads on the same connection.

Mutation applied to crates/store/src/lib.rs:177 (`true` -> `false`), full suite re-run:
```
test rls_isolates_tenants ... ok
test app_role_cannot_update_events ... ok
test rls_coverage_is_the_literal_list ... ok
... test result: ok. 5 passed / 6 passed / 2 passed / 2 passed  (15 passed; 0 failed)
```
Adding a probe that COMMITs a tenant transaction and then reads on a `max_connections(1)` `cadus_app` pool exposes the leak that the shipped suite misses:
```
thread '...' panicked at leak.rs:33:
assertion `left == right` failed: tenant context leaked past COMMIT: saw 1 rows with no unit of work
  left: 1
 right: 0
```
The same probe passes on the unmutated code (`test result: ok. 1 passed`), so it is a valid discriminator.
```

**Failure scenario.** A refactor of `begin_tenant` (or a hand-written `SET app.user_id` added elsewhere in M3/M5) drops the transaction-local flag. Tenant A's request commits its grade transaction; the connection returns to the pool with `app.user_id` still bound to A. The next handler that runs a query outside a `begin_tenant` unit of work — a health scrape, a projector rebuild, or any M5 read that forgets the wrapper — reads and can write tenant A's rows while serving tenant B. `scripts/gate.sh` reports GATE OK for the change.

**Refuter.** The claim reproduces end to end. I could not refute it on any of the three points it depends on.

1. Postgres semantics. ROLLBACK reverts a GUC set with set_config(..., false) exactly as it reverts one set with true. Only COMMIT separates the two. The assertion at crates/store/tests/rls.rs:144 is preceded only by a begin_tenant transaction that rls.rs:136 rolls back, so it observes an unset GUC in both variants. It cannot discriminate the is_local argument.

2. Mutation survival. I copied the repo to a scratchpad, changed crates/store/src/lib.rs:177 from true to false, and ran the full workspace suite against the throwaway cluster. All 15 tests passed, including rls_isolates_tenants, app_role_cannot_update_events, and rls_coverage_is_the_literal_list. No test in crates/core/tests/purity.rs, crates/web/tests/http.rs, or crates/worker/tests/run.rs touches begin_tenant; grep shows begin_ten

### #16 [major] A failing test leaks its throwaway database: TestDb has no Drop impl

File: `crates/store/src/test_support.rs:103` — IDs: M0-U3

**Claim.** TestDb::drop(self) is only reachable as the last statement of a passing test, and there is no `impl Drop for TestDb`, so every assertion failure or panic leaves a fully migrated database behind on the shared cluster.

**Evidence.**

```
`grep -rn "impl Drop" crates/` returns nothing. Every test ends with `db.drop().await;` after its assertions (rls.rs:108,159,178,243,259; http.rs:160,188,216,274,302; run.rs:67,112); a panic unwinds past all of them. Live state of the throwaway cluster before I ran anything: `select count(*), pg_size_pretty(sum(pg_database_size(datname))) from pg_database where datname like 'cadus2_t_%'` → `46 | 360 MB`. Forced one failure (`RUST_LOG=off ./target/debug/deps/http-... binary_exits_3_with_a_superuser_dsn` → FAILED at http.rs:211) and the count went from 47 to 48, i.e. exactly one leaked database per failed test. A clean full run leaks nothing (46 before, 46 after). PROGRESS.md:51-52 lists this as an open finding; it is still unfixed.
```

**Failure scenario.** The mutation-check workflow that HANDOVER.md §3 mandates ("break the feature, confirm the tests fail") is precisely a sequence of red runs. Each red test permanently adds ~8 MB and one more set of grants referencing cadus_app to the cluster. After the M0 mutation round the cluster already holds 46 orphans and 360 MB. They also pin the cluster-scoped roles: `DROP ROLE cadus_app` fails while any leaked database still holds its grants, so a developer cannot reset the role set without hunting every orphan by hand.

**Refuter.** The claim is demonstrable in the code and on the live cluster. `TestDb::drop(self)` at /home/deploy/dev/cadus2.0/crates/store/src/test_support.rs:103 is an inherent async method that consumes `self`, and it is the last statement of each test. No `impl Drop for TestDb` exists in the workspace, and a `Drop` impl cannot run an async DROP DATABASE anyway. A panic in a test unwinds past the call, so the throwaway database stays on the cluster. No other code path removes it: scripts/gate.sh, scripts/check_migrations.sh, and the CI workflow hold no `cadus2_t_*` cleanup. The maintainers already know the defect; PROGRESS.md:51-52 lists it as an open finding. The consequences that the reviewer gives are also true. The cluster at 127.0.0.1:55434 keeps a persistent Docker volume, and it now holds 50 orphan databases at 392 MB, about 8.5 MB each. `DROP ROLE cadus_app` fails on that cluster because of

### #18 [major] cadus-migrate is executed by no test; its exit-code contract survives inversion

File: `crates/store/src/bin/cadus-migrate.rs:18` — IDs: D9

**Claim.** The `cadus-migrate` binary — the process the whole compose stack gates on — has zero test coverage, so a mutation that turns its documented failure exit code 2 into 0 and makes its applied-migration count always report 0 leaves the entire 15-test suite green.

**Evidence.**

```
grep over crates/, scripts/ and .github/ finds no execution of the binary at all:

  $ grep -rn "CARGO_BIN_EXE" crates/
  crates/worker/tests/run.rs:76:  ...CARGO_BIN_EXE_cadus-worker...
  crates/web/tests/http.rs:198:  ...CARGO_BIN_EXE_cadus-web...
  crates/web/tests/http.rs:228:  ...CARGO_BIN_EXE_cadus-web...

(`git ls-files | grep tests/` lists only purity.rs, rls.rs, http.rs, run.rs — the worktree file crates/store/tests/migrate_bin.rs was never integrated.)

The untested contract, from the file's own doc comment (lines 4-5): "The program prints the number of migrations that this run applied and exits 0. On an error it prints the error on stderr and exits 2."

Mutation applied in a scratch copy: `ExitCode::from(2)` -> `ExitCode::SUCCESS` (line 18) plus an early `return Ok(0)` in `applied_count`. Result of `cargo test --workspace` with CADUS_TEST_DATABASE_URL set:
  purity 2 passed; rls 5 passed; http 6 passed; run 2 passed — 15 passed, 0 failed.

Direct proof of the behavior change, same DSN, both binaries:
  mutated: `cadus-migrate: database error: pool timed out ...`  MUTATED EXIT=0
  original: `cadus-migrate: database error: pool timed out ...`  ORIG EXIT=2
```

**Failure scenario.** docker-compose.yml runs `migrate` as a one-shot and both `web` (line 79-80) and `worker` (line 95-96) start on `condition: service_completed_successfully` — that condition is exactly this exit code. A regression that swallows a migration failure (a mis-mapped error arm, an `Ok(())` on a partially applied migration, `anyhow` glue that returns 0) ships green through `scripts/gate.sh` and CI. On the next `docker compose up -d --build`, migrate exits 0 with the schema half-applied, web and worker start, and every `sqlx::query!` in the request path fails at runtime against missing tables while `/api/health` keeps answering 200. The same blind spot hides a wrong "applied N migrations" line, the only signal `docs/SELF_HOST.md` step 5 tells the operator to read (`docker compose logs migrate  # every migration applied, or nothing to apply`).

**Refuter.** The core of the claim holds and I reproduced it. crates/store/src/bin/cadus-migrate.rs has no test of any kind: no test invokes CARGO_BIN_EXE_cadus-migrate, and scripts/check_migrations.sh (the D9 script) applies migrations with `cargo sqlx migrate run`, not with the shipped binary. I copied the tree to a scratch dir, changed line 18 from `ExitCode::from(2)` to `ExitCode::SUCCESS`, and ran the gate steps in order: `cargo fmt --all --check` passed, `cargo clippy --all-targets --workspace -- -D warnings` exited 0, and `cargo test --workspace` exited 0 with 15 passed and 0 failed. The full merge gate stays green with the failure exit code inverted. That exit code is load-bearing: docker-compose.yml starts `web` (lines 79-80) and `worker` (lines 95-96) on `condition: service_completed_successfully`, which is exit 0 from `migrate`. The project also already tests this exact contract for its ot

### #21 [major] trust auth plus an unpublished port still exposes superuser Postgres to every local user on the host

File: `docker-compose.yml:19` — IDs: C3, C2

**Claim.** `POSTGRES_HOST_AUTH_METHOD: trust` with no password anywhere is defended by the claim that the `backend` network is a private segment, but the Docker host routes directly to user-defined bridge subnets, so any local process on the server can connect as the `postgres` superuser and read or write every tenant's rows, bypassing RLS, the boot guard, and the append-only REVOKE.

**Evidence.**

```
docker-compose.yml:19-27 — `# Postgres ... It has no \`ports:\` and it sits on the\n  # \`backend\` network only, so Caddy has no route to it. \`trust\` auth is safe on\n  # that private, password-free segment.` with `POSTGRES_HOST_AUTH_METHOD: trust`. docs/SELF_HOST.md:50-52 repeats it: `The database publishes no port and sits on the private \`backend\` network ... \`trust\` auth is safe on that segment, so no DSN carries a password.` Proof on this host, against an unpublished container port on a user-defined bridge (`cadus-demo-db`, `Ports: 5432/tcp`, no published mapping): `exec 3<>/dev/tcp/172.21.0.2/5432` -> `TCP connect to 172.21.0.2:5432 from host SUCCEEDED`. The connect is a plain kernel socket, not a Docker API call, so it needs no docker-group membership.
```

**Failure scenario.** An unprivileged local account or a compromised unrelated service on the same one-server deployment runs `docker network inspect`-free discovery by scanning 172.16.0.0/12, finds `db` on port 5432, and runs `psql -h 172.x.0.2 -U postgres -d cadus`. trust auth accepts it with no password. The attacker is a superuser: RLS does not apply (C3), the `REVOKE UPDATE, DELETE, TRUNCATE ON events FROM cadus_app` does not apply (C2), and every learner's event log is readable and rewritable.

**Refuter.** The claim is demonstrable. I reproduced the full failure scenario on this host against a faithful replica of the `db` service.

Why the defense in docker-compose.yml:19-27 and docs/SELF_HOST.md:50-52 is false: `backend` is a plain user-defined bridge (`networks:` / `frontend:` / `backend:`, no `internal: true`). Docker installs a host route to every such bridge subnet. "No `ports:`" stops only the published-port DNAT path. It does not stop the Docker host itself from routing to the container IP. So "private, password-free segment" is true for Caddy and for containers on other bridges, and false for every process on the host. The comment states a security property the configuration does not have, and the whole trust-auth decision rests on that property.

The attack needs no docker-group membership. The connect is an AF_INET socket to a routable address, and the kernel routing decision doe

### #30 [minor] /api/ready blocks 30 s before it reports unready

File: `crates/store/src/lib.rs:105` — IDs: R2, U4

**Claim.** `connect` builds the pool with no `acquire_timeout`, so sqlx's 30 s default applies, and the readiness probe holds the request open for 30 s before it can answer 503 during exactly the outage it exists to report.

**Evidence.**

```
crates/store/src/lib.rs:105-108:
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .connect(&cfg.database_url)
        .await?;
~/.cargo/registry/src/*/sqlx-core-0.9.0/src/pool/options.rs:160: `acquire_timeout: Duration::from_secs(30),`

Measured against the real binary with the database blackholed by a TCP proxy:
  ready before: HTTP/1.0 200 OK
  ready during outage: HTTP/1.0 503 Service Unavailable after 30.0s
```

**Failure scenario.** The database stops answering. An operator or monitor scrapes `web:8080/api/ready` over the compose network, as deploy/Caddyfile:11-14 instructs. Instead of a prompt `{"ready":false}`, the request hangs for 30 s; any scraper with a shorter client timeout (a Docker `healthcheck` with the usual 5-10 s `timeout:`, or a load-balancer probe) records a timeout rather than the 503 the handler is documented to return (crates/web/src/lib.rs:57-59), and each pending probe pins an axum task and a socket for the full 30 s.

**Refuter.** The claim is correct and I reproduced it with the real binary. `connect` at /home/deploy/dev/cadus2.0/crates/store/src/lib.rs:105-108 sets only `max_connections(16)`. It sets no `acquire_timeout`, so the sqlx-core 0.9.0 default of 30 s applies. The readiness handler at /home/deploy/dev/cadus2.0/crates/web/src/lib.rs:60-62 acquires from that pool, so the request holds open for the full 30 s before the 503.

I found no mitigation in the repository. /home/deploy/dev/cadus2.0/docker-compose.yml defines no `healthcheck` for the `web` service, and /home/deploy/dev/cadus2.0/deploy/Caddyfile:11-14 tells the operator to scrape `web:8080/api/ready` over the compose network. So the documented scrape path is the affected path.

My measurement makes the defect wider than the report. The report shows only the blackhole shape. A refused connection gives the same 30 s delay, because sqlx retries the con

### #45 [minor] The redacting Debug impl for DbConfig is asserted nowhere; replacing it with a derive leaks the database password and the suite stays green

File: `crates/store/src/lib.rs:61` — IDs: R2, C3

**Claim.** `impl Debug for DbConfig` exists only to keep the DSN password out of logs and panic messages, and no test in the workspace constructs a `DbConfig` or formats one, so the guard can be removed without a single failing assertion.

**Evidence.**

```
crates/store/src/lib.rs:59-67:

    /// The connection string holds the database password. Keep it out of every log
    /// line and every panic message.
    impl std::fmt::Debug for DbConfig {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("DbConfig")
                .field("database_url", &"<redacted>")
                .finish()
        }
    }

`grep -rn "DbConfig\|redacted" crates/*/tests/` returns nothing. The test functions in the workspace are the 15 listed by `grep -rn "fn " crates/*/tests/*.rs`; none names `DbConfig`, and `DbConfig::from_env` has four branches (lines 45-54) with no test either.
```

**Failure scenario.** A later change replaces the hand-written impl with `#[derive(Debug)]` — a routine cleanup, since the struct has one field — or adds `tracing::debug!(?cfg, "store: config")` in a crate that never learned why the impl exists. `cargo test --workspace` reports 15 passed. The self-host deployment then writes `DbConfig { database_url: "postgresql://cadus_app:<password>@db:5432/cadus" }` into the container log on every start, where `docker logs` and any log shipper pick it up.

**Refuter.** The claim is demonstrable, so I cannot refute it. The hand-written `impl Debug for DbConfig` (crates/store/src/lib.rs:59-67) is a pure redaction guard, and nothing in the workspace holds it in place. No test constructs a `DbConfig`, no test calls `DbConfig::from_env`, no test or script or CI step mentions "redacted", and the subprocess tests that run the binaries with a real DSN assert only positive `contains` on the captured output, never the absence of the DSN. I proved removability: in a scratchpad copy of the repo I deleted the impl and replaced it with `#[derive(Clone, Debug)]`, then ran the gate steps against the throwable Postgres. `cargo test --workspace` reported 15 passed, 0 failed, and `cargo clippy --all-targets --workspace -- -D warnings` exited 0. The guard therefore falls out silently, exactly as the reviewer describes, which matters for R2/C3 because `cadus-web` and `cadu

## FIX4 — crates/web/** + crates/worker/** + new dependency-pin tests

### #9 [major] cadus-web graceful shutdown never completes while a client holds a half-sent request

File: `crates/web/src/bin/cadus-web.rs:86` — IDs: R2, U4

**Claim.** `axum::serve(...).with_graceful_shutdown(shutdown_signal())` has no deadline and no HTTP header-read timeout, so one connection that opened a request and never finished the headers keeps the process alive forever after SIGTERM.

**Evidence.**

```
crates/web/src/bin/cadus-web.rs:85-90:
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    pool.close().await;

Measured against the real binary (debug build, DATABASE_URL=cadus_app on a migrated throwaway DB):
  mode=idle    exited=True  rc=0 after 0.1s      (completed keep-alive request, held open)
  mode=open    exited=True  rc=0 after 0.1s      (accepted TCP connection, nothing sent)
  mode=partial exited=False rc=None after 30.1s  (sent `GET /api/health HTTP/1.1\r\nHost: x\r\n`, no terminating CRLF)
The log shows `cadus-web: graceful shutdown starts` and then nothing; the process had to be SIGKILLed.
```

**Failure scenario.** An operator runs `docker compose up -d --build` to deploy. One client (a scanner, a slow mobile link, or a stalled proxy) has a connection with headers begun but not terminated. Compose sends SIGTERM to the `web` container; `axum::serve` logs `graceful shutdown starts` and then waits without bound. docker-compose.yml sets no `stop_grace_period`, so Docker's 10 s default expires and the container is SIGKILLed, exit 137. Every in-flight request is dropped and the documented contract in the module header (`crates/web/src/bin/cadus-web.rs:10-12`: "let the open requests finish, and exit 0") is violated on every deploy that a single stalled connection is present.

**Refuter.** I tried to refute the claim and failed. I reproduced the hang on the real binary, so the mechanism is demonstrable, not theoretical.

What I confirmed:

1. The code has no deadline. crates/web/src/bin/cadus-web.rs:85-87 awaits axum::serve(...).with_graceful_shutdown(shutdown_signal()) directly. No tokio::time::timeout wraps the await, and no header-read timeout or request timeout layer exists. A grep over crates/, docs/, Dockerfile, and README.md finds no timeout in the web path. The single "timeout" hit in crates/web is a test-client socket read timeout (crates/web/tests/http.rs:75).

2. The measurement repeats exactly. I built nothing new. I used the existing target/debug binaries, created a throwaway database cadus2_rv_shutdown on the given DSN, applied the 6 migrations, and started cadus-web as cadus_app on 127.0.0.1:18099. I then sent SIGTERM in three states. Idle keep-alive and bar

### #10 [major] cadus-worker ignores SIGTERM for as long as a heartbeat query is in flight

File: `crates/worker/src/lib.rs:129` — IDs: R4, U5

**Claim.** `heartbeat(pool).await?` runs inside the `tokio::select!` branch body, so while the tick's database work is pending the shutdown future is not polled at all; with no statement timeout and no TCP keepalive on the pool, a stuck query makes the worker unstoppable by SIGTERM.

**Evidence.**

```
crates/worker/src/lib.rs:128-139:
    loop {
        tokio::select! {
            biased;
            _ = &mut shutdown => break,
            _ = interval.tick() => {
                heartbeat(pool).await?;
                ticks += 1;
                tracing::info!("heartbeat tick={ticks}");
            }
        }
    }

Measured against the real binary through a TCP proxy that stops relaying bytes but keeps the sockets open (a blackholed database):
  worker alive after 3s: True
  exited=False rc=None after 60.1s   (SIGTERM sent while a heartbeat was mid-query)
The process had to be SIGKILLed. `crates/store/src/lib.rs:105-108` builds the pool with `PgPoolOptions::new().max_connections(16)` only — no `acquire_timeout`, no socket or statement timeout.
```

**Failure scenario.** The database node stops answering on an established connection (network partition, a Postgres backend wedged in an uninterruptible wait, a stalled overlay network). The worker's next tick blocks inside `heartbeat`. The operator runs `docker compose stop worker`; the SIGTERM is delivered but the select loop never gets polled again, so the worker cannot break out. Docker's 10 s grace expires and the container is SIGKILLed (exit 137), contradicting the binary's documented "exits 0 after a clean stop" (crates/worker/src/bin/cadus-worker.rs:4-5) and the M0 U5 acceptance check "stops on SIGTERM". The M0 test only exercises the idle case (crates/worker/tests/run.rs:72-113), so it passes. The exposure grows in M4/M5, whose pool refill and `FOR UPDATE SKIP LOCKED` claims are documented to go inside this same branch body (crates/worker/src/lib.rs:109-115).

**Refuter.** The claim is demonstrable and I reproduced it twice with the real binary, so it stands. tokio::select! polls the shutdown future only between branch bodies; `biased` orders the first poll of each pass and does not preempt a branch body already suspended inside `.await`. `heartbeat(pool).await?` (crates/worker/src/lib.rs:129) is such a body, and sqlx applies no timeout to statement execution, so a SIGTERM that arrives while a heartbeat query waits for a response is registered by the signal driver and never polled. The worker then survives SIGTERM indefinitely and needs SIGKILL, which contradicts the documented "exits 0 after a clean stop" (crates/worker/src/bin/cadus-worker.rs:4-5) and gives container exit 137 instead of 0 under the compose 10 s grace. One piece of the reviewer's evidence is wrong but does not save the code: PgPoolOptions is not timeout-free — sqlx-core-0.9.0 defaults acq

### #12 [major] `binary_exits_3_with_a_superuser_dsn` fails when the shell already sets RUST_LOG

File: `crates/web/tests/http.rs:211` — IDs: U4, C3

**Claim.** The test asserts on child-process stderr produced by a `tracing` subscriber whose level comes from the inherited `RUST_LOG`, and unlike the worker test it never sets `RUST_LOG` on the child, so an ambient log setting turns a correct binary into a red test.

**Evidence.**

```
http.rs:198-214 spawns the child with `.env("DATABASE_URL", &dsn)` and `.env("BIND_ADDR", ...)` and nothing else, then asserts `stderr.contains("bypasses RLS")`. The message comes from `tracing::error!` in crates/web/src/bin/cadus-web.rs:43, filtered by `EnvFilter::try_from_default_env()` (cadus-web.rs:55). The sibling worker test does set it — crates/worker/tests/run.rs:79 `.env("RUST_LOG", "info")`. Reproduced:
```
$ RUST_LOG=off CADUS_TEST_DATABASE_URL=... ./http-… binary_exits_3_with_a_superuser_dsn --exact
thread '…' panicked at crates/web/tests/http.rs:211:5:
stderr does not name the reason: 
test result: FAILED. 0 passed; 1 failed
```
```

**Failure scenario.** A developer exports `RUST_LOG=off` (or `error` scoped to another target, or `cadus_store=debug` which replaces the default directive) and runs `scripts/gate.sh`. The gate fails on a C3 test with the message `stderr does not name the reason:` and an empty tail, pointing at the boot guard rather than at the environment. The reverse also holds: a run under an unrelated RUST_LOG value can hide the assertion's intent, since the test never controls the level it depends on.

**Refuter.** The claim is demonstrable and I confirmed it by execution. crates/web/tests/http.rs:198-203 spawns the cadus-web child with only DATABASE_URL and BIND_ADDR, so the child inherits the ambient RUST_LOG. crates/web/src/bin/cadus-web.rs:55 builds its filter with EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")), so an inherited RUST_LOG fully replaces the "info" default and decides whether the tracing::error! line at cadus-web.rs:43 reaches stderr. The assertion at http.rs:211 then depends on a value the test never controls. The sibling worker test does control it (crates/worker/tests/run.rs:79 sets RUST_LOG=info), which confirms the asymmetry. scripts/gate.sh neither sets nor clears RUST_LOG, so the ambient value flows through cargo test into the child and the gate fails on a C3 test for an environment reason. The exit-code assertion at line 210 passes in every ca

### #19 [major] The worker heartbeat query is asserted nowhere; deleting the database round trip keeps both worker tests green

File: `crates/worker/src/lib.rs:148` — IDs: R4

**Claim.** `heartbeat()` is the worker's only real work in M0 and its only liveness signal, but no test observes that it touches the database, so replacing its body with `Ok(())` passes the whole suite while the `heartbeat tick=N` log line becomes a lie.

**Evidence.**

```
The doc comment on `run` (crates/worker/src/lib.rs lines 101-103) states the guarantee under test: "Each tick runs one `SELECT 1` on the pool and logs `heartbeat tick=<n>` at info level. The query proves that the pool still reaches the database, so a dead connection shows up in the log instead of at the first real job."

Mutation applied in a scratch copy — the whole body of `heartbeat` replaced:

    async fn heartbeat(pool: &PgPool) -> Result<(), WorkerError> {
        let _ = pool;
        Ok(())
    }

`cargo test --workspace` result:
  test run_counts_ticks_until_shutdown ... ok
  test binary_ticks_and_exits_zero_on_sigterm ... ok
  test result: ok. 2 passed; 0 failed

Neither test reaches for evidence of the query: `run_counts_ticks_until_shutdown` (crates/worker/tests/run.rs:58-65) only asserts `3 <= ticks <= 4`, and `binary_ticks_and_exits_zero_on_sigterm` (line 107-110) only asserts `log.contains("heartbeat tick=2")` — both are satisfied by the tick counter alone.
```

**Failure scenario.** The database goes away under a running worker (Postgres restart, network partition, credentials revoked after `ALTER ROLE cadus_admin NOLOGIN`). The intended behavior is that `heartbeat(pool).await?` propagates the error out of `run`, the binary logs it and exits 2, and compose's `restart: unless-stopped` restarts it. Because nothing pins the query, any refactor that drops it — an early return, a cached result, a `#[cfg]` guard, an error arm that logs and returns `Ok(())` — merges green. The worker then prints `heartbeat tick=N` at info level forever against a dead pool and still exits 0 on SIGTERM, so the operator's only M0 health signal reports healthy while the process cannot reach the database at all; the breakage surfaces first in M4/M5 at the first real `serving_pool` refill or `diagnosis_jobs` claim.

**Refuter.** The claim is demonstrable, and I reproduced it against a clean cluster. `heartbeat` at /home/deploy/dev/cadus2.0/crates/worker/src/lib.rs:147-153 is the only database access inside the tick loop. The only two worker tests (/home/deploy/dev/cadus2.0/crates/worker/tests/run.rs) read the tick counter and the log text alone: `run_counts_ticks_until_shutdown` asserts `3 <= ticks <= 4` (lines 58-65), and `binary_ticks_and_exits_zero_on_sigterm` asserts exit code 0 and `log.contains("heartbeat tick=2")` (lines 100-110). Both values come from `ticks += 1` and `tracing::info!("heartbeat tick={ticks}")` in `run`, not from the query. No other test in the workspace calls `cadus_worker::run` or observes the pool, and `crates/worker/src/lib.rs` holds no unit-test module.

I also did a check of the full merge gate, not only `cargo test`, because a stale-query check is the one step that could still catc

### #22 [major] The gate enforces R3 core purity mechanically but enforces R4 nowhere, while SELF_HOST.md tells the operator it does

File: `docs/SELF_HOST.md:64` — IDs: R4, L6, T1

**Claim.** No step of scripts/gate.sh, the CI job, or any test inspects crates/web or crates/worker for a model call, an HTTP client, or a network dependency, so the documented promise that "a change that puts one on a request path does not merge" is false and R4 — an M0 key requirement ID — ships with zero mechanical enforcement.

**Evidence.**

```
docs/SELF_HOST.md:63-65 — "- **Latency and token budgets (L\*, T\*).** The benchmarks land with M4 and M5 / and run in the same gate job. Model calls run in the worker (R4); a change that / puts one on a request path does not merge."

HANDOVER.md:79-81 — "**Budgets are merge gates.** A change that puts a model call on a request path, or breaks an L* number in the milestone's benchmark, does not merge — there is no \"fix it later\" lane for NFR-L/NFR-T."

The whole of scripts/gate.sh:23-48 is:
  cargo fmt --all --check
  cargo clippy --all-targets --workspace -- -D warnings
  cargo test --workspace
  cargo sqlx prepare --check --workspace -- --all-targets
  scripts/check_migrations.sh
  echo "GATE OK"
None reads crates/web/Cargo.toml, crates/worker/Cargo.toml, or handler source.

The twin requirement R3 *is* enforced, by crates/core/tests/purity.rs:14 and :76 —
  const FORBIDDEN: [&str; 6] = ["tokio", "sqlx", "axum", "hyper", "reqwest", "tower"];
  ...
  assert!(!keys.contains(name), "R3: cadus-core must not depend on `{name}`; ...")

`grep -rn "reqwest|hyper-tls|anthropic|openai|http-client" crates/ --include=*.toml --include=*.rs` returns exactly one line: crates/core/tests/purity.rs:14. There is no web or worker analogue of the purity test, and docs/plans/M0.md:44-51 lists no R4 acceptance check for any of U1-U6 — the only R4 evidence in the tree is the doc comment at crates/web/src/lib.rs:5-10.
```

**Failure scenario.** An M5 unit implements A4 diagnosis and, to avoid the queue round trip, adds `reqwest = { workspace = true }` to crates/web/Cargo.toml and awaits the model call inside the grade handler in crates/web/src/lib.rs. The agent runs scripts/gate.sh: fmt passes, clippy -D warnings passes (reqwest raises no lint), `cargo test --workspace` passes (the 15 tests assert health/ready bodies, RLS SQLSTATEs, exit codes, and tick counts — none reads a manifest or a handler), `cargo sqlx prepare --check` passes (the SQL did not change), check_migrations.sh passes (migrations/ did not change). The script prints GATE OK and the change merges. L6 ("Any model call that a learner waits on: none exist"), T1 (0 model tokens on the grade path), and R4 are all broken, with the p95 of L2 now bound to provider latency, and nothing in the build cycle reports it. The identical edit against crates/core is rejected within seconds by crates/core/tests/purity.rs.

**Refuter.** The claim is demonstrable, and I reproduced it end to end. R4 has no mechanical enforcement anywhere in the M0 tree: no gate step, no CI step, and no test reads crates/web/Cargo.toml, crates/worker/Cargo.toml, or any handler source. The only manifest-inspecting test is crates/core/tests/purity.rs, and it reads one file — env!("CARGO_MANIFEST_DIR")/Cargo.toml, which resolves to crates/core/Cargo.toml only. There is no clippy.toml (so no disallowed_types/disallowed_methods list), no deny.toml or cargo-deny step, no --locked on the workspace build, and scripts/ holds exactly two files (gate.sh, check_migrations.sh). docs/plans/M0.md units U1-U6 name a purity acceptance check for R3 alone; the sole R4 evidence in the tree is the doc comment at crates/web/src/lib.rs:1-10. So the present-tense sentence at docs/SELF_HOST.md:64, under the heading "Where the budgets are checked", and the HANDOVER

### #31 [minor] A non-Unicode BIND_ADDR is silently discarded and the server binds 0.0.0.0:8080

File: `crates/web/src/bin/cadus-web.rs:95` — IDs: R2, U4

**Claim.** `std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string())` collapses `VarError::NotUnicode` into the default, so a corrupted BIND_ADDR makes the process listen on every interface instead of failing, unlike `WorkerConfig::from_env`, which rejects the same condition.

**Evidence.**

```
crates/web/src/bin/cadus-web.rs:95: `let raw = std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_BIND_ADDR.to_string());`
crates/worker/src/lib.rs:53-57 handles the same case explicitly:
    Err(std::env::VarError::NotUnicode(_)) => {
        return Err(WorkerError::Config(format!("{TICK_SECS_VAR} is not valid Unicode")));
    }

Measured: the binary started with BIND_ADDR set to the bytes `127.0.0.1:19099\xff` logs
  `INFO cadus_web: cadus-web: listening address=0.0.0.0:8080`
and keeps running (exit code 2 for a bad BIND_ADDR is never reached).
```

**Failure scenario.** An operator restricts the listener to loopback because a host-level reverse proxy already fronts the service, and the value reaches the process with a corrupted byte (a mangled env file, a shell locale mismatch, an orchestrator that writes raw bytes). `bind_addr()` throws the value away without a word and binds `0.0.0.0:8080`, so the tutor is reachable on every interface of the host instead of on loopback, and nothing in the start sequence reports the discarded setting.

**Refuter.** The claim is demonstrable. I reproduced it end to end with the real binary against a live Postgres, and the refutation attempt failed.

`bind_addr()` at /home/deploy/dev/cadus2.0/crates/web/src/bin/cadus-web.rs:95 uses `unwrap_or_else(|_| ...)`. The closure takes every `VarError`, so `NotUnicode` and `NotPresent` give the same result: the default `0.0.0.0:8080`. The process starts on all interfaces and no log line names the discarded value.

The same value without the corrupted byte stops the process with exit code 2 and a clear message. One byte therefore turns a hard start failure into a silent bind to every interface.

The asymmetry with the worker is real. `WorkerConfig::from_env` at /home/deploy/dev/cadus2.0/crates/worker/src/lib.rs:53-57 matches `VarError::NotUnicode(_)` and returns `WorkerError::Config`. Its doc comment states the principle that the web binary breaks: "a silent fa

### #39 [minor] Signal handlers install only after the pool opens, so SIGTERM during the 30 s connect kills the process by signal

File: `crates/web/src/bin/cadus-web.rs:86` — IDs: M0-U4, M0-U5, C3

**Claim.** cadus-web and cadus-worker register their SIGTERM/Ctrl-C handlers only when `shutdown_signal()` is first polled, which happens after `cadus_store::connect` returns; a SIGTERM that arrives during the up-to-30 s pool-acquire wait therefore takes the default disposition and terminates the process by signal, not with the documented exit code 0.

**Evidence.**

```
cadus-web.rs:64 `let pool = cadus_store::connect(&cfg).await` runs before cadus-web.rs:86 `.with_graceful_shutdown(shutdown_signal())`. Measured on this box:

  $ DATABASE_URL=postgresql://cadus_app@10.255.255.1:5432/cadus BIND_ADDR=127.0.0.1:18080 ./target/debug/cadus-web &
  $ sleep 2; kill -TERM $!; wait $!
  rc=143            # stderr empty

  $ DATABASE_URL=postgresql://cadus_admin@10.255.255.1:5432/cadus ./target/debug/cadus-worker &
  $ sleep 2; kill -TERM $!; wait $!
  worker rc=143     # stderr empty

Control, signal after the loop starts: `worker rc=0`. The wait length is the sqlx default acquire timeout:

  rc=2 elapsed_s=30
  ERROR cadus_web: cadus-web: database error: pool timed out while waiting for an open connection

The cadus-web.rs header states "Exit codes: 0 for a clean stop, 2 for a start error, 3 for the boot guard."
```

**Failure scenario.** The Postgres container is down or unreachable. An operator runs `docker compose restart` or `docker compose down`. Each web/worker container is inside the 30 s connect window, so it never sees the SIGTERM as a handled signal. On a host it exits 143; as container PID 1 the default action does not apply, so the signal is ignored and Docker sends SIGKILL after the 10 s grace period (exit 137). Neither path is the documented clean stop, and every restart attempt costs another 30 s of silent hang with no log line at all.

**Refuter.** The claim is correct and fully demonstrable. `shutdown_signal` is an `async fn` in both binaries, so the call only builds a future. The body — `tokio::signal::unix::signal(SignalKind::terminate())` — runs on the first poll, not at the call site. In /home/deploy/dev/cadus2.0/crates/web/src/bin/cadus-web.rs, line 64 is `let pool = cadus_store::connect(&cfg).await` and line 86 is `.with_graceful_shutdown(shutdown_signal())`; the first poll happens inside the `axum::serve(...).await` on lines 85-87, which is after `connect` returns. /home/deploy/dev/cadus2.0/crates/worker/src/bin/cadus-worker.rs has the same order: `connect` first, then `cadus_worker::run(&pool, &cfg, shutdown_signal())`. The wait length comes from /home/deploy/dev/cadus2.0/crates/store/src/lib.rs, where `connect` sets `max_connections(16)` and keeps the sqlx default 30 s acquire timeout. During that window no SIGTERM handle

### #40 [minor] An empty WORKER_TICK_SECS silently falls back to 5 s, against the documented contract

File: `crates/worker/src/lib.rs:62` — IDs: M0-U5, R4

**Claim.** `WorkerConfig::from_env` returns the 5 s default for an empty or whitespace-only `WORKER_TICK_SECS`, although the doc comment three lines above states that a value which is not a positive whole number of seconds is a configuration error "because a silent fallback hides an operator mistake".

**Evidence.**

```
crates/worker/src/lib.rs:60-63

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Self::default());
    }

Measured on this box:

  WORKER_TICK_SECS= ...      -> INFO worker: loop starts tick_ms=5000
  WORKER_TICK_SECS="   " ... -> INFO worker: loop starts tick_ms=5000
  WORKER_TICK_SECS=0 ...     -> ERROR cadus-worker: configuration error: WORKER_TICK_SECS must be 1 or more

Compare `DbConfig::from_env` (crates/store/src/lib.rs:45), which rejects an empty `DATABASE_URL` as `StoreError::Config`.
```

**Failure scenario.** An operator adds `WORKER_TICK_SECS=` to `.env` and forgets the number, or a templating step substitutes an empty value. The worker starts and reports success. M4 pool refill and M5 queue claims then run at 5 s instead of the intended period, and no log line names the discarded setting, so the misconfiguration is invisible until a throughput problem is investigated.

**Refuter.** The claim is demonstrable, and I reproduced it on this box with the built binary. `WorkerConfig::from_env` (/home/deploy/dev/cadus2.0/crates/worker/src/lib.rs:49-79) has an explicit branch at lines 60-63 that trims the raw value and returns `Self::default()` when the result is empty. An empty or whitespace-only `WORKER_TICK_SECS` therefore gives a 5 s tick, and no log line names the discarded setting. The doc comment three lines above, at lines 44-48, states the opposite policy: "A value that is not a positive whole number of seconds is a configuration error, because a silent fallback hides an operator mistake." An empty string is a present value, not an absent variable; the code proves that the two cases are separate, because `VarError::NotPresent` returns the default at line 52 and the empty-string case needs its own branch at line 61. So the code contradicts its own contract.

I looke

### #42 [minor] cadus-web's documented exit code 2 and its default BIND_ADDR are untested; both can be changed silently

File: `crates/web/tests/http.rs:194` — IDs: C3, M0-U4

**Claim.** The only binary-level assertions are exit 3 (boot guard) and exit 0 (SIGTERM); nothing covers the documented `2 for a start error` path or the default bind address, so both can be mutated without a failing test.

**Evidence.**

```
crates/web/src/bin/cadus-web.rs:12  "Exit codes: 0 for a clean stop, 2 for a start error, 3 for the boot guard."
crates/web/src/bin/cadus-web.rs:23  const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
Both binary tests set BIND_ADDR explicitly (crates/web/tests/http.rs:200, :232).

Mutation run:
  -const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
  +const DEFAULT_BIND_ADDR: &str = "127.0.0.1:9999";
  -ExitCode::from(2)
  +ExitCode::from(7)

  cargo test --workspace -> 15/15 green, SURVIVED
```

**Failure scenario.** A refactor changes DEFAULT_BIND_ADDR to a loopback address. The gate passes. docker-compose.yml sets BIND_ADDR explicitly today, so the break is invisible until someone runs the image without that variable (the Dockerfile documents 8080 as the default) and Caddy gets connection refused with the container reporting itself healthy. Likewise a changed start-error exit code silently breaks any supervisor that distinguishes 2 from 3.

**Refuter.** The claim holds. I copied the repo to a scratch directory, applied both mutations (DEFAULT_BIND_ADDR "0.0.0.0:8080" -> "127.0.0.1:9999"; ExitCode::from(2) -> ExitCode::from(7)), and ran the tests. Both binary tests passed with the mutated binary, so both mutants survive.

Inspection gives the same result. crates/web/tests/http.rs has 6 tests. Tests 1, 2, 3, and 6 drive the router and the guard in process and never start the binary, so they cannot see either constant. Test 4 (line ~200) sets BIND_ADDR=127.0.0.1:0 and asserts exit code 3. Test 5 (line ~232) sets BIND_ADDR to a free port and asserts exit code 0 after SIGTERM. No test leaves BIND_ADDR unset, so bind_addr() never reads DEFAULT_BIND_ADDR under test. No test constructs a start error (a bad DATABASE_URL, an unparsable BIND_ADDR, or a port that is in use), so no test asserts exit code 2.

The gate adds no other cover. scripts/gat

### #43 [minor] The worker tick test asserts a wall-clock tick count with no upper bound on how slow the heartbeat query may be

File: `crates/worker/tests/run.rs:58` — IDs: R4, M0-U5

**Claim.** run_counts_ticks_until_shutdown fixes a 50 ms period against a 180 ms budget and asserts 3 <= ticks <= 4, but each tick performs a real Postgres round trip, so the assertion depends on database and scheduler latency that the test never bounds.

**Evidence.**

```
crates/worker/tests/run.rs:46-65
  tick: Duration::from_millis(50)
  shutdown: tokio::time::sleep(Duration::from_millis(180))
  assert!(ticks >= 3, ...); assert!(ticks <= 4, ...);
crates/worker/src/lib.rs:123 interval.set_missed_tick_behavior(MissedTickBehavior::Delay) — a slow tick pushes the next one out and the loop never catches up, so cumulative heartbeat latency subtracts directly from the tick count.
crates/worker/src/lib.rs:135 each tick awaits heartbeat(pool), a `SELECT 1` over TCP to Postgres.
Honest note: I could not reproduce a failure on this 16-core box, including 300 concurrent spin loops and pinning the test to one saturated CPU (5 runs, all ok). The margin is the claim, not an observed flake: roughly 80 ms of cumulative added latency across the first three heartbeats drops the count to 2.
```

**Failure scenario.** On a 2-core GitHub runner with the Postgres service in a sibling container and four cargo test binaries running in parallel, three heartbeat round trips take ~40 ms each instead of ~1 ms. The loop reaches only 2 ticks in 180 ms and the test fails with `the loop must reach at least 3 ticks in 180 ms, it reached 2` on a commit that changed nothing in the worker.

**Refuter.** I tried to refute the claim and failed. The test is a real wall-clock flake, and I reproduced the exact failure twice on this box with an unmodified worker.

Mechanism, corrected. The reviewer's arithmetic is wrong, but the conclusion is right. `MissedTickBehavior::Delay` does not accumulate sub-period latency: a heartbeat that takes less than the 50 ms period leaves the next deadline where it was. Write h1 and h2 for the first two heartbeat round trips. The third tick starts at s3 = max(50, h1) + max(50, h2). The `ticks >= 3` assertion at crates/worker/tests/run.rs:58 holds only while s3 < 180. So the true tolerance is: one heartbeat up to about 130 ms, or two heartbeats up to about 90 ms each. It is not "80 ms cumulative across three heartbeats", and the count drops to 1, not 2, when the first heartbeat alone passes 180 ms.

That tolerance is still an unbounded quantity. `heartbeat` (c

## FIX5 — .github, Dockerfile, .dockerignore, docker-compose.yml, .env.example, scripts/*, README.md, docs/SELF_HOST.md, migration comments are FIX2

### #1 [blocker] CI gate can never pass: the gate database is created but never migrated

File: `.github/workflows/ci.yml:55` — IDs: R2, D9, M0-U5

**Claim.** The CI job creates `cadus2_ci` with `cargo sqlx database create` (create only, no migrations), then `scripts/gate.sh` line 36 runs `cargo sqlx prepare --check` against that empty database, so the query macros fail to compile and the gate step always exits non-zero.

**Evidence.**

```
ci.yml:54-57 `- name: Create the gate database / run: cargo sqlx database create / env: DATABASE_URL: ${{ env.CADUS_TEST_DATABASE_URL }}` (no `sqlx migrate run`, no `sqlx database setup`).
gate.sh:36 `DATABASE_URL="$CADUS_TEST_DATABASE_URL" cargo sqlx prepare --check --workspace -- --all-targets`.
Reproduced on an empty database of the throwaway cluster:
$ docker exec cadus2-testdb psql -U test -d postgres -c 'CREATE DATABASE cadus2_rv_correct'
$ DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_rv_correct cargo sqlx prepare --check --workspace -- --all-targets
error: error returned from database: relation "users" does not exist at line 1449
  --> crates/store/src/test_support.rs:92:9
error: could not compile `cadus-store` (lib) due to 1 previous error
error: `cargo check` failed with status: exit status: 101
The same command against the migrated database passes:
$ DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate cargo sqlx prepare --check --workspace -- --all-targets
    Finished `dev` profile
```

**Failure scenario.** A developer pushes any commit. The `gate` job installs sqlx-cli, creates the empty `cadus2_ci`, and runs `scripts/gate.sh`. fmt, clippy, and `cargo test` pass (they compile offline from `.sqlx`). The `cargo sqlx prepare --check` step then aborts with `relation "users" does not exist`, and the job fails. The merge gate of HANDOVER.md stage 3 is red on every push and pull request, so it gives no signal about the code.

**Refuter.** The claim is demonstrable and I could not refute it. .github/workflows/ci.yml creates cadus2_ci with `cargo sqlx database create` only; there is no `sqlx migrate run`, no `database setup`, and no `cadus-migrate` step. Nothing else populates that database before gate.sh line 36: there is no .env at the repo root and no build.rs in any crate, so no alternate DATABASE_URL and no build-time migration; `cargo test --workspace` (gate.sh line 30) runs earlier but every test goes through TestDb::create(), which makes a separate `cadus2_t_<hex>` database, migrates that one, and drops it, using the DSN database as a maintenance connection only (crates/store/tests/rls.rs, crates/web/tests/http.rs, crates/worker/tests/run.rs all use TestDb); scripts/check_migrations.sh works on `<base>_migcheck_fresh`, states it never touches the base database, and runs after the prepare step anyway. I reproduced th

### #3 [blocker] CI never migrates cadus2_ci, so `cargo sqlx prepare --check` fails on every gate run

File: `.github/workflows/ci.yml:54` — IDs: U5, R2, D9 — duplicate of #1

**Claim.** The CI job creates the empty database `cadus2_ci` and never applies the migrations to it, so the `cargo sqlx prepare --check` step inside `scripts/gate.sh` cannot expand a single query macro and the gate exits non-zero on every push and pull request.

**Evidence.**

```
ci.yml:54-57 `- name: Create the gate database` / `run: cargo sqlx database create` / `env: DATABASE_URL: ${{ env.CADUS_TEST_DATABASE_URL }}` — the only DDL the job runs. gate.sh:36 `DATABASE_URL="$CADUS_TEST_DATABASE_URL" cargo sqlx prepare --check --workspace -- --all-targets`. Reproduced against an empty database on the throwable cluster:
```
$ DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_rv_tq cargo sqlx prepare --check --workspace -- --all-targets
error: error returned from database: relation "users" does not exist at line 1449
  --> crates/store/src/test_support.rs:92:9
error: could not compile `cadus-store` (lib) due to 1 previous error
error: `cargo check` failed with status: exit status: 101
```
The same command against the hand-migrated local database `cadus2_gate` finishes with exit 0. The base database of the developer DSN on this box (`postgres`) also holds no tables (`\dt` -> `Did not find any relations.`), so the documented local flow in README.md:9-10 fails the same way. Nothing in the repo states that the DSN database must carry the schema first.
```

**Failure scenario.** Push any commit. The `gate` job reaches `Run the gate`, passes fmt, clippy and the tests (they build offline from `.sqlx` and create their own databases), then dies at the prepare step with `relation "users" does not exist`. CI is red on every commit of M0, and the one guard that keeps `.sqlx` current — the guard the Dockerfile depends on, since it builds with `SQLX_OFFLINE=true` (Dockerfile:26) — never runs green.

**Refuter.** The claim is correct and I reproduced it. The `gate` job creates `cadus2_ci` with `cargo sqlx database create` (`.github/workflows/ci.yml:54-57`), which creates an empty database and applies no migration. No other step in the job, and no step in `scripts/gate.sh`, applies the migrations to that database: I grepped `.github`, `scripts`, `Dockerfile`, `docker-compose.yml`, `README.md`, `HANDOVER.md`, and `docs` — the only `cargo sqlx migrate run` calls live in `scripts/check_migrations.sh:125` and `:157`, and they run against the derived database `<base>_migcheck_fresh`, never against the base database (the header of that script states this at lines 13-15). `scripts/gate.sh:36` runs `DATABASE_URL="$CADUS_TEST_DATABASE_URL" cargo sqlx prepare --check --workspace -- --all-targets`. `cargo sqlx prepare` compiles with the online macro path, so every `query!`/`query_scalar!` macro connects to t

### #4 [blocker] CI creates the gate database but never migrates it, so the gate job fails on every push and PR

File: `.github/workflows/ci.yml:55` — IDs: D9, R2, C3 — duplicate of #1

**Claim.** The CI job creates an empty `cadus2_ci` database and then runs `scripts/gate.sh`, whose `cargo sqlx prepare --check` step compiles `crates/store/src/test_support.rs` against that database and fails with `relation "users" does not exist`; nothing in the repository ever applies the migrations to the DSN's own database.

**Evidence.**

```
ci.yml:54-57 `- name: Create the gate database` / `run: cargo sqlx database create`. Simulating the job exactly (create empty DB, then run the gate):
$ DATABASE_URL=.../cadus2_rv_ops_ci2 cargo sqlx database create --no-dotenv  -> ok
$ CADUS_TEST_DATABASE_URL=.../cadus2_rv_ops_ci2 ./scripts/gate.sh
== cargo sqlx prepare --check --workspace -- --all-targets
error: error returned from database: relation "users" does not exist at line 1449
  --> crates/store/src/test_support.rs:92:9
error: `cargo check` failed with status: exit status: 101
GATE EXIT=1
The same failure occurs with the DSN the repo itself documents (crates/store/src/test_support.rs:39, README.md:10):
$ CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/postgres ./scripts/gate.sh -> same error, GATE EXIT=1
(`cargo sqlx migrate info` on that database reports `1/pending ... 6/pending`.)
With the same database migrated first, the identical command passes (`Finished dev profile`, exit 0). The local box only passes because a hand-migrated database `cadus2_gate` exists on the test cluster (`select count(*) from _sqlx_migrations` -> 6); CI has no such database.
```

**Failure scenario.** Any push or pull request runs the `gate` job: checkout, toolchain, sqlx-cli install, `cargo sqlx database create` (empty `cadus2_ci`), then `scripts/gate.sh`. fmt, clippy and tests pass, then step 4 fails with `relation "users" does not exist` and the job exits 1. The merge gate that HANDOVER.md §2.3 calls non-negotiable is red 100% of the time and can never be green without a manual `cargo sqlx migrate run` step that no file in the repo performs.

**Refuter.** The claim is correct and reproducible. `.github/workflows/ci.yml` sets `CADUS_TEST_DATABASE_URL: postgresql://test:test@localhost:5432/cadus2_ci`, then runs one database step only — `cargo sqlx database create` (ci.yml:53-57) — which makes an EMPTY database. The next and last step runs `scripts/gate.sh`. Step 4 of that script (`scripts/gate.sh:35-36`) exports `DATABASE_URL="$CADUS_TEST_DATABASE_URL"` and runs `cargo sqlx prepare --check --workspace -- --all-targets`. That command turns the offline mode off and compiles every `sqlx::query!` macro against the live DSN, so `cadus2_ci` must hold the schema. No file in the repository applies the migrations to that database. The earlier gate steps do not help: `cargo fmt`, `cargo clippy` and `cargo test` run with `DATABASE_URL` unset, so they compile from the checked-in `.sqlx/` data, and the tests migrate only their own throwaway databases `c

### #13 [major] The builder stage downloads a second complete Rust toolchain, so the image build needs static.rust-lang.org

File: `Dockerfile:19` — IDs: R1, D9

**Claim.** `rust:1-bookworm` installs a minimal-profile toolchain named `1.98.0` with no rustfmt and no clippy, while `rust-toolchain.toml` pins `channel = "stable"` and `components = ["rustfmt", "clippy"]`, so rustup downloads and installs a whole extra toolchain inside the builder before `cargo build --release --workspace` can start.

**Evidence.**

```
$ docker run --rm rust:1-bookworm sh -c 'rustup toolchain list; rustup component list --installed'
1.98.0-x86_64-unknown-linux-gnu (active, default)
cargo-x86_64-unknown-linux-gnu / rust-std-x86_64-unknown-linux-gnu / rustc-x86_64-unknown-linux-gnu   (no rustfmt, no clippy)
$ docker run --rm -v <repo>:/src:ro -w /src rust:1-bookworm sh -c 'cargo --version; rustup toolchain list'
info: syncing channel updates for stable-x86_64-unknown-linux-gnu
info: downloading 5 components
stable-x86_64-unknown-linux-gnu (active)
1.98.0-x86_64-unknown-linux-gnu (default)
$ docker run --rm --network none -v <repo>:/src:ro -w /src rust:1-bookworm sh -c 'cargo --version'
info: syncing channel updates for stable-x86_64-unknown-linux-gnu
error: could not download file from 'https://static.rust-lang.org/dist/channel-rust-stable.toml.sha256' ... dns error
```

**Failure scenario.** A self-host operator builds on a server whose egress is limited to the container registry (or behind a proxy that does not cover static.rust-lang.org). `docker compose up -d --build` fails in the builder stage before a single crate compiles, with a rustup download error rather than a build error. Where egress is open, every source change busts the `COPY . .` layer and the build re-downloads five toolchain components (rustfmt and clippy are never used in the image) on top of the full dependency rebuild.

**Refuter.** The claim is correct in each part, and I reproduced all of it with Docker on this machine.

1. Base image. `rust:1-bookworm` installs a minimal-profile toolchain with the numeric name `1.98.0-x86_64-unknown-linux-gnu`. Its installed components are cargo, rust-std, and rustc only. rustfmt and clippy are absent.

2. Toolchain file. `/home/deploy/dev/cadus2.0/rust-toolchain.toml` holds `channel = "stable"` and `components = ["rustfmt", "clippy"]`. rustup treats the name `stable` as a different toolchain from the name `1.98.0`, even at the same version. The two names do not alias.

3. Build context. `/home/deploy/dev/cadus2.0/.dockerignore` excludes only `target`, `.git`, `.gitignore`, `.claude`, `node_modules`, `.env`, and `docker-compose.override.yml`. It does not exclude `rust-toolchain.toml`, so `COPY . .` at Dockerfile:29 puts the file at `/src/rust-toolchain.toml`, one directory above 

### #17 [major] check_migrations.sh passes on an edited already-shipped migration, and the failure lands in production instead

File: `scripts/check_migrations.sh:118` — IDs: D9

**Claim.** The migration check always works on a brand-new database it drops first, so it can never detect a checksum change to a migration that a running deployment already applied; the gate stays green and `cadus-migrate` fails on the next upgrade.

**Evidence.**

```
scripts/check_migrations.sh:118 `sqlx_db drop -y >/dev/null 2>&1 || true` then :122 `sqlx_db create` — every run starts from an empty database, so sqlx's ledger has nothing to compare against. Proof, with one comment line appended to migrations/0003_event_log.sql:

  $ bash scripts/check_migrations.sh
  PASS: name    -- 6 migration files, numbered 0001 to 0006 with no gap
  PASS: fresh   -- 6 migrations applied on cadus2_rv_ops_migcheck_fresh, 0 pending
  PASS: rerun   -- the second migrate run applied nothing
  PASS: drop    -- dropped database cadus2_rv_ops_migcheck_fresh
  CHECK_EXIT=0

  $ cargo sqlx migrate run   # against a database that already ran 0001-0006
  error: migration 3 was previously applied but has been modified
  MIGRATE_EXIT=1

docs/plans/M0.md:10 states the rule the script is supposed to hold: "Migrations: `migrations/NNNN_name.sql`, forward-only". docs/SELF_HOST.md:60 bills the script as "Migration discipline (D9)".
```

**Failure scenario.** A developer edits migrations/0003_event_log.sql (adds a column to the CREATE TABLE, or even just a comment) instead of adding 0007. scripts/gate.sh and the CI gate both report PASS. The operator follows docs/SELF_HOST.md:21-23 (`docker compose up -d --build`) on a server that already has the schema. The `migrate` service exits non-zero on `migration 3 ... has been modified`, `restart: "no"` keeps it dead, and `web` and `worker` never start because both wait on `condition: service_completed_successfully`. The whole stack is down and the only signal is a compose log line.

**Refuter.** The claim is demonstrable and I reproduced it. `scripts/check_migrations.sh:118` runs `sqlx_db drop -y` and `:122` runs `sqlx_db create`, so check (b) always starts from an empty database. sqlx compares a migration checksum only against the rows of `_sqlx_migrations` in the target database. An empty database holds no row, so the checksum of an edited file is never compared with anything. Checks (a), (c), and (d) do not help: (a) reads file names only, (c) re-runs on the same fresh database, (d) drops it.

No other part of M0 closes the gap. I searched the workspace for a checksum guard: there is none. `crates/store/src/test_support.rs` creates a new `cadus2_t_<hex>` database per test and migrates it, so `cargo test` is also blind to an edited-and-already-applied migration. `cargo sqlx prepare --check` compares queries with `.sqlx/`, not migration checksums. The CI job (`.github/workflows

### #27 [minor] Dockerfile claims cadus-migrate reads /app/migrations at runtime; sqlx embeds the files at compile time

File: `Dockerfile:61` — IDs: D9

**Claim.** The COPY comment states the binary reads the SQL from `/app/migrations`, but `sqlx::migrate!("../../migrations")` embeds the files into the binary when the builder stage compiles, so the copied directory is never read.

**Evidence.**

```
Dockerfile:61-62 `# The SQL migrations ship as data. cadus-migrate reads them from this path.` / `COPY migrations /app/migrations`
crates/store/src/lib.rs:28-30 `/// The migration set of the repository. `sqlx::migrate!` embeds the files at` / `/// compile time, so the binaries carry the schema and need no file access.` / `static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");`
```

**Failure scenario.** An operator follows the comment and patches a migration by mounting a volume over `/app/migrations`, or edits a file inside the running image, then re-runs `docker compose up migrate`. `cadus-migrate` applies the version that was compiled into the binary, prints `cadus-migrate: applied 0 migrations (6 total)`, and exits 0. The operator reads that as success while the database never received the change.

**Refuter.** I tried to refute the claim and failed. The claim is demonstrable from the code.

The comment at /home/deploy/dev/cadus2.0/Dockerfile:61 makes two statements. The first statement ("The SQL migrations ship as data") is correct. The second statement ("cadus-migrate reads them from this path") is false.

Proof of the mechanism:
1. /home/deploy/dev/cadus2.0/crates/store/src/bin/cadus-migrate.rs:28 calls `cadus_store::migrate(&pool)`. That binary opens no file and reads no path. It reads only `DATABASE_URL`.
2. /home/deploy/dev/cadus2.0/crates/store/src/lib.rs:116 makes `migrate` call `MIGRATOR.run(pool)`. `MIGRATOR` is the `sqlx::migrate!("../../migrations")` static at lib.rs:30.
3. The sqlx 0.9.0 macro expansion puts the SQL text into the binary at compile time. In /home/deploy/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/sqlx-macros-core-0.9.0/src/migrate.rs the macro emits `includ

### #28 [minor] The documented bring-up check calls curl, which the runtime image does not install

File: `docs/SELF_HOST.md:17` — IDs: M0-U6

**Claim.** Step 5 of the bring-up tells the operator to run `docker compose exec web curl ... /api/health`, but the runtime stage installs only `ca-certificates` and `postgresql-client` on `debian:bookworm-slim`, which ships no curl and no wget.

**Evidence.**

```
docs/SELF_HOST.md:17 `   docker compose exec web curl -fsS http://127.0.0.1:8080/api/health`
Dockerfile:36 `FROM debian:bookworm-slim AS runtime`
Dockerfile:47-49 `RUN apt-get update \` / ` && apt-get install -y --no-install-recommends ca-certificates postgresql-client \` / ` && rm -rf /var/lib/apt/lists/*`
```

**Failure scenario.** An operator brings the stack up and runs the third check of step 5. The command fails with `exec: "curl": executable file not found in $PATH` and a non-zero exit code. The operator reads that as a broken web service and starts to debug a healthy container.

**Refuter.** The claim is demonstrable, so I cannot refute it. docs/SELF_HOST.md:17 tells the operator to run `docker compose exec web curl -fsS http://127.0.0.1:8080/api/health`. The `web` service in docker-compose.yml:71-73 uses the `*app-image` anchor, so it runs the image that Dockerfile builds. The runtime stage starts from `debian:bookworm-slim` (Dockerfile:36) and installs only `ca-certificates` and `postgresql-client` (Dockerfile:47-49). It then adds only the three Rust binaries and `migrations/` (Dockerfile:57-62). Neither the base image nor those two packages supply `curl` or `wget`, and no other file in the repo installs an HTTP client. I built a runtime-equivalent image and probed it: no curl, no wget. A direct exec of the documented command failed with the exact error text and exit code 127 of the claimed failure scenario. The step-5 check therefore fails on a healthy container, which is

### #29 [minor] /api/ready is documented as reporting worker state; the handler only runs SELECT 1

File: `docs/SELF_HOST.md:66` — IDs: R4

**Claim.** docs/SELF_HOST.md and deploy/Caddyfile both state that `/api/ready` reports datastore and worker state, but the handler sends one `SELECT 1` through the web pool and knows nothing about `cadus-worker`.

**Evidence.**

```
docs/SELF_HOST.md:66 `- **Runtime.** `/api/ready` reports datastore and worker state.`
deploy/Caddyfile:11-12 `# Operational endpoints stay off the internet. /api/ready reports datastore` / `# and worker state.`
crates/web/src/lib.rs:60-65 `async fn ready(State(state): State<AppState>) -> Response {` / `    match sqlx::query_scalar!(r#"SELECT 1 AS "one!""#)` / `        .fetch_one(&state.pool)` / `        .await` / `    {` / `        Ok(1) => (StatusCode::OK, Json(json!({ "ready": true }))).into_response(),`
The crate has no reference to the worker: the response body is exactly `{"ready":true}` or `{"ready":false}`.
```

**Failure scenario.** An operator wires a monitor to `web:8080/api/ready` and treats a 200 as proof that the async layer runs. The `worker` container crashes on a bad DSN and exits 2. `/api/ready` keeps answering 200 with `{"ready":true}` because the database still replies, so diagnosis jobs (A4) pile up unclaimed and no alert fires.

**Refuter.** The factual core of the claim is directly demonstrable and I could not refute it. crates/web/src/lib.rs defines AppState with exactly one field (pub pool: PgPool); the ready handler runs a single `SELECT 1` against that pool and returns only {"ready":true} or {"ready":false}. A grep for "worker" across crates/web/ returns only `worker_threads = 2` inside two #[tokio::test] attributes, which is unrelated tokio runtime configuration. The gap is absolute rather than merely unimplemented: crates/worker/src/lib.rs heartbeat() runs `SELECT 1` and logs at trace/info level, writing no row, table, or timestamp, so nothing the worker does is observable from the web pool. docker-compose.yml has no healthcheck on the worker service and Dockerfile has no HEALTHCHECK, so the worker has no monitoring path at all and the two prose lines are the operator's only guidance. Two qualifications shrink the fin

### #32 [minor] gate.sh prints GATE OK when the migration check script is missing

File: `scripts/gate.sh:44` — IDs: U1, U5, D9

**Claim.** The gate treats an absent `scripts/check_migrations.sh` as a skip, then falls through to `echo "GATE OK"` and exits 0, which contradicts the script's own stated rule that a gate must not skip a check.

**Evidence.**

```
gate.sh:16-17 states the rule: "The database steps are part of the gate, not an option. A gate that skips half its checks is not a gate". gate.sh:44-48 breaks it:
```
else
    echo "SKIPPED: scripts/check_migrations.sh (file does not exist)"
fi

echo "GATE OK"
```
The unset-DSN case on lines 18-21 does the correct thing (`exit 2`); the missing-script case does not.
```

**Failure scenario.** An agent working in a worktree deletes or renames `scripts/check_migrations.sh`, or a merge drops it. `scripts/gate.sh` prints one `SKIPPED:` line among ~40 lines of cargo output, then `GATE OK`, and exits 0. The migration name/sequence/idempotence check (D9) silently stops running, and the orchestrator merges on a green gate that no longer covers it.

**Refuter.** The claim is correct and demonstrable. I did not refute it.

1. The code does what the reviewer says. /home/deploy/dev/cadus2.0/scripts/gate.sh lines 38-48 test for the file. If the file is absent, the else branch prints one SKIPPED line. Control then reaches `echo "GATE OK"` at line 48 and the script exits 0. I ran the extracted branch with the file absent and got `SKIPPED: ...` then `GATE OK` then exit code 0. `set -euo pipefail` does not stop this, because the else branch reads no unset variable and returns success.

2. The contradiction with the script's own rule is real. Lines 16-21 state that the database steps are part of the gate, not an option, and an unset CADUS_TEST_DATABASE_URL exits 2. The migration check is one of those database steps, and it uses the same DSN at lines 40 and 43. The two cases get opposite treatment: an unset DSN is fatal, a missing check script is free.

3

### #33 [minor] docs/SELF_HOST.md bring-up check runs curl inside an image that has no curl

File: `docs/SELF_HOST.md:17` — IDs: D9 — duplicate of #28

**Claim.** The documented verification command `docker compose exec web curl -fsS http://127.0.0.1:8080/api/health` cannot work, because the runtime stage installs only `ca-certificates` and `postgresql-client` on `debian:bookworm-slim`, which ships no curl and no wget.

**Evidence.**

```
$ docker run --rm cadus2_rv_ops:test sh -c 'command -v curl || echo "NO curl"; command -v psql'
NO curl
/usr/bin/psql
$ docker run --rm debian:bookworm-slim sh -c 'command -v curl || echo "NO curl"; command -v wget || echo "NO wget"'
NO curl
NO wget
Dockerfile:47-49 installs `ca-certificates postgresql-client` only.
```

**Failure scenario.** An operator follows step 5 of the bring-up procedure and gets `OCI runtime exec failed: exec: "curl": executable file not found in $PATH`. The only documented end-to-end check of a fresh deployment cannot be run, and the operator has no way to distinguish a missing tool from a broken web service.

**Refuter.** The claim is demonstrable, so I cannot refute it. docs/SELF_HOST.md line 17 documents `docker compose exec web curl -fsS http://127.0.0.1:8080/api/health` as step 5 of the bring-up check. The `web` service in docker-compose.yml (lines 71-73) uses the shared `x-app-image` build, so it runs the image from Dockerfile. The runtime stage is `debian:bookworm-slim` (line 36) and installs only `ca-certificates postgresql-client` with `--no-install-recommends` (lines 47-49). I reproduced that exact package set in a container: no curl, no wget, no nc, no python3. Only `psql` and `openssl` are present, and neither does a plain HTTP GET on port 8080. The command therefore fails with an exec lookup error, not a health result. Two secondary points also hold: no other document gives an alternate end-to-end check (grep of README.md, HANDOVER.md, REQUIREMENTS.md for `api/health` and `api/ready` returns n

### #34 [minor] The image copies migrations to /app/migrations but cadus-migrate never reads that directory

File: `Dockerfile:62` — IDs: D9 — duplicate of #27

**Claim.** The Dockerfile comment states "The SQL migrations ship as data. cadus-migrate reads them from this path", but `crates/store/src/lib.rs:30` uses `sqlx::migrate!("../../migrations")`, which embeds the SQL at compile time; the copied `/app/migrations` tree is never opened at runtime.

**Evidence.**

```
Dockerfile:61-62 `# The SQL migrations ship as data. cadus-migrate reads them from this path.` / `COPY migrations /app/migrations`
crates/store/src/lib.rs:30 `static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");`
Proof with the directory masked empty:
$ docker run --rm --network host --tmpfs /app/migrations -e DATABASE_URL=... cadus2_rv_ops:test sh -c 'ls -a /app/migrations; cadus-migrate'
.
..
cadus-migrate: applied 6 migrations (6 total)
```

**Failure scenario.** An operator hot-patches a broken migration by mounting a corrected file over `/app/migrations`, or drops a new `0007_*.sql` there, and re-runs `docker compose run --rm migrate`. The command prints `applied 0 migrations (6 total)` and exits 0. The operator reads that as success while the patch or the new migration was silently ignored, and the image now carries two migration sets that can disagree.

**Refuter.** The claim is correct, and I reproduced both halves of it without Docker. `crates/store/src/lib.rs:30` uses `sqlx::migrate!("../../migrations")`. That macro embeds the SQL text in the binary at compile time. The file's own doc comment on lines 27-29 says the same thing: "`sqlx::migrate!` embeds the files at compile time, so the binaries carry the schema and need no file access." `crates/store/src/bin/cadus-migrate.rs` calls `cadus_store::migrate(&pool)`, which runs that embedded `MIGRATOR`. It opens no path. Nothing in the repository names `/app/migrations` except the `COPY` on `Dockerfile:62`; a grep over the tree (excluding `target/` and `.claude/`) returns that single line. So the Dockerfile comment on line 61 makes a false statement about the runtime behavior, and the copied tree is inert data. The failure scenario is also real: a new or corrected SQL file at that path changes nothing

### #35 [minor] SITE_ADDRESS ships as a placeholder domain, so the documented http-only bring-up cannot work

File: `.env.example:14` — IDs: C3

**Claim.** docs/SELF_HOST.md:8-9 tells the operator to `cp .env.example .env` and "Leave the default `:80`" for an http-only test on a bare IP, but `.env.example` assigns `SITE_ADDRESS=math.example.com`, so the copied file never yields the `:80` default.

**Evidence.**

```
.env.example:14 `SITE_ADDRESS=math.example.com`
docs/SELF_HOST.md:8-9 `2. \`cp .env.example .env\`, and set SITE_ADDRESS to your domain. Leave the default \`:80\` for an http-only test on a bare IP.`
deploy/Caddyfile:8 `{$SITE_ADDRESS} {` — the value becomes the Caddy site address, i.e. a host matcher plus automatic HTTPS when it is a domain.
```

**Failure scenario.** An operator testing on a bare IP copies `.env.example` unchanged, as step 2 permits, and runs `docker compose up -d --build`. Caddy loads the site `math.example.com`, repeatedly attempts an ACME challenge for a domain the operator does not control, and answers requests to `http://<server-ip>/` with no matching site instead of proxying to `web`. The documented http-only test path never works unless the operator edits the file the doc said to leave alone.

**Refuter.** The claim is demonstrable, so I cannot refute it. The `:80` default exists only in `docker-compose.yml:110` as `${SITE_ADDRESS:-:80}`. Bash `:-` substitution applies only when the key is unset or empty. `/home/deploy/dev/cadus2.0/.env.example:14` assigns a non-empty value, `math.example.com`. A copied `.env.example` therefore always suppresses the default, and the instruction at `docs/SELF_HOST.md:8-9` to "Leave the default `:80`" cannot be obeyed. I proved this: I copied `docker-compose.yml` and `.env.example` (as `.env`) into a scratch directory and ran `docker compose config`; the resolved caddy service shows `SITE_ADDRESS: math.example.com`, not `:80`. `deploy/Caddyfile:8` puts that value in the site address position, so Caddy loads a host matcher for `math.example.com` plus automatic HTTPS. A request to `http://<server-ip>/` matches no site, and Caddy starts ACME attempts for a doma

### #37 [minor] README presents CADUS_TEST_DATABASE_URL as optional, but gate.sh refuses to run any check without it

File: `README.md:9` — IDs: D9

**Claim.** README.md:9-10 says `scripts/gate.sh` "runs fmt, clippy, and the tests" and that setting `CADUS_TEST_DATABASE_URL` only "adds the two SQL steps", but `scripts/gate.sh:18-21` exits 2 before the fmt step when the variable is unset.

**Evidence.**

```
README.md:9-10 `Run \`scripts/gate.sh\` before every commit. It runs fmt, clippy, and the tests. Set \`CADUS_TEST_DATABASE_URL\` to a superuser Postgres DSN to add the two SQL steps.`
scripts/gate.sh:16-21 `# The database steps are part of the gate, not an option. ... if [ -z "${CADUS_TEST_DATABASE_URL:-}" ]; then` / `echo "GATE FAILED: set CADUS_TEST_DATABASE_URL"` / `exit 2`
```

**Failure scenario.** A new contributor follows the README, runs `scripts/gate.sh` before a commit without the variable, and gets `GATE FAILED: set CADUS_TEST_DATABASE_URL` with exit code 2 and zero checks executed — not the fmt, clippy and test run the README promises. The README also gives no hint that the DSN's own database must already carry the schema, which is the precondition the CI job violates.

**Refuter.** The claim is demonstrable by direct execution, so the refutation fails. `scripts/gate.sh` lines 16-21 make `CADUS_TEST_DATABASE_URL` a hard precondition: with the variable unset the script prints `GATE FAILED: set CADUS_TEST_DATABASE_URL` and exits 2 at line 20, before the first `cargo fmt` step at line 23. README.md lines 9-10 describe the opposite contract — that the script "runs fmt, clippy, and the tests" unconditionally, and that the DSN only "add[s] the two SQL steps". A reader takes the variable as optional and the three Rust steps as guaranteed. Neither is true. The script's own comment at line 16 states the intended design ("The database steps are part of the gate, not an option"), which confirms the script is correct and the README is the stale half. The defect is real: the README must say that the DSN is required and that the gate runs nothing without it.

I confirmed the revi

### #38 [minor] SELF_HOST.md tells the operator the BYPASSRLS worker re-enters RLS through SET ROLE, and no such code exists

File: `docs/SELF_HOST.md:47` — IDs: C3, R4

**Claim.** The self-host document states in the present tense that the worker runs `SET ROLE cadus_app` inside each per-tenant unit of work, but the shipped worker contains no `SET ROLE` statement at all, so the only process that holds BYPASSRLS runs with row-level security off for its whole life.

**Evidence.**

```
docs/SELF_HOST.md:47 — `worker does \`SET ROLE cadus_app\` inside each per-tenant unit of work, so RLS` / `stays a backstop there.`

`grep -rn "set_config\|SET ROLE" crates/ --include=*.rs` returns only:
```
crates/store/src/lib.rs:168:/// `set_config(..., true)` makes the setting local to the transaction, so the
crates/store/src/lib.rs:177:        "SELECT set_config('app.user_id', $1, true)",
```
crates/worker/src/bin/cadus-worker.rs:44-49 confirms the opposite intent: the worker "logs the role but does NOT call `assert_rls_enforced`", and it never narrows the role afterward.
```

**Failure scenario.** An operator reads the role model section before opening ports, and accepts `ALTER ROLE cadus_admin LOGIN` on the strength of the stated backstop. The worker service then runs continuously as a BYPASSRLS login role with no policy applied and with full UPDATE and DELETE on `events`. When M4 and M5 add pool refill and queue claims to the tick loop, every one of those statements runs cross-tenant by default, and the documented backstop that would have caught a wrong predicate is not present in the code. The same claim appears in migrations/0001_roles.sql:46, so a reader who checks the schema finds the assertion repeated, not contradicted.

**Refuter.** The claim is demonstrable and I cannot refute it. docs/SELF_HOST.md:47 and migrations/0001_roles.sql:46 both assert, in the present tense, that the worker does `SET ROLE cadus_app` inside each per-tenant unit of work. No `SET ROLE` statement exists anywhere in the shipped code: `grep -rn "SET ROLE" crates/ migrations/` returns only that one SQL comment, and no .rs file matches. docker-compose.yml:92 connects the worker as `cadus_admin`, which migrations/0001_roles.sql:40 creates with BYPASSRLS, and docker-compose.yml:56 grants that role LOGIN in every bring-up. crates/worker/src/bin/cadus-worker.rs:44-49 states the opposite intent and only logs the role, so the process keeps BYPASSRLS for its whole life. The absence is wider than the reviewer states: cadus_store::begin_tenant (crates/store/src/lib.rs:171-186), the only unit-of-work helper in the workspace, sets `app.user_id` alone and co

### #44 [minor] check_migrations.sh uses a fixed database name, so two runs on one cluster silently destroy each other's database

File: `scripts/check_migrations.sh:49` — IDs: D9

**Claim.** The throwaway database is named `<base>_migcheck_fresh` with no per-run suffix, and line 118 drops it unconditionally at the start, so a second run on the same cluster deletes the first run's database mid-check and the first run fails with an error that names no cause.

**Evidence.**

```
scripts/check_migrations.sh:49 `fresh_db="${base_db}_migcheck_fresh"` and :118 `sqlx_db drop -y >/dev/null 2>&1 || true`.

Two runs started 0.3 s apart against the one shared cluster:

  === A ===
  PASS: name    -- 6 migration files, numbered 0001 to 0006 with no gap
  FAIL: fresh   -- migrate info failed on cadus2_rv_tq_migcheck_fresh: error: error returned from database: database "cadus2_rv_tq_migcheck_fresh" does not exist at line 1085
  SKIP: rerun   -- the fresh check failed
  A exit=1
  === B ===
  PASS: fresh   -- 6 migrations applied on cadus2_rv_tq_migcheck_fresh, 0 pending
  B exit=0

`crates/store/src/test_support.rs:49` already solves this for the test suite with `format!("cadus2_t_{}", &Uuid::new_v4()...)`; the script does not.
```

**Failure scenario.** HANDOVER.md asks for serialized gate runs, but nothing enforces it and the box carries seven parallel worktrees against one `cadus2-testdb` cluster. A developer runs `scripts/gate.sh` while another agent's gate run is in its migration-check step. One run reports `FAIL: fresh -- ... does not exist`, which reads as a broken migration set, and the developer spends the debugging time on migrations that are in fact correct. The second run also drops a database it does not own.

**Refuter.** I tried to refute the claim and failed. The code reads exactly as the reviewer states, and I reproduced the failure on the shared cluster.

Facts in the file:
- scripts/check_migrations.sh:49 builds one fixed name: fresh_db="${base_db}_migcheck_fresh". No PID, no timestamp, no UUID. Two runs with the same DATABASE_URL get the same database name.
- :118 `sqlx_db drop -y >/dev/null 2>&1 || true` starts the fresh check with an unconditional drop. The `|| true` hides the outcome, so a run that deletes another run's live database is silent.
- The EXIT trap (:63-71) drops the same fixed name a second time, also with no ownership test. A run that never created the database still prints `PASS: drop` after it deleted a database that belongs to a different run.

Reproduction (two runs, 0.3 s apart, one cluster, base name cadus2_rv_ref) matches the reviewer's transcript line for line:

  === A ===


### #46 [minor] The gate and CI never build the image or validate the compose file, though M0.md makes both U6's acceptance check

File: `.github/workflows/ci.yml:59` — IDs: C3, D9

**Claim.** The only CI step is `scripts/gate.sh`, and that script runs fmt, clippy, tests, `cargo sqlx prepare --check`, and `check_migrations.sh` — nothing builds the Dockerfile or runs `docker compose config`, so the whole ops surface named in docs/plans/M0.md:49 merges unverified.

**Evidence.**

```
.github/workflows/ci.yml:59-60 (the last and only build/test step):
```
      - name: Run the gate
        run: scripts/gate.sh
```
`grep -n -i docker .github/workflows/ci.yml scripts/gate.sh` returns nothing.
docs/plans/M0.md:49: `| U6 | \`Dockerfile\`, \`docker-compose.yml\` (db + migrate + web + worker) | \`docker compose config\` validates; image builds |`
```

**Failure scenario.** A developer renames the binary target in crates/store/Cargo.toml:14 from `cadus-migrate` to `cadus-migrator` and updates every Rust caller. `cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace`, `cargo sqlx prepare --check`, and `check_migrations.sh` all pass, so the gate reports `GATE OK` and CI is green. Dockerfile:59 still says `COPY --from=builder /src/target/release/cadus-migrate /usr/local/bin/cadus-migrate`, so the failure first appears on the operator's server at `docker compose up -d --build`, after the commit is already on the default branch. The same blind spot covers every Dockerfile and compose defect already on file for this milestone.

**Refuter.** The claim survives. I tried to find a second CI job, a docker step inside the gate, or a Rust test that ties the binary names to the Dockerfile. None exists.

1. `/home/deploy/dev/cadus2.0/.github/workflows/ci.yml` is the only workflow file. `.github/workflows/` holds `ci.yml` and nothing else. The job has 6 steps: checkout, toolchain, cargo cache, `cargo install sqlx-cli`, `cargo sqlx database create`, and `scripts/gate.sh` at lines 59-60. `scripts/gate.sh` is the last step and the only build/test step.

2. `scripts/gate.sh` runs exactly five commands: `cargo fmt --all --check`, `cargo clippy --all-targets --workspace -- -D warnings`, `cargo test --workspace`, `cargo sqlx prepare --check --workspace -- --all-targets`, and `scripts/check_migrations.sh`. It then prints `GATE OK`. The word `docker` does not appear in it, except in one comment in `scripts/check_migrations.sh:18` about a `ps
