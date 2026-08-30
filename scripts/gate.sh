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
# Memory rule for this box (2026-08-30): a 16-job rustc build beside other work
# pushed the machine into swap and froze sshd. Six jobs is the cap unless the
# caller sets its own value.
: "${CARGO_BUILD_JOBS:=6}"
export CARGO_BUILD_JOBS

# Put the project toolchain first, if it is installed on this machine.
for dir in "$HOME/.local/share/cadus2-tooling/gcc/bin" \
    "$HOME/.local/share/cadus2-tooling/shellcheck-bin/bin" \
    "$HOME/.cargo/bin"; do
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

# FIX-M6-G. The SPA route table gets its oracle from
# `crates/web/tests/route_table.rs`: the test dumps every route of `create_app`
# into `web/src/api/routes.generated.json`, and `web/test/api-contract.test.ts`
# reads that fixture. The `cargo test` step above already fails on a committed
# fixture that the router does not match.
#
# This step covers the other half: it runs the REWRITE path and proves the
# rewrite changes nothing. Without it, a dump that wrote one file and compared
# another would pass the check above and still hand the SPA a stale table. The
# file is restored before the failure exit, so the gate never leaves a rewritten
# fixture in the tree.
echo "== the route fixture is unchanged after a rewrite"
routes_fixture="web/src/api/routes.generated.json"
routes_saved="target/routes.generated.json.gate"
mkdir -p target
cp "$routes_fixture" "$routes_saved"
CADUS_ROUTES_BLESS=1 cargo test -p cadus-web --test route_table
if ! cmp -s "$routes_saved" "$routes_fixture"; then
    cp "$routes_saved" "$routes_fixture"
    rm -f "$routes_saved"
    echo "GATE FAILED: $routes_fixture is not the route table of create_app"
    exit 2
fi
rm -f "$routes_saved"

# The parity fold runs a SECOND time in the release profile (spec section 7, trap
# T21). The debug profile emits a real `pow` call for every `powf`, while an
# optimized build rewrites a literal base into `exp2`, which is a different number
# in the last bit. The digests must hold in both profiles, so a rewrite that the
# debug run cannot see fails the gate here.
echo "== cargo test --release -p cadus-core --test parity_events --test projector"
cargo test --release -p cadus-core --test parity_events --test projector

# The budget benchmarks (L1, L2) run AFTER the test suite and never beside it:
# parallel suites contend on this box and on a two-core runner, and a contended
# benchmark measures the scheduler, not the code (spec section 10.5).
# `scripts/bench.sh` runs benchmark A always and benchmark B when
# CADUS_TEST_DATABASE_URL is set, which the gate always sets above.
# docs/reference/l1-budget.md holds the split both benchmarks assert.
if [ ! -f scripts/bench.sh ]; then
    echo "GATE FAILED: scripts/bench.sh is missing"
    exit 2
fi

echo "== scripts/bench.sh"
if [ -x scripts/bench.sh ]; then
    scripts/bench.sh
else
    bash scripts/bench.sh
fi

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
for tool in docker python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "GATE FAILED: $tool is required (scripts/check_ops.sh)"
        exit 2
    fi
done

# The shell scripts are ops code, and the gate reads them like the Rust code.
# A missing linter is a failure, not a skip: the same rule as the unset DSN
# above. The GitHub ubuntu runners ship shellcheck. README.md, section
# "The gate", gives the one-line install for a laptop.
if ! command -v shellcheck >/dev/null 2>&1; then
    echo "GATE FAILED: shellcheck is required"
    echo "get it with: nix build nixpkgs#shellcheck.bin -o ~/.local/share/cadus2-tooling/shellcheck"
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
