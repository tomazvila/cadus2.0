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
#   4. Replace `web`, `worker`, and `caddy` with the new image, then prove that
#      the new containers stay up.
#
# Step 4 uses `--no-deps`, so compose starts exactly these three services and
# touches neither `db` nor `migrate`.
#
# Step 4 then does a check, because `docker compose up -d` returns 0 as soon as
# the containers START, not when they stay up. A `web` that reads a bad value
# out of `.env` exits 2 before it binds, `restart: unless-stopped` restarts it
# without end, and the old script printed DEPLOY OK over a site that answers
# every visitor with 502 (review round 4, finding #13). The check waits up to
# START_LIMIT_SECS for all three facts:
#
#   - `docker compose ps web --format json` reports State `running`;
#   - the same for `worker`;
#   - `docker compose logs web` holds the literal line `listening on`.
#
# If the deadline passes, the script prints the last LOG_TAIL_LINES log lines of
# the service that failed and exits 1.
#
# Input: docker with the Compose plugin, and a `.env` file beside
# docker-compose.yml. The script writes no file.
#
# Options:
#   --no-caddy            Start `web` and `worker` only, and leave `caddy`
#                         alone. `DEPLOY_SKIP_CADDY=1` does the same. Use it on
#                         a stack that terminates TLS somewhere else, and in a
#                         test bring-up that binds no port 80.
#
# Exit codes: 0 for a finished upgrade, 1 for a failed step, 2 for a bad
# argument or a missing input.
#
# See docs/SELF_HOST.md, section "Upgrade".
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

skip_caddy="${DEPLOY_SKIP_CADDY:-0}"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --no-caddy)
            skip_caddy=1
            ;;
        *)
            echo "deploy: unknown argument \`$1\`" >&2
            echo "usage: scripts/deploy.sh [--no-caddy]" >&2
            exit 2
            ;;
    esac
    shift
done

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

# Step 4: how long the new `web` and `worker` get to reach state `running` and
# to log `listening on`. Both processes open one pool and bind one socket, so
# 30 s is generous. A crash loop never reaches the state, so the wait always
# runs to the deadline and then reports the failure.
START_LIMIT_SECS=30

# How many log lines of the failed service the script prints on that failure.
LOG_TAIL_LINES=40

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

services=(web worker)
if [ "$skip_caddy" = "1" ]; then
    echo "== 4/4 start the new web and worker (caddy is left alone)"
else
    services+=(caddy)
    echo "== 4/4 start the new web, worker, and caddy"
fi
docker compose up -d --no-deps "${services[@]}"

# Read the State field of one service out of `docker compose ps --format json`.
# The command prints one JSON object for one service. An absent container gives
# an empty string, which the caller reports as `unknown`.
service_state() {
    local report state
    report="$(docker compose ps "$1" --format json 2>/dev/null || true)"
    state="$(printf '%s' "$report" | sed -n 's/.*"State":[ ]*"\([^"]*\)".*/\1/p')"
    printf '%s' "${state%%$'\n'*}"
}

echo "wait up to ${START_LIMIT_SECS} s for the new web and worker"
deadline=$((SECONDS + START_LIMIT_SECS))
while true; do
    failed=""
    web_state="$(service_state web)"
    worker_state="$(service_state worker)"
    web_log="$(docker compose logs web 2>&1 || true)"

    if [ "$web_state" != "running" ]; then
        failed=web
    elif [ "$worker_state" != "running" ]; then
        failed=worker
    else
        case "$web_log" in
            *"listening on"*) ;;
            *) failed=web ;;
        esac
    fi

    if [ -z "$failed" ]; then
        echo "web and worker are running, and web logged \`listening on\`"
        break
    fi

    if [ "$SECONDS" -ge "$deadline" ]; then
        echo "deploy: the new containers did not come up within ${START_LIMIT_SECS} s" >&2
        echo "deploy: web state ${web_state:-unknown}, worker state ${worker_state:-unknown}" >&2
        echo "deploy: the last ${LOG_TAIL_LINES} log lines of ${failed}:" >&2
        docker compose logs --tail "$LOG_TAIL_LINES" "$failed" >&2 || true
        echo "deploy: the new schema is in place; correct the fault and run this script again" >&2
        exit 1
    fi
    sleep 2
done

echo "DEPLOY OK"
echo "Do a check:"
echo "  docker compose ps"
echo "  docker compose logs web | grep 'listening on'"
echo "  curl -fsS http://localhost/api/health"
