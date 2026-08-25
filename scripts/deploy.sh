#!/usr/bin/env bash
# Cadus 2.0 upgrade (C3, D9). Run it from any directory on the server.
#
# This script is THE upgrade procedure for a stack that already serves traffic.
# Do not upgrade with `docker compose up -d`. That command creates every
# container first and starts them second, so it destroys the serving `web` and
# `worker` BEFORE the `migrate` one-shot runs. A migration that then fails
# leaves both in state `Created`, `restart: unless-stopped` never fires because
# Docker never started them, and the site stays down (review round 3,
# finding #16).
#
# The order here keeps the old version up until the new schema is in place:
#
#   1. Build the new image. The running containers keep the old image.
#   2. Start `db` and wait for the healthcheck.
#   3. Run the migrations in a one-shot container. A non-zero exit stops the
#      script here, and the old `web` and `worker` still serve traffic.
#   4. Replace `web`, `worker`, and `caddy` with the new image.
#
# Step 4 uses `--no-deps`, so compose starts exactly these three services and
# touches neither `db` nor `migrate`.
#
# Input: docker with the Compose plugin, and a `.env` file beside
# docker-compose.yml. The script writes no file and reads no argument.
#
# Exit codes: 0 for a finished upgrade, 1 for a failed step, 2 for a missing
# input.
#
# See docs/SELF_HOST.md, section "Upgrade".
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if ! command -v docker >/dev/null 2>&1; then
    echo "deploy: docker is not on PATH" >&2
    exit 2
fi

if [ ! -f .env ]; then
    echo "deploy: .env is missing; copy .env.example and fill it in" >&2
    exit 2
fi

# The healthcheck of `db` runs every 5 s and retries 20 times, so 120 s covers a
# first start with an initdb phase on a slow disk.
HEALTH_LIMIT_SECS=120

echo "== 1/4 build the new image"
docker compose build

echo "== 2/4 start db and wait for the healthcheck"
docker compose up -d db

deadline=$((SECONDS + HEALTH_LIMIT_SECS))
while true; do
    state="$(docker compose ps db --format '{{.Health}}' | head -n 1)"
    if [ "$state" = "healthy" ]; then
        echo "db is healthy"
        break
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
        echo "deploy: db did not report healthy within ${HEALTH_LIMIT_SECS} s (state: ${state:-unknown})" >&2
        echo "deploy: the old web and worker still serve traffic" >&2
        exit 1
    fi
    sleep 2
done

echo "== 3/4 apply the migrations"
if ! docker compose run --rm migrate; then
    echo "deploy: the migrate one-shot failed; the upgrade stops here" >&2
    echo "deploy: the old web and worker still serve traffic on the old schema" >&2
    echo "deploy: read the output above, fix the migration, and run this script again" >&2
    exit 1
fi

echo "== 4/4 start the new web, worker, and caddy"
docker compose up -d --no-deps web worker caddy

echo "DEPLOY OK"
echo "Do a check:"
echo "  docker compose ps"
echo "  docker compose logs web | grep 'listening on'"
echo "  curl -fsS http://localhost/api/health"
