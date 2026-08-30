#!/usr/bin/env bash
# The S14 acceptance check: the DEPLOYED shape, through a real Caddy.
#
#   web/e2e/packaging.sh [--no-build] [--port=N] [--keep]
#
# `scripts/check_ops.sh` reads the Dockerfile, the compose file and the Caddyfile
# and proves what they SAY. This script brings the stack up and proves what it
# DOES. The three facts spec section 7.1 row S14 names:
#
#   1. the runtime image carries no node;
#   2. a cookie-authed POST through Caddy is accepted, and a cross-site one
#      is 403;
#   3. the built bundle passes the CSP grep.
#
# It reads one more fact with fact 2, because the same login answer carries it:
# on this http origin the session cookie is the plain `cadus_session` name
# without `Secure`, which is the posture CADUS_WEB_INSECURE_COOKIE=1 buys. A
# browser discards a `Secure __Host-` cookie on http without a word (M6 review,
# finding F14).
#
# Fact 2 needs the whole stack, because the `403` comes from the CSRF origin
# layer of the service (`crates/web/src/origin.rs`) and not from Caddy. Only a
# real proxy in front of a real service on ONE origin shows the difference
# between the two POSTs.
#
# It runs the compose stack under its OWN project name and its own port, so it
# touches no other stack on this box, and it removes the project and its volumes
# on the way out. Pass `--keep` to leave the stack up for a look.
#
# Input: docker, curl, and node on PATH. The script writes only under
# `web/e2e/work/` and `web/dist/`, both of which are build output under
# `.gitignore`.
set -euo pipefail

PROJECT=cadus2s14acc
PORT=18080
BUILD=1
KEEP=0

for argument in "$@"; do
    case "$argument" in
        --no-build) BUILD=0 ;;
        --port=*) PORT="${argument#--port=}" ;;
        --keep) KEEP=1 ;;
        *) echo "packaging.sh: unknown argument $argument" >&2; exit 2 ;;
    esac
done

web_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo_root="$(cd "$web_dir/.." && pwd)"
work="$web_dir/e2e/work/packaging"
dist="$web_dir/dist"
origin="http://127.0.0.1:$PORT"

for tool in docker curl node; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "FAIL: input    -- $tool is not on PATH" >&2
        exit 2
    fi
done

cd "$repo_root"

# The values `.env` would carry. Three of them are the ones this check turns on:
#   PUBLIC_ORIGIN               -- the origin the CSRF layer compares against. It
#                                  is the origin curl uses below, so a same-site
#                                  POST matches and a cross-site one does not.
#   CADUS_WEB_INSECURE_COOKIE=1 -- plain http, and the pair that .env.example
#                                  ships with SITE_ADDRESS=:80. Without it the
#                                  service writes `__Host-cadus_session; Secure`,
#                                  which a browser discards on an http origin:
#                                  the login answers 200 and the authed POST
#                                  below answers 401 (M6 review, finding F14).
#   CADUS_AUTH_ARGON2_PROFILE   -- `test` is the cheap hash. `prod` is 65536 KiB
#                                  per signup and this check makes two.
export COMPOSE_PROJECT_NAME="$PROJECT"
export SITE_ADDRESS=":80"
export CADDY_HTTP_PORT="$PORT"
export CADDY_HTTPS_PORT="$((PORT + 1))"
export POSTGRES_PASSWORD=acceptance-postgres-password
export CADUS_APP_PASSWORD=0123456789abcdef0123456789abcdef0123456789abcdef
export CADUS_ADMIN_PASSWORD=fedcba9876543210fedcba9876543210fedcba9876543210
export PUBLIC_ORIGIN="$origin"
export CADUS_WEB_INSECURE_COOKIE=1
export CADUS_AUTH_ARGON2_PROFILE=test

rc=0

teardown() {
    if [ "$KEEP" = "1" ]; then
        echo "packaging: the stack stays up on $origin (project $PROJECT)"
        return
    fi
    docker compose -p "$PROJECT" down -v --remove-orphans >/dev/null 2>&1 || true
}
trap teardown EXIT

# ---------------------------------------------------------------------------
# Build, and read the two image names out of the resolved compose file.
# ---------------------------------------------------------------------------
if [ "$BUILD" = "1" ]; then
    echo "packaging: building the two images"
    docker compose -p "$PROJECT" build >/dev/null
fi

read_image() {
    docker compose -p "$PROJECT" config --format json | python3 -c '
import json
import sys

want = sys.argv[1]
doc = json.load(sys.stdin)
project = doc.get("name", "")
for name, service in sorted(doc.get("services", {}).items()):
    build = service.get("build")
    if build and build.get("target") == want:
        print(service.get("image") or "{}-{}".format(project, name))
        break
' "$1"
}

app_image="$(read_image runtime)"
edge_image="$(read_image spa)"

if [ -z "$app_image" ] || [ -z "$edge_image" ]; then
    echo "FAIL: images   -- the compose file names no runtime image or no spa image"
    exit 1
fi

