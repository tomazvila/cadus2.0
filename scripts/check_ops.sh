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
# The script does sixteen checks and prints one line per check:
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
#                   fail the gate and not the operator's upgrade. The check then
#                   RUNS a copy of the script against a docker stub and reads its
#                   decisions: step 4 starts web, worker and caddy together; a
#                   caddy that kept its container gets the Caddyfile of this
#                   commit through `caddy reload`; a caddy restart loop exits 1
#                   and names the container; and `--no-caddy` and
#                   DEPLOY_SKIP_CADDY exit 2.
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
#                   origin; its @ops block answers 404 for /api/ready and
#                   /metrics; its five security headers equal SECURITY_HEADERS of
#                   `crates/web/src/security.rs`, character for character; the
#                   compose file lands the file at /etc/caddy/Caddyfile, by a
#                   bind of the file or of the directory above it; and
#                   `caddy validate` in the edge image accepts it. The Caddyfile
#                   is a bind mount, so no image check reads it.
#   (l) reload   -- an EDITED `deploy/Caddyfile` really reaches the running
#                   proxy. The check starts the edge image with the mount the
#                   compose file declares, replaces the file with a new inode as
#                   `git pull` does, runs `caddy reload`, and reads the answer
#                   the proxy serves. A single-file bind binds the inode, so the
#                   container kept the pre-pull copy, the reload exited 0 on it,
#                   and `scripts/deploy.sh` printed DEPLOY OK on the pre-pull
#                   routing (M6 review 2, finding V9).
#   (k) envpair  -- .env.example pairs SITE_ADDRESS with
#                   CADUS_WEB_INSECURE_COOKIE. An http SITE_ADDRESS needs
#                   CADUS_WEB_INSECURE_COOKIE=1, because a browser discards a
#                   `Secure __Host-` cookie on http and the session then never
#                   persists (M6 review, finding F14).
#
# Compose names a built image `<project>-<service>` when the service declares no
# `image:` key. The script reads the project name and the service names from
# `docker compose config --format json`, so it needs no hard-coded image name.
#
# Input: docker, python3, and shellcheck on PATH. The script reads no `.env`
# file. It writes the deploy sandbox of check (e) and the reload sandbox of check
# (l) under `target/check_ops/`, and no other state outside the local docker
# image store. Check (l) starts one container and removes it again; every other
# container runs with `--rm`.
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
# (e) the upgrade script is present and executable, it parses, and it decides
#     what the runbook says it decides
#
# scripts/deploy.sh is THE upgrade procedure (finding #16), so a broken file must
# fail the gate and not the operator's upgrade. Three static facts open the
# check: the file is there, it is executable, and it parses.
#
# The four runs after them DRIVE the script against a DOCKER STUB. deploy.sh
# reaches the stack through `docker` and through nothing else, so a stub `docker`
# first on PATH plays a whole compose stack, and the gate reads the decisions of
# the script instead of the text of it:
#
#   1. ok       -- a stack that comes up. The script exits 0, prints DEPLOY OK,
#                  and its `up -d --no-deps` line names web, worker AND caddy.
#                  The caddy container holds the SPA bundle (M6 S14), so an
#                  upgrade without it serves the previous commit's bundle against
#                  the new API (M6 review, finding F24). The check command it
#                  prints carries the port that `docker compose port` reported,
#                  and never a guessed one (finding F11).
#   2. loop     -- caddy reports state `running` and its restart count grows: the
#                  restart loop of the sole ingress. The script must exit 1 and
#                  name the caddy CONTAINER. The old script read the state of web
#                  and worker only and printed DEPLOY OK over it (finding F11).
#   3. kept     -- compose keeps the caddy container, which is what a commit that
#                  edits the bind-mounted deploy/Caddyfile alone gives: no image
#                  changes and no service spec changes. The script must apply the
#                  file to that running process with `caddy reload`; otherwise the
#                  proxy keeps the configuration it loaded at its own start
#                  (finding F12).
#   4. no-caddy -- the `--no-caddy` flag and DEPLOY_SKIP_CADDY are gone. Both
#                  skipped the container that holds the bundle, so both must stop
#                  the script with exit 2 (finding F24).
#
# The sandbox is a COPY of the script under target/, beside a `.env` of its own.
# deploy.sh reads its repository root from its own path, so the runs touch no
# stack, and the check reads and writes no `.env` of the operator.
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
    deploy_sandbox="$repo_root/target/check_ops/deploy"
    rm -rf "$deploy_sandbox"
    mkdir -p "$deploy_sandbox/scripts" "$deploy_sandbox/bin"
    cp scripts/deploy.sh "$deploy_sandbox/scripts/deploy.sh"
    : >"$deploy_sandbox/.env"

    cat >"$deploy_sandbox/bin/docker" <<'DOCKERSTUB'
