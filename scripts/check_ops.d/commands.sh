#!/usr/bin/env bash
# Part of scripts/check_ops.sh: check (d) commands.
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

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
check_commands() {
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
}

