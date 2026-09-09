#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
guard="$repo_root/scripts/build_guard.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
expect_fail() { if "$@" >/dev/null 2>&1; then echo "expected failure: $*" >&2; exit 1; fi; }
"$guard" --target-dir "$scratch/check" --reserve-gib 0 --target-cap-gib 1 --check >/dev/null
expect_fail "$guard" --target-dir "$scratch/bad-poll" --poll-seconds 0 --check
expect_fail "$guard" --target-dir "$scratch/reserve" --reserve-gib 100000 --target-cap-gib 1 --check
expect_fail "$guard" --target-dir "$scratch/start" --reserve-gib 0 --target-cap-gib 1 -- does-not-exist
grep -q 'finish status=127 reason=child-exit' "$scratch/start/build-guard/"*.log
"$guard" --target-dir "$scratch/child" --reserve-gib 0 --target-cap-gib 1 -- bash -c 'exit 7' >/dev/null 2>&1 && exit 1
grep -q 'finish status=7 reason=child-exit' "$scratch/child/build-guard/"*.log
sleep 3 &
sentinel=$!
trap 'kill "$sentinel" 2>/dev/null || true; rm -rf "$scratch"' EXIT
expect_fail "$guard" --target-dir "$scratch/cap" --reserve-gib 0 --target-cap-gib 0 --poll-seconds 1 -- bash -c 'dd if=/dev/zero of="$1/blob" bs=1024 count=2 status=none; sleep 2; touch "$1/finished"' _ "$scratch/cap"
[ ! -e "$scratch/cap/finished" ]
kill -0 "$sentinel"
wait "$sentinel" || true
grep -q 'finish status=1 reason=limit-during-command' "$scratch/cap/build-guard/"*.log
echo "build guard tests passed"
