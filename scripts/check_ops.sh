#!/usr/bin/env bash
# Cadus 2.0 ops check (C3, D9). Run it from any directory.
#
# docs/plans/M0.md makes the compose file and the image build the acceptance
# check of U6. Neither runs in `cargo test`, so a renamed binary target or a
# broken compose key first appears on the operator's server. This script runs
# the operator's own commands in the gate instead.
#
# The script does ten checks and prints one line per check:
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
#   (c2) curriculum -- /app/curriculum is a directory in every image the compose
#                   file builds, and it holds courses.yaml. cadus-worker reads
#                   the tree there for the A6 exemplar fallback and exits 2 when
#                   the tree does not load, so an image without it gives a worker
#                   that restarts forever (findings #5 and #6).
#   (d) commands -- the whole `command:` of every service that builds the app
#                   image is correct. `docker compose config` treats a
#                   `command:` as opaque strings, so the compose file itself
#                   proves nothing. The check has three parts:
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
# `built_images` holds one image name per service with a `build:` section.
#
# `service_commands` holds one line per service with a `build:` section, and one
# for EVERY such service: a service that builds the app image but declares no
# `command:` gets a line too. The old extractor skipped it, so a deleted
# `command:` reported PASS (finding #9). The line is
# `<image> <service> <status> [token ...]`, where <status> is:
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
    if not service.get("build"):
        # The app image is the image this repository builds. A service with no
        # build section runs a third-party image (db, caddy) and keeps its own
        # entrypoint.
        continue
    if what == "images":
        print(image)
        continue
    command = service.get("command")
    if isinstance(command, str):
        command = shlex.split(command)
    if not command:
        print(image, name, "none")
        continue
    if any(len(str(token).split()) != 1 for token in command):
        print(image, name, "space")
        continue
    print(image, name, "ok", *command)
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
# (c2) the curriculum tree is in every image the compose file builds
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
$built_images
EOF

if [ "$curriculum_ok" -eq 1 ]; then
    echo "PASS: curriculum -- $CURRICULUM_PATH/courses.yaml exists in every image the compose file builds"
fi

# ---------------------------------------------------------------------------
# (d) every service that builds the app image runs a known binary with allowed
#     arguments
# ---------------------------------------------------------------------------
commands_ok=1
command_count=0
while IFS= read -r line; do
    [ -n "$line" ] || continue
    read -r -a fields <<<"$line"
    image="${fields[0]}"
    service="${fields[1]}"
    status="${fields[2]}"
    command_count=$((command_count + 1))

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

    binary="${fields[3]}"
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
    for argument in "${fields[@]:4}"; do
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
    echo "PASS: commands -- every service that builds the app image runs a known binary with allowed arguments ($command_count checked)"
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

exit "$rc"
