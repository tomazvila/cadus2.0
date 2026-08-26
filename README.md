# Cadus 2.0

Cadus 2.0 is a Rust rewrite of the Cadus math tutor. `REQUIREMENTS.md` is the
authority; `HANDOVER.md` gives the build process. The workspace holds four crates:
`cadus-core` (pure scheduling and pedagogy), `cadus-store` (Postgres adapter),
`cadus-web` (HTTP API), and `cadus-worker` (background jobs). Adapters depend on
the core; the core depends on no adapter (R3).

## The gate

Run `scripts/gate.sh` before every commit. It runs `cargo fmt`, `cargo clippy`
with `-D warnings`, the test suite, the parity fold in the release profile,
`cargo sqlx prepare --check`, `scripts/check_migrations.sh`, and
`scripts/check_ops.sh`.

The release step is `cargo test --release -p cadus-core --test parity_events
--test projector`, and it is not a duplicate of `cargo test --workspace`. The
debug profile emits a real `pow` call for every `powf`; an optimized build
rewrites a call with a LITERAL base into `exp2`, which differs from `pow` by one
unit in the last place. CPython computes `0.5 ** x` through `pow`, so a
rewritten call folds a learner model that is not the 1.0 model. The core hides
every such base behind `std::hint::black_box`, and this step proves that the 20
committed streams keep their 1.0 digests in the profile the Docker image ships.
See `docs/reference/projector-1.0-spec.md`, section 7, trap T21.

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

`scripts/check_ops.sh` needs `docker`, `python3`, and `shellcheck` on PATH. It
runs `docker compose config`, then `docker compose build`, then one container
per built image to prove that `cadus-web`, `cadus-worker`, and `cadus-migrate`
are on the `PATH` there and that every `command:` of `docker-compose.yml` names
a binary the image carries. Its last two checks run
`shellcheck -S warning scripts/*.sh` and prove that every published port of
`.github/workflows/ci.yml` binds `127.0.0.1`. The gate fails with
`GATE FAILED: docker is required` when docker is absent, and with
`GATE FAILED: shellcheck is required` when shellcheck is absent.

The GitHub `ubuntu-latest` runners ship shellcheck. On a laptop, install it
once:

```sh
nix build nixpkgs#shellcheck.bin -o ~/.local/share/cadus2-tooling/shellcheck
```

Nix writes the link for the `bin` output as `shellcheck-bin`, so the binary
lands in `~/.local/share/cadus2-tooling/shellcheck-bin/bin`. `scripts/gate.sh`
and `scripts/check_ops.sh` put that directory on PATH by themselves. Your
package manager works too: any `shellcheck` on PATH satisfies the gate.

Migrations are forward-only. After you add `migrations/NNNN_name.sql`, run
`scripts/check_migrations.sh --freeze` and commit `migrations/CHECKSUMS` with
the new file.

## Deploy

`docker compose up -d --build` brings a new server up. `scripts/deploy.sh`
upgrades a server that already serves traffic: it builds the image, starts `db`,
runs the migrations in a one-shot, and replaces `web`, `worker`, and `caddy`
only after that one-shot exits 0. Never upgrade a running stack with
`docker compose up -d`: compose destroys the serving containers before the
migration runs. See docs/SELF_HOST.md, section "Upgrade".

Caddy publishes host ports 80 and 443. `CADDY_HTTP_PORT` and `CADDY_HTTPS_PORT`
in `.env` move those host ports for a test bring-up on a server where another
stack already holds them. See docs/SELF_HOST.md, section "Proxy ports".
