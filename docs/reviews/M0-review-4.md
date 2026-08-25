# M0 adversarial review — round 4 (2026-08-25)

Run on the tree after FIX8 (commit 1bdfdf1). Two find/refute rounds, major+ only: 28 raised, 14 confirmed. Assigned to FIX9a (auth tables RLS, function and column pins), FIX9b (cadus-migrate, migrate tests), FIX9c (R4 purity, ops scripts).

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `migrations/0006_grants_rls.sql:12` | FIX9a | auth_sessions, auth_tokens, and oauth_accounts carry user_id, hold no RLS, and keep full DML for cadus_app, so a bound tenant forges a session for any other account and reads every tenant table through the policies |
| 2 | major | `migrations/0006_grants_rls.sql:206` | FIX9a | The SECURITY DEFINER login functions hand cadus_app every account's password_hash and is_admin, so the round-3 #11 fix moved the leak instead of closing it |
| 3 | major | `crates/store/tests/migrate_bin.rs:79` | FIX9b | cadus-migrate and the migrate_bin test harness take the role lock in two different databases, so the round-3 #2 fix does not serialize them and the suite races on pg_authid |
| 4 | major | `migrations/0006_grants_rls.sql:178` | FIX9a | The two SECURITY DEFINER login functions declare SET search_path = public without pg_temp, so any cadus_app caller redirects the body to a temp table and makes auth_user_by_email / auth_user_by_id return an attacker-chosen row with is_admin = true |
| 5 | major | `crates/store/tests/rls.rs:772` | FIX9a | The C2 append-only guard is blind to column-level grants: has_table_privilege reports a column grant as false, so GRANT UPDATE (payload) ON events TO cadus_app rewrites the authoritative event document with every C2 test green |
| 6 | major | `crates/web/tests/purity.rs:41` | FIX9c | The R4 dependency guard (the FIX4 fix for round-1 finding #22) reads only the [dependencies] table of two crates, so an HTTP client compiles into cadus-web with the whole gate green |
| 7 | major | `crates/store/tests/rls.rs:707` | FIX9a | A SECURITY DEFINER function in schema public is pinned by no test, so a later migration hands cadus_app a full cross-tenant read of events and the suite stays green |
| 8 | major | `migrations/0003_event_log.sql:13` | FIX9a | The ON DELETE RESTRICT of events_user_id_fkey is pinned by no test; changing it to CASCADE erases the append-only log on account deletion and the whole store suite stays green |
| 9 | major | `scripts/check_ops.sh:124` | FIX9c | check_ops.sh check (d) reads only command[0] and skips a service with no command:, so a wrong flag or a deleted command: passes the gate and stops the stack |
| 10 | major | `crates/store/tests/migrate_bin.rs:371` | FIX9b | The two role-lock tests in migrate_bin.rs spawn cadus-migrate with inherited stdio, so the stderr they print on failure is always empty |
| 11 | major | `migrations/0002_identity.sql:7` | FIX9a | migrations/0002_identity.sql states that 0006 leaves users exempt from row-level security, while 0006 enables, forces, and policies it |
| 12 | major | `crates/store/src/bin/cadus-migrate.rs:101` | FIX9b | cadus-migrate installs no stop-signal handler, so the migrate one-shot is deaf to SIGTERM and SIGINT as PID 1 and only SIGKILL ends it |
| 13 | major | `scripts/deploy.sh:79` | FIX9c | scripts/deploy.sh step 4 checks nothing after it replaces web and worker, so an upgrade that leaves the site crash-looping prints DEPLOY OK and exits 0 |
| 14 | major | `docker-compose.yml:133` | FIX9b | A password with an @ or # is written into the cadus_app role by cadus-migrate and silently misparsed by the compose DSN, so migrate exits 0, deploy.sh reports DEPLOY OK, and web can no longer connect |

## FIX9a

### #1 [blocker] auth_sessions, auth_tokens, and oauth_accounts carry user_id, hold no RLS, and keep full DML for cadus_app, so a bound tenant forges a session for any other account and reads every tenant table through the policies

File: `migrations/0006_grants_rls.sql:12` — IDs: C3

**Claim.** The three auth tables stay outside row-level security on the reason "looked up before a tenant context exists", the same reason round-3 finding #11 rejected for users, yet cadus_app keeps SELECT, INSERT, UPDATE, and DELETE on all rows of all three, which makes the whole users hardening of rounds 2 and 3 reachable through a forged auth_sessions row.

**Evidence.**

```
Live, connected as cadus_app with app.user_id bound to tenant A (e1e711a9-...):

  BEGIN
  SELECT set_config('app.user_id','e1e711a9-20fa-4a92-bdfb-d67f8c4e566a', true);
  SELECT token_hash, user_id FROM auth_sessions;
     token_hash   |               user_id
  ---------------+--------------------------------------
   victimB-token | d5bcc3e8-c3cd-446b-8253-8c4172a38e6a   <- tenant B's live session

  INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at, expires_at)
    VALUES ('attacker-chosen','d5bcc3e8-c3cd-446b-8253-8c4172a38e6a', now(), now(), now()+interval '1 day');
  INSERT 0 1

  INSERT INTO auth_tokens (token_hash, user_id, purpose, expires_at)
    VALUES ('attacker-reset','d5bcc3e8-c3cd-446b-8253-8c4172a38e6a','password_reset', now()+interval '1 day');
  INSERT 0 1

  DELETE FROM auth_sessions;
  DELETE 2
  COMMIT

Catalog: auth_sessions|f|f, auth_tokens|f|f, oauth_accounts|f|f for (relrowsecurity, relforcerowsecurity); information_schema.table_privileges gives cadus_app SELECT+INSERT+UPDATE+DELETE on each. crates/store/tests/rls.rs:134 pins exactly that: ("auth_sessions", [true, true, true, true, false]).
```

**Failure scenario.** An SQL-level defect on the cadus_app connection (the exact threat the C2 revoke and the C3 policies exist to contain) runs one statement: INSERT INTO auth_sessions (token_hash, user_id, created_at, last_seen_at, expires_at) VALUES (sha256(T), <victim uuid>, now(), now(), now()+'30 days'). The attacker then presents cookie T. docs/SCHEMA.md:147 says the session-cookie path reads auth_sessions for a user_id and calls auth_user_by_id, so the web tier binds app.user_id to the victim and every tenant_isolation policy then admits the attacker to the victim's events, learner_models, profiles, diagnosis_jobs, and email_outbox. The same table also lets one statement (DELETE FROM auth_sessions) log out the entire deployment, and oauth_accounts lets the attacker link its own provider account to the victim's user_id for a permanent second door.

**Refuter.** The claim holds. I reproduced the full chain live and I found no control that stops it.

Facts from the source:
1. `/home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql:12-15` exempts `auth_sessions`, `oauth_accounts`, and `auth_tokens` with one reason: "looked up before a tenant context exists".
2. The same file rejects that exact reason for `users`. Lines 100-110 state that the login path "runs before a tenant context exists", and the fix is per-command policies plus two SECURITY DEFINER functions, not an exemption. `auth_sessions` accepts the same shape: an `auth_session_by_hash(text)` SECURITY DEFINER function for the pre-tenant read, plus `ENABLE`, `FORCE`, and a `tenant_isolation` policy. A `WITH CHECK (user_id = app.user_id)` blocks the forged INSERT, so the fix is effective, not cosmetic.
3. The blanket `GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO c

### #2 [major] The SECURITY DEFINER login functions hand cadus_app every account's password_hash and is_admin, so the round-3 #11 fix moved the leak instead of closing it

File: `migrations/0006_grants_rls.sql:206` — IDs: C3

**Claim.** migrations/0006_grants_rls.sql grants EXECUTE on auth_user_by_email(citext) and auth_user_by_id(uuid) to cadus_app, and both functions run as the superuser owner with no filter on the caller, so one cadus_app session reads any other account's password_hash, disabled_at and is_admin - the exact read that the same file's comment and docs/SCHEMA.md:140 claim users_read_self closes.

**Evidence.**

```
migrations/0006_grants_rls.sql:148 states 'A caller reads no other account, and a caller with no argument reads nothing.' and line 111 states '#11: a SELECT reaches the caller's own row only.' Run against a database built from migrations 0001-0006 (two users seeded, victim is_admin=true, password_hash='$argon2-VICTIM-HASH'):

  psql -U cadus_app -d cadus2_rv_corr
  BEGIN;
  SELECT set_config('app.user_id','32990d40-...-311462efdde7', true);
  SELECT email, password_hash, is_admin FROM users;
   -> attacker@x.test | $argon2-ATTACKER | f      (1 row - the policy works)
  SELECT * FROM auth_user_by_email('victim@x.test'::citext);
   -> aeb66000-... | $argon2-VICTIM-HASH | ... | t
  SELECT * FROM auth_user_by_id('aeb66000-cc1c-4d46-b898-64b8e6abdf7a');
   -> aeb66000-... | $argon2-VICTIM-HASH | ... | t

The ids need no guessing: cadus_app holds table-wide SELECT on the RLS-exempt auth_sessions, so an UNBOUND cadus_app session enumerates the whole table:

  psql -U cadus_app -d cadus2_rv_corr -c \
    "SELECT s.user_id, (auth_user_by_id(s.user_id)).password_hash, (auth_user_by_id(s.user_id)).is_admin FROM auth_sessions s;"
   -> aeb66000-... | $argon2-VICTIM-HASH | t

crates/store/tests/rls.rs:692 pins this as intended: assert_eq!(by_id[0].password_hash.as_deref(), Some("VICTIM-HASH")) - the suite asserts that cadus_app reads another account's hash, so no test can ever fail on it.
```

**Failure scenario.** An M5 auth handler (or an injection in one) passes an attacker-supplied id or email to auth_user_by_id / auth_user_by_email while bound to a different tenant. The call returns the target account's password_hash and is_admin. The C3 backstop that round-3 finding #11 was fixed to provide - 'a bound tenant reads no other account's password_hash' - does not exist: the tenant only has to call a function instead of running SELECT * FROM users. The privilege matrix test (app_role_privilege_matrix_is_the_literal_table) enumerates pg_class only, so the function ACL is outside every literal-matrix guard.

**Refuter.** I did not refute the claim. I built a database from migrations 0001-0006 on the throwaway cluster and reproduced every step of the reviewer's evidence, with the same outputs.

The code is exactly as described. /home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql:186 and :206 grant EXECUTE on auth_user_by_email(citext) and auth_user_by_id(uuid) to cadus_app. Both functions are SECURITY DEFINER with SET search_path = public. The owner is the migration runner, which is the postgres superuser in the shipped compose stack, so the body bypasses row-level security. Neither body compares its argument to app.user_id or to any other caller identity. The only filter is WHERE u.email = p_email and WHERE u.id = p_id. A cadus_app session bound to one tenant therefore reads a different account's password_hash, disabled_at and is_admin through either function, while the same session's plain SELECT o

### #4 [major] The two SECURITY DEFINER login functions declare SET search_path = public without pg_temp, so any cadus_app caller redirects the body to a temp table and makes auth_user_by_email / auth_user_by_id return an attacker-chosen row with is_admin = true

File: `migrations/0006_grants_rls.sql:178` — IDs: C3

**Claim.** PostgreSQL searches the temporary schema before the schemas that search_path lists whenever pg_temp is not written explicitly, so the round-3 #11 fix ships a control whose stated guarantee is false: a caller creates pg_temp.users and both superuser-owned SECURITY DEFINER functions read that table instead of public.users.

**Evidence.**

```
migrations/0006_grants_rls.sql:157 claims: "SET search_path = public pins the name resolution of the body, so a caller with its own search_path cannot point the body at a different users table." docs/SCHEMA.md:151 repeats it. crates/store/tests/rls.rs:732 and :740 pin the value Some("{search_path=public}") as proof.

Live, connected as cadus_app:

  SELECT * FROM auth_user_by_email('b@x.test');
    d5bcc3e8-c3cd-446b-8253-8c4172a38e6a | hashB |  |  | f

  CREATE TEMP TABLE users (id uuid, email citext, password_hash text,
                           email_verified_at timestamptz, is_admin boolean,
                           disabled_at timestamptz, created_at timestamptz);
  INSERT INTO pg_temp.users VALUES
    ('00000000-0000-0000-0000-0000deadbeef','b@x.test','$argon2-attacker', now(), true, NULL, now());

  SELECT * FROM auth_user_by_email('b@x.test');
    00000000-0000-0000-0000-0000deadbeef | $argon2-attacker | 2026-08-25 16:33:58 |  | t

auth_user_by_id is poisoned the same way:
  SELECT * FROM auth_user_by_id('d5bcc3e8-...');
    d5bcc3e8-c3cd-446b-8253-8c4172a38e6a | PWNED | 2026-08-25 16:39:07 |  | t
```

**Failure scenario.** A caller on the cadus_app connection runs CREATE TEMP TABLE users (...) and one INSERT, then the auth layer calls auth_user_by_email for a login. docs/SCHEMA.md paragraph 1 makes these two functions the only pre-bind read of users, and the row they return carries password_hash, disabled_at, and is_admin. The function hands back the attacker's row: the password check compares against an attacker-chosen hash, disabled_at is NULL so a banned account passes, is_admin is true so the session binds as an administrator, and id is an attacker-chosen uuid. The whole per-command policy set on users (users_read_self, users_update_self, the id and is_admin column lists) is bypassed, because none of it runs inside a SECURITY DEFINER body. The fix is one token in both function bodies: SET search_path = public, pg_temp.

**Refuter.** The claim is correct and I reproduced it end to end. PostgreSQL searches the temporary schema before every schema that `search_path` names, for relation and type names, whenever `pg_temp` is not written explicitly. The rule applies to the `search_path` that a `SET` clause on a function pins. `SET search_path = public` therefore resolves to the effective list `pg_temp, public`, not to `public` alone.

Both functions in migrations/0006_grants_rls.sql carry `SET search_path = public` and no `pg_temp` entry:
- `auth_user_by_email(citext)` at /home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql:169-186
- `auth_user_by_id(uuid)` at /home/deploy/dev/cadus2.0/migrations/0006_grants_rls.sql:188-205

`SECURITY DEFINER` does not help here. It changes the privileges of the body, not the name resolution of the body. The owner is a superuser, so the body reads the attacker's temp table with superu

### #5 [major] The C2 append-only guard is blind to column-level grants: has_table_privilege reports a column grant as false, so GRANT UPDATE (payload) ON events TO cadus_app rewrites the authoritative event document with every C2 test green

File: `crates/store/tests/rls.rs:772` — IDs: C2

**Claim.** Both guards on the C2 append-only invariant read the table-level ACL only, so a one-line column grant on events lets cadus_app rewrite events.payload, and the round-2 #16 privilege matrix plus app_role_cannot_update_events both stay green.

**Evidence.**

```
app_role_privilege_matrix_is_the_literal_table reads has_table_privilege only (crates/store/tests/rls.rs:770-774) and pins ("events", [true, true, false, false, false]) at rls.rs:142. Its own comment at rls.rs:123 states the blind spot -- "has_table_privilege reports a column-level grant as false" -- and then patches it with has_column_privilege for four columns of users only (rls.rs:813-821). Nothing asserts a column ACL on events.

Live, on the migrated database:

  GRANT UPDATE (payload) ON events TO cadus_app;
  SELECT has_table_privilege('cadus_app','events','UPDATE'), has_column_privilege('cadus_app','events','payload','UPDATE');
   matrix_sees_update | real_update
  --------------------+-------------
   f                  | t

As cadus_app, bound to its own tenant:
  UPDATE events SET type = 'x';                       -- what app_role_cannot_update_events runs
  ERROR:  permission denied for table events          -- test still green

  UPDATE events SET payload = '{"correct":true,"rewritten":true}' WHERE seq = 7;
  UPDATE 1
  SELECT seq, payload FROM events WHERE seq = 7;
   7 | {"correct": true, "rewritten": true}
```

**Failure scenario.** A later migration adds a column grant on events -- the repository already uses that pattern on users (migrations/0006_grants_rls.sql:127 and :134), so it is the house style, and a plausible use is a backfill of events.payload. The gate runs cargo test --workspace: app_role_cannot_update_events passes, because it only tries UPDATE events SET type = 'x' and that column carries no grant; app_role_privilege_matrix_is_the_literal_table passes, because has_table_privilege('cadus_app','events','UPDATE') is still false. The migration ships. cadus_app can then rewrite the payload of any of its own events. 0003_event_log.sql:20 marks payload as "the whole event document; authoritative", so a wrong grade is edited in place instead of superseded by a regraded event, and C2 ("The app role has no UPDATE or DELETE on it") is gone with no red test. The same blind spot covers content_store.status (C6 self-approval) and every column of model_call_log.

**Refuter.** The claim holds. Both C2 guards read the table-level ACL only, and no other test, script, or gate step reads a column ACL on events.

1. app_role_privilege_matrix_is_the_literal_table (crates/store/tests/rls.rs:764-846) selects only has_table_privilege for the five verbs (lines 770-774), compares the result against APP_TABLE_PRIVILEGES, which pins ("events", [true, true, false, false, false]) at rls.rs:141-142, and then adds has_column_privilege for four columns of users alone (rls.rs:813-846).
2. A repo-wide grep for has_column_privilege, attacl, relacl, and information_schema over *.rs, *.sql, and *.sh returns hits only at rls.rs:591-595 and rls.rs:815-821. All seven name users. No assertion in the repository reads a column ACL of events, content_store, or model_call_log. default_privileges_are_the_literal_grants reads pg_default_acl, not pg_attribute.attacl, so it does not close the g

### #7 [major] A SECURITY DEFINER function in schema public is pinned by no test, so a later migration hands cadus_app a full cross-tenant read of events and the suite stays green

File: `crates/store/tests/rls.rs:707` — IDs: C3, C2, D9

**Claim.** The suite pins the view count of schema `public` at 0 (finding #6) but pins nothing about the functions of schema `public`; a new SECURITY DEFINER function owned by the superuser migration runner bypasses row-level security and the append-only revoke, and `EXECUTE` on a new function goes to PUBLIC by default, so `cadus_app` reaches it.

**Evidence.**

```
The only pg_proc assertion filters by name:

    crates/store/tests/rls.rs:707
    WHERE n.nspname = 'public' AND p.proname LIKE 'auth\_user\_by\_%'

The sibling guard for views is name-blind and covers relations only:

    crates/store/tests/rls.rs:1238-1244
    WHERE n.nspname = 'public' AND c.relkind IN ('v', 'm')
    ...
    assert_eq!(views, 0);

I appended one plausible helper to a copy of migrations/0006_grants_rls.sql:

    CREATE FUNCTION recent_events(p_limit integer)
    RETURNS TABLE (user_id uuid, seq bigint, type text)
    LANGUAGE sql SECURITY DEFINER SET search_path = public STABLE
    AS $$ SELECT e.user_id, e.seq, e.type FROM events e ORDER BY e.ts DESC LIMIT p_limit $$;

`cargo test --workspace` stayed fully green (all 79 tests pass; store: 4/6/11/19/10, web: 3/11/2, worker: 6/1/2/6). The leak, as cadus_app bound to tenant A on a database that carries the mutated migration:

     via_plain_select 
    ------------------
                    1
     via_security_definer_helper 
    -----------------------------
                               2
     app_can_execute 
    -----------------
     t
```

**Failure scenario.** M5 adds an operator or auth helper as a SECURITY DEFINER function in a migration and forgets `REVOKE ALL ON FUNCTION ... FROM PUBLIC`. The gate is green: no test enumerates pg_proc except by the literal name prefix `auth_user_by_`, the view count check reads pg_class relkind 'v'/'m' only, and app_role_privilege_matrix_is_the_literal_table reads relations, not functions. In production a session bound to tenant A calls the function and reads every tenant's events, exactly the C3 break that the view guard (#6) was added to stop.

**Refuter.** The claim is demonstrable, and I reproduced it exactly on a throwaway database. Three separate points hold.

1. The gap in the suite is real. `grep -rn "pg_proc\|has_function_privilege\|prosecdef"` over the whole repository (target/ excluded) returns one site only: /home/deploy/dev/cadus2.0/crates/store/tests/rls.rs lines 699-707. That query filters `p.proname LIKE 'auth\_user\_by\_%'`, so it pins the two login functions and nothing else. No script does the job either: `grep -n "SECURITY DEFINER\|pg_proc\|FUNCTION" scripts/*.sh` returns nothing, so /home/deploy/dev/cadus2.0/scripts/gate.sh and /home/deploy/dev/cadus2.0/scripts/check_migrations.sh add no function-level check. The sibling guard at rls.rs:1233-1244 reads `pg_class` with `relkind IN ('v','m')`, and `app_role_privilege_matrix_is_the_literal_table` (rls.rs:~770) reads `relkind IN ('r','p','v','m')`. A function is not a `pg_cla

### #8 [major] The ON DELETE RESTRICT of events_user_id_fkey is pinned by no test; changing it to CASCADE erases the append-only log on account deletion and the whole store suite stays green

File: `migrations/0003_event_log.sql:13` — IDs: C2, D9

**Claim.** The migration names RESTRICT as the guarantee that the event log outlives the account, and round-1 finding #2 was a cascade that destroyed rows through this same parent, but no test asserts the delete action of `events_user_id_fkey`; the mutation to CASCADE survives the whole store suite.

**Evidence.**

```
The line under test:

    migrations/0003_event_log.sql:13
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE RESTRICT,  -- RESTRICT: the log outlives the account

Mutation to `ON DELETE CASCADE`, then `cargo test -p cadus-store`:

    m11: **SURVIVED**

Behavior on a database that carries the mutated migration:

     events_before 
    ---------------
                 1
    DELETE 1
     events_after 
    --------------
                0

The same statements on the unmutated schema:

    ERROR:  update or delete on table "users" violates foreign key constraint "events_user_id_fkey" on table "events"

`app_role_cannot_delete_users` (rls.rs:244) proves only that `cadus_app` holds no DELETE on `users`; it asserts the surviving child count on `learner_models`, never on `events`, and it says nothing about the FK action itself.
```

**Failure scenario.** M5 or M6 adds account deletion (a GDPR erase, an admin repair) and a migration flips this FK to CASCADE so the delete succeeds without a manual purge. The gate is green: nothing reads pg_constraint.confdeltype, and no test deletes a `users` row that has events. The operator or the admin endpoint then runs one `DELETE FROM users` as `cadus_admin` and silently erases that learner's whole event history — the record the C2 append-only guarantee and the M3 replay parity both depend on.

**Refuter.** I tried to refute the claim and failed. Three checks confirm it.

1. No test reads the referential action. A grep of the whole workspace for `confdeltype`, `confupdtype`, and `pg_constraint` returns zero hits in `crates/`, `scripts/`, and the CI workflow. The catalog assertions in `crates/store/tests/rls.rs` read `pg_class` (RLS flags, relkind), `pg_attribute`, `pg_policy`, and `pg_index` (line 1339, `events_attempt_idem`). None of them reads a foreign-key constraint.

2. No test deletes a `users` row that has an `events` row. The only three DELETE statements in the test suite are `DELETE FROM events` (rls.rs:203), `DELETE FROM users` (rls.rs:258), and `DELETE FROM _sqlx_migrations` (rls.rs:314). The one at rls.rs:258 runs as `cadus_app` and stops at the privilege check with SQLSTATE `42501`. Postgres never reaches the referential-action trigger, so the assertion holds under RESTRICT and

### #11 [major] migrations/0002_identity.sql states that 0006 leaves users exempt from row-level security, while 0006 enables, forces, and policies it

File: `migrations/0002_identity.sql:7` — IDs: C3

**Claim.** The header of the migration that defines `users` asserts a C3 property that the very next-but-four migration contradicts: it says all five identity tables stay outside row-level security, but `0006_grants_rls.sql` runs `ENABLE`/`FORCE ROW LEVEL SECURITY` on `users` plus three per-command policies that make an unbound `cadus_app` SELECT return zero rows.

**Evidence.**

```
migrations/0002_identity.sql:1-7 ("the five auth and identity tables" = users, auth_sessions, oauth_accounts, auth_tokens, auth_rate_counters):
    -- 0002_identity: the five auth and identity tables.
    ...
    -- without change. These tables are looked up by their own keys (email,
    -- token_hash, (scope, key, window_start)) before a tenant context exists, so
    -- 0006_grants_rls leaves them exempt from row-level security.

migrations/0006_grants_rls.sql:108-123 does the opposite for users:
    ALTER TABLE users ENABLE ROW LEVEL SECURITY;
    ALTER TABLE users FORCE ROW LEVEL SECURITY;
    CREATE POLICY users_read_self ON users FOR SELECT
        USING (id = nullif(current_setting('app.user_id', true), '')::uuid);
    CREATE POLICY users_insert ON users FOR INSERT WITH CHECK (true);
    CREATE POLICY users_update_self ON users FOR UPDATE ...

docs/SCHEMA.md agrees with 0006, not with 0002: its exempt list is auth_sessions, oauth_accounts, auth_tokens, model_call_log — users is not on it.

Measured on a freshly migrated database as cadus_app with no app.user_id bound:
    INSERT INTO users (email) VALUES ('ret@example.test'::citext) RETURNING id;
    ERROR:  new row violates row-level security policy for table "users"
    INSERT INTO users (email) VALUES ('plain@example.test'::citext);
    INSERT 0 1
    SELECT 'rows visible unbound', count(*) FROM users;  ->  0

The file is checksum-frozen (migrations/CHECKSUMS, sha256 3ea43519... for 0002_identity.sql) and scripts/check_migrations.sh forbids editing an applied migration, so the wrong statement cannot be corrected in place.
```

**Failure scenario.** An M5 author implementing the password-login path opens `migrations/0002_identity.sql` — the file that defines `users` — reads "0006_grants_rls leaves them exempt from row-level security", and writes the lookup as a plain `SELECT id, password_hash, is_admin FROM users WHERE email = $1` on the `cadus_app` pool instead of calling `auth_user_by_email`. Because the connection is unbound at that point by definition, `users_read_self` matches no row: the query returns zero rows with no error and no SQLSTATE, so every sign-in reports "unknown account" for accounts that exist, and the same silent-empty result hits the password-reset and email-verification lookups. The sign-up path fails differently and more loudly under the same wrong assumption: `INSERT INTO users (...) RETURNING id` aborts with `new row violates row-level security policy for table "users"`, as shown above.

**Refuter.** The claim is demonstrable and I found no defense for it. migrations/0002_identity.sql:1-7 titles itself "the five auth and identity tables" and states that "These tables are looked up by their own keys (email, token_hash, (scope, key, window_start)) before a tenant context exists, so 0006_grants_rls leaves them exempt from row-level security." The parenthesized key list names email, which is the key of users, so users is unambiguously inside the sentence's scope. migrations/0006_grants_rls.sql:108-123 does the opposite for that one table: ENABLE plus FORCE ROW LEVEL SECURITY, then three per-command policies (users_read_self, users_insert, users_update_self). The other four tables of the header list keep the stated property, so the sentence is wrong for exactly one of five, which is the hardest kind of stale comment to notice.

Every other authority in the repo sides with 0006, not with 0

## FIX9b

### #3 [major] cadus-migrate and the migrate_bin test harness take the role lock in two different databases, so the round-3 #2 fix does not serialize them and the suite races on pg_authid

File: `crates/store/tests/migrate_bin.rs:79` — IDs: D9, C3

**Claim.** crates/store/tests/migrate_bin.rs opens its RoleLock connection on the database that CADUS_TEST_DATABASE_URL names (cadus2_gate on a laptop, cadus2_ci in CI), while crates/store/src/bin/cadus-migrate.rs:72 defaults CADUS_MAINTENANCE_DB to 'postgres'. A PostgreSQL advisory lock is database-scoped, so the two locks never exclude each other, and the file header claim at migrate_bin.rs:11 - 'Every actor that alters a role takes the one advisory lock of ROLE_LOCK_KEY on the maintenance database of the cluster' - is false for the documented configuration.

**Evidence.**

```
Proof that the two lock domains differ. Hold the harness lock exactly as RoleLock::acquire() does, then run the binary with its shipped default:

  psql -U test -d cadus2_gate -c "SELECT pg_advisory_lock(7241001)" -c "SELECT pg_sleep(12)" &
  psql -U test -d postgres -c "SELECT d.datname, l.granted FROM pg_locks l JOIN pg_database d ON d.oid=l.database WHERE l.locktype='advisory' AND l.objid=7241001;"
   -> cadus2_gate | t
  DATABASE_URL=...:55434/cadus2_rv_corr CADUS_APP_PASSWORD=probe-pw target/debug/cadus-migrate --admin-login
   -> cadus-migrate: password set for cadus_app
      exit=0 elapsed_ms=35        (it never waited)

The same actor exists inside the suite: two_migrate_runs_on_two_databases_both_exit_zero spawns six children with migrate_command(&first_dsn, None) / (&second_dsn, None) at migrate_bin.rs:368 and :373, and migrate_command(dsn, None) does env_remove("CADUS_MAINTENANCE_DB"), so those children lock in 'postgres' while four sibling tests hold the harness lock in cadus2_gate and write the same pg_authid row.

Reproduced failure - run the test target while one such actor runs concurrently:

  ( for k in $(seq 1 400); do DATABASE_URL=.../cadus2_rv_corr CADUS_APP_PASSWORD=pw-bg-$k target/debug/cadus-migrate --admin-login; done ) &
  cargo test -p cadus-store --test migrate_bin
   -> test a_quote_in_the_app_password_reaches_the_role ... FAILED
      panicked at crates/store/tests/migrate_bin.rs:532: the precondition is a role with no password
      test result: FAILED. 10 passed; 1 failed

the_role_lock_of_the_binary_lives_in_the_maintenance_database passes only because it forces CADUS_MAINTENANCE_DB to the harness's own database (Some(&maintenance)); it never exercises the shipped default, so the mismatch is invisible to the suite.
```

**Failure scenario.** A second gate run, a scripts/deploy.sh run, or the suite's own two_migrate_runs_on_two_databases_both_exit_zero children run `ALTER ROLE cadus_app PASSWORD ...` in the 'postgres' lock domain while admin_login_sets_the_app_password or a_quote_in_the_app_password_reaches_the_role holds the cadus2_gate lock and runs clear_app_password. The harness statements at migrate_bin.rs:479 and :532 have no retry and no lock coverage, so the run panics on the precondition assertion, or the unprotected ALTER ROLE fails with 'tuple concurrently updated' (SQLSTATE XX000) and .unwrap() aborts the test. The merge gate then goes red for a reason that is not in the diff - the failure mode round-3 finding #2 was fixed to remove.

**Refuter.** The refutation fails. The two advisory-lock domains are demonstrably different in the documented configuration, and the resulting failure is reproducible. crates/store/tests/migrate_bin.rs:79 opens the RoleLock connection with the raw CADUS_TEST_DATABASE_URL value, so the lock lands in cadus2_gate (README.md:21, scripts/gate.sh:8) or cadus2_ci (.github/workflows/ci.yml:51). crates/store/src/bin/cadus-migrate.rs:72 sets DEFAULT_MAINTENANCE_DB to "postgres", and maintenance_db() at line 226 applies that default when CADUS_MAINTENANCE_DB is absent; docker-compose.yml:107 and .env.example:85 also use "postgres". A PostgreSQL advisory lock is database-scoped, so the harness lock and the binary lock never exclude each other. The file header at migrate_bin.rs:11-13 therefore states a fact that does not hold. The suite does not catch the mismatch: the_role_lock_of_the_binary_lives_in_the_mainten

### #10 [major] The two role-lock tests in migrate_bin.rs spawn cadus-migrate with inherited stdio, so the stderr they print on failure is always empty

File: `crates/store/tests/migrate_bin.rs:371` — IDs: D9

**Claim.** `two_migrate_runs_on_two_databases_both_exit_zero` and `the_role_lock_of_the_binary_lives_in_the_maintenance_database` call `Command::spawn()` without `stdout(Stdio::piped())`/`stderr(Stdio::piped())` and then read `wait_with_output().stderr`, which `std` always returns empty for an inherited handle, so the failure diagnostic of the regression test for the cluster-wide role lock is a guaranteed empty string and the child's real error text is written straight to the terminal instead.

**Evidence.**

```
crates/store/tests/migrate_bin.rs:368-386 (no .stdout/.stderr on the builder, unlike every other spawn site in the repo, e.g. crates/web/tests/http.rs:284-291):
            let left = migrate_command(&first_dsn, None)
                .arg("--admin-login")
                .env("CADUS_APP_PASSWORD", format!("pw-left-{round}"))
                .spawn()
                .unwrap();
            ...
            let left = left.wait_with_output().unwrap();
            ...
                left_stderr: String::from_utf8_lossy(&left.stderr).into_owned(),
                right_stderr: String::from_utf8_lossy(&right.stderr).into_owned(),
The same shape is at crates/store/tests/migrate_bin.rs:255-291 (`.spawn()` at 258, `child.wait_with_output()` at 279, `assert_eq!(output.status.code(), Some(0), "stderr: {stderr}")` at 291).

Observed on this box (cargo test -p cadus-store --test migrate_bin, both runs really exited 2):
    thread 'two_migrate_runs_on_two_databases_both_exit_zero' panicked at crates/store/tests/migrate_bin.rs:406:13:
    assertion `left == right` failed: round 1: left stderr: right stderr: 
      left: (Some(2), Some(2))
     right: (Some(0), Some(0))

The binary provably does write to stderr on exit 2, so the empty capture is the harness's, not the binary's:
    $ DATABASE_URL="postgresql://x@127.0.0.1:1/x" ./target/debug/cadus-migrate 2>&1 1>/dev/null
    cadus-migrate: database error: pool timed out while waiting for an open connection
    exit=2
```

**Failure scenario.** The cluster-wide advisory-lock fix regresses and one of the two paired `--admin-login` runs dies with `tuple concurrently updated`. The test fails, but the panic message it prints is `round 1: left stderr: right stderr:` with `left: (Some(2), Some(2))` — no SQLSTATE, no message, no indication whether the cause was the role race, a missing maintenance database, or a bad DSN. The child's real error line is not attributed to the failing test at all: with inherited stdio it goes to the test binary's own file descriptors, interleaved with the other 10 tests of the file. HANDOVER.md §3 requires failing output to be quoted rather than summarized; this test structurally cannot produce any.

**Refuter.** The claim is correct and demonstrable. `migrate_command` (crates/store/tests/migrate_bin.rs:127-138) sets no stdout/stderr on the builder, so a child started with `.spawn()` inherits the descriptors of the test binary. `std::process::Child::wait_with_output` fills the returned `stderr` only from a piped handle; for an inherited handle it returns an empty Vec. Both `.spawn()` sites in this file (line 258 and lines 371/376) then read `.stderr` from `wait_with_output()` and interpolate it into the failure message (line 291 `"stderr: {stderr}"`, lines 405-411 `"round {}: left stderr: {}right stderr: {}"`). Those messages are therefore guaranteed empty, and the real error line of `cadus-migrate` (which does write to stderr — `eprintln!("cadus-migrate: {err}")` at crates/store/src/bin/cadus-migrate.rs:115, exit 2) goes straight to the terminal of the test process, unattributed to the failing t

### #12 [major] cadus-migrate installs no stop-signal handler, so the migrate one-shot is deaf to SIGTERM and SIGINT as PID 1 and only SIGKILL ends it

File: `crates/store/src/bin/cadus-migrate.rs:101` — IDs: D9, R4

**Claim.** cadus-migrate is the one binary of the three with no `Shutdown::install()`, so in the compose `migrate` container it runs as PID 1 with the default disposition replaced by nothing; the kernel therefore drops SIGTERM and SIGINT, and a run that waits on the unbounded `pg_advisory_lock(7241001)` of `RoleLock::acquire` (line 193, taken on a connection with `statement_timeout` 0) can only be ended by SIGKILL and exit 137.

**Evidence.**

```
cadus-web.rs:112 `let mut shutdown = Shutdown::install()?;` and cadus-worker.rs:70 have the same line. cadus-migrate.rs has no Shutdown type at all:

    #[tokio::main]
    async fn main() -> ExitCode {
        let args: Vec<OsString> = std::env::args_os().skip(1).collect();
        ...
        match run(mode).await { Ok(()) => ExitCode::SUCCESS, Err(err) => { eprintln!(...); ExitCode::from(2) } }
    }

$ grep -c 'signal' crates/store/src/bin/cadus-migrate.rs  ->  0

Live run. I held the role lock from a second session (`SELECT pg_advisory_lock(7241001); SELECT pg_sleep(90);` on the maintenance database `postgres`), then started the shipped image as PID 1:

  docker run -d --name rvmig --network host -e DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_rv_unsafe \
    -e CADUS_APP_PASSWORD=xyz cadus2-migrate:latest cadus-migrate --admin-login

  rvmig  pid=3692794  SigCgt=0000000100000440   SIGTERM caught=False  SIGINT caught=False
  rvweb  pid=3692867  SigCgt=0000000100004442   SIGTERM caught=True   SIGINT caught=True

  docker stop -t 5 rvmig   ->  stop took 5s   ExitCode=137 OOM=false

The same container completed with exit 0 the moment I terminated the lock holder, which proves the wait itself is unbounded:
  --- after the lock is released: exited exit=0
  cadus-migrate: applied 0 migrations (6 total)

A local (non-PID-1) run with `timeout -s TERM 20` also never returned early: `exit=124 elapsed=20s`.
```

**Failure scenario.** Two upgrades touch one cluster at the same time, or an operator's psql session holds key 7241001 in `postgres`. `scripts/deploy.sh` step 3 runs `docker compose run --rm migrate`; the process blocks inside `RoleLock::acquire` with `statement_timeout` 0, prints nothing, and the script waits with no deadline of its own (only step 2 has one, HEALTH_LIMIT_SECS=120). The operator presses Ctrl-C: SIGINT reaches PID 1 of the container, the kernel drops it, and the run continues. `docker compose down` then costs the full 10 s default stop_grace_period and ends the container with SIGKILL and exit 137. The same shape blocks a first bring-up: `web` and `worker` carry `depends_on: migrate: condition: service_completed_successfully`, so `docker compose up -d` hangs on the deaf one-shot. Round-2 findings #7 and #8 and round-3 finding #7 made a bounded, signal-driven stop a merge gate for cadus-web and cadus-worker; cadus-migrate — the binary that gates the whole stack and holds the one genuinely unbounded wait in the tree — was never given a handler, and no test in crates/store/tests/migrate_bin.rs sends it a signal.

**Refuter.** The claim is demonstrable and I reproduced every element of it, so it stands. cadus-migrate installs no SIGTERM/SIGINT handler (no Shutdown type, no tokio::signal use anywhere in crates/store/src/bin/cadus-migrate.rs), the Dockerfile sets no ENTRYPOINT and no init, and docker-compose.yml gives the migrate service no `init: true`, so the binary is container PID 1. A PID 1 whose disposition for a signal is SIG_DFL has that signal dropped by the kernel (SIGNAL_UNKILLABLE), which the repo already accepted in docs/reviews/M0-review-1.md:809 and fixed for cadus-web and cadus-worker only. The wait in RoleLock::acquire is genuinely unbounded: migrate_config() sets statement_timeout_ms 0, crates/store/src/lib.rs:185 then adds no server option, no lock_timeout exists in the tree, and both the binary doc comment ("The connection carries statement_timeout 0, so the wait has no bound") and crates/sto

### #14 [major] A password with an @ or # is written into the cadus_app role by cadus-migrate and silently misparsed by the compose DSN, so migrate exits 0, deploy.sh reports DEPLOY OK, and web can no longer connect

File: `docker-compose.yml:133` — IDs: C3, D9

**Claim.** The three runtime DSNs interpolate the raw `.env` password into a URL with no percent-encoding and no validation, while `cadus-migrate --admin-login` accepts the same value and writes it into the role, so a password that is legal for `ALTER ROLE` but illegal inside a URL leaves the role rotated and the application locked out.

**Evidence.**

```
docker-compose.yml:133 `DATABASE_URL: postgresql://cadus_app:${CADUS_APP_PASSWORD:?set CADUS_APP_PASSWORD in .env}@db:5432/cadus`. crates/store/src/bin/cadus-migrate.rs:289 `alter_role_password_statement` escapes only the single quote, and password_from_env rejects only an empty or non-Unicode value. .env.example:26 is advice, not a check: "Hex output is safe inside a DSN: it needs no percent-encoding." Reproduced with CADUS_APP_PASSWORD=corr@horse#battery: `docker compose config` resolved the web DSN to `postgresql://cadus_app:corr@horse#battery@db:5432/cadus`; `docker compose run --rm migrate` printed `cadus-migrate: password set for cadus_app` and `migrate exit=0`; pg_authid confirmed `cadus_app|SCRAM-SHA-256$`; `docker compose up -d --no-deps web worker` printed `step4 exit=0`; `docker compose ps -a` then showed `cadus2rv4ops-web-1 ... Restarting (2)` with `cadus-web: database error: error communicating with database: failed to lookup address information: No address associated with hostname`.
```

**Failure scenario.** The operator follows docs/SELF_HOST.md "Rotate a password" -> "The two runtime passwords": put the new value in .env, run scripts/deploy.sh. The value comes from a password manager and contains `@`. Step 3 succeeds and rewrites the cadus_app role to that password. Step 4 exits 0 and the script prints DEPLOY OK. `cadus-web` parses the DSN with `horse#battery` as the host, fails DNS, exits 2, and crashloops; the site is down. Reverting .env alone does not recover, because the role now holds the new password -- the operator must both revert .env and re-run deploy.sh, and nothing in the output points at the password as the cause. A `%` in the password is worse: it is a valid percent-escape introducer, so the DSN can parse into a different password with no error at all.

**Refuter.** The claim is demonstrable end to end, and no guard exists anywhere in the repo. docker-compose.yml interpolates the raw .env value into all three DSNs with no percent-encoding (lines 104, 133, and the worker block). cadus-migrate validates only emptiness and Unicode validity (password_from_env, crates/store/src/bin/cadus-migrate.rs:267) and escapes only the single quote for ALTER ROLE (alter_role_password_statement, line 289), so a value such as `corr@horse#battery` is legal for the role but not for the URL. crates/store/src/lib.rs:184 parses the DSN with no validation. A grep over crates/, scripts/, and docs/ found no charset check; .env.example:26 and docs/SELF_HOST.md:133 are advice only. I reproduced the parse failure with the built cadus-web binary, and confirmed PostgreSQL accepts the same password for CREATE ROLE. scripts/deploy.sh:79-85 runs `docker compose up -d --no-deps web wo

## FIX9c

### #6 [major] The R4 dependency guard (the FIX4 fix for round-1 finding #22) reads only the [dependencies] table of two crates, so an HTTP client compiles into cadus-web with the whole gate green

File: `crates/web/tests/purity.rs:41` — IDs: R4, L6, T1

**Claim.** `runtime_dependency_keys` inspects only the literal `[dependencies]` table of the crate it lives in, so a forbidden HTTP client added under `[target.'cfg(...)'.dependencies]` of `crates/web`, or added to `crates/store` (which has no purity test at all and is a `[dependencies]` entry of `cadus-web`), links into the request-handling binary while both R4 purity tests and clippy pass — the round-1 #22 fix does not enforce what its own docs claim.

**Evidence.**

```
crates/web/tests/purity.rs:39-47 and crates/worker/tests/purity.rs:40-48 are identical:
```rust
fn runtime_dependency_keys(manifest: &toml::Value) -> Vec<String> {
    let table = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
```
Both `web_dependency_set_is_exactly_the_declared_list` (line 51) and `web_declares_no_http_client_and_no_model_sdk` (line 70) call it, so `[target.*.dependencies]` is never read. crates/core/tests/purity.rs:34-54 proves the omission is not a limitation of the approach — `all_dependency_keys` there walks `dependencies`, `dev-dependencies`, `build-dependencies` AND every `[target.<spec>]` sub-table, and the R3 test uses it. The R4 tests do not.

Bypass 1 — target-scoped table in the guarded crate itself. In a scratch copy of the tree I added to crates/web/Cargo.toml:
```toml
[target.'cfg(unix)'.dependencies]
# R4 violation: an outbound HTTP client on the request-handling crate.
hyper-util = { version = "0.1.20" }
```
`hyper-util` is item 2 of the eight-name FORBIDDEN list at crates/web/tests/purity.rs:19-28. Result:
```
   Compiling hyper-util v0.1.20
   Compiling cadus-web v0.1.0 (.../crates/web)
running 2 tests
test web_dependency_set_is_exactly_the_declared_list ... ok
test web_declares_no_http_client_and_no_model_sdk ... ok
test result: ok. 2 passed; 0 failed
```
and `cargo clippy -p cadus-web --all-targets -- -D warnings` -> `Finished `dev` profile`. The crate really links it; it is not a dev-only artifact.

Bypass 2 — the unguarded crate every handler already imports. Reverting that and instead adding `hyper-util = { version = "0.1.20" }` to `[dependencies]` of crates/store/Cargo.toml (crates/store/tests holds only rls.rs, store_api.rs, migrate_bin.rs — no purity test):
```
test core_declares_no_network_or_database_dependency ... ok
test core_dependency_set_is_exactly_serde_serde_json_thiserror ... ok
test web_dependency_set_is_exactly_the_declared_list ... ok
test web_declares_no_http_client_and_no_model_sdk ... ok
test worker_declares_no_http_client_and_no_model_sdk ... ok
test worker_dependency_set_is_exactly_the_declared_list ... ok
test result: ok. 6 passed; 0 failed
```
crates/web/Cargo.toml:19 is `cadus-store = { workspace = true }`, so the client is in scope for every handler in crates/web/src/lib.rs.

The operator-facing claim that is false as written, docs/SELF_HOST.md:219-220: "a new HTTP-client or model-SDK dependency on either crate is a test failure and a reviewable diff." The sen
```

**Failure scenario.** M5 implements A4 diagnosis. To avoid the diagnosis_jobs round trip the unit puts the model client in cadus-store (or under a `[target.'cfg(unix)'.dependencies]` table of cadus-web, which reads as an ordinary platform-gating line in review) and awaits the call inside the grade handler. The agent runs scripts/gate.sh: fmt passes, `clippy --all-targets -- -D warnings` passes, `cargo test --workspace` passes — all six purity tests included, demonstrated above — `cargo sqlx prepare --check` passes (no SQL changed), check_migrations.sh passes (migrations/ unchanged), check_ops.sh passes. The script prints GATE OK and it merges. L6 ("any model call that a learner waits on: none exist"), T1 (0 model tokens on the grade path), and R4 are all broken, the L2 p95 is now bound to provider latency, and the only mechanism HANDOVER.md §3 names as the merge gate for budgets reported nothing. The identical edit against crates/core is rejected in seconds by crates/core/tests/purity.rs:73-82, which does walk the target tables.

**Refuter.** The claim is correct and I cannot refute it. `runtime_dependency_keys` at crates/web/tests/purity.rs:39-47 and the identical helper at crates/worker/tests/purity.rs:40-48 call `manifest.get("dependencies")` on the root table. A `[target.'cfg(...)'.dependencies]` table nests under root key `target`, so the helper never reaches it. Both R4 tests in each crate use only this helper, so both bypasses hold.

The repository proves the omission is an oversight and not a design limit. crates/core/tests/purity.rs:34-54 defines `all_dependency_keys`, which walks `dependencies`, `dev-dependencies`, `build-dependencies`, and every `[target.<spec>]` sub-table, and the R3 test at line 73 uses it. The R4 tests diverge from the author's own pattern in the same tree.

I tested four refutation paths and all four fail. First, the sibling test `web_dependency_set_is_exactly_the_declared_list` uses the same n

### #9 [major] check_ops.sh check (d) reads only command[0] and skips a service with no command:, so a wrong flag or a deleted command: passes the gate and stops the stack

File: `scripts/check_ops.sh:124` — IDs: C3, D9, R4

**Claim.** The gate step that exists to stop a broken `command:` (round-3 finding #12) proves only that the first token of `command:` names a binary in the image, and silently skips any service whose `command:` is absent, so two realistic edits to docker-compose.yml keep the gate green while `docker compose up -d` leaves the site down.

**Evidence.**

```
scripts/check_ops.sh:119-124
    command = service.get("command")
    if not command:
        continue
    if isinstance(command, str):
        command = shlex.split(command)
    print(image, command[0], name)

and scripts/check_ops.sh:165 runs only `command -v "$1"` on that first token:
    docker run --rm --entrypoint sh "$image" -c 'command -v "$1"' sh "$binary"

Demonstration A (wrong flag). I took the real `docker compose config --format json` of this repo, set migrate's command to ["cadus-migrate","--admin-loginn"], and ran the script's own read_plan extractor verbatim. Output is byte-identical to the correct config:
    cadus2-migrate cadus-migrate migrate
    cadus2-web cadus-web web
    cadus2-worker cadus-worker worker
so `command -v cadus-migrate` succeeds and (d) prints PASS. The real binary disagrees:
    $ DATABASE_URL=... ./target/debug/cadus-migrate --admin-loginn
    usage: cadus-migrate [--admin-login]
    ...
    exit=2

Demonstration B (deleted command). Setting worker's command to null and running the same extractor plus the script's own `while read -r image binary service` loop (scripts/check_ops.sh:162-177) prints:
    --- service_commands ---
    cadus2-migrate cadus-migrate migrate
    cadus2-web cadus-web web
    PASS: commands -- every command: binary exists in its image (2 checked)
Check (c) still passes because all three binaries are in the image. `command_count` is never compared with the number of built services, and `built_images` still lists cadus2-worker.

The file header and the operator docs both claim the opposite:
    scripts/check_ops.sh:22-27  "every `command:` binary of the compose file exists in the image that runs it"
    docs/SELF_HOST.md:213-216   "A renamed binary target, a wrong `dockerfile:` key, or a mistyped `command:` then fails the gate instead of the operator's next bring-up"
```

**Failure scenario.** A developer mistypes the migrate flag as `command: ["cadus-migrate", "--admin-loginn"]`. `scripts/gate.sh` runs `scripts/check_ops.sh`, which prints `PASS: commands -- every command: binary exists in its image (3 checked)` and the whole gate reports GATE OK, so the change merges. On the server `docker compose up -d --build` starts the `migrate` one-shot, which prints its usage and exits 2. `web` and `worker` both declare `depends_on: migrate: condition: service_completed_successfully` (docker-compose.yml:158-160, 175-177), so neither ever starts and no migration is applied. The same PASS is printed if the `command:` key is deleted from `worker`: the worker container then runs the Dockerfile default `CMD ["cadus-web"]` (Dockerfile:75) with the `cadus_admin` DSN, the C3 boot guard rejects the BYPASSRLS role, the container exits 3 and crash-loops under `restart: unless-stopped`, and no async tier runs at all.

**Refuter.** I cannot refute the claim. I reproduced both demonstrations with the script's own extractor and the repository's real docker-compose.yml, and the code paths are exactly as the reviewer describes.

1. Scope of check (d). scripts/check_ops.sh:162-177 runs one test per line: `docker run --rm --entrypoint sh "$image" -c 'command -v "$1"' sh "$binary"`. `$binary` is `command[0]` only (scripts/check_ops.sh:124). The check proves binary existence on PATH. It never runs the argument list, so a wrong flag passes.

2. Silent skip. scripts/check_ops.sh:119-121 does `command = service.get("command")` and `if not command: continue`. A service with no `command:` key produces no line. `command_count` counts only the lines that the extractor emits, so the PASS text reports the reduced coverage as a success. Nothing compares `command_count` with the number of built services, and `built_images` (check (c)

### #13 [major] scripts/deploy.sh step 4 checks nothing after it replaces web and worker, so an upgrade that leaves the site crash-looping prints DEPLOY OK and exits 0

File: `scripts/deploy.sh:79` — IDs: D9, C3

**Claim.** `docker compose up -d --no-deps web worker caddy` returns 0 as soon as the containers start, not when they stay up, and deploy.sh reads no exit code, no health state, and no log line afterward; an upgrade whose new `web` dies at start therefore ends with `DEPLOY OK` and script exit 0, which contradicts the script's own contract at line 26 ("0 for a finished upgrade, 1 for a failed step").

**Evidence.**

```
scripts/deploy.sh:78-85 — the last step has no verification at all:

    echo "== 4/4 start the new web, worker, and caddy"
    docker compose up -d --no-deps web worker caddy

    echo "DEPLOY OK"
    echo "Do a check:"
    echo "  docker compose ps"

Step 2 gates on `{{.Health}}` and step 3 gates on the migrate exit code; step 4 gates on nothing.

Proof that compose reports success for a container that starts and then dies (alpine control case):
  compose up -d rc=0
  web restarting 0
  worker running 0

Proof with the shipped image and a value that only `web` reads (`SHUTDOWN_DEADLINE_SECS: 10s` in the environment block):
  step 4 equivalent:
    rc=0  -> deploy.sh would now print DEPLOY OK and exit 0
  web restarting
  web-1  | ERROR cadus_web: cadus-web: SHUTDOWN_DEADLINE_SECS must be a whole number of seconds, not "10s"

And the binary's own exit code for that value:
  cadus-web: SHUTDOWN_DEADLINE_SECS must be a whole number of seconds, not "10s"
  exit=2
```

**Failure scenario.** An operator edits `.env` and writes `SHUTDOWN_DEADLINE_SECS=10s` (or `DB_STATEMENT_TIMEOUT_MS=5s`), then runs `scripts/deploy.sh`. Neither key reaches the `migrate` service, so step 3 passes and the schema lands. Step 4 destroys the serving `web` and `worker` and starts the new ones. `cadus-web` reads the value, exits 2 before it binds, and `restart: unless-stopped` restarts it without end. `docker compose up -d` still returns 0, so the script prints `DEPLOY OK` and exits 0. Caddy answers every visitor with 502 while the operator's automation records a successful upgrade. The exit-code-3 path has the same shape: a later migration that grants `cadus_app` BYPASSRLS makes the C3 boot guard refuse to start, and deploy.sh still reports DEPLOY OK. The script exists because round-3 finding #16 established that a failed upgrade must stop with the old version still serving; here the old version is already gone and the failure is reported as success.

**Refuter.** The claim is demonstrable and I could not refute it. scripts/deploy.sh:79 runs `docker compose up -d --no-deps web worker caddy` and line 81 prints DEPLOY OK with no exit-code test, no health test, and no log test between. `set -euo pipefail` propagates a non-zero exit of that command, but the failure mode in question returns 0: I proved with a live alpine control stack that `docker compose up -d --no-deps` returns rc=0 for a service that starts, exits 2, and enters a `restart: unless-stopped` loop (`docker compose ps -a` then shows `dies restarting`). cadus-web has exactly that failure shape: crates/web/src/bin/cadus-web.rs maps `Fatal::Startup` to ExitCode::from(2) and the C3 RLS boot guard to ExitCode::from(3), and `shutdown_deadline()` returns Fatal::Startup for a value such as "10s". docker-compose.yml gives the `migrate` service only DATABASE_URL, CADUS_MAINTENANCE_DB, CADUS_APP_PA
