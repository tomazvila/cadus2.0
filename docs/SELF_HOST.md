# Self-host Cadus 2.0

One server, one `docker compose` stack. No cloud vendor, no managed service.

## Bring-up

1. Install Docker Engine with the Compose plugin, then clone this repository.
2. `cp .env.example .env`, and set `SITE_ADDRESS` to your domain. Leave the
   default `:80` for an http-only test on a bare IP.
3. For a domain, point its DNS record at this server and open ports 80 and 443.
   Caddy then gets a Let's Encrypt certificate by itself.
4. `docker compose up -d --build`
5. Do a check of the bring-up:
   ```sh
   docker compose ps            # db healthy, migrate exited 0, web and worker up
   docker compose logs migrate  # every migration applied, or nothing to apply
   docker compose exec web curl -fsS http://127.0.0.1:8080/api/health
   ```

`migrate` is a one-shot: it runs `cadus-migrate` and exits. `web` and `worker`
start only after it exits 0, so the schema is never behind the code. To upgrade,
pull the new commit and run step 4 again; the migrate step applies only what is
new.

## The role model (C3)

Migration `0001_roles` creates three cluster roles. Each process uses exactly
one. The DSNs are fixed in `docker-compose.yml`, because the role split is the
tenant-isolation control, not a preference.

| Service | Role | Rights |
|---|---|---|
| `migrate` | `postgres` (superuser) | DDL, extensions, role creation |
| `web` | `cadus_app` | RLS applies; no `UPDATE`/`DELETE`/`TRUNCATE` on `events` |
| `worker` | `cadus_admin` | `BYPASSRLS` for cross-tenant sweeps; member of `cadus_app` |

Three rules hold this together. The event log is append-only: migration
`0006_grants_rls` revokes `UPDATE`, `DELETE`, and `TRUNCATE` on `events` from
`cadus_app`, and a store test asserts the literal SQLSTATE `42501`. Every tenant
table carries `FORCE ROW LEVEL SECURITY` and the `tenant_isolation` policy, so
`cadus_app` reads only the current tenant. `cadus-web` refuses to start on a DSN
whose role bypasses RLS — never point `web` at the superuser DSN.

`cadus_admin` is `NOLOGIN` in the schema, because a `BYPASSRLS` login is a
deployment decision. The `migrate` service therefore runs one more statement
after the migrations: `ALTER ROLE cadus_admin LOGIN`. It is idempotent. The
worker does `SET ROLE cadus_app` inside each per-tenant unit of work, so RLS
stays a backstop there.

The database publishes no port and sits on the private `backend` network. Caddy
sits on `frontend` only and has no route to it. `trust` auth is safe on that
segment, so no DSN carries a password.

## Where the budgets are checked

- **The gate.** `scripts/gate.sh` runs fmt, clippy with `-D warnings`, the tests,
  `cargo sqlx prepare --check`, and `scripts/check_migrations.sh`. It is the
  merge gate on a laptop and in CI (`.github/workflows/ci.yml`, every push and
  pull request).
- **Migration discipline (D9).** `scripts/check_migrations.sh` proves the file
  names run `0001`, `0002`, ... with no gap, that a fresh database takes every
  migration, and that a second run applies nothing.
- **Latency and token budgets (L\*, T\*).** The benchmarks land with M4 and M5
  and run in the same gate job. Model calls run in the worker (R4); a change that
  puts one on a request path does not merge.
- **Runtime.** `/api/ready` reports datastore and worker state. Caddy 404s it at
  the edge on purpose; scrape it over the compose network at `web:8080`.