#!/usr/bin/env bash
# The `docker` stand-in of check (e) in scripts/check_ops.sh. It plays one
# compose stack out of DEPLOY_STUB_SCENARIO and records every call in
# $DEPLOY_STUB_STATE/log.
set -uo pipefail

state="$DEPLOY_STUB_STATE"
log="$state/log"
printf '%s\n' "$*" >>"$log"

# A per-key call counter. The `loop` scenario reports it as the restart count of
# caddy, so the count grows from one poll to the next.
bump() {
    local file="$state/count-$1" n=0
    if [ -f "$file" ]; then
        n="$(cat "$file")"
    fi
    n=$((n + 1))
    printf '%s' "$n" >"$file"
    printf '%s' "$n"
}

# The container id of caddy. `kept` keeps one id over the whole run, which is
# what compose does for a service whose image and spec did not change. Every
# other scenario gives a new id after the `up`.
caddy_id() {
    if [ "$DEPLOY_STUB_SCENARIO" = "kept" ]; then
        printf 'caddy-old'
    elif grep -q 'up -d --no-deps' "$log"; then
        printf 'caddy-new'
    else
        printf 'caddy-old'
    fi
}

case "$*" in
    "compose build") ;;
    "compose up -d db") ;;
    "compose ps db --format {{.Health}}") echo healthy ;;
    "compose run --rm migrate") ;;
    "compose up -d --no-deps"*) ;;
    "compose ps -q web") echo web-1 ;;
    "compose ps -q worker") echo worker-1 ;;
    "compose ps -q caddy") caddy_id; echo ;;
    # The reader form of an older deploy.sh: one JSON object for one service. The
    # stub answers it, so a script that reads the state this way makes a wrong
    # DECISION in the checks above and not a stub error.
    "compose ps web --format json") echo '{"Name":"cadus2-web-1","State":"running"}' ;;
    "compose ps worker --format json") echo '{"Name":"cadus2-worker-1","State":"running"}' ;;
    "compose ps caddy --format json")
        if [ "$DEPLOY_STUB_SCENARIO" = "loop" ]; then
            echo "{\"Name\":\"cadus2-caddy-1\",\"State\":\"running\",\"RestartCount\":$(bump caddy-inspect)}"
        else
            echo '{"Name":"cadus2-caddy-1","State":"running","RestartCount":7}'
        fi
        ;;
    "inspect --format"*)
        case "$*" in
            *web-1) echo "running 0 /cadus2-web-1" ;;
            *worker-1) echo "running 0 /cadus2-worker-1" ;;
            *caddy-*)
                if [ "$DEPLOY_STUB_SCENARIO" = "loop" ]; then
                    echo "running $(bump caddy-inspect) /cadus2-caddy-1"
                else
                    echo "running 7 /cadus2-caddy-1"
                fi
                ;;
            *) exit 1 ;;
        esac
        ;;
    "compose logs"*)
        case "$*" in
            *web*) echo "web-1  | cadus-web: listening on 0.0.0.0:8080" ;;
            *worker*) echo "worker-1  | cadus-worker: started" ;;
            *caddy*) echo 'caddy-1  | {"level":"info","msg":"serving initial configuration"}' ;;
        esac
        ;;
    "compose exec -T caddy caddy reload"*) ;;
    "compose port caddy 80") echo "0.0.0.0:18080" ;;
    *)
        printf 'docker stub: no answer for `docker %s`\n' "$*" >&2
        exit 9
        ;;
