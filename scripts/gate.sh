#!/usr/bin/env bash
# Cadus 2.0 merge gate. Run it from any directory. It fails on the first failed step.
#
# Before the first run, create the gate database and apply the migrations to it.
# `cargo sqlx prepare --check` compiles the query macros against that database,
# so an empty database fails the run with `relation "users" does not exist`:
#
#   export DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate
#   cargo sqlx database create
#   cargo sqlx migrate run          # from the repository root
#   CADUS_TEST_DATABASE_URL="$DATABASE_URL" scripts/gate.sh
#
# Repeat `cargo sqlx migrate run` after every new migration.
set -euo pipefail

# Put the project toolchain first, if it is installed on this machine.
for dir in "$HOME/.local/share/cadus2-tooling/gcc/bin" "$HOME/.cargo/bin"; do
    if [ -d "$dir" ]; then
        PATH="$dir:$PATH"
    fi
done
export PATH

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# The database steps are part of the gate, not an option. A gate that skips half
# its checks is not a gate, so an unset test DSN fails the run here.
if [ -z "${CADUS_TEST_DATABASE_URL:-}" ]; then
    echo "GATE FAILED: set CADUS_TEST_DATABASE_URL"
    exit 2
fi

echo "== cargo fmt --all --check"
cargo fmt --all --check

echo "== cargo clippy --all-targets --workspace -- -D warnings"
cargo clippy --all-targets --workspace -- -D warnings

echo "== cargo test --workspace"
cargo test --workspace

# `--all-targets` puts the queries of the tests into the check too. Without it
# the check covers the library and the binaries only, and a stale query file of a
# test stays hidden until an offline build breaks.
echo "== cargo sqlx prepare --check --workspace -- --all-targets"
DATABASE_URL="$CADUS_TEST_DATABASE_URL" cargo sqlx prepare --check --workspace -- --all-targets

# A missing check script is a failure, not a skip. The same rule as the unset
# DSN above: the gate runs every check or it fails.
if [ ! -f scripts/check_migrations.sh ]; then
    echo "GATE FAILED: scripts/check_migrations.sh is missing"
    exit 2
fi

echo "== scripts/check_migrations.sh"
if [ -x scripts/check_migrations.sh ]; then
    DATABASE_URL="$CADUS_TEST_DATABASE_URL" scripts/check_migrations.sh
else
    DATABASE_URL="$CADUS_TEST_DATABASE_URL" bash scripts/check_migrations.sh
fi

# The ops surface is the last step: it builds the image, and the build takes the
# most time. docs/plans/M0.md makes `docker compose config` and the image build
# the acceptance check of U6, so the gate runs both.
if ! command -v docker >/dev/null 2>&1; then
    echo "GATE FAILED: docker is required"
    exit 2
fi

if [ ! -f scripts/check_ops.sh ]; then
    echo "GATE FAILED: scripts/check_ops.sh is missing"
    exit 2
fi

echo "== scripts/check_ops.sh"
if [ -x scripts/check_ops.sh ]; then
    scripts/check_ops.sh
else
    bash scripts/check_ops.sh
fi

echo "GATE OK"
