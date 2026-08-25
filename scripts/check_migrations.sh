#!/usr/bin/env bash
# Cadus 2.0 migration check (D9). Run it from any directory.
#
# The script does four checks and prints one line per check:
#   (a) name    -- every file in migrations/ matches ^[0-9]{4}_[a-z0-9_]+\.sql$,
#                  and the numbers run 0001, 0002, ... with no gap and no repeat.
#   (b) fresh   -- a new database takes every migration, and `migrate info`
#                  reports every migration as installed and none as pending.
#   (c) rerun   -- a second `migrate run` on that database applies nothing.
#   (d) drop    -- the script drops the database it made, also after a failure.
#
# Input: DATABASE_URL, a superuser DSN. The script reads the database name in
# that DSN as a BASE name only. It never touches the base database. It derives
# `<base>_migcheck_fresh` and works on that database.
#
# The script uses `cargo sqlx database create` and `cargo sqlx database drop`.
# They need no psql client. If a later check needs raw SQL and psql is not on
# PATH, use `docker exec -i cadus2-testdb psql -U test -d <db>` as the fallback.
set -euo pipefail

# Put the project toolchain first, if it is installed on this machine.
for dir in "$HOME/.local/share/cadus2-tooling/gcc/bin" "$HOME/.cargo/bin"; do
    if [ -d "$dir" ]; then
        PATH="$dir:$PATH"
    fi
done
export PATH

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [ -z "${DATABASE_URL:-}" ]; then
    echo "FAIL: input   -- DATABASE_URL is not set" >&2
    exit 2
fi

# Derive the work DSN. Split the query string off first, then the last path
# segment. The last path segment is the base database name.
dsn_no_query="${DATABASE_URL%%\?*}"
dsn_query="${DATABASE_URL#"$dsn_no_query"}"
base_db="${dsn_no_query##*/}"
dsn_prefix="${dsn_no_query%/*}"

if [ -z "$base_db" ]; then
    echo "FAIL: input   -- DATABASE_URL names no database: $DATABASE_URL" >&2
    exit 2
fi

fresh_db="${base_db}_migcheck_fresh"
fresh_url="${dsn_prefix}/${fresh_db}${dsn_query}"

rc=0

sqlx_db() { # sqlx_db create|drop [extra args]
    local action="$1"
    shift
    DATABASE_URL="$fresh_url" cargo sqlx database "$action" --no-dotenv "$@"
}

# The EXIT trap invokes cleanup. shellcheck does not see an indirect call.
# shellcheck disable=SC2329
cleanup() {
    local exit_code=$?
    if sqlx_db drop -y >/dev/null 2>&1; then
        echo "PASS: drop    -- dropped database $fresh_db"
    else
        echo "FAIL: drop    -- database $fresh_db is still present"
        exit_code=1
    fi
    exit "$exit_code"
}
trap cleanup EXIT

# ---------------------------------------------------------------------------
# (a) file-name discipline
# ---------------------------------------------------------------------------
name_ok=1
files=()
while IFS= read -r name; do
    files+=("$name")
done < <(find migrations -maxdepth 1 -type f -printf '%f\n' | LC_ALL=C sort)

if [ "${#files[@]}" -eq 0 ]; then
    echo "FAIL: name    -- migrations/ holds no file"
    name_ok=0
else
    index=0
    for name in "${files[@]}"; do
        index=$((index + 1))
        if [[ ! "$name" =~ ^[0-9]{4}_[a-z0-9_]+\.sql$ ]]; then
            echo "FAIL: name    -- bad file name: migrations/$name"
            name_ok=0
            continue
        fi
        want="$(printf '%04d' "$index")"
        got="${name:0:4}"
        if [ "$got" != "$want" ]; then
            echo "FAIL: name    -- migrations/$name breaks the sequence: expected number $want, got $got"
            name_ok=0
        fi
    done
fi

if [ "$name_ok" -eq 1 ]; then
    echo "PASS: name    -- ${#files[@]} migration files, numbered 0001 to $(printf '%04d' "${#files[@]}") with no gap"
else
    rc=1
    echo "SKIP: fresh   -- the name check failed"
    echo "SKIP: rerun   -- the name check failed"
    exit "$rc"
fi

want_count="${#files[@]}"

# ---------------------------------------------------------------------------
# (b) fresh path
# ---------------------------------------------------------------------------
sqlx_db drop -y >/dev/null 2>&1 || true

fresh_ok=1
fresh_log=""
if ! fresh_log="$(sqlx_db create 2>&1)"; then
    echo "FAIL: fresh   -- cannot create database $fresh_db: $fresh_log"
    fresh_ok=0
elif ! fresh_log="$(DATABASE_URL="$fresh_url" cargo sqlx migrate run --no-dotenv 2>&1)"; then
    echo "FAIL: fresh   -- migrate run failed on $fresh_db: $fresh_log"
    fresh_ok=0
else
    info_log=""
    if ! info_log="$(DATABASE_URL="$fresh_url" cargo sqlx migrate info --no-dotenv 2>&1)"; then
        echo "FAIL: fresh   -- migrate info failed on $fresh_db: $info_log"
        fresh_ok=0
    else
        installed="$(printf '%s\n' "$info_log" | grep -c '/installed' || true)"
        pending="$(printf '%s\n' "$info_log" | grep -c '/pending' || true)"
        if [ "$installed" -ne "$want_count" ] || [ "$pending" -ne 0 ]; then
            echo "FAIL: fresh   -- migrate info reports $installed installed and $pending pending, expected $want_count installed and 0 pending"
            printf '%s\n' "$info_log"
            fresh_ok=0
        fi
    fi
fi

if [ "$fresh_ok" -eq 1 ]; then
    echo "PASS: fresh   -- $want_count migrations applied on $fresh_db, 0 pending"
else
    rc=1
    echo "SKIP: rerun   -- the fresh check failed"
    exit "$rc"
fi

# ---------------------------------------------------------------------------
# (c) re-run path
# ---------------------------------------------------------------------------
rerun_log=""
rerun_status=0
rerun_log="$(DATABASE_URL="$fresh_url" cargo sqlx migrate run --no-dotenv 2>&1)" || rerun_status=$?

if [ "$rerun_status" -ne 0 ]; then
    echo "FAIL: rerun   -- the second migrate run exited $rerun_status: $rerun_log"
    rc=1
elif printf '%s\n' "$rerun_log" | grep -q 'Applied'; then
    echo "FAIL: rerun   -- the second migrate run applied a migration: $rerun_log"
    rc=1
else
    echo "PASS: rerun   -- the second migrate run applied nothing"
fi

exit "$rc"
