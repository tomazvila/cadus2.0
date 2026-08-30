#!/usr/bin/env bash
# Cadus 2.0 ops check (C3, D9). Run it from any directory.
#
# docs/plans/M0.md makes the compose file and the image build the acceptance
# check of U6. Neither runs in `cargo test`, so a renamed binary target or a
# broken compose key first appears on the operator's server. This script runs
# the operator's own commands in the gate instead.
#
# The repository builds TWO images out of one Dockerfile (M6 S14): the APP image
# (`target: runtime`, the three binaries and the curriculum) and the EDGE image
# (`target: spa`, Caddy and the built SPA bundle). Every image check below reads
# the `target:` key of the service and holds the rules of that image alone.
#
# The script does fourteen checks and prints one line per check:
#   (a) compose  -- `docker compose config` resolves docker-compose.yml. The
#                   placeholder values below stand in for `.env`, which the
#                   repository never carries. Every `:?` variable of the compose
#                   file needs a value here.
#   (b) build    -- `docker compose build` builds every service that has a
#                   `build:` section. `docker build .` is not enough: it reads
#                   ./Dockerfile directly and never opens docker-compose.yml, so
#                   a wrong `dockerfile:` key passed the old gate and then broke
#                   the operator's `docker compose up -d --build` (finding #12).
#   (targets) -- every service with a `build:` section names `runtime` or `spa`,
#                   and both images are built. An absent `target:` builds the
#                   LAST stage of the Dockerfile, so a reordering of the stages
#                   would hand `web` the Caddy image without one edit to this
#                   file.
#   (c) binaries -- the three binaries of the Dockerfile exist in every APP image
#                   the compose file builds. The app-image promise is one image
#                   and three commands.
#   (c2) curriculum -- /app/curriculum is a directory in every APP image the
#                   compose file builds, and it holds courses.yaml. cadus-worker
#                   reads the tree there for the A6 exemplar fallback and exits 2
#                   when the tree does not load, so an image without it gives a
#                   worker that restarts forever (findings #5 and #6).
#   (c3) spa     -- every EDGE image carries the built bundle under /srv --
#                   index.html, the hashed assets/, and the vendored KaTeX that
#                   `public/` ships verbatim -- and carries caddy to serve it.
#   (c4) nonode  -- NO image the compose file builds carries node, npm, or npx.
#                   node builds the bundle in a stage that ships nothing. This is
#                   the M6 S14 acceptance check.
#   (d) commands -- the whole `command:` of every service is correct for the
#                   image it builds. `docker compose config` treats a `command:`
#                   as opaque strings, so the compose file itself proves nothing.
#                   An edge service must declare NO command:, because a command
#                   replaces the caddy entrypoint. For an app service the check
#                   has three parts:
#                     1. The service HAS a `command:`. Without one the container
#                        runs the Dockerfile `CMD` (`cadus-web`), so a deleted
#                        `command:` on `worker` starts a second web server on the
#                        cadus_admin DSN, the C3 boot guard rejects the
#                        BYPASSRLS role, and the container crash-loops
#                        (finding #9).
#                     2. The first token names one of the three binaries and
#                        exists in the image. A renamed binary target gave the
#                        operator an
#                        `exec: "cadus-webb": executable file not found in $PATH`
#                        crash loop (finding #12).
#                     3. Every further token is in the allowlist of that binary
#                        (ALLOWED_ARGUMENTS below). `cadus-migrate --admin-loginn`
#                        prints its usage and exits 2, and `web` and `worker`
#                        then never start, because both wait for
#                        `service_completed_successfully` (finding #9).
#   (e) deploy   -- scripts/deploy.sh exists, is executable, and parses. It is
#                   THE upgrade procedure (finding #16), so a broken file must
#                   fail the gate and not the operator's upgrade.
#   (f) invariants -- four compose facts that a review round paid for:
#                   the `db` healthcheck probes TCP (`-h`), because the initdb
#                   temp server answers the unix socket while port 5432 still
#                   refuses (finding #15); `migrate` gets neither
#                   DB_STATEMENT_TIMEOUT_MS nor DB_CLIENT_TIMEOUT_MS, because a
#                   migration runs without a query bound (finding #1); and
#                   `worker` gets the seven A4 model variables, because a worker
#                   that never sees OPENAI_API_KEY reads an empty key, builds no
#                   diagnosis job, and leaves the queue standing for ever
#                   (M5 review finding F8).
#   (g) shell    -- `shellcheck -S warning scripts/*.sh`. The scripts here are
#                   ops code: deploy.sh is THE upgrade procedure, and an unquoted
#                   expansion or a lost exit code in it lands on the operator's
#                   server. The gate reads the shell like the Rust.
#   (h) ci       -- every published port of .github/workflows/ci.yml binds
#                   127.0.0.1. The gate database in CI runs with trust auth, so
#                   a `5432:5432` line puts a superuser port on every interface
#                   of the runner for the length of the job.
#   (i) bench    -- the budget benchmarks run AFTER `cargo test --workspace` and
#                   in the one CI job. A contended benchmark measures the
#                   scheduler, not the code (spec section 10.5).
#   (j) caddy    -- `deploy/Caddyfile` proxies /api/* to web:8080 and serves
#                   everything else out of the Dockerfile's own bundle root with
#                   an index.html fallback, so the SPA and the API share ONE
#                   origin; and its five security headers equal SECURITY_HEADERS
#                   of `crates/web/src/security.rs`, character for character. The
#                   Caddyfile is a bind mount, so no image check reads it.
#
# Compose names a built image `<project>-<service>` when the service declares no
# `image:` key. The script reads the project name and the service names from
# `docker compose config --format json`, so it needs no hard-coded image name.
#
# Input: docker, python3, and shellcheck on PATH. The script reads no `.env`
# file and writes no state outside the local docker image store.
set -euo pipefail

