# Self-host Cadus 2.0

One server, one `docker compose` stack. No cloud vendor, no managed service.

## Bring-up

1. Install Docker Engine with the Compose plugin, then clone this repository.
2. `cp .env.example .env`. The copied file carries `SITE_ADDRESS=:80`, which
   serves http only and suits a test on a bare IP. For a real site, set
   `SITE_ADDRESS` to your domain. Every key in the `Required` block of `.env`
   has no default: each `docker compose` command fails with
   `required variable <KEY> is missing a value` until you set the key. A silent
   fallback on a domain that the operator believes is on HTTPS is worse than a
   loud stop.
3. Set the three passwords in `.env`. Generate each one separately:
   ```sh
   openssl rand -hex 24
   ```
   `POSTGRES_PASSWORD` is the superuser password. `CADUS_APP_PASSWORD` and
   `CADUS_ADMIN_PASSWORD` are the passwords of the two runtime roles. Hex output
   needs no percent-encoding inside a DSN.
4. For a domain, point its DNS record at this server and open ports 80 and 443.
   Caddy then gets a Let's Encrypt certificate by itself.
5. `docker compose up -d --build`
6. Do a check of the bring-up:
   ```sh
   docker compose ps            # db healthy, migrate exited 0, web and worker up
   docker compose logs migrate  # every migration applied, or nothing to apply
   docker compose logs web      # the line `listening on` means the server is up
   curl -fsS http://localhost/api/health   # from the host, through Caddy
   ```
   Run the `curl` command on the host, not in a container: the runtime image
   carries no `curl` and no `wget`. For a domain in `SITE_ADDRESS`, replace
   `http://localhost` with `https://<your domain>`.

`migrate` is a one-shot: it runs `cadus-migrate --admin-login` and exits. `web`
and `worker` start only after it exits 0, so the schema is never behind the
code. To upgrade, pull the new commit and run step 5 again; the migrate step
applies only what is new.

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
deployment decision. The `migrate` service therefore runs three more statements
after the migrations: `ALTER ROLE cadus_app PASSWORD ...`,
`ALTER ROLE cadus_admin PASSWORD ...`, and `ALTER ROLE cadus_admin LOGIN`. The
`--admin-login` flag of `cadus-migrate` runs them from `CADUS_APP_PASSWORD` and
`CADUS_ADMIN_PASSWORD`, so the runtime image carries no `psql` and no other
database client. A migration holds no password, because a password is
deployment state, not a schema fact. Every statement is idempotent, so a re-run
is a no-op and a changed `CADUS_APP_PASSWORD` or `CADUS_ADMIN_PASSWORD` in
`.env` reaches the database on the next `docker compose up -d`.
`POSTGRES_PASSWORD` is the exception: see "Rotate a password" below.
`cadus-migrate` with any other argument prints its usage and exits 2.

In M0 the worker process holds `BYPASSRLS` for its whole life. It runs no
`SET ROLE`, so RLS is not a backstop inside the worker. The M0 worker writes no
tenant row: it ticks and runs a heartbeat query. The per-tenant `SET ROLE
cadus_app` lands with M5, together with the first cross-tenant sweep. Until
then, treat every worker statement as cross-tenant by default and do a review of
each new one for its tenant predicate.

The database publishes no port and sits on the private `backend` network. Caddy
sits on `frontend` only and has no route to it. The network segment is not the
only control: every DSN carries a password, and the `db` service runs without
`POSTGRES_HOST_AUTH_METHOD: trust`. Trust auth accepts every process that
reaches the port, and a container network is not an authentication boundary.

## Rotate a password

`.env` holds three passwords, and they do not behave the same way.

| Key | Role | Reaches the database through |
|---|---|---|
| `CADUS_APP_PASSWORD` | `cadus_app` | `cadus-migrate --admin-login`, on every bring-up |
| `CADUS_ADMIN_PASSWORD` | `cadus_admin` | `cadus-migrate --admin-login`, on every bring-up |
| `POSTGRES_PASSWORD` | `postgres` (superuser) | initdb only, on the FIRST start |

### The two runtime passwords

1. Put the new value in `.env`. Generate it with `openssl rand -hex 24`.
2. Run `docker compose up -d`.

`migrate` runs `ALTER ROLE ... PASSWORD` for both roles and `web` and `worker`
start with the new DSN. No manual SQL is needed.

### The superuser password