esac
exit 0
DOCKERSTUB
    chmod +x "$deploy_sandbox/bin/docker"

    deploy_rc=0
    deploy_out="$deploy_sandbox/out"
    deploy_log="$deploy_sandbox/state/log"

    # Run the sandbox copy under one scenario. Every further argument goes to
    # deploy.sh. START_LIMIT_SECS drops to 4 s, so the failure path reports
    # inside the gate and waits no 30 s for it. DEPLOY_SKIP_CADDY_UNDER_TEST
    # carries the value of the retired variable into one run and leaves it empty
    # in every other.
    run_deploy() {
        local scenario="$1"
        shift
        rm -rf "$deploy_sandbox/state"
        mkdir -p "$deploy_sandbox/state"
        : >"$deploy_log"
        deploy_rc=0
        DEPLOY_STUB_SCENARIO="$scenario" \
            DEPLOY_STUB_STATE="$deploy_sandbox/state" \
            DEPLOY_START_LIMIT_SECS=4 \
            DEPLOY_SKIP_CADDY="${DEPLOY_SKIP_CADDY_UNDER_TEST:-}" \
            PATH="$deploy_sandbox/bin:$PATH" \
            bash "$deploy_sandbox/scripts/deploy.sh" "$@" >"$deploy_out" 2>&1 || deploy_rc=$?
    }

    deploy_problems=()

    run_deploy ok
    if [ "$deploy_rc" -ne 0 ]; then
        deploy_problems+=("the ok scenario exited $deploy_rc, not 0")
    fi
    if ! grep -q '^DEPLOY OK$' "$deploy_out"; then
        deploy_problems+=("the ok scenario printed no DEPLOY OK line")
    fi
    if ! grep -qx 'compose up -d --no-deps web worker caddy' "$deploy_log"; then
        deploy_problems+=("step 4 of the ok scenario did not start web, worker and caddy together; the caddy container holds the SPA bundle")
    fi
    if ! grep -q 'curl -fsS http://127.0.0.1:18080/api/health' "$deploy_out"; then
        deploy_problems+=("the ok scenario named another health URL than the published port 18080 of caddy")
    fi

    run_deploy loop
    if [ "$deploy_rc" -ne 1 ]; then
        deploy_problems+=("the caddy restart loop exited $deploy_rc, not 1")
    fi
    if grep -q '^DEPLOY OK$' "$deploy_out"; then
        deploy_problems+=("the caddy restart loop reported DEPLOY OK")
    fi
    if ! grep -q 'cadus2-caddy-1' "$deploy_out"; then
        deploy_problems+=("the caddy restart loop named no caddy container in its failure report")
    fi

    run_deploy kept
    if [ "$deploy_rc" -ne 0 ]; then
        deploy_problems+=("the kept-container scenario exited $deploy_rc, not 0")
    fi
    if ! grep -q '^compose exec -T caddy caddy reload' "$deploy_log"; then
        deploy_problems+=("a caddy that kept its container got no \`caddy reload\`, so an edited deploy/Caddyfile never reaches the running proxy")
    fi

    run_deploy ok --no-caddy
    if [ "$deploy_rc" -ne 2 ]; then
        deploy_problems+=("\`scripts/deploy.sh --no-caddy\` exited $deploy_rc, not 2; the flag that skipped the SPA container is gone")
    fi

    DEPLOY_SKIP_CADDY_UNDER_TEST=1 run_deploy ok
    if [ "$deploy_rc" -ne 2 ]; then
        deploy_problems+=("DEPLOY_SKIP_CADDY=1 exited $deploy_rc, not 2; the variable that skipped the SPA container is gone")
    fi

    if [ "${#deploy_problems[@]}" -ne 0 ]; then
        for problem in "${deploy_problems[@]}"; do
            printf 'FAIL: deploy   -- %s\n' "$problem"
        done
        printf '%s\n' "the last run of scripts/deploy.sh is in $deploy_out"
        deploy_ok=0
        rc=1
    fi
fi

