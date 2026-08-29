#!/usr/bin/env bash
# Cadus 2.0 budget benchmarks (L1, L2). Run it from any directory.
#
# The script runs the two benchmarks of docs/reference/l1-budget.md:
#
#   A  crates/core/tests/bench_l1.rs                -- core only, always
#   A  crates/web/tests/bench_grade_cpu.rs          -- the grade CPU, always
#   B  crates/store/tests/bench_serve_roundtrip.rs  -- Postgres, when the test
#   B  crates/store/tests/bench_grade_transaction.rs   DSN is set
#
# It also runs the M2 L2 budget tests (crates/core/tests/answer_check.rs) with
# CADUS_RELEASE_BENCH=1. Those tests hold a 5 ms per-check budget and a 1 s
# corpus budget in a release build, and a budget ten times wider without the
# variable. Before M4 review 1 (finding 20) no script set it, so the release
# budgets of the checker never ran anywhere.
#
# Both run in the RELEASE profile, because the budget numbers of
# docs/reference/l1-budget.md are release numbers. A debug run measures the
# unoptimized exact arithmetic and holds a budget ten times wider (the same rule
# crates/core/tests/answer_check.rs carries for L2).
#
# The benchmarks run AFTER `cargo test` and never beside it. Spec section 10.5:
# parallel suites contend on this box and on a two-core runner, and a contended
# benchmark measures the scheduler. `CADUS_BENCH` is the switch: without it in
# the environment every timing test prints one skip line, so a plain
# `cargo test --workspace` stays a test run.
#
# Each benchmark writes its numbers to $CADUS_BENCH_DIR (default target/bench)
# as JSON. CI uploads that directory, so a trend is visible per run.
#
# Benchmark A prints one deterministic number beside the timings: `<n>
# allocations`. ALLOCATION_BOUND in crates/core/tests/bench_l1.rs is
# floor(n * 1005 / 1000), and docs/reference/l1-budget.md section 8 records n and
# the four steps that re-pin it. If a change moves n on purpose, do those steps
# in the same commit (M4 review 2, findings 7 and 11).
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

export CADUS_BENCH=1
export CADUS_BENCH_DIR="${CADUS_BENCH_DIR:-$repo_root/target/bench}"
mkdir -p "$CADUS_BENCH_DIR"
echo "benchmark artifacts go to $CADUS_BENCH_DIR"

# One test thread. Benchmark A counts the allocations of its own thread and
# times a loop; a second test beside it competes for the same core.
echo "== benchmark A: cargo test --release -p cadus-core --test bench_l1"
cargo test --release -p cadus-core --test bench_l1 -- --test-threads=1 --nocapture

# Benchmark A, the grade half of the request tier: the D5 hash, `answer::check`
# and the D-M5-2 work quality tier, through `cadus_web::grade::deterministic_grade`
# (M5 U12). It needs no database at run time: it reads the 1.0 answer corpus.
echo "== benchmark A (grade CPU): cargo test --release -p cadus-web --test bench_grade_cpu"
cargo test --release -p cadus-web --test bench_grade_cpu -- --test-threads=1 --nocapture

# The L2 budgets of the M2 checker, at their release numbers. `cargo test
# --workspace` runs the same file in the debug profile against the ten-times
# budget, beside every other suite; this step runs it alone, optimized, against
# the 5 ms number docs/reference/l1-budget.md cites.
echo "== L2 budgets: CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test answer_check"
CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test answer_check -- --test-threads=1

# Benchmark B needs a throwaway Postgres. The gate always sets the DSN, so the
# gate always runs B. A laptop run without a cluster still gets A.
if [ -n "${CADUS_TEST_DATABASE_URL:-}" ]; then
    echo "== benchmark B (serve): cargo test --release -p cadus-store --test bench_serve_roundtrip"
    cargo test --release -p cadus-store --test bench_serve_roundtrip -- \
        --test-threads=1 --nocapture

    # The GRADE transaction of D-O2 (M5 U12). It runs after the serve half and
    # never beside it: two transaction benchmarks on one cluster contend on the
    # same connection budget, and a contended benchmark measures the scheduler.
    echo "== benchmark B (grade): cargo test --release -p cadus-store --test bench_grade_transaction"
    cargo test --release -p cadus-store --test bench_grade_transaction -- \
        --test-threads=1 --nocapture

    # The 20,000-event log of FIX-M5-G (L1, L4, L5). It runs after the grade
    # half, for the same reason: one transaction benchmark at a time.
    echo "== benchmark B (long log): cargo test --release -p cadus-store --test bench_long_log"
    cargo test --release -p cadus-store --test bench_long_log -- \
        --test-threads=1 --nocapture
else
    echo "SKIPPED benchmark B: CADUS_TEST_DATABASE_URL is not set"
fi

echo "BENCHMARKS OK"