# ---------------------------------------------------------------------------
# (1) neither image carries node
# ---------------------------------------------------------------------------
node_found=""
for image in "$app_image" "$edge_image"; do
    found="$(docker run --rm --entrypoint sh "$image" \
        -c 'for tool in node npm npx; do command -v "$tool" || true; done' 2>/dev/null || true)"
    if [ -n "$found" ]; then
        node_found="$node_found $image:$(printf '%s' "$found" | tr '\n' ',')"
    fi
done

if [ -n "$node_found" ]; then
    echo "FAIL: nonode   -- an image carries node:$node_found"
    rc=1
else
    echo "PASS: nonode   -- neither the app image ($app_image) nor the edge image ($edge_image) carries node, npm, or npx"
fi

# ---------------------------------------------------------------------------
# (2) the bundle the EDGE IMAGE SHIPS passes the CSP audit
#
# The bundle comes out of the image and not out of a host build, so the audit
# reads the exact bytes a browser would be served. `dist/` is build output under
# .gitignore, so replacing it costs nothing and `npm run build` restores it.
#
# `docker cp` reads a created container and needs no shell, no tar and no node
# inside the image.
# ---------------------------------------------------------------------------
rm -rf "$dist"
mkdir -p "$dist"
container="$(docker create "$edge_image")"
docker cp "$container:/srv/." "$dist" >/dev/null
docker rm -f "$container" >/dev/null

csp_log=""
if csp_log="$(cd "$web_dir" && npm run --silent csp 2>&1)"; then
    printf 'PASS: csp      -- the bundle inside %s passes the audit: %s\n' \
        "$edge_image" "$(printf '%s' "$csp_log" | tail -n 1)"
else
    echo "FAIL: csp      -- the bundle inside $edge_image fails the audit"
    printf '%s\n' "$csp_log"
    rc=1
fi

# ---------------------------------------------------------------------------
# Bring the stack up. `caddy` depends on `web`, `web` waits for `migrate` to
# exit 0, and `migrate` waits for a healthy `db`, so one service name starts the
# whole chain in the right order.
# ---------------------------------------------------------------------------
mkdir -p "$work"
jar="$work/cookies.txt"
rm -f "$jar"

echo "packaging: starting the stack on $origin"
docker compose -p "$PROJECT" up -d caddy >/dev/null

ready=0
for _ in $(seq 1 90); do
    if curl -fsS -o /dev/null "$origin/api/health" 2>/dev/null; then
        ready=1
        break
    fi
    sleep 2
done

if [ "$ready" != "1" ]; then
    echo "FAIL: bringup  -- $origin/api/health never answered 200"
    docker compose -p "$PROJECT" ps
    docker compose -p "$PROJECT" logs --tail 40 web caddy migrate
    exit 1
fi

# The status line of one request through Caddy.
status_of() {
    curl -sS -o /dev/null -w '%{http_code}' "$@"
}

# ---------------------------------------------------------------------------
# (3) one origin: Caddy serves the document and proxies /api to the service
# ---------------------------------------------------------------------------
edge_ok=1
document="$(curl -sS -D "$work/root.headers" "$origin/" || true)"
if ! printf '%s' "$document" | grep -q '<title>Cadus</title>'; then
    echo "FAIL: edge     -- GET / does not serve the SPA document"
    edge_ok=0
fi

# The five headers of crates/web/src/security.rs on a file Caddy serves.
CSP_LITERAL="default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; font-src 'self'; base-uri 'none'; frame-ancestors 'none'; connect-src 'self' http://localhost:* http://127.0.0.1:*"
while IFS='|' read -r name value; do
    [ -n "$name" ] || continue
    if ! grep -iq "^$name: $(printf '%s' "$value" | sed 's/[].[^$*\/]/\\&/g')" "$work/root.headers"; then
        printf 'FAIL: edge     -- GET / does not carry %s: %s\n' "$name" "$value"
        edge_ok=0
    fi
done <<EOF
content-security-policy|$CSP_LITERAL
x-content-type-options|nosniff
x-frame-options|DENY
referrer-policy|no-referrer
cache-control|no-cache
EOF

# The SPA fallback: /ops is a route of the app and a file of nothing.
if ! curl -sS "$origin/ops" | grep -q '<title>Cadus</title>'; then
    echo "FAIL: edge     -- GET /ops does not fall back to index.html"
    edge_ok=0
fi

# The SERVICE and not the fallback document. A broken @api matcher hands /api to
# the file server, and `try_files` then answers index.html with a 200, so a
# status-only check reports PASS on a stack whose API is unreachable.
if [ "$(curl -sS "$origin/api/health")" != '{"ok":true}' ]; then
    echo "FAIL: edge     -- GET /api/health does not reach the service"
    edge_ok=0
fi

# The ops endpoints stay off the edge.
if [ "$(status_of "$origin/api/ready")" != "404" ]; then
    echo "FAIL: edge     -- GET /api/ready is reachable through Caddy"
    edge_ok=0