WARNING: Do not rotate `POSTGRES_PASSWORD` in `.env` alone. The value is
write-once. The `postgres:16` image reads it at initdb, that is on the first
start with an empty `db-data` volume. On an existing volume the `db` container
ignores a new value, but compose still puts
that value into the `migrate` DSN. A rotation in `.env` alone therefore
recreates `web` and `worker`, stops `migrate` with
`password authentication failed for user "postgres"`, and takes the site down
(review round 2, finding #13).

Change the role first and `.env` second:

1. Generate the new value:
   ```sh
   openssl rand -hex 24
   ```
2. Write it into the running role:
   ```sh
   docker compose exec db psql -U postgres -c "ALTER ROLE postgres PASSWORD '<new value>'"
   ```
   The same command form rotates a runtime role, for example
   `ALTER ROLE cadus_app PASSWORD '<new value>'`, but the two runtime roles need
   no manual step: `migrate` writes them from `.env`.
3. Put the same value into `POSTGRES_PASSWORD` in `.env`.
4. Run `docker compose up -d`.
5. Do a check: `docker compose ps` shows `migrate` exited 0, and `web` and
   `worker` up.

### Recover a rotation that ran in the wrong order

If `.env` already carries the new value and `migrate` exits 2 with
`password authentication failed for user "postgres"`, the database still holds
the OLD password. Write the new one into the role and start the stack again:

```sh
docker compose exec db psql -U postgres -c "ALTER ROLE postgres PASSWORD '<the value now in .env>'"
docker compose up -d
```

`docker compose exec db psql -U postgres` needs no password: the image writes
`local all all trust` into `pg_hba.conf`, and `exec` runs inside the container.

CAUTION: Do not run `docker compose down -v` to recover. That command deletes the
`db-data` volume and every learner row with it. No data loss is needed here: the
steps above keep the volume.

## Where the budgets are checked

- **The gate.** `scripts/gate.sh` runs fmt, clippy with `-D warnings`, the tests,
  `cargo sqlx prepare --check`, `scripts/check_migrations.sh`, and
  `scripts/check_ops.sh`. It is the merge gate on a laptop and in CI
  (`.github/workflows/ci.yml`, every push and pull request). It needs
  `CADUS_TEST_DATABASE_URL` and `docker`: it exits 2 and runs no check when the
  variable is unset, and it fails with `GATE FAILED: docker is required` when
  docker is absent. See `README.md` for the one-time database setup.
- **Migration discipline (D9).** `scripts/check_migrations.sh` proves the file
  names run `0001`, `0002`, ... with no gap, that every shipped migration still
  matches `migrations/CHECKSUMS`, that a fresh database takes every migration,
  and that a second run applies nothing. Migrations are forward-only: an edit to
  a file that a deployment already applied stops the `migrate` service on the
  next upgrade, so the checksum record fails the gate first.
- **The ops surface (U6).** `scripts/check_ops.sh` runs the operator's own
  commands: `docker compose config` resolves `docker-compose.yml`, and
  `docker compose build` builds every service that has a `build:` section. It
  then runs a container from each built image and proves that `cadus-web`,
  `cadus-worker`, and `cadus-migrate` are on the `PATH` there, and that every
  `command:` of the compose file names a binary the image carries. A renamed
  binary target, a wrong `dockerfile:` key, or a mistyped `command:` then fails
  the gate instead of the operator's next bring-up (review round 2, finding
  #12).
- **Latency and token budgets (L\*, T\*).** The benchmarks land with M4 and M5
  and run in the same gate job. Model calls run in the worker (R4). The gate pins the
  dependency lists of `cadus-web` and `cadus-worker` (`tests/purity.rs` in each
  crate): a new HTTP-client or model-SDK dependency on either crate is a test
  failure and a reviewable diff. The gate does not inspect handler bodies.
- **Runtime.** `/api/ready` runs one `SELECT 1` through the web pool. It reports
  the datastore only, and it reports nothing about `cadus-worker`: a 200 from
  `/api/ready` is no proof that the async layer runs. M0 gives the worker no
  monitoring endpoint; read `docker compose logs worker` instead. Caddy 404s
  `/api/ready` at the edge on purpose; scrape it over the compose network at
  `web:8080`.

## Query bound

`DB_STATEMENT_TIMEOUT_MS` (default `5000`) bounds every query of the web and worker
pools through `statement_timeout`. Set `0` to remove the bound. A value that is not a
whole number stops `cadus-web` and `cadus-worker` at start with a configuration error.
sqlx 0.9 exposes no TCP keepalive, so a socket that a firewall drops silently is not
bounded by this setting.
