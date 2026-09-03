#!/usr/bin/env bash
# Cadus 2.0 quality gate. Run it from any directory. It runs every check, prints one
# PASS or FAIL line per check, and exits 1 when any check failed.
#
# Limits (a function or a file passes when its value is on the safe side):
#   lines per file          < 500     every tracked .rs .ts .tsx .js .mjs .py .sh file
#   cyclomatic complexity   < 22      rust-code-analysis (Rust), ESLint `complexity` (web)
#   cognitive complexity    < 22      rust-code-analysis (Rust), eslint-plugin-sonarjs (web)
#   Halstead difficulty     < 80      rust-code-analysis (Rust), web/scripts/halstead.mjs
#   test coverage           = 100%    cargo llvm-cov (Rust), Vitest v8 (web)
#   CRAP                    < 25      cc^2 * (1 - coverage)^3 + cc, per function
#   surviving mutants       = 0       cargo-mutants (Rust), Stryker (web)
#   dead code               = 0       clippy -D warnings, unused pub items, cargo-machete, knip
#   redundant code          = 0       jscpd, 50 tokens or 5 lines, both languages
#   `any` or `unknown`      = 0       ESLint, every TypeScript file, tests included
#
# Usage:
#   scripts/quality.sh                 every check, Rust and web
#   scripts/quality.sh --rust          the Rust checks only
#   scripts/quality.sh --web           the web checks only
#   scripts/quality.sh --no-mutants    skip the two mutation runs (hours on this box)
#
# The Rust coverage and mutation checks need CADUS_TEST_DATABASE_URL, the same value
# that scripts/gate.sh uses. Reports land under target/quality/.
set -uo pipefail
: "${CARGO_BUILD_JOBS:=6}"
export CARGO_BUILD_JOBS
for dir in "$HOME/.local/share/cadus2-tooling/gcc/bin" "$HOME/.cargo/bin"; do
    if [ -d "$dir" ]; then PATH="$dir:$PATH"; fi
done
export PATH
# rust-code-analysis-cli links against libstdc++, which this box holds in the nix
# store only.
if [ -z "${LD_LIBRARY_PATH:-}" ]; then
    libdir="$(find /nix/store -maxdepth 3 -name 'libstdc++.so.6' 2>/dev/null | head -1)"
    if [ -n "$libdir" ]; then export LD_LIBRARY_PATH="$(dirname "$libdir")"; fi
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"
out="$repo_root/target/quality"
mkdir -p "$out"
# cargo-mutants copies the tree once per job. Keep those copies off the tmpfs /tmp.
export TMPDIR="$HOME/.cache/cadus2_mutants"
mkdir -p "$TMPDIR"

run_rust=1
run_web=1
run_mutants=1
for arg in "$@"; do
    case "$arg" in
        --rust) run_web=0 ;;
        --web) run_rust=0 ;;
        --no-mutants) run_mutants=0 ;;
        *) echo "unknown option: $arg"; exit 2 ;;
    esac
done

failed=0
# check <name> <command...>: runs the command, keeps its output in target/quality/<name>.log,
# prints the last line of that output beside PASS or FAIL.
check() {
    local name="$1"
    shift
    if "$@" >"$out/$name.log" 2>&1; then
        echo "PASS $name: $(tail -n 1 "$out/$name.log")"
    else
        failed=1
        echo "FAIL $name: $(tail -n 1 "$out/$name.log")  (target/quality/$name.log)"
    fi
}

source_files() {
    git ls-files crates web scripts | grep -E '\.(rs|ts|tsx|js|mjs|py|sh)$' | grep -vE '\.d\.ts$|/e2e/work/'
}

check loc bash -c 'source_files | python3 scripts/quality/loc.py'

if [ "$run_rust" = 1 ]; then
    rca="$out/rca"
    rm -rf "$rca"
    mkdir -p "$rca"
    rust-code-analysis-cli -m -O json -o "$rca" -p crates -j 4 >/dev/null 2>&1
    check rust-complexity python3 scripts/quality/rust_complexity.py "$rca"
    check rust-dead bash -c 'git ls-files crates | grep "\.rs$" | python3 scripts/quality/rust_dead.py'
    check rust-unused-deps cargo machete
    check rust-clippy cargo clippy --all-targets --workspace -- -D warnings
    check rust-clones web/node_modules/.bin/jscpd --config .jscpd.json crates
    if [ -z "${CADUS_TEST_DATABASE_URL:-}" ]; then
        echo "FAIL rust-coverage: set CADUS_TEST_DATABASE_URL"
        failed=1
    else
        check rust-coverage bash -c "cargo llvm-cov --workspace --all-targets --json --output-path '$out/rust-cov.json' >/dev/null && python3 scripts/quality/rust_coverage.py '$out/rust-cov.json' '$rca'"
        if [ "$run_mutants" = 1 ]; then
            # The pure crates run mutants in parallel; the database-backed crates run one
            # at a time, because their tests share the roles of one Postgres cluster.
            for crate in cadus-core:3 cadus-model-client:3 cadus-store:1 cadus-web:1 cadus-worker:1; do
                name="${crate%%:*}"
                jobs="${crate##*:}"
                check "rust-mutants-$name" bash -c "cargo mutants -p '$name' --jobs $jobs --output '$out/mutants-$name' >/dev/null 2>&1; python3 scripts/quality/mutants.py cargo '$out/mutants-$name/mutants.out'"
            done
        fi
    fi
fi

if [ "$run_web" = 1 ]; then
    cd "$repo_root/web"
    check web-lint npm run --silent lint
    check web-types npm run --silent types
    check web-halstead bash -c 'node scripts/halstead.mjs $(git ls-files . | grep -E "\.(ts|tsx)$" | grep -v "\.d\.ts$")'
    check web-dead npx knip --no-progress
    check web-clones ../web/node_modules/.bin/jscpd --config ../.jscpd.json src test scripts e2e
    check web-coverage bash -c "npx vitest run --coverage --coverage.provider=v8 --coverage.reporter=json --coverage.reportsDirectory='$out/webcov' --coverage.include='src/**' >/dev/null 2>&1; node scripts/web-coverage.mjs '$out/webcov/coverage-final.json'"
    if [ "$run_mutants" = 1 ]; then
        check web-mutants bash -c "npx stryker run --jsonReporter.fileName '$out/stryker.json' >/dev/null 2>&1; python3 ../scripts/quality/mutants.py stryker '$out/stryker.json'"
    fi
    cd "$repo_root"
fi

if [ "$failed" = 1 ]; then
    echo "QUALITY GATE FAILED"
    exit 1
fi
echo "QUALITY GATE PASSED"
