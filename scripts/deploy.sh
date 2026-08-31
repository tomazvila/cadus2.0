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
#   4. Replace `web`, `worker`, and `caddy` with the new image, apply the
#      Caddyfile of this commit, then prove that all three containers stay up.
#
# Step 4 uses `--no-deps`, so compose starts exactly these three services and
# touches neither `db` nor `migrate`.
#
# THE THREE SERVICES GO TOGETHER. `caddy` is not TLS termination alone: the
# `spa` image bakes the built SPA bundle into that container (M6 S14), and the
# browser gets every byte of the front end from it. A deploy that starts the new
# `web` and leaves `caddy` alone serves the PREVIOUS commit's bundle against the
# new API. The script therefore has no flag that skips `caddy` (M6 review,
# findings F24 and F12).
#
# Step 4 then does a check, because `docker compose up -d` returns 0 as soon as
# the containers START, not when they stay up. A `web` that reads a bad value
# out of `.env` exits 2 before it binds, `restart: unless-stopped` restarts it
# without end, and the old script printed DEPLOY OK over a site that answers
# every visitor with 502 (review round 4, finding #13). The check covers EVERY
# service that step 4 starts, `caddy` included: `caddy` carries no healthcheck,
# so a Caddyfile that Caddy refuses gave a restart loop on the sole ingress
# under a DEPLOY OK line (M6 review, finding F11). The check waits up to
# START_LIMIT_SECS for these facts:
#
#   - `web`, `worker`, and `caddy` each report container state `running`;
#   - none of the three restarts while the check runs;
#   - `docker compose logs web` holds the literal line `listening on`;
#   - `docker compose logs caddy` holds the literal line
#     `serving initial configuration`, for a `caddy` that step 4 recreated.
#
# The facts must hold STABLE_POLLS times, and the polls are 2 s apart. One
# observation says the container is up at one instant. A container in a restart
# loop is up for a fraction of every cycle, so a single observation can catch it
# in that fraction and report a stack that is already down again.
#
# If the deadline passes, the script names the CONTAINER that failed, prints its
# last LOG_TAIL_LINES log lines, and exits 1.
#
# Input: docker with the Compose plugin, and a `.env` file beside
# docker-compose.yml. The script writes no file.
#
# CADDY_HTTP_PORT and CADDY_HTTPS_PORT in `.env` move the HOST ports of the
# proxy (defaults 80 and 443), so a box on which another stack already holds 80
# and 443 still runs the whole four-service deploy. The check command at the end
# reads the published port and never guesses it.
#
# Options: none. Any argument stops the script with exit 2.
#
# Exit codes: 0 for a finished upgrade, 1 for a failed step, 2 for a bad
# argument or a missing input.
#
# See docs/SELF_HOST.md, section "Upgrade".
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [ "$#" -gt 0 ]; then
    echo "deploy: unknown argument \`$1\`" >&2
    echo "usage: scripts/deploy.sh" >&2
    echo "deploy: the script starts web, worker, and caddy together, because the caddy container holds the SPA bundle" >&2
    exit 2
fi

# The old `--no-caddy` flag and its DEPLOY_SKIP_CADDY variable are gone. Both
# skipped the container that holds the SPA bundle. Stop loudly on the old
# variable rather than run an upgrade the operator did not ask for.
if [ -n "${DEPLOY_SKIP_CADDY:-}" ]; then
    echo "deploy: DEPLOY_SKIP_CADDY is gone; the caddy container holds the SPA bundle and always starts with web and worker" >&2
    echo "deploy: unset DEPLOY_SKIP_CADDY and run the script again" >&2
    exit 2
fi

if ! command -v docker >/dev/null 2>&1; then
    echo "deploy: docker is not on PATH" >&2
    exit 2
fi

if [ ! -f .env ]; then
    echo "deploy: .env is missing; copy .env.example and fill it in" >&2
    exit 2
fi

# The services of step 4, in one list. `caddy` is in it always.
SERVICES=(web worker caddy)

# The healthcheck of `db` runs every 5 s and retries 20 times, so 120 s covers a
# first start with an initdb phase on a slow disk.
HEALTH_LIMIT_SECS=120

