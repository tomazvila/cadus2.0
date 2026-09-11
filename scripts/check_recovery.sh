#!/usr/bin/env bash
# Rehearse Cadus database recovery on the disposable local test cluster.
#
# This script creates three databases whose names start with `cadus2_recovery_`.
# It removes all three databases when it stops. It never accepts another cluster.
set -euo pipefail

for dir in "$HOME/.local/share/cadus2-tooling/gcc/bin" "$HOME/.cargo/bin"; do
    if [ -d "$dir" ]; then
        PATH="$dir:$PATH"
    fi
done
export PATH
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export SQLX_OFFLINE=true

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

container="${CADUS_RECOVERY_TEST_CONTAINER:-cadus2-testdb}"
if [ "$container" != "cadus2-testdb" ]; then
    echo "FAIL: safety -- CADUS_RECOVERY_TEST_CONTAINER must equal cadus2-testdb" >&2
    exit 2
fi
if [ "$(docker inspect -f '{{.State.Running}}' "$container" 2>/dev/null)" != "true" ]; then
    echo "FAIL: safety -- cadus2-testdb is not active" >&2
    exit 2
fi
published="$(docker port "$container" 5432/tcp 2>/dev/null)"
if [ "$published" != "127.0.0.1:55434" ]; then
    echo "FAIL: safety -- cadus2-testdb does not publish 127.0.0.1:55434" >&2
    exit 2
fi

run_id="${CADUS_RECOVERY_RUN_ID:-$$}"
if [[ ! "$run_id" =~ ^[0-9]+$ ]]; then
    echo "FAIL: safety -- CADUS_RECOVERY_RUN_ID must contain decimal digits" >&2
    exit 2
fi
source_db="cadus2_recovery_${run_id}_source"
restored_db="cadus2_recovery_${run_id}_restored"
rollback_db="cadus2_recovery_${run_id}_rollback"
scratch="$HOME/.cache/cadus2_recovery_${run_id}"
marker="$scratch/projector-stop.marker"
dump_file="$scratch/source.dump"
mkdir -p "$scratch"

admin_url() {
    printf 'postgresql://test:test@127.0.0.1:55434/%s' "$1"
}

app_url() {
    printf 'postgresql://cadus_app@127.0.0.1:55434/%s' "$1"
}

cluster_sql() {
    docker exec -i "$container" psql -X -v ON_ERROR_STOP=1 -U test -d postgres -Atqc "$1"
}

database_sql() {
    local database="$1"
    local statement="$2"
    docker exec -i "$container" psql -X -v ON_ERROR_STOP=1 -U test -d "$database" -Atqc "$statement"
}

drop_database() {
    local database="$1"
    cluster_sql "DROP DATABASE IF EXISTS \"$database\" WITH (FORCE)" >/dev/null
}

cleanup() {
    local result=$?
    trap - EXIT INT TERM
    drop_database "$rollback_db" || result=1
    drop_database "$restored_db" || result=1
    drop_database "$source_db" || result=1
    rm -rf "$scratch"
    exit "$result"
}
trap cleanup EXIT INT TERM

drop_database "$source_db"
drop_database "$restored_db"
drop_database "$rollback_db"
cluster_sql "CREATE DATABASE \"$source_db\"" >/dev/null

DATABASE_URL="$(admin_url "$source_db")" \
    cargo run --quiet -p cadus-store --bin cadus-migrate
echo "PASS: migrate -- the source database has all migrations"

run_probe() {
    local database="$1"
    local test_name="$2"
    CADUS_RECOVERY_ADMIN_URL="$(admin_url "$database")" \
    CADUS_RECOVERY_APP_URL="$(app_url "$database")" \
        cargo test --quiet -p cadus-store --test recovery_rehearsal "$test_name" -- \
        --ignored --exact --nocapture
}

run_probe "$source_db" seed_source_snapshot

event_fingerprint_sql="SELECT count(*)::text || '|' || COALESCE(md5(string_agg(user_id::text || ':' || seq::text || ':' || payload::text, E'\\n' ORDER BY user_id, seq)), md5('')) FROM events"
source_fingerprint="$(database_sql "$source_db" "$event_fingerprint_sql")"
if [ "${source_fingerprint%%|*}" != "268" ]; then
    echo "FAIL: seed -- the source event count differs from 268" >&2
    exit 1
fi

docker exec "$container" pg_dump -U test -d "$source_db" --format=custom --no-owner >"$dump_file"
if [ ! -s "$dump_file" ]; then
    echo "FAIL: backup -- pg_dump wrote an empty file" >&2
    exit 1
fi
docker exec -i "$container" pg_restore --list <"$dump_file" >"$scratch/dump.list"
if ! grep -q 'TABLE DATA public events' "$scratch/dump.list"; then
    echo "FAIL: backup -- the archive has no events table data" >&2
    exit 1
fi
echo "PASS: backup -- pg_dump wrote a readable custom archive"