fi
if [ "$(status_of "$origin/metrics")" != "404" ]; then
    echo "FAIL: edge     -- GET /metrics is reachable through Caddy"
    edge_ok=0
fi

if [ "$edge_ok" = "1" ]; then
    echo "PASS: edge     -- one origin: / and /ops serve the bundle with the five security headers, /api/health answers {\"ok\":true} from the service, /api/ready and /metrics answer 404"
else
    rc=1
fi

# ---------------------------------------------------------------------------
# (4) the cookie-authed POST through Caddy, in both polarities
#
# `POST /api/session/start` is a real authed write: it takes no body, it needs a
# session, and it appends a `SessionStart` event. The session cookie is the only
# credential, which is what makes rule 1 of the CSRF layer apply.
#
# The account signs up through the real route, so the password takes the
# service's own Argon2 path. Only `email_verified_at` is stamped directly: M5
# sends no mail and stores the token hashed, so no link can be recovered
# (`web/e2e/seed.sh` says the same).
# ---------------------------------------------------------------------------
email="packaging@cadus.local"
password="a-long-enough-packaging-password-2026"
body="{\"email\":\"$email\",\"password\":\"$password\"}"

curl -sS -o /dev/null -X POST "$origin/api/auth/signup" \
    -H 'content-type: application/json' -H "Origin: $origin" -d "$body"

docker compose -p "$PROJECT" exec -T db \
    psql -U postgres -d cadus -v ON_ERROR_STOP=1 -tAc \
    "UPDATE users SET email_verified_at = now() WHERE email = lower('$email')" >/dev/null

login_status="$(curl -sS -o "$work/login.json" -w '%{http_code}' -c "$jar" \
    -D "$work/login.headers" \
    -X POST "$origin/api/auth/login" \
    -H 'content-type: application/json' -H "Origin: $origin" -d "$body")"

csrf_ok=1
if [ "$login_status" != "200" ]; then
    echo "FAIL: csrf     -- the login through Caddy answered $login_status"
    cat "$work/login.json"
    csrf_ok=0
fi

if ! grep -q 'cadus_session' "$jar"; then
    echo "FAIL: csrf     -- the login set no session cookie through Caddy"
    csrf_ok=0
fi

# The cookie posture of this origin, in the answer a browser reads.
#
# The stack runs on http, and this check exports CADUS_WEB_INSECURE_COOKIE=1 with
# it, which is the pair .env.example ships (M6 review, finding F14). The service
# must then serve the plain `cadus_session` name WITHOUT `Secure`. A browser
# discards a `Secure __Host-` cookie on http without a word, so the login answers
# 200 and the next authed write answers 401, and only the Set-Cookie line shows
# it.
cookie_line="$(grep -i '^set-cookie:.*cadus_session' "$work/login.headers" | head -n 1 | tr -d '\r')"
if [ -z "$cookie_line" ]; then
    echo "FAIL: cookie   -- the login answer carries no session Set-Cookie header"
    csrf_ok=0
else
    case "$cookie_line" in
        *__Host-*)
            printf 'FAIL: cookie   -- the http origin got a __Host- cookie, which the browser discards: %s\n' "$cookie_line"
            csrf_ok=0
            ;;
    esac
    case "$cookie_line" in
        *Secure*)
            printf 'FAIL: cookie   -- the http origin got a Secure cookie, which the browser discards: %s\n' "$cookie_line"
            csrf_ok=0
            ;;
    esac
    case "$cookie_line" in
        *HttpOnly*) ;;
        *)
            printf 'FAIL: cookie   -- the session cookie carries no HttpOnly: %s\n' "$cookie_line"
            csrf_ok=0
            ;;
    esac
fi

same_status="$(curl -sS -o "$work/same.json" -w '%{http_code}' -b "$jar" \
    -X POST "$origin/api/session/start" -H "Origin: $origin")"
cross_status="$(curl -sS -o "$work/cross.json" -w '%{http_code}' -b "$jar" \
    -X POST "$origin/api/session/start" -H 'Origin: https://evil.example')"

if [ "$same_status" != "200" ]; then
    echo "FAIL: csrf     -- the same-origin cookie POST answered $same_status, not 200"
    cat "$work/same.json"
    csrf_ok=0
fi

if [ "$cross_status" != "403" ]; then
    echo "FAIL: csrf     -- the cross-site cookie POST answered $cross_status, not 403"
    cat "$work/cross.json"
    csrf_ok=0
elif ! grep -q 'cross_origin_rejected' "$work/cross.json"; then
    echo "FAIL: csrf     -- the cross-site POST answered 403 with another code"
    cat "$work/cross.json"
    csrf_ok=0
fi

if [ "$csrf_ok" = "1" ]; then
    printf 'PASS: csrf     -- through Caddy, the cookie POST from %s is %s and the same cookie POST from https://evil.example is %s cross_origin_rejected; the http origin got %s\n' \
        "$origin" "$same_status" "$cross_status" "$cookie_line"
else
    rc=1
fi

exit "$rc"