# Put the project toolchain first, if it is installed on this machine.
for dir in "$HOME/.local/share/cadus2-tooling/gcc/bin" \
    "$HOME/.local/share/cadus2-tooling/shellcheck-bin/bin" \
    "$HOME/.cargo/bin"; do
    if [ -d "$dir" ]; then
        PATH="$dir:$PATH"
    fi
done
export PATH

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in docker python3 shellcheck; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "FAIL: input   -- $tool is not on PATH" >&2
        exit 2
    fi
done

# The placeholder values stand in for `.env`. Compose fails on an unset `:?`
# variable, and the repository carries no `.env`.
export SITE_ADDRESS=:80
export POSTGRES_PASSWORD=x
# Placeholders that obey the cadus-migrate password rule (16..=128 of [A-Za-z0-9_-]).
export CADUS_APP_PASSWORD=0123456789abcdef0123456789abcdef0123456789abcdef
export CADUS_ADMIN_PASSWORD=fedcba9876543210fedcba9876543210fedcba9876543210

# The three binaries that the Dockerfile installs.
BINARIES=(cadus-web cadus-worker cadus-migrate)

# Check (d) part 3: the arguments that each binary accepts in a `command:`.
# Keep this list literal. A binary with an empty value takes no argument at all.
# `cadus-migrate` reads one flag (`--admin-login`); `cadus-web` and
# `cadus-worker` read their whole configuration from the environment.
declare -A ALLOWED_ARGUMENTS=(
    [cadus-migrate]="--admin-login"
    [cadus-web]=""
    [cadus-worker]=""
)

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

# ---------------------------------------------------------------------------
# (c) the three binaries exist in every APP image the compose file builds
#
# The edge image is Caddy and a bundle; it carries none of the three and must
# not, so the loop reads `app_images` and never `build_plan`.
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
$app_images
EOF

if [ "$binaries_ok" -eq 1 ]; then
    echo "PASS: binaries -- ${BINARIES[*]} exist in every $APP_TARGET image the compose file builds"
fi

# ---------------------------------------------------------------------------
# (c2) the curriculum tree is in every APP image the compose file builds
#
# The tree is the input of the D-O4 pool refill. cadus-worker exits 2 when it
# does not load, so a missing COPY line turns every deployment into a worker
# restart loop. The check reads the path the Dockerfile sets as the default.
# ---------------------------------------------------------------------------
CURRICULUM_PATH=/app/curriculum

