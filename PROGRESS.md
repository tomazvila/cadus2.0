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

### Open findings

- (M3) `events.payload` is `jsonb` (D7). 1.0 stored `json` because its diagnostic
  projection read key order. The 2.0 projector must not depend on key order; the M3
  parity test must cover diagnostic events.
- (deploy) `citext` needs the Postgres contrib package; the official `postgres:16`
  image ships it.
- (tests) `TestDb::drop` is best effort; a panicking test leaks its `cadus2_t_*`
  database on the throwaway cluster.
- (docs drift, 1.0) `web_states` payload column is `doc` in 1.0 DDL, `state` in 1.0
  docs; 2.0 follows the DDL.
