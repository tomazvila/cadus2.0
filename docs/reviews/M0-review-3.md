# M0 adversarial review — round 3 (2026-08-25)

Run on the tree after FIX7 (commit ffe0f62). Two find/refute rounds, major+ only: 31 raised, 16 confirmed. Assigned to FIX8a (grants + RLS tests), FIX8b (store bin + test support), FIX8c (web bin, tests, compose, docs).

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `docker-compose.yml:84` | FIX8c | The migrate one-shot applies every migration under a 5 s statement_timeout that the compose file never forwards, so one slow or lock-blocked DDL statement takes the whole stack down with no operator knob |
| 2 | major | `crates/store/src/bin/cadus-migrate.rs:106` | FIX8b | The review-2 #11 fix is wrong: cadus-migrate's advisory lock is database-scoped, so two migrate runs on one cluster still collide on ALTER ROLE |
| 3 | major | `crates/store/src/bin/cadus-migrate.rs:88` | FIX8b (dup of #1) | cadus-migrate silently inherits the 5 s DB_STATEMENT_TIMEOUT_MS default and applies it to the migration run, so a blocked or slow migration exits 2 instead of waiting |
| 4 | major | `migrations/0006_grants_rls.sql:93` | FIX8a | The round-2 is_admin fix covers UPDATE only: cadus_app still writes is_admin = true and a chosen id through INSERT, contradicting the migration comment and docs/SCHEMA.md |
| 5 | major | `migrations/0006_grants_rls.sql:93` | FIX8a (dup of #4) | cadus_app keeps table-wide INSERT on users, so the runtime role mints an is_admin account that the round-2 column list was supposed to make impossible |
| 6 | major | `migrations/0006_grants_rls.sql:53` | FIX8a | ALTER DEFAULT PRIVILEGES grants cadus_app arwd on views too, and both guard tests enumerate only tables, so a view in a later migration silently re-opens UPDATE and DELETE on events |
| 7 | major | `crates/web/src/bin/cadus-web.rs:175` | FIX8c | cadus-web spends SHUTDOWN_DEADLINE_SECS twice, so the stop takes 20.01 s against a 20 s stop_grace_period and Docker sends SIGKILL |
| 8 | major | `crates/store/src/bin/cadus-migrate.rs:185` | FIX8b | The ALTER ROLE password escape is a mutation survivor; deleting it opens role-option injection into a superuser statement |
| 9 | major | `crates/store/tests/store_api.rs:126` | FIX8b | A panicking test body leaks a LOGIN SUPERUSER cluster role on the shared trust-auth cluster; TestDb cleans up databases only |
| 10 | major | `docker-compose.yml:102` | FIX8c | The documented bring-up check for `web` greps `listening on`, a string `cadus-web` never logs, and the compose file cites that string as the reason `web` carries no healthcheck |
| 11 | major | `migrations/0006_grants_rls.sql:91` | FIX8a | users_read USING (true) plus the table-wide SELECT grant exposes every user's password_hash and is_admin to every tenant |
| 12 | major | `migrations/0006_grants_rls.sql:45` | FIX8a | cadus_app holds USAGE and SELECT on model_call_log_id_seq, so the runtime role does reach the T6 ledger — the exact claim that keeps model_call_log outside RLS |
| 13 | major | `crates/store/src/bin/cadus-migrate.rs:62` | FIX8b | cadus-migrate aborts with a Rust panic (exit 101) on a non-Unicode argument instead of the documented usage + exit 2 |
| 14 | major | `crates/web/tests/http.rs:276` | FIX8c | A panicking binary test orphans a cadus-web server process, which then runs forever holding its listen port |
| 15 | major | `docker-compose.yml:58` | FIX8c | The db healthcheck probes the unix socket, so `migrate` starts while Postgres still refuses TCP |
| 16 | major | `docker-compose.yml:121` | FIX8c | `docker compose up -d` destroys the serving web and worker before `migrate` proves it can succeed |

## FIX8a

### #4 [major] The round-2 is_admin fix covers UPDATE only: cadus_app still writes is_admin = true and a chosen id through INSERT, contradicting the migration comment and docs/SCHEMA.md

File: `migrations/0006_grants_rls.sql:93` — IDs: C3

**Claim.** `REVOKE UPDATE ON users` plus the column-level `GRANT UPDATE (email, password_hash, email_verified_at, disabled_at, created_at)` narrows only UPDATE, while `GRANT ... INSERT ... ON ALL TABLES` (line 40) and `CREATE POLICY users_insert ON users FOR INSERT WITH CHECK (true)` (line 93) leave the runtime role a table-wide INSERT over every column, so `cadus_app` writes `is_admin = true` and picks its own `id` — the migration comment at lines 80-81 ("id and is_admin leave the UPDATE grant, so no policy and no code defect decides the admin flag") and docs/SCHEMA.md ("The column list keeps `id` and `is_admin` out of reach of the runtime role") are both false.

**Evidence.**

```
Live proof on a fresh database built from migrations 0001-0006, connected as `cadus_app` with NO tenant context bound (the sign-up path):
  $ psql -h 127.0.0.1 -U cadus_app -d cadus2_rv_correct
  INSERT INTO users (email, password_hash, is_admin) VALUES ('evil@x.test','h',true) RETURNING id||'|'||email||'|'||is_admin;
  6d5b4245-6164-4841-b89f-d4edad951446|evil@x.test|true
  INSERT 0 1
  INSERT INTO users (id, email, is_admin) VALUES ('00000000-0000-0000-0000-0000000000ff','evil2@x.test',true) RETURNING id||'|'||is_admin;
  00000000-0000-0000-0000-0000000000ff|true
  INSERT 0 1
Catalog on the same database:
  has_column_privilege('cadus_app','users','is_admin','INSERT') = t
  has_column_privilege('cadus_app','users','is_admin','UPDATE') = f
  has_table_privilege('cadus_app','users','INSERT')             = t
The UPDATE half really is closed (the same session gets `ERROR: permission denied for table users` on `UPDATE users SET is_admin = true`, and `INSERT ... ON CONFLICT (email) DO UPDATE SET password_hash` is refused with `new row violates row-level security policy (USING expression) for table "users"`), so INSERT is the one remaining door. crates/store/tests/rls.rs:551-566 asserts only the two UPDATE column privileges; no test asserts an INSERT column privilege, so this passes the whole suite.
```

**Failure scenario.** M5 adds the sign-up handler, which is an INSERT into `users` on the `cadus_app` connection — the one statement the `users_insert WITH CHECK (true)` policy exists to allow. A mass-assignment defect that folds request fields into the column list, or an injection into that statement, writes `is_admin = true` on the new account, and the attacker holds the admin flag the schema defines with no policy and no grant in the path. The same INSERT also chooses the primary key, so an attacker can plant a `users.id` that a later import or a fixture expects to own. Demonstrated above end to end with no tenant context bound at all.

**Refuter.** The claim is demonstrable and I could not refute it. On a fresh database built from migrations 0001-0006, the runtime role cadus_app holds table-wide INSERT on users over every column, including id and is_admin. The REVOKE at migrations/0006_grants_rls.sql:99 narrows UPDATE only; the blanket GRANT at line 40 and the users_insert WITH CHECK (true) policy at line 93 leave INSERT unnarrowed. I reproduced both INSERT statements from the evidence with no tenant context bound: cadus_app wrote is_admin = true and chose its own primary key. users carries no trigger that resets the flag. The migration comment at lines 80-81 ("id and is_admin leave the UPDATE grant, so no policy and no code defect decides the admin flag") and docs/SCHEMA.md:46 ("The column list keeps id and is_admin out of reach of the runtime role") are therefore both false as written; crates/store/tests/rls.rs:412 repeats the ov

### #5 [major] cadus_app keeps table-wide INSERT on users, so the runtime role mints an is_admin account that the round-2 column list was supposed to make impossible

File: `migrations/0006_grants_rls.sql:93` — IDs: C3 — duplicate of #4

**Claim.** The round-2 #4 fix narrowed UPDATE to a column list and added users_update_self, but INSERT stayed table-wide with a policy of WITH CHECK (true), so cadus_app writes is_admin = true on a new row; the migration's own claim at line 80-81 that "no policy and no code defect decides the admin flag" is false.

**Evidence.**

```
migrations/0006_grants_rls.sql:80-81 "-- 2. A column list. id and is_admin leave the UPDATE grant, so no policy and no code defect decides the admin flag."; line 93 "CREATE POLICY users_insert ON users FOR INSERT WITH CHECK (true);"; lines 102-103 grant a column list for UPDATE only. Live column ACL: users.is_admin -> has_column_privilege('cadus_app','users','is_admin','INSERT') = t, ...,'UPDATE') = f. As cadus_app, tenant bound:
  UPDATE users SET is_admin=true WHERE email='attacker@example.test';  -> ERROR: permission denied for table users
  INSERT INTO users (email,is_admin,password_hash) VALUES ('newadmin@example.test',true,'x') RETURNING id,email,is_admin;
  -> 23c2d9af-... | newadmin@example.test | t   (INSERT 0 1)
docs/SCHEMA.md:46 repeats the same false claim: "The column list keeps `id` and `is_admin` out of reach of the runtime role."
```

**Failure scenario.** The M5 sign-up handler builds its INSERT from the request body (the classic mass-assignment shape, or simply an extra column added later by a developer who trusts the comment at line 80). A learner posts {"email":"x@y","password":"...","is_admin":true}. The row is created with is_admin = true. No policy and no grant stops it, because users_insert is WITH CHECK (true) and the INSERT grant carries every column. The privilege pin test app_role_privilege_matrix_is_the_literal_table records users INSERT as true and checks column privileges only for UPDATE, so the suite stays green. The stated control (the UPDATE column list) never runs, because the admin flag is decided at INSERT time, not at UPDATE time.

**Refuter.** The claim is demonstrable and I could not refute it. The round-2 #4 fix closed the UPDATE path to users.is_admin only. The INSERT path stays table-wide: migrations/0006_grants_rls.sql:35 grants INSERT on all tables to cadus_app, no statement revokes INSERT on users, no column list narrows it, and line 93 makes the policy `WITH CHECK (true)`. I applied migrations 0001-0006 to a throwaway Postgres 16 database and ran the two statements as cadus_app with a tenant bound. The UPDATE failed with 42501. The INSERT created a row with is_admin = true. There is no CHECK constraint and no trigger on users (pg_trigger returned an empty list), and the DEFAULT false in 0002_identity.sql:14 is overridden by an explicit value, so nothing else in the schema decides the flag. The comment at 0006_grants_rls.sql:80-81 states the conclusion without a scope -- "so no policy and no code defect decides the admi

### #6 [major] ALTER DEFAULT PRIVILEGES grants cadus_app arwd on views too, and both guard tests enumerate only tables, so a view in a later migration silently re-opens UPDATE and DELETE on events

File: `migrations/0006_grants_rls.sql:53` — IDs: C2, C3

**Claim.** The default privilege covers every relation that Postgres classes as a table for ACL purposes, views and materialized views included; a view is owned by the migration runner (a superuser in the shipped stack) so it neither honors RLS nor the events revoke, and neither app_role_privilege_matrix_is_the_literal_table (pg_tables) nor rls_coverage_is_the_literal_list (relkind = 'r') can see it.

**Evidence.**

```
migrations/0006_grants_rls.sql:52-53 "ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO cadus_app, cadus_admin;". crates/store/tests/rls.rs:520 "FROM pg_tables t" (views excluded) and rls.rs:848 "WHERE n.nspname = 'public' AND c.relkind = 'r'" (views excluded), under a docstring at rls.rs:832 that promises "A new tenant table without its own policy fails this test." On the migrated database I ran, as the superuser migration runner: CREATE VIEW public.recent_events AS SELECT user_id, seq, ts, type, payload FROM public.events;  ->  pg_class shows relname=recent_events, relkind=v, relacl={test=arwdDxt/test,cadus_app=arwd/test,cadus_admin=arwd/test}. Neither test query returns that row. Then, as cadus_app with NO tenant context bound:
  SELECT count(*) FROM events;            -> 0        (RLS closed)
  UPDATE events SET type='tampered';      -> ERROR: permission denied for table events   (C2 revoke holds)
  SELECT user_id, seq, type FROM recent_events;  -> 2 rows, both tenants
  UPDATE recent_events SET type='tampered';      -> UPDATE 2
  DELETE FROM recent_events;                     -> DELETE 2
  SELECT count(*) FROM recent_events;            -> 0
```

**Failure scenario.** M3 or M5 adds a convenience view in a migration -- a due-review view, a session summary, a `recent_events` reporting view. The migration writes no GRANT, because 0006 comment lines 47-51 promise the default privileges handle it. The runner is the postgres superuser, so the view is auto-updatable and executes with the owner's rights: row-level security is skipped and the `REVOKE UPDATE, DELETE, TRUNCATE ON events FROM cadus_app` is skipped with it. The gate stays green, because the two tests that exist precisely to stop grant widening (findings #16 and #23) enumerate pg_tables and relkind = 'r' and never see a relkind 'v' or 'm'. The C2 append-only guarantee and the C3 tenant isolation are both gone through one CREATE VIEW that no check in the repository looks at.

**Refuter.** I cannot refute the claim. I reproduced every step of it on a fresh database at /home/deploy/dev/cadus2.0 with migrations 0001-0006 applied, and each element holds.

1. The default privilege covers views. Postgres classes views and materialized views under the TABLES object type of ALTER DEFAULT PRIVILEGES. After the superuser migration runner created a plain view, pg_class reported relkind = 'v' with relacl {test=arwdDxt/test,cadus_app=arwd/test,cadus_admin=arwd/test}. The migration wrote no GRANT. The line at migrations/0006_grants_rls.sql:52-53 supplied all four DML privileges to cadus_app.

2. The view bypasses both controls. The shipped stack runs migrations as the postgres superuser; migrations/0001_roles.sql:19-22 states this itself ("in the shipped compose stack the migration runner is the postgres superuser, which owns every object and bypasses row-level security"). A view witho

### #11 [major] users_read USING (true) plus the table-wide SELECT grant exposes every user's password_hash and is_admin to every tenant

File: `migrations/0006_grants_rls.sql:91` — IDs: C3

**Claim.** The round-2 fix for finding #4 narrowed UPDATE on users to the caller's own row and to a column list, but the SELECT side stayed a table-wide grant plus an unconditional policy, so a bound tenant reads every other account's email, password_hash, is_admin and disabled_at.

**Evidence.**

```
migrations/0006_grants_rls.sql:91 `CREATE POLICY users_read ON users FOR SELECT USING (true);` with the comment at line 90 "the login path reads users by email before a tenant context exists" — the predicate does not distinguish the pre-tenant login lookup from a bound tenant session. The SELECT grant is table-wide (line 40, never revoked for users; crates/store/tests/rls.rs APP_TABLE_PRIVILEGES pins `("users", [true, true, false, false, false])`), so it covers password_hash and is_admin.

Live proof, connected as cadus_app with app.user_id bound to tenant A:

  BEGIN; SELECT set_config('app.user_id','9e32f433-...',true);
  SELECT email, password_hash FROM users ORDER BY email;
    email    | password_hash
  -----------+---------------
   a@x.test  |
   b@x.test  | VICTIM-HASH
  SELECT email, is_admin FROM users ORDER BY email;  -> both rows returned

docs/SCHEMA.md:103 documents the same predicate and gives the same login-path reason.
```

**Failure scenario.** Any registered learner's session (or any handler defect reachable from it) runs `SELECT email, password_hash, is_admin FROM users` and gets the whole user table: every account's e-mail address and every stored password hash, ready for offline cracking, plus the list of admin accounts to target. The tenant context is correctly bound the whole time, so no policy fires and no test fails — app_role_updates_only_its_own_user_row (crates/store/tests/rls.rs) even asserts a successful cross-tenant read as part of its login-path check (`SELECT count(*) FROM users WHERE email = 'self-b@example.test'` -> 1). The pre-tenant login lookup that motivates USING (true) needs only email/password_hash for one row and could be served by a bound-context-aware predicate or a column-level grant; the shipped statement grants it to every session forever.

**Refuter.** The claim stands. I tried to refute it and failed. I applied migrations 0001-0006 to a throwaway database, connected as cadus_app (NOBYPASSRLS, NOSUPERUSER, confirmed from pg_roles), bound app.user_id to tenant A, and read every column of every other account. The two controls of the round-2 fix for finding #4 both act on UPDATE only: `REVOKE UPDATE ON users FROM cadus_app` plus a column list, and the `users_update_self` row predicate. The SELECT side keeps the blanket `GRANT SELECT ... ON ALL TABLES` of line 40 (never revoked for users; has_table_privilege('cadus_app','public.users','SELECT') is true, and has_column_privilege is true for all seven columns, id, email, password_hash, email_verified_at, is_admin, disabled_at, created_at), and the only SELECT policy is `USING (true)`. A bound tenant session therefore reads every account's email, password_hash, is_admin, and disabled_at. This

### #12 [major] cadus_app holds USAGE and SELECT on model_call_log_id_seq, so the runtime role does reach the T6 ledger — the exact claim that keeps model_call_log outside RLS

File: `migrations/0006_grants_rls.sql:45` — IDs: C3, T6, T2

**Claim.** 0006 revokes every table privilege on model_call_log from cadus_app and then documents that revoke as the reason the table needs no tenant policy, but the blanket sequence grant on line 45 (and the default privilege on line 55, which repeats it for every sequence a later migration creates) leaves cadus_app with USAGE and SELECT on that table's identity sequence.

**Evidence.**

```
migrations/0006_grants_rls.sql:43-45
  -- bigserial columns need the sequence too (model_call_log.id). Finding #5: only
  -- cadus_admin writes that table now, so the grant matters for that role.
  GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO cadus_app, cadus_admin;
migrations/0006_grants_rls.sql:54-55
  ALTER DEFAULT PRIVILEGES IN SCHEMA public
      GRANT USAGE, SELECT ON SEQUENCES TO cadus_app, cadus_admin;

Live ACL on a fresh migrated database:
  model_call_log_id_seq | {test=rwU/test,cadus_app=rU/test,cadus_admin=rU/test}

Demonstrated as cadus_app:
  select count(*) from model_call_log;              -> ERROR: permission denied for table model_call_log
  select last_value from model_call_log_id_seq;     -> 1
  select nextval('model_call_log_id_seq');          -> 2
  select nextval('model_call_log_id_seq');          -> 3

This contradicts migrations/0006_grants_rls.sql:113-119 ("The table stays outside the RLS set for a new reason: cadus_app reaches it with no statement at all") and docs/SCHEMA.md:131 ("cadus_app holds no privilege on it, so no runtime statement reaches the table").

Nothing pins it. crates/store/tests/rls.rs:505 reads pg_tables only, so no sequence appears in APP_TABLE_PRIVILEGES; default_privileges_are_the_literal_grants (rls.rs:585) asserts the pg_default_acl entry "cadus_app=rU" for object type 'S' as CORRECT, so the grant is pinned in place rather than out.
```

**Failure scenario.** A tenant-bound cadus_app connection runs SELECT last_value FROM model_call_log_id_seq and reads the cluster-wide count of model calls across every tenant — T6 cost-ledger data that the RLS exemption on model_call_log is justified by asserting cadus_app cannot reach. The same connection runs nextval in a loop and advances the ledger's primary key, so the worker's later inserts land on non-contiguous ids and any id-gap reasoning over the cost ledger is wrong. Line 55 makes this automatic for every bigserial or identity column a later migration adds, including one on a table whose grants a future revoke deliberately closes.

**Refuter.** The claim is demonstrable in full. I reproduced each part end to end on a fresh database built from migrations 0001-0006.

1. The grant is real. migrations/0006_grants_rls.sql:45 names cadus_app in a blanket sequence grant, and line 55 repeats it as a default privilege for every sequence a later migration creates. The live ACL matches the reviewer's text exactly: model_call_log_id_seq -> {test=rwU/test,cadus_app=rU/test,cadus_admin=rU/test}.

2. The revoke at line 126 closes the table only, not its identity sequence. model_call_log.id is bigserial (migrations/0005_content.sql:49), so the table has an owned sequence that REVOKE ALL ON model_call_log never touches. As cadus_app, SELECT on the table fails with 42501 permission denied, but SELECT last_value FROM model_call_log_id_seq returns a value, and nextval succeeds.

3. The documented invariant is false. migrations/0006_grants_rls.sql:

## FIX8b

### #2 [major] The review-2 #11 fix is wrong: cadus-migrate's advisory lock is database-scoped, so two migrate runs on one cluster still collide on ALTER ROLE

File: `crates/store/src/bin/cadus-migrate.rs:106` — IDs: D9, C3

**Claim.** `cadus-migrate --admin-login` takes `pg_advisory_lock(7241001)` on its own `DATABASE_URL` database, but PostgreSQL scopes a session advisory lock to the current database, so the lock does not serialize the exact case its own comment names ("Two migrate runs on one cluster (two databases, or a test next to a deploy)") and one of the two runs still dies with `tuple concurrently updated` and exit code 2.

**Evidence.**

```
crates/store/src/bin/cadus-migrate.rs:99-108 — the comment claims "A cluster-wide advisory lock serializes them. The key is the one that crates/store/tests/migrate_bin.rs holds", then runs `sqlx::query("SELECT pg_advisory_lock(7241001)").execute(&mut *lock_conn)` on a connection from the pool of the migrated database. crates/store/tests/migrate_bin.rs:51-54 states the opposite fact and acts on it: "PostgreSQL scopes an advisory lock to the database of the session, so the lock connection opens the maintenance database that CADUS_TEST_DATABASE_URL names and not the throwaway database of the test." The binary never got that treatment.

Live proof that the same key on two databases of one cluster grants twice:
  psql -d cadus2_rv_correct: SELECT pg_advisory_lock(7241001);  -- granted
  psql -d cadus2_rv_lockb  : SELECT pg_advisory_lock(7241001);  -- ALSO granted
  SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND objid=7241001 AND granted;  -> 2

End-to-end proof with the real binary, two databases on one cluster, 8 paired runs:
  CADUS_APP_PASSWORD=p1 CADUS_ADMIN_PASSWORD=p1 DATABASE_URL=.../cadus2_rv_r1 cadus-migrate --admin-login &
  CADUS_APP_PASSWORD=p2 CADUS_ADMIN_PASSWORD=p2 DATABASE_URL=.../cadus2_rv_r2 cadus-migrate --admin-login &
  ITER 1..8 all printed:
    cadus-migrate: database error: error returned from database: tuple concurrently updated
  rc summary: 4 A rc=0 / 4 A rc=2 / 4 B rc=0 / 4 B rc=2   (8 of 8 rounds lost one run)
```

**Failure scenario.** Two Cadus databases on one Postgres cluster (a staging database next to production, or a gate run next to a deploy) bring up at the same time. Both `migrate` one-shots reach `alter_roles` and run `ALTER ROLE cadus_admin LOGIN` / `ALTER ROLE cadus_app PASSWORD ...` concurrently on the shared pg_authid tuple. One process exits 2 with `tuple concurrently updated`. In docker-compose.yml the `web` and `worker` services carry `depends_on: migrate: condition: service_completed_successfully` and `migrate` carries `restart: "no"`, so that stack never starts and the site stays down until an operator re-runs `docker compose up -d` by hand. Reproduced 8 times out of 8 above.

**Refuter.** The claim is demonstrable and I reproduced it end to end with the shipped binary. PostgreSQL scopes a session advisory lock to the current database. crates/store/src/bin/cadus-migrate.rs:105 takes the lock connection from `pool.acquire()`, so the lock session sits in the database that DATABASE_URL names. Two migrate runs against two databases of one cluster both get the lock granted, both reach `alter_roles`, and both ALTER the cluster-scoped pg_authid tuples of cadus_admin / cadus_app at the same time. One run dies with `tuple concurrently updated` and exits 2. The code comment at lines 99-104 names exactly that case ("Two migrate runs on one cluster (two databases, or a test next to a deploy)") and claims "A cluster-wide advisory lock serializes them", which the code does not deliver. crates/store/tests/migrate_bin.rs:48-53 states the correct rule and opens its lock connection on the m

### #3 [major] cadus-migrate silently inherits the 5 s DB_STATEMENT_TIMEOUT_MS default and applies it to the migration run, so a blocked or slow migration exits 2 instead of waiting

File: `crates/store/src/bin/cadus-migrate.rs:88` — IDs: D9, M0-U6 — duplicate of #1

**Claim.** `cadus-migrate` builds its pool with `DbConfig::from_env()`, which defaults `statement_timeout_ms` to 5000, so every migration statement AND sqlx's migration advisory-lock wait (which sqlx documents as "this function will not return until the lock is acquired") is cancelled at 5 s with SQLSTATE 57014 and the process exits 2 — while `.env.example:69` and `docs/SELF_HOST.md:189` both tell the operator the bound applies only to "the web and worker pools".

**Evidence.**

```
crates/store/src/bin/cadus-migrate.rs:88-92 `let cfg = DbConfig::from_env()?; let pool = cadus_store::connect(&cfg).await?; ... cadus_store::migrate(&pool).await?;` — the same `DbConfig` path that crates/store/src/lib.rs:45 gives `DEFAULT_STATEMENT_TIMEOUT_MS: u64 = 5000` and crates/store/src/lib.rs:188 turns into the startup option `-c statement_timeout=5000`. docker-compose.yml:81-92 (the `migrate` service) sets no `DB_STATEMENT_TIMEOUT_MS`, so the 5000 default applies there. sqlx-postgres-0.9.0/src/migrate.rs:186-200 `lock()` runs `SELECT pg_advisory_lock($1)` with the comment "this function will not return until the lock is acquired".

Live proof — one session holds the sqlx migration lock for database `cadus2_rv_slow` (lock id 3573294902520606192 = 0x3d32ad9e * crc32(name)), then the real binary runs against that database:
  $ DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_rv_slow cadus-migrate
  cadus-migrate: migration error: while executing migrations: error returned from database: canceling statement due to statement timeout
  rc=2
  elapsed=5s
  $ psql -d cadus2_rv_slow -c "select to_regclass('public.users')"   -> (empty, nothing applied)
```

**Failure scenario.** Two overlapping `docker compose up -d` bring-ups, or a first bring-up whose migrations run longer than 5 s (a `CREATE INDEX` or a backfill in any M1+ migration), make `cadus-migrate` exit 2 after exactly 5 s with `canceling statement due to statement timeout` and zero migrations applied. `web` and `worker` declare `depends_on: migrate: condition: service_completed_successfully`, so both stay down and the operator sees a query-cancel error for a step that should simply have waited. Reproduced above: rc=2 at 5 s, schema untouched. The same 5 s bound also caps the `pg_advisory_lock(7241001)` wait at cadus-migrate.rs:106, so even the same-database serialization of `--admin-login` is only 5 s deep.

**Refuter.** The claim is correct and I reproduced it end to end with the real binary, plus a counterfactual that isolates the cause.

Mechanism, confirmed by code read:
1. /home/deploy/dev/cadus2.0/crates/store/src/bin/cadus-migrate.rs:88-92 builds the pool from `DbConfig::from_env()` and then calls `cadus_store::migrate(&pool)`.
2. /home/deploy/dev/cadus2.0/crates/store/src/lib.rs:45 sets `DEFAULT_STATEMENT_TIMEOUT_MS: u64 = 5000`, and lib.rs:111-118 returns that default when `DB_STATEMENT_TIMEOUT_MS` is absent.
3. /home/deploy/dev/cadus2.0/crates/store/src/lib.rs:188 puts the value into the connection startup options (`-c statement_timeout=5000`), so the bound holds for every connection of the pool, the migration connection included.
4. sqlx-postgres-0.9.0/src/migrate.rs:185-203 acquires the migration lock with `SELECT pg_advisory_lock($1)` on that same connection. The source comment says "this fu

### #8 [major] The ALTER ROLE password escape is a mutation survivor; deleting it opens role-option injection into a superuser statement

File: `crates/store/src/bin/cadus-migrate.rs:185` — IDs: C3, R2

**Claim.** No test gives cadus-migrate a password that contains a single quote, so the one escape that keeps CADUS_APP_PASSWORD / CADUS_ADMIN_PASSWORD inside an SQL string literal can be deleted and all 65 workspace tests stay green.

**Evidence.**

```
Production code: `let literal = password.replace('\'', "''");` then `sqlx::query(AssertSqlSafe(format!("ALTER ROLE {role} PASSWORD '{literal}'")))`.
The only test password (crates/store/tests/migrate_bin.rs:212) is `let password = format!("pw-{}", &Uuid::new_v4().simple().to_string()[..8]);` — hex only, no quote.
Mutation (scratchpad copy, `let literal = password.to_string();`), `cargo test --workspace`:
  test result: ok. 16 passed ... ok. 9 passed ... ok. 10 passed ... ok. 6 passed ... (0 failed in every binary; 65 passed total)
Postgres accepts the injected options in ONE statement, so the extended protocol does not stop it:
  ALTER ROLE rvx_c PASSWORD 'x' SUPERUSER --'
   rolname | rolsuper
   rvx_c   | t
```

**Failure scenario.** A later refactor of `set_role_password` (for example, moving the quoting into a helper and dropping the `.replace`) passes fmt, clippy, the whole test suite, `cargo sqlx prepare --check`, check_migrations.sh and check_ops.sh. The compose `migrate` service runs cadus-migrate as the `postgres` superuser (docker-compose.yml:85). An operator who sets CADUS_APP_PASSWORD to a value containing a quote — for example `s3cret' SUPERUSER --` — then gets `ALTER ROLE cadus_app PASSWORD 's3cret' SUPERUSER --'` executed as superuser. cadus_app becomes a cluster superuser, every RLS policy stops applying to it, and cadus-web then refuses to start (exit 3), so the site is down and the runtime credential owns the cluster. The suite reports nothing at any point.

**Refuter.** I reproduced every part of the claim and could not refute it. The escape at /home/deploy/dev/cadus2.0/crates/store/src/bin/cadus-migrate.rs:185 is the only defense of the interpolated SQL string literal, and no test exercises it. I copied the repository to a work directory, replaced `let literal = password.replace('\'', "''");` with `let literal = password.to_string();`, and ran the full suite against a private database (cadus2_rv_c3). All 65 tests passed, and `cargo fmt --all --check` and `cargo clippy --all-targets --workspace -- -D warnings` also passed. A grep of the whole workspace shows exactly one test that sets a password variable (crates/store/tests/migrate_bin.rs:212), and its value is `pw-` plus 8 hex characters, so no quote reaches the statement text. The binary holds no `#[cfg(test)]` module (223 lines total), so no unit test covers the quoting either. I also confirmed that 

### #9 [major] A panicking test body leaks a LOGIN SUPERUSER cluster role on the shared trust-auth cluster; TestDb cleans up databases only

File: `crates/store/tests/store_api.rs:126` — IDs: C3

**Claim.** `TestDb::with` guarantees only that the throwaway database goes away when the body panics; the two boot-guard tests create cluster-scoped roles (one of them LOGIN SUPERUSER) and drop them inside the body, so any panic before the DROP leaves the role on the shared cluster forever.

**Evidence.**

```
store_api.rs:125-127 creates `CREATE ROLE "cadus2_t_super_<hex>" LOGIN SUPERUSER`; the DROP is at store_api.rs:147, after `db.pool_as(&role, 1).await`, which panics on any pool failure (test_support.rs:208 `panic!("pool as {role} for {} failed: {e}")`).
test_support.rs:226-230 `drop_database` closes the pools and drops the database. Nothing drops a role.
Demonstration (scratchpad copy, `pool_as` pointed at a role that does not exist to reproduce the panic path):
  thread 'boot_guard_rejects_superuser_without_bypassrls' panicked at crates/store/src/test_support.rs:208:33
  === leftover roles and databases on the shared cluster:
           rolname         | rolcanlogin | rolsuper | rolbypassrls
   cadus2_t_super_357c3f7b | t           | t        | f
   (databases: 0 rows)
```

**Failure scenario.** The cluster runs `POSTGRES_HOST_AUTH_METHOD: trust` by contract (ci.yml:41, README, test_support.rs:93), and docs/plans/M0.md runs several worktrees against one cluster. When `db.pool_as(&role, 1)` fails — an exhausted connection slot under the parallel-suite contention HANDOVER §3 already warns about, or a cluster restart between the CREATE ROLE and the pool open — the test panics, the database is dropped, and `cadus2_t_super_<hex>` stays behind with LOGIN and SUPERUSER. Any local user who can reach port 55434 then logs in as a cluster superuser with no password, and each further failure adds another such role. The suite reports a normal red test and never names the leak.

**Refuter.** The claim holds on the mechanism, and I reproduced it with unmodified repository code and a realistic failure, not with an injected one. `TestDb::with` cleans up one object only: the throwaway database. `drop_database` and `drop_database_named` in /home/deploy/dev/cadus2.0/crates/store/src/test_support.rs:226-259 close the pools and run `DROP DATABASE ... WITH (FORCE)`. No code path drops a role: the only two `DROP ROLE` statements in the whole repository are the happy-path statements inside the two test bodies (/home/deploy/dev/cadus2.0/crates/store/tests/store_api.rs:91 and :147). scripts/gate.sh, scripts/check_migrations.sh, and .github/workflows/ci.yml hold no role cleanup.

The test at /home/deploy/dev/cadus2.0/crates/store/tests/store_api.rs:119-168 creates the cluster-scoped role `cadus2_t_super_<hex>` with LOGIN SUPERUSER at line 126 and drops it at line 147. Two panic sites sit 

### #13 [major] cadus-migrate aborts with a Rust panic (exit 101) on a non-Unicode argument instead of the documented usage + exit 2

File: `crates/store/src/bin/cadus-migrate.rs:62` — IDs: D9

**Claim.** `main` collects the command line with `std::env::args()`, which panics internally on any argument that is not valid Unicode, so the one-shot that gates the whole compose stack aborts with exit code 101 and a panic message before `parse_args` — and before the `ExitCode` contract in `main` — can run.

**Evidence.**

```
crates/store/src/bin/cadus-migrate.rs:62
    let args: Vec<String> = std::env::args().skip(1).collect();

Demonstrated on the release binary (no DATABASE_URL set, so this is argv handling alone):

  $ cadus-migrate $'--help\xff'
  rc = 101
  thread 'main' (3249927) panicked at library/std/src/env.rs:878:51:
  called `Result::unwrap()` on an `Err` value: "--help\xFF"
  note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

Control, same binary, a bad but valid-Unicode argument:

  $ cadus-migrate --bogus
  control rc (valid unicode bad arg) = 2 | first line: usage: cadus-migrate [--admin-login]

The contract this breaks is stated three times:
  crates/store/src/bin/cadus-migrate.rs:14-15  "An unknown argument prints the usage on stderr and exits 2."
  crates/store/src/bin/cadus-migrate.rs:29     const USAGE (never printed on this path)
  docs/SELF_HOST.md:71                         "`cadus-migrate` with any other argument prints its usage and exits 2."
```

**Failure scenario.** An operator or a wrapper script invokes the migrate one-shot with an argument carrying a non-UTF-8 byte — for example `docker compose run --rm migrate cadus-migrate $'--admin\xffogin'`, or any CI/ops wrapper that forwards a locale-mangled argument. Instead of the usage text and exit 2, the process aborts with exit 101 and a raw `library/std/src/env.rs` panic naming an internal `Result::unwrap()`. In the compose stack `migrate` carries `restart: "no"` and both `web` and `worker` depend on it with `condition: service_completed_successfully` (docker-compose.yml:88-92, 119-121, 136-138), so the stack stays down and `docker compose logs migrate` shows a Rust panic backtrace hint rather than the actionable usage block. Structurally this is the exact class the workspace lint regime is meant to eliminate — `Cargo.toml:48-53` sets `unwrap_used`, `expect_used`, and `panic` to warn and `scripts/gate.sh:38` runs clippy with `-D warnings` — but the unwrap lives inside `std::env::args()`, so the gate reports the binary as panic-free while this path is live. `std::env::args_os()` with an explicit non-Unicode branch (the pattern the same file already uses for `CADUS_APP_PASSWORD` at line 163 and that cadus-web uses for `BIND_ADDR` at crates/web/src/bin/cadus-web.rs:202) removes it.

**Refuter.** The defect is real. I reproduced it exactly on the committed release binary. `main` at crates/store/src/bin/cadus-migrate.rs:62 uses `std::env::args()`, whose iterator calls an internal `unwrap()` on each argument. One non-UTF-8 byte in argv aborts the process with exit code 101 and a raw `library/std/src/env.rs` panic message. `parse_args` never runs, `USAGE` never prints, and the `ExitCode::from(2)` contract in `main` never applies.

The documented contract is explicit, and this path breaks it:
- crates/store/src/bin/cadus-migrate.rs:14-15 — "An unknown argument prints the usage on stderr and exits 2."
- docs/SELF_HOST.md:71 — "`cadus-migrate` with any other argument prints its usage and exits 2."

The repository treats "not valid Unicode" as a start error, not a panic, in two other places. This makes the finding an inconsistency inside the codebase, not a style preference:
- crates/st

## FIX8c

### #1 [blocker] The migrate one-shot applies every migration under a 5 s statement_timeout that the compose file never forwards, so one slow or lock-blocked DDL statement takes the whole stack down with no operator knob

File: `docker-compose.yml:84` — IDs: C3, D9, M0-U6

**Claim.** `cadus-migrate` builds its pool with `DbConfig::from_env()`, so an absent `DB_STATEMENT_TIMEOUT_MS` puts `DEFAULT_STATEMENT_TIMEOUT_MS = 5000` into the connection's startup options; the `migrate` service's `environment:` block (docker-compose.yml:84-87) is the only one of the three app services that does not forward `DB_STATEMENT_TIMEOUT_MS`, so every migration statement runs under a 5 s server-side cancel that the operator cannot raise from `.env`, and a cancel exits the one-shot 2 and leaves `web` and `worker` in `Created`.

**Evidence.**

```
crates/store/src/lib.rs:45  `pub const DEFAULT_STATEMENT_TIMEOUT_MS: u64 = 5000;`
crates/store/src/bin/cadus-migrate.rs:88  `let cfg = DbConfig::from_env()?;`
crates/store/src/lib.rs:180-183  `Ok(options.options([("statement_timeout", cfg.statement_timeout_ms.to_string())]))`

Resolved compose config, with DB_STATEMENT_TIMEOUT_MS=0 exported (the documented way to lift the bound):
  migrate env: {'CADUS_ADMIN_PASSWORD': 'p3NEW', 'CADUS_APP_PASSWORD': 'p2NEW', 'DATABASE_URL': 'postgresql://postgres:p1@db:5432/cadus'}
  web env DB_STATEMENT_TIMEOUT_MS: 0

Live on the shipped stack (`docker compose -p cadus2rvops4 up -d db migrate web worker`, built by scripts/check_ops.sh), with one other session holding sqlx's own migrator advisory lock on database `cadus`:
  $ docker compose -p cadus2rvops4 up -d --force-recreate migrate web worker
   Container cadus2rvops4-migrate-1 Error service "migrate" didn't complete successfully: exit 2
  elapsed 7s
  migrate-1  | cadus-migrate: migration error: while executing migrations: error returned from database: canceling statement due to statement timeout at line 3394
  db running Up 2 minutes (healthy)
  migrate exited Exited (2) Less than a second ago
  web created Created
  worker created Created
Re-running the identical command with DB_STATEMENT_TIMEOUT_MS=0 exported produced the identical failure, proving the knob does not reach `migrate`.
Control on the same binary: `DB_STATEMENT_TIMEOUT_MS=0 ./target/debug/cadus-migrate --admin-login` under the same held lock did not abort (killed by `timeout 20` -> exit 124); with the variable unset it aborted at exactly 5 s with exit 2.

docs/SELF_HOST.md "Query bound" states the bound applies to "the web and worker pools" and never mentions migrate.
```

**Failure scenario.** M1 adds a migration that touches populated tables - a `CREATE INDEX` on `events`, an `ALTER TABLE ... ADD COLUMN` that rewrites, or a backfill `UPDATE`. On the operator's server the statement needs more than 5 s. `cadus-migrate` is cancelled with SQLSTATE 57014, prints `canceling statement due to statement timeout`, and exits 2. `web` and `worker` have `condition: service_completed_successfully`, so they are never started, `migrate` has `restart: "no"` so nothing retries, and the site is down. Every re-run of `docker compose up -d` reproduces the same 5 s cancel. Setting `DB_STATEMENT_TIMEOUT_MS=0` in `.env` - the one bound `.env.example:69-71` and `docs/SELF_HOST.md` document - changes `web` and `worker` only and does not help; the operator must edit `docker-compose.yml` to recover. The same abort fires today with no slow DDL at all whenever anything else holds the sqlx migrator advisory lock for 5 s (a second concurrent `docker compose up -d`, or a `cadus-migrate` still running against the same cluster), which is the run reproduced above.

**Refuter.** The claim is correct and I reproduced it end to end. Four checks, each independent:

1. Code path. /home/deploy/dev/cadus2.0/crates/store/src/bin/cadus-migrate.rs:88 calls `DbConfig::from_env()`. /home/deploy/dev/cadus2.0/crates/store/src/lib.rs:110-113 returns `DEFAULT_STATEMENT_TIMEOUT_MS` (5000) when the variable is absent, and lib.rs:176-183 puts that value into the connection startup options. `cadus-migrate` therefore opens its pool with a 5 s server-side `statement_timeout` by default. This is the same `connect()` the web and worker use; there is no migrate-specific exemption.

2. Compose does not forward the knob. /home/deploy/dev/cadus2.0/docker-compose.yml:84-87 gives `migrate` exactly three variables: DATABASE_URL, CADUS_APP_PASSWORD, CADUS_ADMIN_PASSWORD. `web` (line 116) and `worker` (line 132) both carry `DB_STATEMENT_TIMEOUT_MS: ${DB_STATEMENT_TIMEOUT_MS:-5000}`; `migrate` 

### #7 [major] cadus-web spends SHUTDOWN_DEADLINE_SECS twice, so the stop takes 20.01 s against a 20 s stop_grace_period and Docker sends SIGKILL

File: `crates/web/src/bin/cadus-web.rs:175` — IDs: C3

**Claim.** The drain deadline and the pool-close deadline are two sequential full-length budgets of the same value, so a stop of cadus-web takes up to 2 x SHUTDOWN_DEADLINE_SECS; at the compose default of 10 s the measured stop time is 20.01 s, which is past the stop_grace_period of 20 s that docker-compose.yml sets, and the container ends with SIGKILL and exit 137 — the exact outcome that the fix for review-2 finding #6 and the compose comment both promise to prevent.

**Evidence.**

```
crates/web/src/bin/cadus-web.rs:164-175
    let result = tokio::select! {
        outcome = &mut server => outcome,
        _ = fired_rx => match tokio::time::timeout(deadline, &mut server).await {
            Ok(outcome) => outcome,
            Err(_elapsed) => { tracing::info!("shutdown deadline reached; closing"); Ok(()) }
        },
    };
    close_within(deadline, pool.close()).await;   // <-- the SAME `deadline` again

docker-compose.yml:112-118
      SHUTDOWN_DEADLINE_SECS: ${SHUTDOWN_DEADLINE_SECS:-10}
    # Longer than SHUTDOWN_DEADLINE_SECS, so the process exits by itself and
    # Docker never has to send SIGKILL (exit 137).
    stop_grace_period: 20s

Live run. Real cadus-web (debug build) against migrated database cadus2_rv_unsafe through a TCP proxy that black-holes the Postgres->client direction, 24 concurrent GET /api/ready clients, SIGTERM 3 s after the black hole starts:

  SHUTDOWN_DEADLINE_SECS=3  -> EXIT rc=0 after 6.00 s
  SHUTDOWN_DEADLINE_SECS=10 -> EXIT rc=0 after 20.01 s

Process log of the 10 s run (ANSI stripped):
  15:01:52.679423 INFO cadus-web: SIGTERM received
  15:01:52.679452 INFO cadus-web: graceful shutdown starts
  15:02:02.681371 INFO shutdown deadline reached; closing      (+10.002 s, drain budget)
  15:02:12.683397 INFO pool close deadline reached             (+20.004 s, a SECOND 10 s budget)
  15:02:12.683447 INFO cadus-web: stopped

The worker does not have the defect: crates/worker/src/bin/cadus-worker.rs:34 uses a separate constant POOL_CLOSE_DEADLINE = 5 s, and the same black-hole test gives `worker EXIT rc=0 after 5.00 s`, inside the 10 s Docker default.
```

**Failure scenario.** The db container restarts, or the `backend` bridge drops packets, after the TCP handshake and while a few /api/ready (or later M5) requests hold pool connections. sqlx has no read timeout, so those connections never come back. The operator runs `docker compose up -d --build` or `docker compose restart web`. Docker sends SIGTERM and starts the 20 s stop_grace_period timer. cadus-web drains for the full 10 s, logs `shutdown deadline reached; closing`, then starts a second 10 s budget inside close_within and logs `pool close deadline reached` at 20.004 s. Docker sends SIGKILL at 20.000 s, so the container reports exit 137 on every such restart instead of the documented exit 0. A second SIGTERM does not shorten the wait: tokio owns the handler and the Shutdown struct is already consumed by the graceful-shutdown block. Any operator who raises SHUTDOWN_DEADLINE_SECS above 10 (the knob that docker-compose.yml:113 exposes) makes exit 137 certain rather than marginal: a value of 30 gives a 60 s stop against the 20 s grace period. The fix is to carry the remaining budget into close_within (start one Instant at the signal and pass `deadline - elapsed`), or to bound the pool close with its own small constant as cadus-worker already does.

**Refuter.** I tried to refute the claim and failed. The claim is demonstrable, and my own live run matches the reviewer's numbers to the hundredth of a second.

The code path is unambiguous. In /home/deploy/dev/cadus2.0/crates/web/src/bin/cadus-web.rs, `deadline` comes from one call to `shutdown_deadline()` at line 97. Line 166 spends the whole of it on the drain: `tokio::time::timeout(deadline, &mut server)`. Line 175 then starts a second, fresh budget of the same length: `close_within(deadline, pool.close()).await`. No `Instant` starts at the signal, and no remaining budget passes from the first bound to the second. The two bounds are sequential, so the worst case from SIGTERM to exit is 2 x SHUTDOWN_DEADLINE_SECS.

One fault fills both budgets. A database that accepts the socket and then answers nothing (a db restart or a `backend` packet drop after the handshake) leaves the in-flight `/api/ready

### #10 [major] The documented bring-up check for `web` greps `listening on`, a string `cadus-web` never logs, and the compose file cites that string as the reason `web` carries no healthcheck

File: `docker-compose.yml:102` — IDs: C3, M0-U6

**Claim.** `cadus-web` logs `cadus-web: listening` with the address as a structured field, so the literal substring `listening on` appears nowhere in its output, yet docker-compose.yml:101-104 and docs/SELF_HOST.md:29 both tell the operator that this exact string is how to confirm the web tier is up - and docker-compose.yml uses it to justify shipping `web` with no `healthcheck:` at all.

**Evidence.**

```
crates/web/src/bin/cadus-web.rs:144
  tracing::info!(address = %local, "cadus-web: listening");

Actual output of the shipped image in the running stack:
  web-1  | 2026-08-25T15:09:20.653578Z  INFO cadus_web: cadus-web: listening address=0.0.0.0:8080
The substring `listening on` is absent.

docker-compose.yml:101-104
  # No `healthcheck:` -- the runtime image carries no curl and no wget, and the
  # image stays small. `docker compose logs web` prints `listening on` when the
  # server is up. ...
docs/SELF_HOST.md:29
  docker compose logs web      # the line `listening on` means the server is up
```

**Failure scenario.** `web` is the one service in the stack with no `healthcheck:` and no published port, and `caddy` depends on it with the default `service_started` condition, so `docker compose ps` reports `web ... Up` the instant the process is exec'd, whether or not it ever bound its socket. The operator follows step 6 of docs/SELF_HOST.md, runs `docker compose logs web | grep 'listening on'`, gets nothing, and concludes the web tier failed - on a stack that is healthy. The reverse case is worse in a script: an operator or a later deploy job that gates on `docker compose logs web | grep -q 'listening on'` never succeeds and will loop or fail the bring-up forever. The one remaining check the docs offer, `curl -fsS http://localhost/api/health`, goes through Caddy and so cannot distinguish a web tier that is down from a Caddy or SITE_ADDRESS mistake.

**Refuter.** The claim is demonstrable and I confirmed every step with the real binary; the refutation attempt failed. crates/web/src/bin/cadus-web.rs:144 emits `tracing::info!(address = %local, "cadus-web: listening")`, so the address is a structured field and not part of the message. init_tracing at crates/web/src/bin/cadus-web.rs:200-207 uses the default tracing_subscriber::fmt() format, which renders fields as `key=value` after the message. The token after `listening` is therefore always `address=`, never `on`. I ran the shipped release binary against a live Postgres with a NOSUPERUSER NOBYPASSRLS role and captured `2026-08-25T15:20:41.002005Z  INFO cadus_web: cadus-web: listening address=127.0.0.1:19741`. The exact grep the documentation prescribes returns 0 matches and exit code 1. A repo-wide grep for `listening` gives only five hits: the source line, the two documentation claims, and two prio

### #14 [major] A panicking binary test orphans a cadus-web server process, which then runs forever holding its listen port

File: `crates/web/tests/http.rs:276` — IDs: M0-U4

**Claim.** `binary_serves_health_and_stops_on_sigterm` and `binary_exits_zero_with_a_half_sent_request_open` assert on the response before they send SIGTERM, and `std::process::Child` neither kills nor reaps on drop, so a panic between the spawn and `send_sigterm` leaves a live `cadus-web` process reparented to PID 1 after the test binary has exited.

**Evidence.**

```
crates/web/tests/http.rs:270-283 — the child is spawned at :276, `wait_until_healthy` returns at :279, then `assert_eq!(code, 200)` (:280) and `assert_eq!(body, ...)` (:281) run BEFORE `send_sigterm(&child)` (:283). `wait_until_healthy` (http.rs:119-134) kills the child on its own two failure paths, but a panic in the two asserts after it does not. `TestDb::with` (crates/store/src/test_support.rs:161-169) guards the database, not the child process, and the module header at http.rs:13-14 claims only "a failed assertion drops the throwaway database".

Demonstrated on a scratch copy by changing the body assertion at :281 to a value the server does not return, then running the single test:

  thread 'tokio-rt-worker' panicked at crates/web/tests/http.rs:281:9:
   right: "MUTANT-EXPECTED-BODY"
  test result: FAILED. 0 passed; 1 failed

  $ ps -eo pid,ppid,etimes,cmd | grep [c]adus-web
  3220479       1       1 .../scratchpad/mut/target/debug/cadus-web
  $ ps -o pid,ppid,etimes -p 3220479
      PID    PPID ELAPSED
  3220479       1      10
  $ cat /proc/3220479/net/tcp | head -3   # still holding a listening socket

The throwaway database was dropped correctly (`select count(*) from pg_database where datname like 'cadus2_t_%'` -> 0), so the process outlives the database it was pointed at.
```

**Failure scenario.** Any regression that makes `/api/health` answer with the wrong body or the wrong status -- exactly the regression this test exists to catch -- panics at http.rs:281 before `send_sigterm`. The test reports FAILED, `TestDb::with` drops the throwaway database with `DROP DATABASE ... WITH (FORCE)`, and cargo exits. The spawned `cadus-web` survives: it reparents to PID 1, keeps its listener bound on the port `free_port()` handed out, and keeps answering `/api/health` with 200 forever, because the liveness handler (crates/web/src/lib.rs:51-53) touches no database. Every red gate run adds one more orphan; the developer sees a green-looking machine with N stale servers holding ephemeral ports and must find and kill them by hand. The same shape sits at http.rs:340-345 (`binary_exits_zero_with_a_half_sent_request_open`, which additionally leaves the deliberately stalled TCP client attached to the orphan) and at http.rs:546 and crates/worker/tests/run.rs:156. The one-line fix is a guard that kills the child on unwind, or moving the asserts after `send_sigterm` and `wait_for_exit`.

**Refuter.** The claim is correct and I reproduced it. The two tests own the child through `std::process::Child`, which does not kill and does not reap the process on drop. In `binary_serves_health_and_stops_on_sigterm` the spawn is at /home/deploy/dev/cadus2.0/crates/web/tests/http.rs:270-277, `wait_until_healthy` returns at :279, and the two asserts run at :280-281, before `send_sigterm(&child)` at :283. A panic in those asserts unwinds the `tokio::spawn` task of `TestDb::with` (/home/deploy/dev/cadus2.0/crates/store/src/test_support.rs:161-169), drops the `Child`, and leaves the `cadus-web` process alive. The database is dropped correctly, but the process survives because the liveness handler touches no database (/home/deploy/dev/cadus2.0/crates/web/src/lib.rs:51-53 returns a constant). I found no `Drop` guard, no `kill_on_drop`, and no process group in the workspace (grep for `Guard|impl Drop|kil

### #15 [major] The db healthcheck probes the unix socket, so `migrate` starts while Postgres still refuses TCP

File: `docker-compose.yml:58` — IDs: C3

**Claim.** `pg_isready -U postgres -d cadus` carries no `-h`, so it connects over the unix socket in `/var/run/postgresql`; the `postgres:16` entrypoint runs its initdb temp server with `listen_addresses=''`, so the probe succeeds and Docker marks `db` healthy at a moment when no other container reaches port 5432, and the `migrate` one-shot then starts against a database that refuses the connection.

**Evidence.**

```
docker-compose.yml:57-62
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d cadus"]
      interval: 5s

Measured with the same probe and the same interval/timeout/retries on a plain `postgres:16` container (health state vs. a TCP connect from a second container on the same network):
  t=5s   health=starting tcp=REFUSED
  t=10s  health=healthy  tcp=REFUSED
  t=40s  health=healthy  tcp=REFUSED
  t=45s  health=healthy  tcp=OPEN
35 seconds of `healthy` while every TCP connect is refused.

The same probe on the shipped stack, with the initdb phase widened and nothing else changed:
  Container ...-db-1 Healthy
  Container ...-migrate-1 Error service "migrate" didn't complete successfully: exit 2
  db       Up 11 seconds (healthy)
  migrate  Exited (2)
  web      Created
  worker   Created
  migrate-1 | cadus-migrate: database error: pool timed out while waiting for an open connection

The natural window on an unloaded box, with no init script, is 334 ms (pg_isready OK at 624 ms, TCP OK at 958 ms). The window grows with the length of the initdb phase, so a slow disk, a loaded server, or any file in /docker-entrypoint-initdb.d moves the 5 s probe into it. `.github/workflows/ci.yml:45` carries the same defect (`--health-cmd "pg_isready -U test"`).
```

**Failure scenario.** The operator follows docs/SELF_HOST.md step 5 and runs `docker compose up -d --build` on a fresh server. The initdb phase takes longer than the 5 s probe interval. The first probe reaches the initdb temp server over the unix socket and returns 0. Docker marks `db` healthy. `migrate` starts, fails to reach 5432, and exits 2. `web` and `worker` stay in `Created` and never start. `docker compose up -d` exits non-zero with `service "migrate" didn't complete successfully: exit 2`. The migrate log names a pool timeout, not a refused connection, so the operator has no pointer to the cause. Adding `-h 127.0.0.1` to the probe removes the whole class.

**Refuter.** I tried to refute the claim and failed. Every step of the mechanism reproduces with the shipped probe and the shipped binary.

1. The probe uses the unix socket. Inside a running postgres:16 container, `pg_isready -U postgres -d cadus` prints `/var/run/postgresql:5432 - accepting connections` and returns 0. It never touches TCP.

2. The initdb temp server answers that socket. The entrypoint starts its temp server with `listen_addresses=''`, so the socket answers while port 5432 refuses every connect. With the initdb phase widened by one file in /docker-entrypoint-initdb.d, and with the exact interval/timeout/retries of docker-compose.yml:59-61, Docker marked the container healthy at t=5356 ms, and every TCP connect from the host to the container IP stayed REFUSED for the next 34 s.

3. The one-shot dies in that window. I ran the real /home/deploy/dev/cadus2.0/target/release/cadus-migrate

### #16 [major] `docker compose up -d` destroys the serving web and worker before `migrate` proves it can succeed

File: `docker-compose.yml:121` — IDs: C3

**Claim.** Compose recreates `web` and `worker` first and starts the `migrate` one-shot second, so a `migrate` that exits non-zero leaves the two runtime containers in `Created`; `restart: unless-stopped` never applies to a container that Docker did not start, so an upgrade whose migration fails converts a running site into a full outage that no restart policy recovers.

**Evidence.**

```
docker-compose.yml:119-121 (web) and 136-138 (worker)
    depends_on:
      migrate:
        condition: service_completed_successfully

Run against a stack that was serving `{"ok":true}` on /api/health, with a `migrate` step that fails:
  Container ...-worker-1 Recreate
  Container ...-web-1    Recreate
  Container ...-web-1    Recreated          <-- the serving container is already gone
  Container ...-migrate-1 Starting
  Container ...-migrate-1 Error service "migrate" didn't complete successfully: exit 2
  --- state after the failed upgrade ---
  db       Up 6 seconds (healthy)
  migrate  Exited (2)
  web      Created
  worker   Created
  --- is the site still served? ---
  wget: can't connect to remote host: Connection refused
  NO ANSWER - site is down

docs/SELF_HOST.md:36-39 states only the safety half: "`web` and `worker` start only after it exits 0, so the schema is never behind the code. To upgrade, pull the new commit and run step 5 again". Neither the doc nor the compose comment says that a failed `migrate` leaves the previous, working containers destroyed. The round-2 #13 fix documents this for one trigger only (a POSTGRES_PASSWORD rotation); the mechanism fires for every trigger.
```

**Failure scenario.** The operator upgrades with `docker compose up -d --build` after a `git pull` that adds migration 0007. The new image changes, so compose recreates `web` and `worker` and removes the containers that were serving traffic. `migrate` then starts and migration 0007 fails on the operator's real data — a constraint violation, a lock timeout, or the 5 s statement timeout. `migrate` exits 2. `web` and `worker` stay in `Created`. The site is down and stays down; `docker compose start web` also refuses while the dependency is unsatisfied, and the operator has no rollback path in the documentation. A migration that fails must leave the previous version serving, not remove it first.

**Refuter.** I did not refute the claim. I reproduced the mechanism end to end with Docker Engine 29.6.2 and Docker Compose v5.3.1, and every load-bearing part of it is demonstrable.

What I proved:

1. Compose creates all containers before it starts any. `docker compose up -d` runs a create phase and a start phase. The create phase removes and re-creates `web` and `worker` because the image changed. Only after that does the start phase run `migrate` and block on `service_completed_successfully`. The container that served traffic is already gone at the moment `migrate` starts.

2. A `migrate` that exits non-zero stops the start phase. `web` and `worker` stay in state `Created`.

3. `restart: unless-stopped` gives no recovery. The restart policy applies to a container that Docker started and that then exited. Docker never started these two containers, so the policy never fires. `web` and `worker` stay
