#!/usr/bin/env bash
# Cadus 2.0 ops check (C3, D9). Run it from any directory.
#
# docs/plans/M0.md makes two commands the acceptance check of U6: the compose
# file validates, and the image builds. Neither runs in `cargo test`, so a
# renamed binary target or a broken compose key first appears on the operator's
# server. This script runs both in the gate instead.
#
# The script does two checks and prints one line per check:
#   (a) compose -- `docker compose config` resolves docker-compose.yml. The
#                  placeholder values below stand in for `.env`, which the
#                  repository never carries. Every `:?` variable of the compose
#                  file needs a value here.
#   (b) build   -- `docker build` builds the image and tags it `cadus2:gate`.
#
# Input: docker on PATH. The script reads no `.env` file and writes no state
# outside the local docker image store.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! command -v docker >/dev/null 2>&1; then
    echo "FAIL: input   -- docker is not on PATH" >&2
    exit 2
fi

rc=0

# ---------------------------------------------------------------------------
# (a) the compose file resolves
# ---------------------------------------------------------------------------
compose_log=""
if compose_log="$(SITE_ADDRESS=:80 \
    POSTGRES_PASSWORD=x \
    CADUS_APP_PASSWORD=x \
    CADUS_ADMIN_PASSWORD=x \
    docker compose config 2>&1 >/dev/null)"; then
    echo "PASS: compose -- docker compose config resolves docker-compose.yml"
else
    echo "FAIL: compose -- docker compose config failed"
    printf '%s\n' "$compose_log"
    rc=1
fi

# ---------------------------------------------------------------------------
# (b) the image builds
# ---------------------------------------------------------------------------
build_log=""
if build_log="$(docker build -t cadus2:gate . 2>&1)"; then
    echo "PASS: build   -- docker build tagged cadus2:gate"
else
    echo "FAIL: build   -- docker build failed"
    printf '%s\n' "$build_log"
    rc=1
fi

exit "$rc"
