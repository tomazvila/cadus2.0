#!/usr/bin/env bash
# Part of scripts/check_ops.sh: checks (a) compose, (b) build, and (targets).
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

# ---------------------------------------------------------------------------
# (a) the compose file resolves
# ---------------------------------------------------------------------------
check_compose() {
config_json=""
config_log=""
if config_json="$(docker compose config --format json 2>/dev/null)"; then
    echo "PASS: compose -- docker compose config resolves docker-compose.yml"
else
    config_log="$(docker compose config 2>&1 >/dev/null || true)"
    echo "FAIL: compose -- docker compose config failed"
    printf '%s\n' "$config_log"
    exit 1
fi
}

# ---------------------------------------------------------------------------
# (b) every service with a `build:` section builds through compose
# ---------------------------------------------------------------------------
check_build() {
build_log=""
if build_log="$(docker compose build 2>&1)"; then
    echo "PASS: build   -- docker compose build built every service"
else
    echo "FAIL: build   -- docker compose build failed"
    printf '%s\n' "$build_log"
    # Every check below runs a container from an image that this step makes, so
    # a failed build makes them meaningless. Stop here.
    exit 1
fi
}

# ---------------------------------------------------------------------------
# Read the plan out of the resolved compose file.
#
# The repository builds TWO images out of one Dockerfile, and the `target:` key
# of a service names which (M6 S14):
#
#   runtime -- the app image: cadus-web, cadus-worker, cadus-migrate, and the
#              curriculum tree.
#   spa     -- the edge image: Caddy plus the built SPA bundle in /srv.
#
# The two carry different things, so every check below reads the target and never
# assumes one image. A service with a `build:` section and any OTHER target is a
# FAILURE and not a skip: a new image that no check reads is an image nothing
# proves.
#
# `read_plan images` prints `<image> <target>`, one line per service with a
# `build:` section.
#
# `read_plan commands` prints one line per such service, and one for EVERY such
# service: a service that builds the app image but declares no `command:` gets a
# line too. The old extractor skipped it, so a deleted `command:` reported PASS
# (finding #9). The line is `<image> <service> <target> <status> [token ...]`,
# where <status> is:
#   ok    -- the tokens after it are the whole `command:`
#   none  -- the service declares no `command:`
#   space -- a `command:` token is empty or holds a space, so the line below
#            cannot carry it in a word-split field
# ---------------------------------------------------------------------------
read_plan() {
    printf '%s' "$config_json" | python3 -c '
import json
import shlex
import sys

what = sys.argv[1]
doc = json.load(sys.stdin)
project = doc.get("name", "")

for name, service in sorted(doc.get("services", {}).items()):
    image = service.get("image") or "{}-{}".format(project, name)
    build = service.get("build")
    if not build:
        # A service with no build section runs a third-party image (db) and
        # keeps its own entrypoint.
        continue
    # An absent target builds the LAST stage of the Dockerfile, which the
    # checks cannot name. Report the literal `-` and let the caller fail. It is
    # a placeholder and never the empty string: the caller word-splits this
    # line, an empty field collapses, and every field after it would shift by
    # one and be read as another thing entirely.
    target = build.get("target") or "-"
    if what == "images":
        print(image, target)
        continue
    command = service.get("command")
    if isinstance(command, str):
        command = shlex.split(command)
    if not command:
        print(image, name, target, "none")
        continue
    if any(len(str(token).split()) != 1 for token in command):
        print(image, name, target, "space")
        continue
    print(image, name, target, "ok", *command)
' "$1"
}

# The two build targets the Dockerfile carries and this script knows.
APP_TARGET=runtime
SPA_TARGET=spa

check_targets() {
build_plan="$(read_plan images | sort -u)"
service_commands="$(read_plan commands)"

if [ -z "$build_plan" ]; then
    echo "FAIL: binaries -- docker-compose.yml declares no service with a build section"
    exit 1
fi

app_images="$(printf '%s\n' "$build_plan" | awk -v t="$APP_TARGET" '$2 == t { print $1 }' | sort -u)"
spa_images="$(printf '%s\n' "$build_plan" | awk -v t="$SPA_TARGET" '$2 == t { print $1 }' | sort -u)"
odd_targets="$(printf '%s\n' "$build_plan" |
    awk -v a="$APP_TARGET" -v s="$SPA_TARGET" '$2 != a && $2 != s { print $1 " target=" ($2 == "-" ? "(none)" : $2) }')"

if [ -n "$odd_targets" ]; then
    printf 'FAIL: targets  -- a built service names no known target (%s, %s): %s\n' \
        "$APP_TARGET" "$SPA_TARGET" "$(printf '%s' "$odd_targets" | tr '\n' ' ')"
    rc=1
elif [ -z "$app_images" ]; then
    echo "FAIL: targets  -- no service builds the app image (target: $APP_TARGET)"
    rc=1
elif [ -z "$spa_images" ]; then
    echo "FAIL: targets  -- no service builds the SPA edge image (target: $SPA_TARGET)"
    rc=1
else
    echo "PASS: targets  -- every built service names $APP_TARGET or $SPA_TARGET, and both are built"
fi
}