# Step 4: how long the new containers get to reach state `running` and to log
# their start line. Each process opens one pool and binds one socket, so 30 s is
# generous. A crash loop never reaches the state, so the wait always runs to the
# deadline and then reports the failure.
#
# `scripts/check_ops.sh` check (e) drives the failure path against a docker stub
# and sets this variable to a few seconds, so the gate proves the failure and
# waits no 30 s for it.
START_LIMIT_SECS="${DEPLOY_START_LIMIT_SECS:-30}"

# How many polls in a row must hold every fact before the script prints
# DEPLOY OK. The polls are 2 s apart, so two of them cover 2 s of steady state.
STABLE_POLLS=2

# How many log lines of the failed service the script prints on that failure.
LOG_TAIL_LINES=40

# The container id of one compose service, or the empty string when compose has
# no container for it.
container_id() {
    docker compose ps -q "$1" 2>/dev/null | head -n 1
}

# Three facts of one service in one line: `<state> <restart count> <name>`.
# An absent container gives an empty line, which the caller reports.
service_facts() {
    local id
    id="$(container_id "$1")"
    if [ -z "$id" ]; then
        return 0
    fi
    docker inspect --format '{{.State.Status}} {{.RestartCount}} {{.Name}}' "$id" 2>/dev/null || true
}

# The literal line that proves the process of one service came up. A service
# with no line here is proved by its container state alone.
start_line() {
    case "$1" in
        web) printf 'listening on' ;;
        caddy) printf 'serving initial configuration' ;;
        *) printf '' ;;
    esac
}

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
caddy_before="$(container_id caddy)"
docker compose up -d --no-deps "${SERVICES[@]}"
caddy_after="$(container_id caddy)"

# Apply the Caddyfile of this commit.
#
# `deploy/Caddyfile` is a BIND MOUNT, and Caddy reads its configuration once, at
# start. A commit that edits the Caddyfile alone changes no image and no service
# spec, so `docker compose up -d` reports `Running`, keeps the container, and the
# proxy goes on with the configuration it loaded weeks ago (M6 review, finding
# F12). The container id is the fact that says which happened: a NEW id is a new
# container that read the file at start; the SAME id is the old process with the
# old configuration.
#
# `caddy reload` hands the running process the file through the admin API, and
# drops no connection. A configuration that Caddy refuses fails that command, and
# the recreate below then puts the failure where the start check sees it.
#
# The reload reads the file THROUGH THE MOUNT, so the mount decides whether it
# reads this commit's file at all. docker-compose.yml binds the DIRECTORY
# `deploy/` at /etc/caddy for that reason. A single-file bind binds the inode the
# container started with; `git pull` replaces the working-tree file with a new
# inode, so the container kept the pre-pull copy, this reload re-read that copy,
# logged `using config from file` and exited 0, and the script below printed
# DEPLOY OK on the pre-pull routing (M6 review 2, finding V9). A directory bind
# follows the replacement, so the command below needs no `docker compose cp` step
# ahead of it. `scripts/check_ops.sh` check (l) drives that whole sequence
# against the real edge image.
#
# NOTE: The upgrade ACROSS the commit that made this change recreates the caddy
# container by itself, because the volume list of the service changed. The
# reload path below then does not run for that one upgrade.
caddy_recreated=1
if [ -n "$caddy_after" ] && [ "$caddy_before" = "$caddy_after" ]; then
    caddy_recreated=0
    echo "caddy kept its container; reload the Caddyfile of this commit into it"
    if ! docker compose exec -T caddy caddy reload --config /etc/caddy/Caddyfile --adapter caddyfile; then
        echo "deploy: the caddy reload failed; recreate the container instead" >&2
        docker compose up -d --no-deps --force-recreate caddy
        caddy_recreated=1
    fi
fi

