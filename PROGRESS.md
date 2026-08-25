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

### Gate on integrated `main` (before FIX1 and review)

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

### Open findings

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
