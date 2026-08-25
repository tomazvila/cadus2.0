#!/usr/bin/env bash
# Cadus 2.0 merge gate. Run it from any directory. It fails on the first failed step.
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

if [ -x scripts/check_migrations.sh ]; then
    echo "== scripts/check_migrations.sh"
    DATABASE_URL="$CADUS_TEST_DATABASE_URL" scripts/check_migrations.sh
elif [ -f scripts/check_migrations.sh ]; then
    echo "== scripts/check_migrations.sh"
    DATABASE_URL="$CADUS_TEST_DATABASE_URL" bash scripts/check_migrations.sh
else
    echo "SKIPPED: scripts/check_migrations.sh (file does not exist)"
fi

echo "GATE OK"
