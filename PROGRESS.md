# Cadus 2.0 — build progress

One entry per milestone cycle (HANDOVER.md §2). Newest first.

## M0 — workspace scaffold, schema v1, RLS proof (2026-08-25)

Requirement IDs: R1–R4, C2, C3, D9. Plan: `docs/plans/M0.md`.
Owner decisions: O1/O2/O3 unanswered; the build uses the HANDOVER.md defaults
(build the migrator but do not run it; T4 defaults; keep the 1.0 SPA).

### Environment set up on this box

- rustup stable (rustc 1.98), rustfmt, clippy, sqlx-cli 0.9 in `~/.cargo/bin`; C linker
  from `nix build nixpkgs#gcc` (no sudo on the box).
- Throwaway Postgres 16 container `cadus2-testdb` on `127.0.0.1:55434`, trust auth.

### Units and outcome

| Unit | Branch | Result |
|---|---|---|
| U1 scaffold + gate + core purity test | `m0/u1` | gate green; purity test fails when `sqlx` is added to core (mutation-checked) |
| U2 migrations 0001–0006 + `docs/SCHEMA.md` | `m0/u2` | applied twice on a fresh DB; `cadus_app` UPDATE/DELETE on `events` → 42501; 10 tables RLS-forced, 6 exempt |
| U3 store crate: config, pool, migrate, boot guard, `begin_tenant`, test support, 5 RLS tests, `cadus-migrate` bin | `m0/u3` | gate green |
| U4 web crate: `/api/health`, `/api/ready`, boot guard exit 3, SIGTERM shutdown, 6 tests | `m0/u4` | gate green |
| U5 worker skeleton: heartbeat loop, SIGTERM, 2 tests | `m0/u5` | gate green |
| U6 ops: CI workflow, `scripts/check_migrations.sh`, Dockerfile, compose, Caddyfile, `docs/SELF_HOST.md` | `m0/u6` | actionlint clean; migration check fails on a numbering gap (checked) |
| Mutation check of U3 tests | `m0/mut-u3` | 5/6 mutants killed; survivor M4 (policy `WITH CHECK` removal, DML-equivalent) → fix unit FIX1 |

Orchestrator glue: migration 0001 absorbs a lost `CREATE ROLE` race
(`duplicate_object OR unique_violation`); verified with three concurrent migrations.
The policy text uses `nullif(current_setting('app.user_id', true), '')::uuid` so a
`RESET` GUC fails closed (zero rows) instead of raising 22P02.

### Gate on integrated `main` before review (commit 48be9e9)

```
cargo fmt --all --check                                  ok
cargo clippy --all-targets --workspace -- -D warnings    ok
cargo test --workspace                                   15 passed, 0 failed
cargo sqlx prepare --check --workspace -- --all-targets  ok
scripts/check_migrations.sh                              name/fresh/rerun/drop PASS
```

### Adversarial review round 1 (stage 4)

Six lenses, four find/refute rounds: 104 findings raised, 46 confirmed by independent
refuters (`docs/reviews/M0-review-1.md`). Blockers: CI never migrated its gate database;
`cadus_app` could delete another tenant's rows through the `users` FK cascade; the
transaction-local scope of the tenant GUC and the BYPASSRLS half of the boot guard had
no test; the RLS tests read cluster role state instead of the migration. Fix units
FIX1–FIX5 address all 46; round 2 runs on the fixed tree.

Cost note: round 1 used 128 agents. Rounds 2 and 3 are capped at two find/refute
rounds each, major+ only.

### Adversarial review round 2

On the tree after FIX1–FIX5: 32 raised, 16 confirmed (`docs/reviews/M0-review-2.md`).
Blocker: FIX5 removed trust auth from the CI Postgres service, so the passwordless
`cadus_app` test pool cannot authenticate in CI. Majors: `cadus_app` kept table-wide
UPDATE on `users` (cross-tenant `password_hash`/`is_admin` writes), no privilege on
`model_call_log` and `content_store` was revoked, the superuser half of the boot guard
had no test, `pool.close()` and the boot probes ran outside the shutdown select, no
statement timeout on the pool. Fix units FIX7a–c address all 16.

### Adversarial review round 3

On the tree after FIX7 (commit ffe0f62): 31 raised, 16 confirmed
(`docs/reviews/M0-review-3.md`). Several are follow-on defects of earlier fixes: the
orchestrator's advisory lock in `cadus-migrate` was database-scoped, not cluster-wide
(reproduced 8/8); the new 5 s statement timeout also bound the migration run; `users`
INSERT still wrote `is_admin`, and SELECT still exposed every `password_hash`; the web
shutdown spent its deadline twice (20.01 s against a 20 s grace period). Fix units
FIX8a–c address all 16.

