#!/usr/bin/env bash
# Part of scripts/check_ops.sh: check (l) reload.
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

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

check_reload() {
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
}

