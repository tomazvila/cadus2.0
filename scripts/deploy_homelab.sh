#!/usr/bin/env bash
# Deploy HEAD of this checkout onto the homelab topology (H-7, ISSUES.md).
#
# The repo's own `scripts/deploy.sh` assumes the repo's docker-compose.yml and
# the repo's .env: the repo stack with its own caddy. This box serves Cadus from
# the homelab project instead (`/home/deploy/homelab`, services `cadus2-db
# -migrate -web -worker -report-worker -edge`), where the TLS ingress is the
# homelab proxy and the SPA bundle rides `cadus2-edge`. Until this script
# existed, deploys were a hand-typed sequence, which is how the box once served
# a build one commit behind HEAD (ISSUE-8's stale 404).
#
# The supported order (HANDOVER-OUTSTANDING.md §1), in four steps:
#
#   0. Refuse to deploy a dirty tree. `cadus2-edge` mounts `deploy/ from THIS
#      checkout read-only, so an uncommitted Caddyfile or curriculum file would
#      reach the containers while the images claim another commit.
#   1. Build both images from HEAD: `cadus2:latest` (runtime) and
#      `cadus2-edge:latest` (spa). THE TWO IMAGES GO TOGETHER: a new web beside
#      the previous commit's bundle serves the old SPA against the new API.
#   2. Run the `cadus2-migrate` one-shot. A failure stops the script, and the
#      old containers keep serving.
#   3. Replace the serving services with `--no-deps`, so neither db nor
#      migrate is touched.
#   4. Check the containers came up and STAYED up. `docker compose up -d`
#      returns 0 as soon as the containers START, so one poll proves nothing.
#
# Input: docker with the Compose plugin, and `.env` beside compose.yaml in the
# homelab root (CADUS_HOMELAB_ROOT and --homelab-dir override it). The script
# writes no file.
#
# Exit codes: 0 for a finished deploy, 1 for a failed step, 2 for a bad
# argument or a missing input.
#
# See docs/SELF_HOST.md, section "Homelab topology".
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
homelab_root="${CADUS_HOMELAB_ROOT:-/home/deploy/homelab}"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --homelab-dir)
            if [ -z "${2:-}" ]; then
                echo "deploy_homelab: --homelab-dir needs a value" >&2
                exit 2
            fi
            homelab_root="$2"
            shift 2
            ;;
        --help|-h)
            echo "usage: scripts/deploy_homelab.sh [--homelab-dir DIR]" >&2
            exit 0
            ;;
        *)
            echo "deploy_homelab: unknown argument: $1" >&2
            echo "usage: scripts/deploy_homelab.sh [--homelab-dir DIR]" >&2
            exit 2
            ;;
    esac
done

if ! command -v docker >/dev/null 2>&1; then
    echo "deploy_homelab: docker is not on PATH" >&2
    exit 2
fi

for file in "$homelab_root/compose.yaml" "$homelab_root/.env"; do
    if [ ! -f "$file" ]; then
        echo "deploy_homelab: $file is missing" >&2
        exit 2
    fi
done

# Step 0: a dirty tree deploys a build nobody can name. The check ignores
# untracked files, which a checkout carries without lying about its build.
head_commit="$(git -C "$repo_root" rev-parse HEAD)"
if [ -n "$(git -C "$repo_root" status --porcelain --untracked-files=no)" ]; then
    echo "deploy_homelab: the checkout is DIRTY; commit or stash before deploying" >&2
    git -C "$repo_root" status --porcelain --untracked-files=no >&2
    echo "deploy_homelab: deploying would serve uncommitted files through the edge mount" >&2
    exit 1
fi

compose() {
    docker compose --project-directory "$homelab_root" -f "$homelab_root/compose.yaml" "$@"
}

# The services step 3 replaces. They must exist in the homelab compose, or the
# topology changed and the script must stop instead of half-deploy.
SERVICES=(cadus2-web cadus2-worker cadus2-report-worker cadus2-edge)
known="$(compose config --services)"
for service in "${SERVICES[@]}"; do
    if ! grep -qx "$service" <<<"$known"; then
        echo "deploy_homelab: homelab compose has no service: $service; the topology changed" >&2
        exit 2
    fi
done

echo "== 1/3 build the images of HEAD ($head_commit)"
echo "    cadus2:latest (runtime)"
docker build -t cadus2:latest --target runtime "$repo_root"
echo "    cadus2-edge:latest (spa)"
docker build -t cadus2-edge:latest --target spa "$repo_root"

echo "== 2/3 apply the migrations"
if ! compose run --rm cadus2-migrate; then
    echo "deploy_homelab: the migrate one-shot failed; the upgrade stops here" >&2
    echo "deploy_homelab: the old cadus2-web and cadus2-worker still serve traffic on the old schema" >&2
    exit 1
fi

echo "== 3/3 start the new services"
compose up -d --no-deps "${SERVICES[@]}"

# The up returned 0 at START; the containers may still exit. The check waits up
# to 30 s for every service to hold state `running on TWO polls 2 s apart.
start_limit=30
deadline=$((SECONDS + start_limit))
baseline=""
clean=0
while true; do
    failed=""
    report=""
    for service in "${SERVICES[@]}"; do
        id="$(compose ps -q "$service" | head -n 1)"
        facts="$(docker inspect --format '{{.State.Status}} {{.RestartCount}}' "$id" 2>/dev/null || true)"
        state="$(printf '%s' "$facts" | cut -d' ' -f1)"
        restarts="$(printf '%s' "$facts" | cut -d' ' -f2)"
        report="${report}${report:+, }${service} ${state:-unknown}"
        grown=0
        if [ -n "$facts" ]; then
            case "$baseline" in
                *"|${service}=${restarts}|"*) ;;
                *"|${service}="*) grown=1 ;;
                *) baseline="${baseline}|${service}=${restarts}|" ;;
            esac
        fi
        if [ -z "$failed" ] && { [ "$state" != "running" ] || [ "$grown" -eq 1 ]; }; then
            failed="$service"
        fi
    done
    if [ -z "$failed" ]; then
        clean=$((clean + 1))
        if [ "$clean" -ge 2 ]; then
            break
        fi
    else
        clean=0
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
        echo "deploy_homelab: the new containers did not stay up within ${start_limit} s" >&2
        echo "deploy_homelab: ${report}" >&2
        echo "deploy_homelab: the service that failed is ${failed}:" >&2
        compose logs --tail 40 "$failed" >&2 || true
        echo "deploy_homelab: the new schema is in place; correct the fault and run this script again" >&2
        exit 1
    fi
    sleep 2
done

echo "DEPLOY OK at ${head_commit} (${homelab_root})"
echo "Do a check:"
echo "  docker compose --project-directory ${homelab_root} ps"
echo "  curl -fsS https://cadus.homelab.tomazvi.la/api/health"