if [ "$deploy_ok" -eq 1 ]; then
    echo "PASS: deploy   -- scripts/deploy.sh parses; against the docker stub it starts web, worker and caddy together, reloads a kept caddy, fails a caddy restart loop by container name, and refuses --no-caddy and DEPLOY_SKIP_CADDY"
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
caddy_ok=1
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
ops_body = []
fallback_body = []
for index, (depth, line) in enumerate(rows):
    if depth != 1 or not line.startswith("handle"):
        continue
    matcher = line[len("handle"):].strip().rstrip("{").strip()
    if matcher == "@api":
        api_body = body_of(rows, index)
    elif matcher == "@ops":
        ops_body = body_of(rows, index)
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

# --- the @ops guard ---------------------------------------------------------
# /api/ready reports the datastore verdict and the age of the diagnosis backlog
# (D-M5-6), and /metrics reports every request series. Both are for the compose
# network. Delete the matcher or the 404 and the edge publishes them to every
# visitor (M6 review, finding F23).
OPS_PATHS = ("/api/ready", "/metrics")
ops_matcher = [line for _, line in rows if line.startswith("@ops ")]
if not ops_matcher:
    problems.append(
        CADDYFILE + " declares no @ops matcher, so " + " and ".join(OPS_PATHS)
        + " answer through the edge"
    )
elif not ops_matcher[0].startswith("@ops path "):
    problems.append(
        CADDYFILE + " declares the ops matcher as " + repr(ops_matcher[0])
        + "; it matches on `path`"
    )
else:
    guarded = ops_matcher[0].split()[2:]
    for path in OPS_PATHS:
        if path not in guarded:
            problems.append(
                CADDYFILE + " does not guard " + path + " in the @ops matcher, so the"
                " edge publishes it"
            )

if not ops_body:
    problems.append(
        CADDYFILE + " has no `handle @ops` block, so the @ops paths fall through to the"
        " SPA handle and answer 200"
    )
elif not any(line.startswith("respond 404") for _, line in ops_body):
    problems.append(CADDYFILE + " does not answer 404 in the `handle @ops` block")

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
        caddy_ok=0
        rc=1
    fi
else
    echo "FAIL: caddy    -- the Caddyfile check did not run"
    printf '%s\n' "$caddy_log"
    caddy_ok=0
    rc=1
fi

# --- 5: the compose file mounts the file this check just read ---------------
#
# Every fact above is a fact about `deploy/Caddyfile` in the repository. The
# `spa` image copies the bundle and NO Caddyfile, so the container runs the
# stock `caddy:2` configuration unless the compose file bind-mounts this file
# over /etc/caddy/Caddyfile. Delete that one line and the deployment serves the
# Caddy welcome page while every check above stays green (M6 review, finding
# F23).
#
# The bind reaches that path in one of two shapes, and the reader below accepts
# both:
#
#   the FILE      -- `./deploy/Caddyfile:/etc/caddy/Caddyfile`
#   the DIRECTORY -- `./deploy:/etc/caddy`, and Caddy reads the Caddyfile inside
#
# The two are NOT the same at run time. Check (l) below drives that difference
# against the real edge image and fails the file shape (M6 review 2, finding
# V9). This reader holds the shape-free fact alone: some bind of this
# repository's `deploy/Caddyfile` lands at /etc/caddy/Caddyfile. It also prints
# one `MOUNT <source relative to the repository root> <container target>` line,
# which check (l) replays in its sandbox, so the probe reads the compose file
# and never a second copy of the mount.
mount_log=""
mount_spec=""
mount_read=""
if mount_read="$(printf '%s' "$config_json" | python3 -c '
import json
import posixpath
import sys

TARGET = "/etc/caddy/Caddyfile"
SOURCE = "deploy/Caddyfile"

root = sys.argv[1].replace("\\", "/").rstrip("/") + "/"
doc = json.load(sys.stdin)
problems = []
spec = ""
edge = []

for name, service in sorted(doc.get("services", {}).items()):
    build = service.get("build") or {}
    if build.get("target") == "spa":
        edge.append((name, service))

if not edge:
    problems.append("no service builds the spa image, so nothing serves the bundle")

