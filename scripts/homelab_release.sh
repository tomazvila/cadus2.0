#!/usr/bin/env bash
# Exact-topology Cadus release helper for /home/deploy/homelab.
set -euo pipefail

release_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
homelab_root="${CADUS_HOMELAB_ROOT:-/home/deploy/homelab}"
rollback_commit="${CADUS_ROLLBACK_COMMIT:-2ff3c1f}"
execute=0
bundle=""
receipt_dir=""
recovery_receipt=""

usage() {
    echo "usage: scripts/homelab_release.sh <preflight|deploy|import-pending|readiness|rollback> [--bundle <dir>] [--recovery-receipt <file>] [--receipt-dir <dir>] [--execute]" >&2
    exit 2
}

command_name="${1:-}"
[ -n "$command_name" ] || usage
shift
while [ "$#" -gt 0 ]; do
    case "$1" in
        --bundle) bundle="${2:-}"; shift 2 ;;
        --recovery-receipt) recovery_receipt="${2:-}"; shift 2 ;;
        --receipt-dir) receipt_dir="${2:-}"; shift 2 ;;
        --execute) execute=1; shift ;;
        *) usage ;;
    esac
done

compose() {
    docker compose --project-directory "$homelab_root" -f "$homelab_root/compose.yaml" "$@"
}

release_commit() {
    git -C "$release_root" rev-parse HEAD
}

container_id() {
    compose ps -q "$1" | head -n 1
}

verify_bundle() {
    [ -n "$bundle" ] || { echo "release: --bundle is required" >&2; exit 2; }
    python3 "$release_root/scripts/review/foundations_release_bundle.py" verify \
        --release-root "$release_root" --bundle "$bundle"
}

verify_recovery() {
    [ -n "$recovery_receipt" ] || {
        echo "release: --recovery-receipt is required" >&2; exit 2;
    }
    python3 "$release_root/scripts/review/foundations_release_bundle.py" verify-recovery \
        --release-root "$release_root" --receipt "$recovery_receipt"
}

verify_topology() {
    [ -f "$homelab_root/compose.yaml" ] && [ -f "$homelab_root/.env" ] || {
        echo "release: homelab compose or .env is missing under $homelab_root" >&2; exit 2;
    }
    local services required service
    services="$(compose config --services)"
    required=(cadus2-db cadus2-migrate cadus2-web cadus2-worker cadus2-edge)
    for service in "${required[@]}"; do
        grep -qx "$service" <<<"$services" || {
            echo "release: homelab compose has no $service" >&2; exit 2;
        }
    done
    [ -z "$(git -C "$release_root" status --porcelain)" ] || {
        echo "release: release checkout is dirty" >&2; exit 2;
    }
    git -C "$release_root" diff --exit-code "$rollback_commit" -- migrations >/dev/null
    local edge_id mount_source web_image worker_image
    edge_id="$(container_id cadus2-edge)"
    [ -n "$edge_id" ] || { echo "release: cadus2-edge is not running" >&2; exit 2; }
    mount_source="$(docker inspect -f '{{range .Mounts}}{{if eq .Destination "/etc/caddy"}}{{.Source}}{{end}}{{end}}' "$edge_id")"
    [ -n "$mount_source" ] && cmp -s "$release_root/deploy/Caddyfile" "$mount_source/Caddyfile" || {
        echo "release: live homelab Caddy mount differs from the candidate" >&2; exit 2;
    }
    web_image="$(docker inspect -f '{{.Image}}' "$(container_id cadus2-web)")"
    worker_image="$(docker inspect -f '{{.Image}}' "$(container_id cadus2-worker)")"
    [ "$web_image" = "$worker_image" ] || {
        echo "release: live web and worker do not use the same app image" >&2; exit 2;
    }
}

require_execute() {
    [ "$execute" -eq 1 ] || {
        echo "release: $command_name is a production mutation; rerun with --execute only after explicit authorization" >&2
        exit 2
    }
}

wait_healthy_db() {
    local deadline state
    deadline=$((SECONDS + 120))
    while [ "$SECONDS" -lt "$deadline" ]; do
        state="$(docker inspect -f '{{.State.Health.Status}}' "$(container_id cadus2-db)" 2>/dev/null || true)"
        [ "$state" = healthy ] && return 0
        sleep 2
    done
    echo "release: cadus2-db did not become healthy" >&2
    return 1
}

