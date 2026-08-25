#!/usr/bin/env bash
# Cadus 2.0 migration check (D9). Run it from any directory.
#
# Usage:
#   scripts/check_migrations.sh            run every check
#   scripts/check_migrations.sh --freeze   rewrite migrations/CHECKSUMS
#
# The script does five checks and prints one line per check:
#   (a) name    -- every SQL file in migrations/ matches
#                  ^[0-9]{4}_[a-z0-9_]+\.sql$, and the numbers run 0001, 0002,
#                  ... with no gap and no repeat.
#   (b) frozen  -- every SQL file in migrations/ matches its line in
#                  migrations/CHECKSUMS, and every SQL file has a line there.
#   (c) fresh   -- a new database takes every migration, and `migrate info`
#                  reports every migration as installed and none as pending.
#   (d) rerun   -- a second `migrate run` on that database applies nothing.
#   (e) drop    -- the script drops the database it made, also after a failure.
#
# Why (b) exists: migrations are forward-only. `sqlx` stores the checksum of
# each applied migration in `_sqlx_migrations` and refuses a file that changed
# after it ran. Check (c) works on a database that is new every time, so it
# compares no checksum with anything and an edited shipped migration passes it.
# The failure then lands on the operator's server, where the `migrate` service
# exits non-zero and `web` and `worker` never start. `migrations/CHECKSUMS` is
# the record that makes the edit fail here instead.
#
# To add a migration: write `migrations/NNNN_name.sql`, then run
# `scripts/check_migrations.sh --freeze` and commit `migrations/CHECKSUMS` with
# it. NEVER run `--freeze` to silence a failure on a migration that a
# deployment already applied. Write a new migration instead.
#
# Input: DATABASE_URL, a superuser DSN. Checks (c), (d), and (e) need it;
# `--freeze`, (a), and (b) do not. The script reads the database name in that
# DSN as a BASE name only. It never touches the base database. It derives
# `<base>_migcheck_<pid>` and works on that database. The PID keeps two runs on
# one cluster apart: a fixed name lets one run drop the database of another.
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

checksum_file="migrations/CHECKSUMS"

# Collect the SQL migration files, sorted by name. CHECKSUMS is the record of
# this list, so the list never holds it.
sql_files=()
while IFS= read -r name; do
    sql_files+=("$name")
done < <(find migrations -maxdepth 1 -type f -name '*.sql' -printf '%f\n' | LC_ALL=C sort)

freeze=0
case "${1:-}" in
    --freeze) freeze=1 ;;
    "") ;;
    *)
        echo "FAIL: input   -- unknown argument: $1" >&2
        echo "usage: scripts/check_migrations.sh [--freeze]" >&2
        exit 2
        ;;
esac

# ---------------------------------------------------------------------------
# --freeze: rewrite the record, then stop
# ---------------------------------------------------------------------------
if [ "$freeze" -eq 1 ]; then
    if [ "${#sql_files[@]}" -eq 0 ]; then
        echo "FAIL: freeze  -- migrations/ holds no .sql file" >&2
        exit 2
    fi
    printf '%s\n' \
        "# Cadus 2.0 migration checksums (D9). Migrations are forward-only." \
        "# scripts/check_migrations.sh reads this file and fails when a listed" \
        "# migration changed or when a migration has no line here." \
        "# Rewrite it with: scripts/check_migrations.sh --freeze" \
        > "$checksum_file"
    for name in "${sql_files[@]}"; do
        sha256sum "migrations/$name" >> "$checksum_file"
    done
    echo "FROZE: ${#sql_files[@]} migrations recorded in $checksum_file"
    exit 0
fi

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

# The PID makes the name unique per run. Two gate runs on one cluster then never
# drop each other's database.
fresh_db="${base_db}_migcheck_$$"
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

# ---------------------------------------------------------------------------
# (a) file-name discipline
# ---------------------------------------------------------------------------
name_ok=1

if [ "${#sql_files[@]}" -eq 0 ]; then
    echo "FAIL: name    -- migrations/ holds no .sql file"
    name_ok=0
else
    index=0
    for name in "${sql_files[@]}"; do
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
    echo "PASS: name    -- ${#sql_files[@]} migration files, numbered 0001 to $(printf '%04d' "${#sql_files[@]}") with no gap"
else
    echo "SKIP: frozen  -- the name check failed"
    echo "SKIP: fresh   -- the name check failed"
    echo "SKIP: rerun   -- the name check failed"
    echo "SKIP: drop    -- the name check failed"
    exit 1
fi

want_count="${#sql_files[@]}"

# ---------------------------------------------------------------------------
# (b) frozen: the shipped migrations never change
# ---------------------------------------------------------------------------
frozen_ok=1
if [ ! -f "$checksum_file" ]; then
    echo "FAIL: frozen  -- $checksum_file does not exist: run scripts/check_migrations.sh --freeze"
    frozen_ok=0
else
    verify_log=""
    if ! verify_log="$(sha256sum -c --quiet "$checksum_file" 2>&1)"; then
        echo "FAIL: frozen  -- a shipped migration changed or is missing. Migrations are forward-only: add a new file instead of editing one."
        printf '%s\n' "$verify_log"
        frozen_ok=0
    fi
    # `sha256sum -c` reads the list, so it never sees a file that the list
    # leaves out. Do a check of the other direction here.
    for name in "${sql_files[@]}"; do
        if ! grep -q "  migrations/$name\$" "$checksum_file"; then
            echo "FAIL: frozen  -- migrations/$name has no line in $checksum_file: run scripts/check_migrations.sh --freeze"
            frozen_ok=0
        fi
    done
fi

if [ "$frozen_ok" -eq 1 ]; then
    echo "PASS: frozen  -- $want_count migrations match $checksum_file"
else
    echo "SKIP: fresh   -- the frozen check failed"
    echo "SKIP: rerun   -- the frozen check failed"
    echo "SKIP: drop    -- the frozen check failed"
    exit 1
fi

# From here on the script owns a database, so the trap must drop it.
trap cleanup EXIT

# ---------------------------------------------------------------------------
# (c) fresh path
# ---------------------------------------------------------------------------
sqlx_db drop -y >/dev/null 2>&1 || true

fresh_ok=1
fresh_log=""
if ! fresh_log="$(sqlx_db create 2>&1)"; then
    echo "FAIL: fresh   -- the create of database $fresh_db failed: $fresh_log"
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
# (d) re-run path
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