for name, service in edge:
    # The bind that carries TARGET: the file itself, or a directory above it.
    carrier = None
    for volume in service.get("volumes", []) or []:
        if not isinstance(volume, dict):
            continue
        target = str(volume.get("target", "")).replace("\\", "/")
        if target == TARGET or TARGET.startswith(target.rstrip("/") + "/"):
            carrier = (volume, target.rstrip("/") or "/")
            break
    if carrier is None:
        problems.append(
            "the " + name + " service mounts nothing at " + TARGET
            + "; the container then runs the stock caddy:2 configuration and serves"
            " the Caddy welcome page"
        )
        continue
    volume, target = carrier
    source = str(volume.get("source", "")).replace("\\", "/")
    # The repository path that reaches TARGET through this bind. A file bind
    # reaches it directly; a directory bind reaches it through the rest of the
    # container path.
    inside = TARGET[len(target):].lstrip("/")
    reached = posixpath.normpath(posixpath.join(source, inside)) if inside else source
    if not reached.endswith(SOURCE):
        problems.append(
            "the " + name + " service mounts " + repr(source) + " at " + target
            + ", so " + TARGET + " reads " + repr(reached) + " and not " + SOURCE
            + ", which is the file this check reads"
        )
        continue
    relative = source[len(root):] if source.startswith(root) else source
    spec = relative + " " + target

for line in problems:
    print(line)
if spec:
    print("MOUNT " + spec)
' "$repo_root")"; then
    mount_log="$(printf '%s\n' "$mount_read" | grep -v '^MOUNT ' || true)"
    mount_spec="$(printf '%s\n' "$mount_read" | sed -n 's/^MOUNT //p' | head -n 1)"
    if [ -n "$mount_log" ]; then
        printf 'FAIL: caddy    -- %s\n' "$mount_log"
        caddy_ok=0
        rc=1
    fi
else
    echo "FAIL: caddy    -- the Caddyfile mount check did not run"
    caddy_ok=0
    rc=1
fi

# --- 6: Caddy itself accepts the file ---------------------------------------
#
# The reader above holds no grammar of the Caddyfile: it counts braces and reads
# lines. A file that Caddy REFUSES therefore passed the whole ops gate, and the
# operator met the fault as a restart loop on the sole ingress (M6 review,
# finding F13). `caddy validate` in the edge image is the authority: it runs the
# same Caddy build that the deployment runs, and it reads the same file through
# the same path. SITE_ADDRESS stands in for `.env`, because the file names it.
validate_ok=1
while read -r image; do
    [ -n "$image" ] || continue
    validate_log=""
    if ! validate_log="$(docker run --rm --entrypoint caddy \
        -e SITE_ADDRESS="$SITE_ADDRESS" \
        -v "$repo_root/deploy/Caddyfile:/etc/caddy/Caddyfile:ro" \
        "$image" validate --config /etc/caddy/Caddyfile --adapter caddyfile 2>&1)"; then
        echo "FAIL: caddy    -- caddy validate refuses deploy/Caddyfile in image $image"
        printf '%s\n' "$validate_log"
        validate_ok=0
        caddy_ok=0
        rc=1
    fi
done <<EOF
$spa_images
EOF

if [ "$caddy_ok" -eq 1 ] && [ "$validate_ok" -eq 1 ]; then
    echo "PASS: caddy    -- caddy validate accepts deploy/Caddyfile in the edge image, the compose file mounts it at /etc/caddy/Caddyfile, /api proxies to web:8080, @ops answers 404 for /api/ready and /metrics, the SPA falls back to index.html under the Dockerfile's root, and the five security headers match crates/web/src/security.rs"
fi

