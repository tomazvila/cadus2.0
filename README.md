# Cadus 2.0

Cadus 2.0 is a Rust rewrite of the Cadus math tutor. `REQUIREMENTS.md` is the
authority; `HANDOVER.md` gives the build process. The workspace holds four crates:
`cadus-core` (pure scheduling and pedagogy), `cadus-store` (Postgres adapter),
`cadus-web` (HTTP API), and `cadus-worker` (background jobs). Adapters depend on
the core; the core depends on no adapter (R3).

## The gate

Run `scripts/gate.sh` before every commit. It runs `cargo fmt`, `cargo clippy`
with `-D warnings`, the test suite, `cargo sqlx prepare --check`,
`scripts/check_migrations.sh`, and `scripts/check_ops.sh`.

`CADUS_TEST_DATABASE_URL` is required, not optional: the gate exits 2 and runs
no check when the variable is unset. The DSN needs superuser rights, and the
database it names must exist and must carry the schema. Create it and migrate it
once:

```sh
export DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate
cargo sqlx database create
cargo sqlx migrate run          # from the repository root
CADUS_TEST_DATABASE_URL="$DATABASE_URL" scripts/gate.sh
```

Run `cargo sqlx migrate run` again after every new migration. `cargo sqlx
prepare --check` compiles the query macros against that database, so an empty
database fails the gate with `relation "users" does not exist`.

`scripts/check_ops.sh` needs `docker` and `python3` on PATH. It runs
`docker compose config`, then `docker compose build`, then one container per
built image to prove that `cadus-web`, `cadus-worker`, and `cadus-migrate` are
on the `PATH` there and that every `command:` of `docker-compose.yml` names a
binary the image carries. The gate fails with `GATE FAILED: docker is required`
when docker is absent.

Migrations are forward-only. After you add `migrations/NNNN_name.sql`, run
`scripts/check_migrations.sh --freeze` and commit `migrations/CHECKSUMS` with
the new file.
