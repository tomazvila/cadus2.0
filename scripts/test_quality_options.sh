#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
quality="$repo_root/scripts/quality.sh"
bash -n "$quality"
grep -q -- '--no-coverage) skip_coverage=1' "$quality"
grep -q 'SKIP rust-coverage: waived by --no-coverage' "$quality"
grep -q 'SKIP web-coverage: waived by --no-coverage' "$quality"
grep -q 'check rust-coverage bash -c' "$quality"
grep -q 'check web-coverage bash -c' "$quality"
echo "quality --no-coverage option plan passed"