verify_runtime() {
    local service state restarts baseline stable id db_id web_id network ready_ip
    stable=0
    declare -A baseline=()
    for service in cadus2-web cadus2-worker cadus2-edge; do
        id="$(container_id "$service")"
        [ -n "$id" ] || { echo "release: $service has no container" >&2; return 1; }
        baseline[$service]="$(docker inspect -f '{{.RestartCount}}' "$id")"
    done
    for _ in {1..15}; do
        local ok=1
        for service in cadus2-web cadus2-worker cadus2-edge; do
            id="$(container_id "$service")"
            state="$(docker inspect -f '{{.State.Status}}' "$id" 2>/dev/null || true)"
            restarts="$(docker inspect -f '{{.RestartCount}}' "$id" 2>/dev/null || true)"
            [ "$state" = running ] && [ "$restarts" = "${baseline[$service]}" ] || ok=0
        done
        if [ "$ok" -eq 1 ]; then stable=$((stable + 1)); else stable=0; fi
        [ "$stable" -ge 2 ] && break
        sleep 2
    done
    [ "$stable" -ge 2 ] || { echo "release: Cadus containers are not stable" >&2; return 1; }
    . "$homelab_root/.env"
    curl -fsS "https://cadus.${DOMAIN}/api/health" >/dev/null
    db_id="$(container_id cadus2-db)"; web_id="$(container_id cadus2-web)"
    network="$(docker inspect -f '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}}{{"\n"}}{{end}}' "$db_id" | head -n 1)"
    ready_ip="$(docker inspect -f "{{(index .NetworkSettings.Networks \"$network\").IPAddress}}" "$web_id")"
    curl -fsS "http://${ready_ip}:8080/api/ready" | grep -q '"ok":true'
}

preflight() {
    verify_bundle
    verify_recovery
    verify_topology
    local commit
    commit="$(release_commit)"
    echo "PREFLIGHT OK release=$commit rollback=$rollback_commit bundle=$bundle"
    echo "production remains unchanged; deploy and import-pending require --execute"
}

deploy_release() {
    preflight
    require_execute
    [ -n "$receipt_dir" ] || { echo "release: deploy requires --receipt-dir" >&2; exit 2; }
    mkdir -p "$receipt_dir"
    local commit short old_app old_edge rollback_file
    commit="$(release_commit)"; short="${commit:0:12}"
    old_app="$(docker inspect -f '{{.Image}}' "$(container_id cadus2-web)")"
    old_edge="$(docker inspect -f '{{.Image}}' "$(container_id cadus2-edge)")"
    rollback_file="$receipt_dir/rollback-images.json"
    python3 -c 'import json,sys; print(json.dumps({"app_image_id":sys.argv[1],"edge_image_id":sys.argv[2],"captured_from":"live latest tags before candidate build"},indent=2,sort_keys=True))' \
        "$old_app" "$old_edge" >"$rollback_file"
    docker tag "$old_app" "cadus2:rollback-${old_app#sha256:}"
    docker tag "$old_edge" "cadus2-edge:rollback-${old_edge#sha256:}"
    docker build --target runtime -t "cadus2:$commit" -t cadus2:latest "$release_root"
    docker build --target spa -t "cadus2-edge:$commit" -t cadus2-edge:latest "$release_root"
    compose up -d cadus2-db
    wait_healthy_db
    compose run --rm cadus2-migrate
    compose up -d --no-deps --force-recreate cadus2-web cadus2-worker cadus2-edge
    verify_runtime
    echo "HOMELAB DEPLOY OK release=$commit rollback_receipt=$rollback_file short=$short"
}

import_pending() {
    preflight
    require_execute
    local commit scratch db_ip db_id network
    commit="$(release_commit)"
    docker image inspect "cadus2:$commit" >/dev/null
    scratch="$(mktemp -d)"
    trap 'rm -rf "$scratch"' EXIT
    local container
    container="$(docker create "cadus2:$commit")"
    trap 'docker rm -f "$container" >/dev/null 2>&1 || true; rm -rf "$scratch"' EXIT
    docker cp "$container:/usr/local/bin/cadus-worker" "$scratch/cadus-worker"
    docker rm "$container" >/dev/null
    container=""
    chmod +x "$scratch/cadus-worker"
    . "$homelab_root/.env"
    [ -n "${CADUS2_ADMIN_PASSWORD:-}" ] || { echo "release: CADUS2_ADMIN_PASSWORD is empty" >&2; exit 2; }
    db_id="$(container_id cadus2-db)"
    network="$(docker inspect -f '{{range $name, $_ := .NetworkSettings.Networks}}{{$name}}{{"\n"}}{{end}}' "$db_id" | head -n 1)"
    db_ip="$(docker inspect -f "{{(index .NetworkSettings.Networks \"$network\").IPAddress}}" "$db_id")"
    export DATABASE_URL="postgresql://cadus_admin:${CADUS2_ADMIN_PASSWORD}@${db_ip}:5432/cadus"
    local member
    for member in 01-templates.json 02-teach.json 03-instruction.json; do
        python3 "$release_root/scripts/authoring/import_local_drafts.py" \
            --manifest "$bundle/$member" --worker "$scratch/cadus-worker" \
            --curriculum "$release_root/curriculum" --missing-only --concurrency 4 --dry-run
    done
    for member in 01-templates.json 02-teach.json 03-instruction.json; do
        python3 "$release_root/scripts/authoring/import_local_drafts.py" \
            --manifest "$bundle/$member" --worker "$scratch/cadus-worker" \
            --curriculum "$release_root/curriculum" --missing-only --concurrency 4
    done
    local inventory expected
    inventory="$(compose exec -T cadus2-db psql -U postgres -d cadus -Atqc \
        "SELECT kind || '|' || status || '|' || count(*) FROM content_store GROUP BY kind,status ORDER BY kind,status")"
    expected=$'hint_ladder|pending|809\nteach|pending|809\ntemplate|pending|809'
    [ "$inventory" = "$expected" ] || {
        echo "release: pending inventory differs from the exact bundle" >&2
        printf '%s\n' "$inventory" >&2
        exit 1
    }
    printf '%s\n' "$inventory"
    echo "PENDING IMPORT OK release=$commit bundle=$bundle; human approval remains required"
}

run_readiness() {
    verify_bundle
    verify_recovery
    verify_topology
    [ -n "$receipt_dir" ] || { echo "release: readiness requires --receipt-dir" >&2; exit 2; }
    mkdir -p "$receipt_dir"
    compose exec -T cadus2-worker cadus-worker readiness --course foundations \
        --json /tmp/foundations-readiness.json --md /tmp/foundations-readiness.md
    local worker_id
    worker_id="$(container_id cadus2-worker)"
    docker cp "$worker_id:/tmp/foundations-readiness.json" "$receipt_dir/readiness.json"
    docker cp "$worker_id:/tmp/foundations-readiness.md" "$receipt_dir/readiness.md"
    compose exec -T cadus2-worker rm -f /tmp/foundations-readiness.json /tmp/foundations-readiness.md
    sha256sum "$receipt_dir/readiness.json" "$receipt_dir/readiness.md"
}

rollback_release() {
    verify_topology
    require_execute
    [ -n "$receipt_dir" ] || { echo "release: rollback requires --receipt-dir" >&2; exit 2; }
    local rollback_file old_app old_edge
    rollback_file="$receipt_dir/rollback-images.json"
    old_app="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["app_image_id"])' "$rollback_file")"
    old_edge="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["edge_image_id"])' "$rollback_file")"
    docker image inspect "$old_app" >/dev/null
    docker image inspect "$old_edge" >/dev/null
    compose stop cadus2-edge cadus2-web cadus2-worker
    docker tag "$old_app" cadus2:latest
    docker tag "$old_edge" cadus2-edge:latest
    compose up -d --no-deps --force-recreate cadus2-web cadus2-worker cadus2-edge
    verify_runtime
    echo "HOMELAB ROLLBACK OK app_image=$old_app edge_image=$old_edge; database and event log retained"
}

case "$command_name" in
    preflight) preflight ;;
    deploy) deploy_release ;;
    import-pending) import_pending ;;
    readiness) run_readiness ;;
    rollback) rollback_release ;;
    *) usage ;;
esac