echo "wait up to ${START_LIMIT_SECS} s for the new ${SERVICES[*]}"
deadline=$((SECONDS + START_LIMIT_SECS))
baseline=""
clean=0
while true; do
    failed=""
    failed_name=""
    reason=""
    report=""
    for service in "${SERVICES[@]}"; do
        facts="$(service_facts "$service")"
        state="$(printf '%s' "$facts" | cut -d' ' -f1)"
        restarts="$(printf '%s' "$facts" | cut -d' ' -f2)"
        name="$(printf '%s' "$facts" | cut -d' ' -f3)"
        name="${name#/}"
        report="${report}${report:+, }${service} ${state:-unknown}"

        # The restart count of the first poll that sees a container is the
        # baseline. `caddy` keeps its container on a Caddyfile-only change, so
        # its count carries the whole history of that container and is not 0. A
        # count that GROWS while the check runs is the restart loop.
        grown=0
        if [ -n "$facts" ]; then
            case "$baseline" in
                *"|${service}=${restarts}|"*) ;;
                *"|${service}="*) grown=1 ;;
                *) baseline="${baseline}|${service}=${restarts}|" ;;
            esac
        fi

        if [ -n "$failed" ]; then
            continue
        fi
        if [ -z "$facts" ]; then
            failed="$service"
            reason="compose has no container for the service"
        elif [ "$state" != "running" ]; then
            failed="$service"
            failed_name="$name"
            reason="container state ${state:-unknown}"
        elif [ "$grown" = "1" ]; then
            failed="$service"
            failed_name="$name"
            reason="the container restarted while the check ran (a restart loop); restart count ${restarts}"
        else
            wanted="$(start_line "$service")"
            if [ "$service" = "caddy" ] && [ "$caddy_recreated" != "1" ]; then
                # The reload above proved the configuration of a caddy that kept
                # its container, and its start line is older than this upgrade.
                wanted=""
            fi
            if [ -n "$wanted" ]; then
                case "$(docker compose logs "$service" 2>&1 || true)" in
                    *"$wanted"*) ;;
                    *)
                        failed="$service"
                        failed_name="$name"
                        reason="the log holds no \`${wanted}\` line"
                        ;;
                esac
            fi
        fi
    done

    if [ -z "$failed" ]; then
        clean=$((clean + 1))
        if [ "$clean" -ge "$STABLE_POLLS" ]; then
            echo "web, worker, and caddy stayed running for ${STABLE_POLLS} checks, and web logged \`listening on\`"
            break
        fi
    else
        clean=0
    fi

    if [ "$SECONDS" -ge "$deadline" ]; then
        echo "deploy: the new containers did not stay up for ${STABLE_POLLS} checks within ${START_LIMIT_SECS} s" >&2
        echo "deploy: ${report}" >&2
        if [ -n "$failed" ]; then
            echo "deploy: the service that failed is ${failed}, container ${failed_name:-unknown}: ${reason}" >&2
            echo "deploy: the last ${LOG_TAIL_LINES} log lines of ${failed}:" >&2
            docker compose logs --tail "$LOG_TAIL_LINES" "$failed" >&2 || true
        else
            echo "deploy: every container ran at the last check, and the deadline passed first" >&2
        fi
        echo "deploy: the new schema is in place; correct the fault and run this script again" >&2
        exit 1
    fi
    sleep 2
done

echo "DEPLOY OK"

# The check command below names the port that Caddy really publishes.
# CADDY_HTTP_PORT moves that host port, so a fixed `http://localhost` line sends
# the operator to another stack, or to a closed port. `docker compose port`
# prints `<address>:<port>`; the part after the last colon is the port. An empty
# answer names no URL at all: a guessed URL is worse than none (M6 review,
# finding F11).
published="$(docker compose port caddy 80 2>/dev/null || true)"
published="${published%%$'\n'*}"
http_port="${published##*:}"

echo "Do a check:"
echo "  docker compose ps"
echo "  docker compose logs web | grep 'listening on'"
if [ -z "$http_port" ]; then
    echo "  docker compose port caddy 80   # read the published port first"
    echo "  curl -fsS http://127.0.0.1:<port>/api/health"
elif [ "$http_port" = "80" ]; then
    echo "  curl -fsS http://localhost/api/health"
else
    echo "  curl -fsS http://127.0.0.1:${http_port}/api/health"
fi