### Adversarial review round 4

On the tree after FIX8 (commit 1bdfdf1): 28 raised, 14 confirmed
(`docs/reviews/M0-review-4.md`). Blocker: the three auth tables carry `user_id`, hold
no RLS, and keep full DML for `cadus_app`, so a bound tenant forges a session for any
account. The round-3 login functions returned any account's `password_hash` to any
caller and declared `search_path = public` without `pg_temp`. Fix units FIX9a–c
address all 14. The loop did not go dry after four rounds (16, 16, 14 confirmed);
see "Decision for the owner" below.

### Final gate on `main` after FIX9 (M0 close)

```
cargo fmt --all --check                                  ok
cargo clippy --all-targets --workspace -- -D warnings    ok
cargo test --workspace                                   96 passed, 0 failed
cargo sqlx prepare --check --workspace -- --all-targets  ok
scripts/check_migrations.sh   name, frozen, fresh, rerun, drop   PASS
scripts/check_ops.sh          compose, build, binaries, commands, deploy, invariants   PASS
```

What M0 delivers: workspace `core`/`store`/`web`/`worker`; migrations 0001–0006 with
frozen checksums; 15 RLS-scoped tables with pinned policy text, a literal privilege
matrix for `cadus_app` (tables, columns, sequences, functions, FK delete actions);
the append-only `events` proof (UPDATE/DELETE → 42501); the C3 boot guard (superuser
and BYPASSRLS, exit 3); `cadus-migrate` with a cluster-wide role lock, password rule,
and signal handling; `/api/health`, `/api/ready`; worker heartbeat loop; CI workflow;
Dockerfile, compose stack, `scripts/deploy.sh`, `docs/SELF_HOST.md`, `docs/SCHEMA.md`.

### Decision for the owner — review loop did not go dry

HANDOVER.md §2 stage 4 loops until two consecutive review rounds find nothing new.
After four rounds the count per round was 46, 16, 16, 14 confirmed findings; every
confirmed finding was fixed and the fix was mutation-checked. Tokens spent by
subagents: implement waves ≈ 2.4 M; review rounds ≈ 7.7 M + 3.9 M + 3.8 M + 3.8 M.
Each further round costs about 4 M tokens and, on the evidence of rounds 2–4, finds
10–16 more findings, most of them second-order effects of earlier fixes on the
grants/RLS surface. The orchestrator stopped after round 4 and asks the owner to
choose: (a) continue the loop on M0 at this cost, or (b) accept M0 with the open
findings below and let M1/M2 proceed, with M5 (auth) as the milestone that revisits
the `users`/auth-table policies with real handler code.

### Owner answers (2026-08-26)

O1: start fresh, no event migration. O3: React + TypeScript rewrite in M6. Review
loop: stop after four rounds, fix the accepted-not-fixed items, continue with M1. See
`docs/DECISIONS.md`.

### Open findings

- (M5 contract) The auth layer must call the five SECURITY DEFINER lookups
  (`auth_user_by_email`, `auth_user_by_id`, `auth_session_by_token_hash`,
  `auth_token_by_hash`, `oauth_account_lookup`) BEFORE binding a tenant, then bind and
  write through the policies. `docs/SCHEMA.md` "The M5 auth contract" has the call order.
- (closed 2026-08-26, FIX10) The accepted-not-fixed items of M0 are fixed: default
  `BIND_ADDR` pinned by a pure function test; R4 purity scans handler sources for socket
  and process tokens; a client-side query bound `DB_CLIENT_TIMEOUT_MS` (sqlx 0.9 has no
  TCP keepalive); CI binds the service port on 127.0.0.1; one shared deaf-Postgres test
  server; the `tuple concurrently updated` retry has unit tests; `cadus-store` has a
  purity test; shellcheck runs in the gate; `scripts/deploy.sh` ran end to end with caddy
  on `CADDY_HTTP_PORT=18080`.
- (closed 2026-08-26) The citext function-count pin is replaced by a per-function check.

- (M3) `events.payload` is `jsonb` (D7). 1.0 stored `json` because its diagnostic
  projection read key order. The 2.0 projector must not depend on key order; the M3
  parity test must cover diagnostic events.
- (deploy) `citext` needs the Postgres contrib package; the official `postgres:16`
  image ships it.
- (tests, closed by FIX6) `TestDb::with` is the only entry point. It drops the
  `cadus2_t_*` database of a test body that panics. `TestDb::create` and
  `TestDb::drop` are gone from the public API.
- (docs drift, 1.0) `web_states` payload column is `doc` in 1.0 DDL, `state` in 1.0
  docs; 2.0 follows the DDL.
