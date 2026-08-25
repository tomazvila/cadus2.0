#!/usr/bin/env bash
# Cadus 2.0 ops check (C3, D9). Run it from any directory.
#
# docs/plans/M0.md makes the compose file and the image build the acceptance
# check of U6. Neither runs in `cargo test`, so a renamed binary target or a
# broken compose key first appears on the operator's server. This script runs
# the operator's own commands in the gate instead.
#
# The script does four checks and prints one line per check:
#   (a) compose  -- `docker compose config` resolves docker-compose.yml. The
#                   placeholder values below stand in for `.env`, which the
#                   repository never carries. Every `:?` variable of the compose
#                   file needs a value here.
#   (b) build    -- `docker compose build` builds every service that has a
#                   `build:` section. `docker build .` is not enough: it reads
#                   ./Dockerfile directly and never opens docker-compose.yml, so
#                   a wrong `dockerfile:` key passed the old gate and then broke
#                   the operator's `docker compose up -d --build` (finding #12).
#   (c) binaries -- the three binaries of the Dockerfile exist in every image the
#                   compose file builds. The image promise is one image and three
#                   commands.
#   (d) commands -- every `command:` binary of the compose file exists in the
#                   image that runs it. `docker compose config` treats a
#                   `command:` as opaque strings, so a renamed binary target
#                   passed the old gate and gave the operator an
#                   `exec: "cadus-webb": executable file not found in $PATH`
#                   crash loop (finding #12).
#
# Compose names a built image `<project>-<service>` when the service declares no
# `image:` key. The script reads the project name and the service names from
# `docker compose config --format json`, so it needs no hard-coded image name.
#
# Input: docker and python3 on PATH. The script reads no `.env` file and writes
# no state outside the local docker image store.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in docker python3; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "FAIL: input   -- $tool is not on PATH" >&2
        exit 2
    fi
done

# The placeholder values stand in for `.env`. Compose fails on an unset `:?`
# variable, and the repository carries no `.env`.
export SITE_ADDRESS=:80
export POSTGRES_PASSWORD=x
export CADUS_APP_PASSWORD=x
export CADUS_ADMIN_PASSWORD=x

# The three binaries that the Dockerfile installs.
BINARIES=(cadus-web cadus-worker cadus-migrate)

rc=0

# ---------------------------------------------------------------------------
# (a) the compose file resolves
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# (b) every service with a `build:` section builds through compose
# ---------------------------------------------------------------------------
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

# ---------------------------------------------------------------------------
# Read the plan out of the resolved compose file.
#
# `built_images` holds one image name per service with a `build:` section.
# `service_commands` holds one `<image> <binary> <service>` line per service
# with a `command:`.
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
    if what == "images":
        if service.get("build"):
            print(image)
        continue
    command = service.get("command")
    if not command:
        continue
    if isinstance(command, str):
        command = shlex.split(command)
    print(image, command[0], name)
' "$1"
}

built_images="$(read_plan images | sort -u)"
service_commands="$(read_plan commands)"

if [ -z "$built_images" ]; then
    echo "FAIL: binaries -- docker-compose.yml declares no service with a build section"
    exit 1
fi

# ---------------------------------------------------------------------------
# (c) the three binaries exist in every image the compose file builds
# ---------------------------------------------------------------------------
binaries_ok=1
while read -r image; do
    [ -n "$image" ] || continue
    if ! docker run --rm --entrypoint sh "$image" \
        -c 'command -v cadus-web && command -v cadus-worker && command -v cadus-migrate' \
        >/dev/null 2>&1; then
        echo "FAIL: binaries -- image $image is missing one of: ${BINARIES[*]}"
        binaries_ok=0
        rc=1
    fi
done <<EOF
$built_images
EOF

if [ "$binaries_ok" -eq 1 ]; then
    echo "PASS: binaries -- ${BINARIES[*]} exist in every image the compose file builds"
fi

# ---------------------------------------------------------------------------
# (d) every `command:` binary exists in the image that runs it
# ---------------------------------------------------------------------------
commands_ok=1
command_count=0
while read -r image binary service; do
    [ -n "$image" ] || continue
    command_count=$((command_count + 1))
    if ! docker run --rm --entrypoint sh "$image" -c 'command -v "$1"' sh "$binary" \
        >/dev/null 2>&1; then
        echo "FAIL: commands -- service $service runs \`$binary\`, which image $image does not carry"
        commands_ok=0
        rc=1
    fi
done <<EOF
$service_commands
EOF

if [ "$commands_ok" -eq 1 ]; then
    echo "PASS: commands -- every command: binary exists in its image ($command_count checked)"
fi

exit "$rc"