# ---------------------------------------------------------------------------
# (l) an edited Caddyfile really reaches the running proxy
#
# Check (j) reads the FILE and check (e) reads the SCRIPT. Both passed while the
# edge served the routing of the commit before the upgrade (M6 review 2, finding
# V9):
#
#   docker-compose.yml bind-mounted `deploy/Caddyfile` as a FILE, and a
#   single-file bind binds the INODE the container started with. `git pull`
#   writes the working-tree file as a NEW inode, and so do every editor and
#   `sed -i`. The container therefore kept the pre-pull copy.
#   `docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile`
#   re-read THAT copy, logged `using config from file` and `adapted config to
#   JSON`, and exited 0. `scripts/deploy.sh` read the exit 0 as proof, skipped
#   the recreate, and printed DEPLOY OK on the pre-pull routing.
#
# Nothing in the text of the three files says which inode a running container
# holds, so this check RUNS the sequence against the real edge image and reads
# the answer the proxy serves:
#
#   1. Build a sandbox that mirrors the repository: `deploy/Caddyfile`, holding
#      a whole site that answers the literal ONE_ANSWER below.
#   2. Start the edge image with the mount the compose file declares. Check (j)
#      part 5 read that mount out of `docker compose config`, so this probe
#      replays the deployment's own mount and carries no second copy of it.
#   3. Read the answer through the proxy's own port. It must be ONE_ANSWER: the
#      mount carries the sandbox file.
#   4. REPLACE the file with a new inode, exactly as `git pull` does: write a
#      temporary file beside it and rename it over the name. The new file
#      answers TWO_ANSWER.
#   5. Run `caddy reload` in the container, as scripts/deploy.sh does.
#   6. Read the answer again. It must be TWO_ANSWER.
#
# A file bind fails step 6 and answers ONE_ANSWER, with a reload that exited 0.
# A directory bind of `deploy/` passes it: the directory is the bound inode, and
# Caddy opens the name inside it on every reload.
# ---------------------------------------------------------------------------
RELOAD_CONFIG_PATH=/etc/caddy/Caddyfile
ONE_ANSWER=cadus-reload-probe-one
TWO_ANSWER=cadus-reload-probe-two

# How long the probe proxy gets to bind its port and answer, in whole seconds.
RELOAD_START_SECS=15

reload_ok=1
reload_image="$(printf '%s\n' "$spa_images" | head -n 1)"

if [ -z "$reload_image" ]; then
    echo "FAIL: reload   -- the reload probe did not run: no service builds the $SPA_TARGET image"
    reload_ok=0
    rc=1
elif [ -z "$mount_spec" ]; then
    echo "FAIL: reload   -- the reload probe did not run: the mount check read no bind that reaches $RELOAD_CONFIG_PATH"
    reload_ok=0
    rc=1
