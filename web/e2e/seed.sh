#!/usr/bin/env bash
# Seed the one account the authed click-through signs in as.
#
# The signup goes through the REAL route, so the password is hashed by the service's own
# Argon2 profile and the account is a normal one in every respect. Only the verification
# stamp is written directly: M5 sends no mail (`crates/web/src/auth/routes.rs`, "M5 has no
# unit that ..."), and the token in `auth_tokens` is stored hashed, so no link can be
# recovered from the database. Login is gated on `email_verified_at`, so the stamp is what
# the click-through needs and the only thing it takes.
#
#   e2e/seed.sh --api=http://127.0.0.1:8099 \
#               --psql="docker exec -i cadus2-testdb psql -U test -d cadus2_s13"
#
# It is idempotent: a second run signs up again (the service answers the same
# `verification_required` for a taken address) and stamps the same row.
set -euo pipefail

api="http://127.0.0.1:8099"
psql_cmd="docker exec -i cadus2-testdb psql -U test -d cadus2_s13"
email="click-through@cadus.local"
pass="a-long-enough-demo-password-2026"

for arg in "$@"; do
    case "$arg" in
        --api=*) api="${arg#--api=}" ;;
        --psql=*) psql_cmd="${arg#--psql=}" ;;
        --email=*) email="${arg#--email=}" ;;
        --pass=*) pass="${arg#--pass=}" ;;
        *) echo "seed.sh: unknown argument $arg" >&2; exit 2 ;;
    esac
done

echo "seed: signing up ${email} at ${api}"
curl -sS -X POST "${api}/api/auth/signup" \
    -H 'content-type: application/json' \
    -d "{\"email\":\"${email}\",\"password\":\"${pass}\"}"
echo

echo "seed: stamping the verification"
# The address is normalized to lower case before it is stored, so the WHERE clause matches
# the same form the service wrote.
$psql_cmd -v ON_ERROR_STOP=1 -tAc \
    "UPDATE users SET email_verified_at = now() WHERE email = lower('${email}') RETURNING id"

echo "seed: done — sign in as ${email}"