curriculum_ok=1
while read -r image; do
    [ -n "$image" ] || continue
    if ! docker run --rm --entrypoint sh "$image" \
        -c '[ -d "$1" ] && [ -f "$1/courses.yaml" ]' sh "$CURRICULUM_PATH" \
        >/dev/null 2>&1; then
        echo "FAIL: curriculum -- image $image carries no $CURRICULUM_PATH/courses.yaml"
        curriculum_ok=0
        rc=1
    fi
done <<EOF
$app_images
EOF

if [ "$curriculum_ok" -eq 1 ]; then
    echo "PASS: curriculum -- $CURRICULUM_PATH/courses.yaml exists in every $APP_TARGET image the compose file builds"
fi

# ---------------------------------------------------------------------------
# (c3) the SPA edge image carries the built bundle, and Caddy to serve it
#
# The `spa` stage is `caddy:2` plus `web/dist`. Three files stand for the whole
# bundle, and each one has its own failure:
#
#   /srv/index.html                     -- the document. Without it Caddy answers
#                                          404 for `/` and the site is a blank
#                                          error page.
#   /srv/assets                         -- the hashed entry chunk and stylesheet
#                                          that `vite build` emits.
#   /srv/vendor/katex/katex.min.js      -- the vendored tree that `public/` ships
#                                          verbatim. `vite.config.ts` marks
#                                          `/vendor/**` external, so Rollup never
#                                          bundles it: a `publicDir` that stops
#                                          being copied gives raw LaTeX on every
#                                          problem and no build error at all
#                                          (1.0 click-through failure 2).
#
# `caddy` itself must be on PATH, because the image keeps the base entrypoint and
# the compose service declares no command.
# ---------------------------------------------------------------------------
SPA_ROOT=/srv

spa_ok=1
while read -r image; do
    [ -n "$image" ] || continue
    if ! docker run --rm --entrypoint sh "$image" -c '
        [ -f "$1/index.html" ] &&
        [ -d "$1/assets" ] &&
        [ -f "$1/vendor/katex/katex.min.js" ] &&
        command -v caddy
    ' sh "$SPA_ROOT" >/dev/null 2>&1; then
        echo "FAIL: spa      -- image $image carries no complete bundle under $SPA_ROOT, or no caddy"
        spa_ok=0
        rc=1
    fi
done <<EOF
$spa_images
EOF

if [ "$spa_ok" -eq 1 ]; then
    echo "PASS: spa      -- $SPA_ROOT/index.html, $SPA_ROOT/assets and the vendored KaTeX exist in every $SPA_TARGET image, and caddy runs them"
fi

# ---------------------------------------------------------------------------
# (c4) NO image the compose file builds carries node
#
# This is the S14 acceptance check. node builds the bundle and node is not part
# of serving it: `web/node_modules` is hundreds of megabytes of third-party code,
# and every line of it in a runtime image is attack surface that answers no
# request. The node lives in the `spa-builder` stage, which ships nothing.
#
# The loop reads BOTH image classes. A `COPY --from=spa-builder /usr/local/bin`
# into either one is the mistake this check exists for, and it would pass every
# other check in this file.
# ---------------------------------------------------------------------------
NODE_TOOLS=(node npm npx)

nonode_ok=1
while read -r line; do
    [ -n "$line" ] || continue
    image="${line%% *}"
    found="$(docker run --rm --entrypoint sh "$image" \
        -c 'for tool in node npm npx; do command -v "$tool" || true; done' 2>/dev/null || true)"
    if [ -n "$found" ]; then
        printf 'FAIL: nonode   -- image %s carries node: %s\n' "$image" "$(printf '%s' "$found" | tr '\n' ' ')"
        nonode_ok=0
        rc=1
    fi
done <<EOF
$build_plan
EOF

if [ "$nonode_ok" -eq 1 ]; then
    echo "PASS: nonode   -- no image the compose file builds carries ${NODE_TOOLS[*]}"
