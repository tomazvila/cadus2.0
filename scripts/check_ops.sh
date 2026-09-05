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
#
# The checks live in scripts/check_ops.d/, one file per group. This file holds
# the setup, the placeholders, and the order of the checks. Each part file
# defines functions and runs nothing by itself. The `source-path` directive
# below lets shellcheck follow the `source` lines from any working directory.
# shellcheck source-path=SCRIPTDIR
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

# The directory of the check parts, read before the `cd` below.
parts_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/check_ops.d"
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

# shellcheck source=check_ops.d/plan.sh
source "$parts_dir/plan.sh"
# shellcheck source=check_ops.d/images.sh
source "$parts_dir/images.sh"
# shellcheck source=check_ops.d/commands.sh
source "$parts_dir/commands.sh"
# shellcheck source=check_ops.d/deploy.sh
source "$parts_dir/deploy.sh"
# shellcheck source=check_ops.d/compose.sh
source "$parts_dir/compose.sh"
# shellcheck source=check_ops.d/caddy.sh
source "$parts_dir/caddy.sh"
# shellcheck source=check_ops.d/reload.sh
source "$parts_dir/reload.sh"
# shellcheck source=check_ops.d/envpair.sh
source "$parts_dir/envpair.sh"

# The checks, in the order the header lists them. (l) runs after (j), because
# it replays the mount that (j) part 5 read.
check_compose
check_build
check_targets
check_binaries
check_curriculum
check_spa
check_nonode
check_commands
check_deploy
check_invariants
check_shell
check_ci
check_bench
check_caddy_file
check_caddy_mount
check_caddy_validate
check_reload
check_envpair

exit "$rc"
