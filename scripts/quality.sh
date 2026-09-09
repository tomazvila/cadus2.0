#!/usr/bin/env bash
# Cadus 2.0 quality gate. Run it from any directory. It runs every check, prints one
# PASS or FAIL line per check, and exits 1 when any check failed.
#
# Limits (a function or a file passes when its value is on the safe side):
#   lines per file          < 500     every tracked .rs .ts .tsx .js .mjs .py .sh file
#   cyclomatic complexity   < 22      rust-code-analysis (Rust, Python), ESLint `complexity` (web)
#   cognitive complexity    < 22      rust-code-analysis (Rust, Python), eslint-plugin-sonarjs (web)
#   Halstead difficulty     < 80      rust-code-analysis (Rust, Python), web/scripts/halstead.mjs
#   test coverage           = 100%    cargo llvm-cov (Rust), Vitest v8 (web)
#   CRAP                    < 25      cc^2 * (1 - coverage)^3 + cc, per function
#   dead code               = 0       clippy -D warnings, unused pub items, cargo-machete, knip
#   redundant code          = 0       jscpd, 50 tokens or 5 lines, every language
#   `any` or `unknown`      = 0       ESLint, every TypeScript file, tests included
#
# Usage:
#   scripts/quality.sh                 every check, Rust and web
#   scripts/quality.sh --rust          the Rust checks only
#   scripts/quality.sh --web           the web checks only
#   scripts/quality.sh --no-coverage   every non-coverage check
#
# The Rust coverage check needs CADUS_TEST_DATABASE_URL, the same value
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
    if [ -n "$libdir" ]; then
        LD_LIBRARY_PATH="$(dirname "$libdir")"
        export LD_LIBRARY_PATH
    fi
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root" || exit 2
out="$repo_root/target/quality"
mkdir -p "$out"
run_rust=1
run_web=1
skip_coverage=0
for arg in "$@"; do
    case "$arg" in
        --rust) run_web=0 ;;
        --web) run_rust=0 ;;
        --no-coverage) skip_coverage=1 ;;
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
# `check` runs its command in a child shell; the function must reach it.
export -f source_files

check loc bash -c 'source_files | python3 scripts/quality/loc.py'

if [ "$run_rust" = 1 ]; then
    rca="$out/rca"
    rm -rf "$rca"
    mkdir -p "$rca"
    # The tool reads Rust and Python; bash under scripts/ has the line limit only.
    rust-code-analysis-cli -m -O json -o "$rca" -p crates -p scripts -j 4 >/dev/null 2>&1
    check rust-complexity python3 scripts/quality/rust_complexity.py "$rca"
    check rust-dead bash -c 'git ls-files crates | grep "\.rs$" | python3 scripts/quality/rust_dead.py'
    check rust-unused-deps cargo machete
    check rust-clippy cargo clippy --all-targets --workspace -- -D warnings
    check rust-clones web/node_modules/.bin/jscpd --config .jscpd.json crates scripts
    if [ "$skip_coverage" = 1 ]; then
        echo "SKIP rust-coverage: waived by --no-coverage"
    elif [ -z "${CADUS_TEST_DATABASE_URL:-}" ]; then
        echo "FAIL rust-coverage: set CADUS_TEST_DATABASE_URL"
        failed=1
    else
        check rust-coverage bash -c "cargo llvm-cov --workspace --all-targets --json --output-path '$out/rust-cov.json' && python3 scripts/quality/rust_coverage.py '$out/rust-cov.json' '$rca'"
    fi
fi

if [ "$run_web" = 1 ]; then
    cd "$repo_root/web" || exit 2
    check web-lint npm run --silent lint
    check web-types npm run --silent types
    check web-halstead bash -c 'node scripts/halstead.mjs $(git ls-files . | grep -E "\.(ts|tsx)$" | grep -v "\.d\.ts$")'
    check web-dead npx knip --no-progress
    check web-clones ../web/node_modules/.bin/jscpd --config ../.jscpd.json src test scripts e2e
    if [ "$skip_coverage" = 1 ]; then
        echo "SKIP web-coverage: waived by --no-coverage"
    else
        check web-coverage bash -c "npx vitest run --coverage --coverage.provider=v8 --coverage.reporter=json --coverage.reportsDirectory='$out/webcov' --coverage.include='src/**' >/dev/null 2>&1; node scripts/web-coverage.mjs '$out/webcov/coverage-final.json'"
    fi
    cd "$repo_root" || exit 2
fi

if [ "$failed" = 1 ]; then
    echo "QUALITY GATE FAILED"
    exit 1
fi
echo "QUALITY GATE PASSED"