fi

# ---------------------------------------------------------------------------
# (d) every service that builds the app image runs a known binary with allowed
#     arguments, and the edge service runs the image's own entrypoint
#
# The two build targets take OPPOSITE rules, and the loop reads the target of
# each line to pick one:
#
#   runtime -- the service MUST declare a command:. Without one it runs the
#              Dockerfile CMD (`cadus-web`), so a deleted command: on `worker`
#              starts a second web server on the cadus_admin DSN (finding #9).
#   spa     -- the service must declare NO command:. The `caddy:2` entrypoint is
#              `caddy run --config /etc/caddy/Caddyfile`, and a command: here
#              REPLACES it, so the container starts a process that reads no
#              Caddyfile and serves nothing.
# ---------------------------------------------------------------------------
commands_ok=1
command_count=0
while IFS= read -r line; do
    [ -n "$line" ] || continue
    read -r -a fields <<<"$line"
    image="${fields[0]}"
    service="${fields[1]}"
    target="${fields[2]}"
    status="${fields[3]}"
    command_count=$((command_count + 1))

    if [ "$target" != "$APP_TARGET" ] && [ "$target" != "$SPA_TARGET" ]; then
        # The targets check above already failed this line. Say so and read no
        # further field: the rules below belong to one image or the other, and
        # this service builds neither.
        echo "FAIL: commands -- service $service builds no known target, so no command rule applies to it"
        commands_ok=0
        rc=1
        continue
    fi

    if [ "$target" = "$SPA_TARGET" ]; then
        if [ "$status" != "none" ]; then
            echo "FAIL: commands -- service $service builds the $SPA_TARGET image and declares a command:, which replaces the caddy entrypoint and reads no Caddyfile"
            commands_ok=0
            rc=1
        fi
        continue
    fi

    if [ "$status" = "none" ]; then
        echo "FAIL: commands -- service $service builds the app image and declares no command:, so the container runs the Dockerfile CMD"
        commands_ok=0
        rc=1
        continue
    fi

    if [ "$status" = "space" ]; then
        echo "FAIL: commands -- service $service has an empty command: token, or one with a space in it"
        commands_ok=0
        rc=1
        continue
    fi

    binary="${fields[4]}"
    if [ -z "${ALLOWED_ARGUMENTS[$binary]+set}" ]; then
        echo "FAIL: commands -- service $service runs \`$binary\`, which is not one of: ${BINARIES[*]}"
        commands_ok=0
        rc=1
        continue
    fi

    if ! docker run --rm --entrypoint sh "$image" -c 'command -v "$1"' sh "$binary" \
        >/dev/null 2>&1; then
        echo "FAIL: commands -- service $service runs \`$binary\`, which image $image does not carry"
        commands_ok=0
        rc=1
        continue
    fi

    allowed=" ${ALLOWED_ARGUMENTS[$binary]} "
    for argument in "${fields[@]:5}"; do
        if [[ "$allowed" != *" $argument "* ]]; then
            echo "FAIL: commands -- service $service gives \`$binary\` the argument \`$argument\`; $binary takes: ${ALLOWED_ARGUMENTS[$binary]:-no argument}"
            commands_ok=0
            rc=1
        fi
    done
done <<EOF
$service_commands
EOF

if [ "$commands_ok" -eq 1 ]; then
    echo "PASS: commands -- every app service runs a known binary with allowed arguments, and every edge service keeps the caddy entrypoint ($command_count checked)"
fi

# ---------------------------------------------------------------------------
# (e) the upgrade script is present, executable, and parses
# ---------------------------------------------------------------------------
deploy_ok=1
if [ ! -f scripts/deploy.sh ]; then
    echo "FAIL: deploy   -- scripts/deploy.sh is missing"
    deploy_ok=0
    rc=1
elif [ ! -x scripts/deploy.sh ]; then
    echo "FAIL: deploy   -- scripts/deploy.sh is not executable"
    deploy_ok=0
    rc=1