else
    reload_source="${mount_spec%% *}"
    reload_target="${mount_spec##* }"
    reload_sandbox="$repo_root/target/check_ops/reload"
    reload_container="cadus-check-ops-reload-$$"
    reload_problems=()

    # Where the sandbox Caddyfile goes, so that the mount lands it at
    # RELOAD_CONFIG_PATH. A file bind carries it directly; a directory bind
    # carries it through the rest of the container path. The layout follows the
    # compose mount, so the probe reads the deployment's own shape.
    reload_inside="${RELOAD_CONFIG_PATH#"$reload_target"}"
    reload_inside="${reload_inside#/}"
    reload_file="$reload_sandbox/$reload_source${reload_inside:+/$reload_inside}"

    rm -rf "$reload_sandbox"
    mkdir -p "$(dirname "$reload_file")"

    # Write one whole site into the sandbox, under a NEW INODE every time. The
    # rename is the point of the probe: it is what `git pull`, every editor, and
    # `sed -i` do to a tracked file.
    write_probe_site() {
        printf ':80 {\n\trespond "%s"\n}\n' "$1" >"$reload_file.new"
        mv "$reload_file.new" "$reload_file"
    }

    # What the proxy answers at its own port, read from inside the container.
    # The edge image is `caddy:2`, which carries busybox wget. wget resolves
    # `localhost` to ::1 and the probe site listens on IPv4, so the URL names
    # 127.0.0.1.
    probe_answer() {
        docker exec "$reload_container" wget -qO- http://127.0.0.1:80/ 2>/dev/null || true
    }

    write_probe_site "$ONE_ANSWER"
    docker rm -f "$reload_container" >/dev/null 2>&1 || true

    if ! docker run -d --name "$reload_container" \
        -v "$reload_sandbox/$reload_source:$reload_target:ro" \
        "$reload_image" >/dev/null 2>&1; then
        reload_problems+=(
            "the edge image $reload_image did not start with the compose mount \`$reload_source:$reload_target\`"
        )
    else
        answer=""
        waited=0
        while [ "$waited" -lt "$RELOAD_START_SECS" ]; do
            answer="$(probe_answer)"
            if [ -n "$answer" ]; then
                break
            fi
            waited=$((waited + 1))
            sleep 1
        done

        if [ "$answer" != "$ONE_ANSWER" ]; then
            reload_problems+=(
                "the probe proxy answered \`${answer:-nothing}\` and not \`$ONE_ANSWER\` within ${RELOAD_START_SECS} s, so the compose mount \`$reload_source:$reload_target\` never carried the sandbox Caddyfile to $RELOAD_CONFIG_PATH"
            )
        else
            write_probe_site "$TWO_ANSWER"
            reload_rc=0
            docker exec "$reload_container" caddy reload \
                --config "$RELOAD_CONFIG_PATH" --adapter caddyfile \
                >/dev/null 2>&1 || reload_rc=$?
            answer="$(probe_answer)"

            if [ "$reload_rc" -ne 0 ]; then
                reload_problems+=(
                    "\`caddy reload --config $RELOAD_CONFIG_PATH\` exited $reload_rc on a Caddyfile that caddy validate accepts"
                )
            fi
            if [ "$answer" != "$TWO_ANSWER" ]; then
                # Name the cause the mount shape points at. A bind whose target
                # IS the config path is the single-file bind of finding V9. Any
                # other shape failed for another reason, and a message that
                # named the inode would send the reader to the wrong place.
                reload_cause="The compose mount \`$reload_source:$reload_target\` carried no new file into the container, or the reload did not apply it."
                if [ "$reload_target" = "$RELOAD_CONFIG_PATH" ]; then
                    reload_cause="The compose mount \`$reload_source:$reload_target\` binds a SINGLE FILE, so it binds that file's inode and follows no replacement of it. Mount the DIRECTORY \`deploy/\` at ${RELOAD_CONFIG_PATH%/*} instead, or copy the file in with \`docker compose cp\` before the reload."
                fi
                reload_problems+=(
                    "the edited Caddyfile never reached the running proxy: the sandbox file was replaced with a NEW INODE, \`caddy reload\` exited $reload_rc, and the proxy still answers \`${answer:-nothing}\` and not \`$TWO_ANSWER\`. ${reload_cause} See M6 review 2, finding V9"
                )
            fi
        fi
    fi

    docker rm -f "$reload_container" >/dev/null 2>&1 || true

    if [ "${#reload_problems[@]}" -ne 0 ]; then
        for problem in "${reload_problems[@]}"; do
            printf 'FAIL: reload   -- %s\n' "$problem"
        done
        reload_ok=0
        rc=1
    fi
fi

if [ "$reload_ok" -eq 1 ]; then
    echo "PASS: reload   -- an edited deploy/Caddyfile reaches the running proxy: through the compose mount \`$reload_source:$reload_target\`, a file replaced by a new inode and a \`caddy reload\` change the answer the edge image serves"
fi

# ---------------------------------------------------------------------------
# (k) .env.example pairs SITE_ADDRESS with CADUS_WEB_INSECURE_COOKIE
#
# The two keys are one decision. `crates/web/src/cookie.rs` writes the session
# cookie as `__Host-cadus_session; Secure` at the default, and a browser
# DISCARDS such a cookie on an http:// origin without a word: the login answers
# 200 and the next authed write answers 401. A copied .env.example that serves
# http (SITE_ADDRESS `:80`, or a `http://` origin) must therefore carry
# `CADUS_WEB_INSECURE_COOKIE=1`, and an https deployment must not (M6 review,
# finding F14).
#
# The check holds three facts about .env.example:
#
#   1. The two keys agree. An http SITE_ADDRESS needs an ACTIVE
#      CADUS_WEB_INSECURE_COOKIE=1 line; a domain needs the value 0, or no
#      active line at all.
#   2. Each of the two blocks names the other key, so an operator who edits one
#      reads about the other in the same place.
#   3. The file carries both worked examples: an http one that sets the cookie
#      knob to 1, and an https one that does not.
# ---------------------------------------------------------------------------
envpair_log=""
if envpair_log="$(python3 - <<'PYENVPAIR'
import io
import re

