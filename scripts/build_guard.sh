#!/usr/bin/env bash
set -euo pipefail
usage(){ echo "usage: build_guard.sh [--check] [--target-dir DIR] [--reserve-gib N] [--target-cap-gib N] -- COMMAND" >&2; exit 2; }
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
target_dir="${CADUS_BUILD_TARGET_DIR:-$repo_root/target}"; reserve_gib="${CADUS_BUILD_RESERVE_GIB:-150}"; cap_gib="${CADUS_BUILD_TARGET_CAP_GIB:-120}"; poll_secs="${CADUS_BUILD_GUARD_POLL_SECS:-5}"; check_only=0
while [ "$#" -gt 0 ]; do case "$1" in --check) check_only=1; shift;; --target-dir) target_dir="${2:-}"; shift 2;; --reserve-gib) reserve_gib="${2:-}"; shift 2;; --target-cap-gib) cap_gib="${2:-}"; shift 2;; --poll-seconds) poll_secs="${2:-}"; shift 2;; --) shift; break;; *) usage;; esac; done
case "$reserve_gib:$cap_gib" in *[!0-9:]*|:*) usage;; esac
[ "$check_only" = 1 ] || [ "$#" -gt 0 ] || usage
mkdir -p "$target_dir"; target_dir="$(cd "$target_dir" && pwd)"
reserve_kib=$((reserve_gib * 1048576)); cap_kib=$((cap_gib * 1048576)); evidence_dir="$target_dir/build-guard"; mkdir -p "$evidence_dir"; evidence="$evidence_dir/$(date -u +%Y%m%dT%H%M%SZ)-$$.log"
record(){ printf '%s %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$evidence" >&2; }
limits(){ local free used; free="$(df -Pk "$target_dir" | awk 'NR==2 {print $4}')"; used="$(du -sk "$target_dir" | awk '{print $1}')"; record "sample free_kib=$free reserve_kib=$reserve_kib target_kib=$used target_cap_kib=$cap_kib"; [ "$free" -ge "$reserve_kib" ] && [ "$used" -le "$cap_kib" ]; }
record "start target_dir=$target_dir reserve_gib=$reserve_gib target_cap_gib=$cap_gib"
if ! limits; then record "BLOCK limit"; exit 1; fi
if [ "$check_only" = 1 ]; then record "BUILD_GUARD_OK check"; exit 0; fi
command -v setsid >/dev/null || { record "BLOCK setsid-missing"; exit 2; }
export CARGO_INCREMENTAL=0 CARGO_TARGET_DIR="$target_dir"
setsid "$@" & child=$!
stop(){ record "STOP process_group=$child"; kill -TERM -- "-$child" 2>/dev/null || true; sleep 1; kill -KILL -- "-$child" 2>/dev/null || true; }
trap 'stop; exit 130' INT TERM HUP
while kill -0 "$child" 2>/dev/null; do if ! limits; then stop; wait "$child" 2>/dev/null || true; record "BUILD_GUARD_FAILED limit"; exit 1; fi; sleep "$poll_secs"; done
wait "$child"; status=$?; record "finish status=$status"; exit "$status"