elif ! bash -n scripts/deploy.sh; then
    echo "FAIL: deploy   -- scripts/deploy.sh does not parse"
    deploy_ok=0
    rc=1
fi

if [ "$deploy_ok" -eq 1 ]; then
    echo "PASS: deploy   -- scripts/deploy.sh is present, executable, and parses"
fi

# ---------------------------------------------------------------------------
# (f) the compose invariants of the review rounds
#
# The seven A4 model variables reach the `worker` service, because model calls
# run in cadus-worker and never on a request path (R4, L6). Compose keeps a key
# whose `${VAR:-}` value is unset, and gives it an empty string, so the check
# reads the KEY and never the value: an empty OPENAI_API_KEY is the documented
# "call no model" deployment (docs/SELF_HOST.md).
# ---------------------------------------------------------------------------
invariant_log=""
if invariant_log="$(printf '%s' "$config_json" | python3 -c '
import json
import sys

doc = json.load(sys.stdin)
services = doc.get("services", {})
problems = []

db = services.get("db", {})
test = db.get("healthcheck", {}).get("test", [])
if isinstance(test, str):
    test = [test]
probe = " ".join(str(part) for part in test)
if "pg_isready" not in probe:
    problems.append("the db healthcheck does not run pg_isready: " + probe)
elif " -h " not in probe:
    problems.append("the db healthcheck does not probe TCP (no -h): " + probe)

migrate_env = services.get("migrate", {}).get("environment", {}) or {}
for key in ("DB_STATEMENT_TIMEOUT_MS", "DB_CLIENT_TIMEOUT_MS"):
    if key in migrate_env:
        problems.append("migrate carries " + key + "; a migration runs unbounded")

# The seven A4 variables of docs/SELF_HOST.md, section "The seven variables".
model_keys = (
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "OPENAI_MODEL",
    "OPENROUTER_PROVIDER_ORDER",
    "DIAGNOSIS_OUTPUT_TOKENS",
    "DIAGNOSIS_REASONING_MAX_TOKENS",
    "DIAGNOSIS_CALLS_PER_SESSION",
)
worker_env = services.get("worker", {}).get("environment", {}) or {}
missing = [key for key in model_keys if key not in worker_env]
if missing:
    problems.append(
        "the worker service does not carry the A4 model variable(s) "
        + ", ".join(missing)
        + "; the diagnosis job then reads an empty OPENAI_API_KEY and calls no model"
    )

for line in problems:
    print(line)
')"; then
    if [ -n "$invariant_log" ]; then
        printf 'FAIL: invariants -- %s\n' "$invariant_log"
        rc=1
    else
        echo "PASS: invariants -- db probes TCP, migrate carries no query bound, and worker carries the seven A4 model variables"
    fi
else
    echo "FAIL: invariants -- the compose invariant check did not run"
    rc=1
fi

# ---------------------------------------------------------------------------
# (g) the shell scripts pass shellcheck
#
# `-S warning` is the gate level: it reports error and warning and holds back
# style and info. Fix a warning; do not silence it. A `# shellcheck disable=`
# line needs a comment above it that says why the rule does not apply here.
# ---------------------------------------------------------------------------
shell_log=""
if shell_log="$(shellcheck -S warning scripts/*.sh 2>&1)"; then
    echo "PASS: shell    -- shellcheck -S warning reports nothing on scripts/*.sh"
else
    echo "FAIL: shell    -- shellcheck -S warning reports a finding on scripts/*.sh"
    printf '%s\n' "$shell_log"
    rc=1
fi

# ---------------------------------------------------------------------------
# (h) the CI workflow publishes every service port on the loopback interface
#
# The Postgres service of the gate job runs with POSTGRES_HOST_AUTH_METHOD=trust
# (`.github/workflows/ci.yml` says why). A `ports:` entry of `5432:5432` binds
# every interface of the runner, so any process that reaches the runner over the
# network connects to that database as the superuser. `127.0.0.1:5432:5432`
# keeps the port on the runner itself, and the gate steps run there.
#
# The reader below is a line scan, not a YAML parser: the runner image and this
# box carry no PyYAML.
# ---------------------------------------------------------------------------
workflow="ci.yml"
ci_log=""
if [ ! -f ".github/workflows/$workflow" ]; then
    echo "FAIL: ci       -- .github/workflows/$workflow is missing"
    rc=1
