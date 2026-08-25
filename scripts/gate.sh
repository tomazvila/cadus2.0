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

echo "== cargo fmt --all --check"
cargo fmt --all --check

echo "== cargo clippy --all-targets --workspace -- -D warnings"
cargo clippy --all-targets --workspace -- -D warnings

echo "== cargo test --workspace"
cargo test --workspace

if [ -n "${CADUS_TEST_DATABASE_URL:-}" ]; then
    echo "== cargo sqlx prepare --check --workspace"
    DATABASE_URL="$CADUS_TEST_DATABASE_URL" cargo sqlx prepare --check --workspace

    if [ -x scripts/check_migrations.sh ]; then
        echo "== scripts/check_migrations.sh"
        DATABASE_URL="$CADUS_TEST_DATABASE_URL" scripts/check_migrations.sh
    elif [ -f scripts/check_migrations.sh ]; then
        echo "== scripts/check_migrations.sh"
        DATABASE_URL="$CADUS_TEST_DATABASE_URL" bash scripts/check_migrations.sh
    else
        echo "SKIPPED: scripts/check_migrations.sh (file does not exist)"
    fi
else
    echo "SKIPPED: cargo sqlx prepare --check --workspace (CADUS_TEST_DATABASE_URL is not set)"
    echo "SKIPPED: scripts/check_migrations.sh (CADUS_TEST_DATABASE_URL is not set)"
fi

echo "GATE OK"