ENV = ".env.example"

SITE = "SITE_ADDRESS"
COOKIE = "CADUS_WEB_INSECURE_COOKIE"

problems = []
text = io.open(ENV, encoding="utf-8").read()
lines = text.splitlines()


def active(key):
    """The value of the last uncommented `KEY=value` line, or None."""
    found = None
    for line in lines:
        stripped = line.strip()
        if stripped.startswith(key + "="):
            found = stripped[len(key) + 1:].strip()
    return found


def block_of(key):
    """The block that documents one key.

    A block is the run of lines above the key, up to the first blank line: the
    comment paragraph and any key that sits in the same paragraph. The ACTIVE
    line of the key comes first. A key that only appears commented out, which is
    what an https deployment does with the cookie knob, falls back to the first
    commented line.
    """
    index = None
    for position, line in enumerate(lines):
        if line.strip().startswith(key + "="):
            index = position
    if index is None:
        for position, line in enumerate(lines):
            if line.strip().lstrip("#").strip().startswith(key + "="):
                index = position
                break
    if index is None:
        return ""
    start = index
    while start > 0 and lines[start - 1].strip() != "":
        start -= 1
    return "\n".join(lines[start:index + 1])


site = active(SITE)
cookie = active(COOKIE)

if site is None:
    problems.append(ENV + " sets no " + SITE)
else:
    http_only = site.startswith(":") or site.startswith("http://")
    if http_only and cookie != "1":
        problems.append(
            ENV + " serves http (" + SITE + "=" + site + ") and carries "
            + (COOKIE + "=" + cookie if cookie is not None else "no active " + COOKIE)
            + "; a browser discards the Secure __Host- cookie on http, so the login"
            " answers 200 and the next authed write answers 401. Set " + COOKIE + "=1"
        )
    if not http_only and cookie == "1":
        problems.append(
            ENV + " serves https (" + SITE + "=" + site + ") and carries " + COOKIE
            + "=1, which drops Secure from the session cookie of a public site"
        )

site_block = block_of(SITE)
cookie_block = block_of(COOKIE)
if COOKIE not in site_block:
    problems.append(
        "the " + SITE + " block of " + ENV + " does not name " + COOKIE
        + "; the two keys are one decision and must be documented as a pair"
    )
if SITE not in cookie_block:
    problems.append(
        "the " + COOKIE + " block of " + ENV + " does not name " + SITE
        + "; the two keys are one decision and must be documented as a pair"
    )

# The two worked examples. Each is a line pair inside a comment block: one
# SITE_ADDRESS line and one CADUS_WEB_INSECURE_COOKIE line for that scheme.
examples = re.findall(
    r"^#\s*(?:" + SITE + r")\s*=\s*(\S+)[^\n]*\n#\s*(?:" + COOKIE + r")\s*=\s*(\S+)",
    text,
    re.M,
)
http_example = [pair for pair in examples if pair[0].startswith((":", "http://"))]
https_example = [pair for pair in examples if not pair[0].startswith((":", "http://"))]

if not any(pair[1] == "1" for pair in http_example):
    problems.append(
        ENV + " carries no http example that pairs an http " + SITE + " with "
        + COOKIE + "=1"
    )
if not any(pair[1] == "0" for pair in https_example):
    problems.append(
        ENV + " carries no https example that pairs a domain in " + SITE + " with "
        + COOKIE + "=0"
    )

for line in problems:
    print(line)
PYENVPAIR
)"; then
    if [ -n "$envpair_log" ]; then
        printf 'FAIL: envpair  -- %s\n' "$envpair_log"
        rc=1
    else
        echo "PASS: envpair  -- .env.example pairs SITE_ADDRESS with CADUS_WEB_INSECURE_COOKIE, documents both keys together, and carries the http example (=1) and the https example (=0)"
    fi
else
    echo "FAIL: envpair  -- the .env.example pair check did not run"
    printf '%s\n' "$envpair_log"
    rc=1
fi

exit "$rc"