elif ci_log="$(python3 -c '
import io
import sys

path = sys.argv[1]
problems = []
in_ports = False
ports_indent = 0
entries = 0

for number, raw in enumerate(io.open(path, encoding="utf-8"), start=1):
    line = raw.rstrip("\n")
    stripped = line.strip()
    if not stripped or stripped.startswith("#"):
        continue
    indent = len(line) - len(line.lstrip())
    if stripped == "ports:":
        in_ports = True
        ports_indent = indent
        continue
    if not in_ports:
        continue
    if not stripped.startswith("-") or indent <= ports_indent:
        in_ports = False
        continue
    value = stripped[1:].strip().strip("\"'"'"'")
    entries += 1
    if not value.startswith("127.0.0.1:"):
        problems.append("line %d publishes %s on every interface" % (number, value))

if entries == 0:
    problems.append("the workflow publishes no port; the reader found no ports: entry")

for problem in problems:
    print(problem)
' ".github/workflows/$workflow" 2>&1)"; then
    if [ -n "$ci_log" ]; then
        printf 'FAIL: ci       -- %s\n' "$ci_log"
        rc=1
    else
        echo "PASS: ci       -- every published port of $workflow binds 127.0.0.1"
    fi
else
    echo "FAIL: ci       -- the workflow port check did not run"
    printf '%s\n' "$ci_log"
    rc=1
fi

# ---------------------------------------------------------------------------
# (i) the budget benchmarks run AFTER the test suite and never beside it
#
# Spec section 10.5 and docs/reference/l1-budget.md section 7: parallel suites
# contend on one box and on a two-core runner, and a contended benchmark
# measures the scheduler and not the code. The rule has two halves, and this
# check reads both:
#
#   1. scripts/gate.sh runs `cargo test --workspace` BEFORE scripts/bench.sh.
#      The gate is one shell script, so the order of the two lines is the order
#      of the two steps.
#   2. .github/workflows/ci.yml declares ONE job. A second job runs beside the
#      gate on its own runner, and a benchmark job among them measures a shared
#      machine. One `runs-on:` line is one job.
#
# M5 U12 added the check. Before it, nothing failed a workflow edit that moved
# the benchmarks into a job of their own.
# ---------------------------------------------------------------------------
bench_rc=0
if [ ! -f scripts/gate.sh ]; then
    echo "FAIL: bench    -- scripts/gate.sh is missing"
    bench_rc=1
else
    # The COMMAND lines, not the `echo` lines that announce them: a moved
    # command under an unmoved banner must fail this check.
    test_line="$(grep -n '^cargo test --workspace$' scripts/gate.sh | head -1 | cut -d: -f1)"
    bench_line="$(grep -n '^ *\(bash \)\?scripts/bench.sh$' scripts/gate.sh | tail -1 | cut -d: -f1)"
    if [ -z "$test_line" ]; then
        echo "FAIL: bench    -- scripts/gate.sh runs no 'cargo test --workspace'"
        bench_rc=1
    elif [ -z "$bench_line" ]; then
        echo "FAIL: bench    -- scripts/gate.sh runs no scripts/bench.sh"
        bench_rc=1
    elif [ "$test_line" -ge "$bench_line" ]; then
        echo "FAIL: bench    -- scripts/gate.sh runs the benchmarks at line $bench_line, at or before the test suite at line $test_line"
        bench_rc=1
    fi
fi

if [ ! -f ".github/workflows/ci.yml" ]; then
    echo "FAIL: bench    -- .github/workflows/ci.yml is missing"
    bench_rc=1
else
    runners="$(grep -c 'runs-on:' .github/workflows/ci.yml || true)"
    if [ "$runners" != "1" ]; then
        echo "FAIL: bench    -- .github/workflows/ci.yml declares $runners jobs; the benchmarks run in the one gate job and never beside it"
        bench_rc=1
    fi
    if ! grep -q 'scripts/gate.sh' .github/workflows/ci.yml; then
        echo "FAIL: bench    -- .github/workflows/ci.yml runs no scripts/gate.sh"
        bench_rc=1
    fi
fi

if [ "$bench_rc" -eq 0 ]; then
    echo "PASS: bench    -- the budget benchmarks run after cargo test, in the one CI job"
else
    rc=1
fi

# ---------------------------------------------------------------------------
# (j) the edge serves the SPA and the API on ONE origin, with the service's own
#     security headers
#
# `crates/web/src/origin.rs` answers `403 cross_origin_rejected` to a
# cookie-carrying write whose `Origin` is not this deployment's own. So the
# bundle and /api MUST come off one origin, and `deploy/Caddyfile` is the only
# file that says so. Nothing else in the gate reads it: it is a bind mount, so a
# broken routing block passes every image check above and fails first on the
# operator's server.
#
# The check reads three files and holds four facts:
#
#   1. /api/* reaches the web service. The `@api` handle proxies to `web:8080`.
#   2. Every other path is the SPA: the fallback `handle` has a `root`, a
#      `file_server`, and the `try_files {path} /index.html` line that makes
#      `/ops` and `/review` reload instead of answer 404.
#   3. That `root` is the path the Dockerfile copies the bundle to. Move the COPY
#      destination alone and Caddy serves an empty directory.
#   4. The five headers of the fallback `handle` equal SECURITY_HEADERS of
#      `crates/web/src/security.rs`, name and value, character for character. The
#      service stamps them on its OWN answers only. The document is a file Caddy
#      serves, and a Content-Security-Policy that never reaches the document
#      protects nothing.
# ---------------------------------------------------------------------------
caddy_log=""
if caddy_log="$(python3 - <<'PYCADDY'
import io
import re

CADDYFILE = "deploy/Caddyfile"
SECURITY = "crates/web/src/security.rs"
DOCKERFILE = "Dockerfile"

problems = []


def records(text):
    """Every non-blank, non-comment line, with the brace depth it opens at."""
    out = []
    depth = 0
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        out.append((depth, line))
        depth += line.count("{") - line.count("}")
    return out


def body_of(rows, start):
    """The rows inside the block that rows[start] opens."""
    floor = rows[start][0]
    out = []
    for depth, line in rows[start + 1:]:
        if depth <= floor:
            break
        out.append((depth, line))
    return out


def header_pairs(rows):
    """The `header { Name value }` block of a handle body, as (name, value)."""
    for index, (_, line) in enumerate(rows):
        if line != "header {":
            continue
        pairs = []
        for _, entry in body_of(rows, index):
            if entry == "}":
                continue
            name, _, value = entry.partition(" ")
            pairs.append((name, value.strip().strip('"')))
        return pairs
    return None


def rust_string(literal):
    """A Rust string literal, with its backslash line continuations folded away."""
    body = literal.strip()
    if not (body.startswith('"') and body.endswith('"')):
        return None
    return re.sub(r"\\\n\s*", "", body[1:-1])


caddy = io.open(CADDYFILE, encoding="utf-8").read()
rows = records(caddy)

# --- 1 and 2: the two handles ----------------------------------------------
api_body = []
fallback_body = []
for index, (depth, line) in enumerate(rows):
    if depth != 1 or not line.startswith("handle"):
        continue
    matcher = line[len("handle"):].strip().rstrip("{").strip()
    if matcher == "@api":
        api_body = body_of(rows, index)
    elif matcher == "":
        fallback_body = body_of(rows, index)

api_matcher = [line for _, line in rows if line.startswith("@api ")]
if not api_matcher:
    problems.append(CADDYFILE + " declares no @api matcher")
elif api_matcher[0] != "@api path /api/*":
    problems.append(
        CADDYFILE + " matches the API as " + repr(api_matcher[0])
        + "; the service prefix is /api/ (crates/web/src/origin.rs API_PREFIX)"
    )

if not api_body:
    problems.append(CADDYFILE + " has no `handle @api` block, so /api reaches no service")
elif not any(
    line in ("reverse_proxy web:8080 {", "reverse_proxy web:8080") for _, line in api_body
):
    problems.append(CADDYFILE + " does not proxy the @api handle to web:8080")

if not fallback_body:
    problems.append(CADDYFILE + " has no matcher-less `handle` block, so no path serves the SPA")

roots = [line for _, line in fallback_body if line.startswith("root ")]
if not roots:
    problems.append(CADDYFILE + " sets no `root` in the SPA handle, so Caddy serves no bundle")
if not any(line == "file_server" for _, line in fallback_body):
    problems.append(CADDYFILE + " runs no `file_server` in the SPA handle")
if not any(line == "try_files {path} /index.html" for _, line in fallback_body):
    problems.append(
        CADDYFILE + " has no `try_files {path} /index.html` in the SPA handle; /ops and"
        " /review then answer 404 on a reload"
    )

# --- 3: the root is the Dockerfile's copy destination -----------------------
dockerfile = io.open(DOCKERFILE, encoding="utf-8").read()
copies = re.findall(r"^COPY --from=spa-builder \S+ (\S+)\s*$", dockerfile, re.M)
if not copies:
    problems.append(DOCKERFILE + " has no `COPY --from=spa-builder ... <root>` line")
elif roots:
    served = roots[0].split()[-1]
    if served != copies[0]:
        problems.append(
            CADDYFILE + " serves " + served + " and " + DOCKERFILE
            + " copies the bundle to " + copies[0]
        )

# --- 4: the five headers match the service's own ----------------------------
rust = io.open(SECURITY, encoding="utf-8").read()
csp_match = re.search(r"pub const CONTENT_SECURITY_POLICY: &str =(.*?);\n", rust, re.S)
array = re.search(
    r"pub const SECURITY_HEADERS: \[\(&str, &str\); (\d+)\] = \[(.*?)\n\];", rust, re.S
)
csp = rust_string(csp_match.group(1)) if csp_match else None

if csp is None or array is None:
    problems.append(
        "could not read CONTENT_SECURITY_POLICY and SECURITY_HEADERS from " + SECURITY
    )
else:
    wanted = []
    for name, value in re.findall(
        r'\(\s*"([^"]+)"\s*,\s*(CONTENT_SECURITY_POLICY|"[^"]*")\s*\)', array.group(2)
    ):
        wanted.append(
            (name.lower(), csp if value == "CONTENT_SECURITY_POLICY" else value[1:-1])
        )
    if len(wanted) != int(array.group(1)):
        problems.append(
            SECURITY + " declares " + array.group(1) + " headers and the reader found "
            + str(len(wanted))
        )
    served_pairs = header_pairs(fallback_body)
    if served_pairs is None:
        problems.append(
            CADDYFILE + " stamps no `header` block in the SPA handle, so the document ships"
            " with no Content-Security-Policy"
        )
    else:
        served_map = {name.lower(): value for name, value in served_pairs}
        for name, value in wanted:
            if name not in served_map:
                problems.append(CADDYFILE + " does not stamp " + name + " on the SPA")
            elif served_map[name] != value:
                problems.append(
                    CADDYFILE + " stamps " + name + " as " + repr(served_map[name])
                    + " and " + SECURITY + " sends " + repr(value)
                )
        for name in served_map:
            if name not in {entry[0] for entry in wanted}:
                problems.append(
                    CADDYFILE + " stamps " + name + ", which " + SECURITY + " does not send"
                )

for line in problems:
    print(line)
PYCADDY
)"; then
    if [ -n "$caddy_log" ]; then
        printf 'FAIL: caddy    -- %s\n' "$caddy_log"
        rc=1
    else
        echo "PASS: caddy    -- /api proxies to web:8080, the SPA falls back to index.html under the Dockerfile's root, and the five security headers match crates/web/src/security.rs"
    fi
else
    echo "FAIL: caddy    -- the Caddyfile check did not run"
    printf '%s\n' "$caddy_log"
    rc=1
fi

exit "$rc"