restore_into() {
    local database="$1"
    cluster_sql "CREATE DATABASE \"$database\"" >/dev/null
    docker exec -i "$container" pg_restore -U test -d "$database" --no-owner \
        --exit-on-error <"$dump_file"
}

restore_into "$restored_db"
restored_fingerprint="$(database_sql "$restored_db" "$event_fingerprint_sql")"
if [ "$restored_fingerprint" != "$source_fingerprint" ]; then
    echo "FAIL: restore -- the restored event fingerprint differs" >&2
    exit 1
fi
stale_versions="$(database_sql "$restored_db" 'SELECT count(*) FROM learner_models WHERE projector_version = 6')"
if [ "$stale_versions" != "5" ]; then
    echo "FAIL: restore -- the restored snapshot has no five version-6 models" >&2
    exit 1
fi
echo "PASS: restore -- 268 events and five version-6 models match the source snapshot"

rm -f "$marker"
set +e
CADUS_RECOVERY_ADMIN_URL="$(admin_url "$restored_db")" \
CADUS_RECOVERY_APP_URL="$(app_url "$restored_db")" \
CADUS_RECOVERY_STOP_MARKER="$marker" \
    cargo test --quiet -p cadus-store --test recovery_rehearsal \
    stop_during_projector_transaction -- --ignored --exact --nocapture \
    >"$scratch/stop.log" 2>&1 &
cargo_pid=$!
set -e
for _ in $(seq 1 240); do
    if [ -s "$marker" ]; then
        break
    fi
    if ! kill -0 "$cargo_pid" 2>/dev/null; then
        cat "$scratch/stop.log" >&2
        echo "FAIL: process-stop -- the probe stopped before it wrote the marker" >&2
        wait "$cargo_pid" || true
        exit 1
    fi
    sleep 0.25
done
if [ ! -s "$marker" ]; then
    kill -TERM "$cargo_pid" 2>/dev/null || true
    wait "$cargo_pid" || true
    echo "FAIL: process-stop -- the probe did not write the marker in 60 seconds" >&2
    exit 1
fi
probe_pid="$(tr -d '[:space:]' <"$marker")"
if [[ ! "$probe_pid" =~ ^[0-9]+$ ]]; then
    kill -TERM "$cargo_pid" 2>/dev/null || true
    wait "$cargo_pid" || true
    echo "FAIL: process-stop -- the marker holds no process id" >&2
    exit 1
fi
probe_command="$(ps -p "$probe_pid" -o args= 2>/dev/null || true)"
if [[ "$probe_command" != *recovery_rehearsal* ]]; then
    kill -TERM "$cargo_pid" 2>/dev/null || true
    wait "$cargo_pid" || true
    echo "FAIL: process-stop -- the marked process is not the recovery probe" >&2
    exit 1
fi
kill -KILL "$probe_pid"
set +e
wait "$cargo_pid"
stopped_result=$?
set -e
if [ "$stopped_result" -eq 0 ]; then
    echo "FAIL: process-stop -- the stopped probe reported success" >&2
    exit 1
fi
after_stop_version="$(database_sql "$restored_db" "SELECT projector_version FROM learner_models WHERE user_id = '71000000-0000-0000-0000-000000000005'")"
after_stop_fingerprint="$(database_sql "$restored_db" "$event_fingerprint_sql")"
if [ "$after_stop_version" != "6" ] || [ "$after_stop_fingerprint" != "$source_fingerprint" ]; then
    echo "FAIL: process-stop -- the uncommitted projector transaction changed the snapshot" >&2
    exit 1
fi
echo "PASS: process-stop -- SIGKILL rolled back the projector write and preserved all events"

run_probe "$restored_db" retry_replays_all_samples_and_commits
run_probe "$restored_db" second_read_resumes_with_identical_models
current_versions="$(database_sql "$restored_db" 'SELECT count(*) FROM learner_models WHERE projector_version = 7')"
after_retry_fingerprint="$(database_sql "$restored_db" "$event_fingerprint_sql")"
if [ "$current_versions" != "5" ] || [ "$after_retry_fingerprint" != "$source_fingerprint" ]; then
    echo "FAIL: retry -- replay changed events or did not persist five version-7 models" >&2
    exit 1
fi
echo "PASS: retry -- five snapshot samples persist version 7 and a second read resumes identically"

if ! git diff --exit-code 2ff3c1f -- migrations >/dev/null; then
    echo "FAIL: rollback -- the release differs from 2ff3c1f under migrations/" >&2
    exit 1
fi
restore_into "$rollback_db"
rollback_fingerprint="$(database_sql "$rollback_db" "$event_fingerprint_sql")"
rollback_versions="$(database_sql "$rollback_db" 'SELECT count(*) FROM learner_models WHERE projector_version = 6')"
if [ "$rollback_fingerprint" != "$source_fingerprint" ] || [ "$rollback_versions" != "5" ]; then
    echo "FAIL: rollback -- the retained archive did not restore the pre-replay state" >&2
    exit 1
fi
echo "PASS: rollback -- migrations are unchanged and the retained archive restores the pre-replay state"
echo "RECOVERY OK"
