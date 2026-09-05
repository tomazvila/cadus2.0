#!/usr/bin/env bash
# Part of scripts/check_ops.sh: checks (c) binaries, (c2) curriculum, (c3) spa, and (c4) nonode.
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

# ---------------------------------------------------------------------------
# (c) the three binaries exist in every APP image the compose file builds
#
# The edge image is Caddy and a bundle; it carries none of the three and must
# not, so the loop reads `app_images` and never `build_plan`.
# ---------------------------------------------------------------------------
check_binaries() {
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
}

# ---------------------------------------------------------------------------
# (c2) the curriculum tree is in every APP image the compose file builds
#
# The tree is the input of the D-O4 pool refill. cadus-worker exits 2 when it
# does not load, so a missing COPY line turns every deployment into a worker
# restart loop. The check reads the path the Dockerfile sets as the default.
# ---------------------------------------------------------------------------
check_curriculum() {
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
}

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
check_spa() {
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
}

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
check_nonode() {
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
}

