#!/usr/bin/env bash
# Part of scripts/check_ops.sh: check (e) deploy.
#
# scripts/check_ops.sh sources this file and calls its functions in order. The
# functions read and write the global variables of that script: `rc`, the
# image lists, and the plan. Run scripts/check_ops.sh, not this file.

# Write the `docker` stand-in of check (e) into the deploy sandbox.
write_docker_stub() {
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
}

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
check_deploy() {
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

    write_docker_stub

    deploy_rc=0
    deploy_out="$deploy_sandbox/out"
    deploy_log="$deploy_sandbox/state/log"

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
}

