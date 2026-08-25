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
   docker compose logs web | grep 'listening on'   # the server bound its port
   curl -fsS http://localhost/api/health   # from the host, through Caddy
   ```
   Run the `curl` command on the host, not in a container: the runtime image
   carries no `curl` and no `wget`. For a domain in `SITE_ADDRESS`, replace
   `http://localhost` with `https://<your domain>`.

   `cadus-web` writes the literal line `cadus-web: listening on <address>` at
   level `info` when it binds its port. That line is the check for the web
   tier: `web` publishes no port and carries no healthcheck, so
   `docker compose ps` reports `Up` from the moment the process is exec'd.

`migrate` is a one-shot: it runs `cadus-migrate --admin-login` and exits. `web`
and `worker` start only after it exits 0, so the schema is never behind the
code.

## Upgrade

Step 5 above is for the FIRST bring-up. To upgrade a stack that already serves
traffic, pull the new commit and run the upgrade script:

```sh
git pull
scripts/deploy.sh
```

`scripts/deploy.sh` is THE upgrade procedure. It does four steps in this order:

1. `docker compose build` -- the running containers keep the old image.
2. `docker compose up -d db`, then a wait for the healthcheck.
3. `docker compose run --rm migrate` -- a non-zero exit stops the script, and
   the old `web` and `worker` still serve traffic on the old schema.
4. `docker compose up -d --no-deps web worker caddy` -- the new image takes
   over. The script then waits up to 30 s for three facts: `web` reports state
   `running`, `worker` reports state `running`, and `docker compose logs web`
   holds the line `listening on`. If the deadline passes, the script prints the
   last 40 log lines of the service that failed and exits 1.

`docker compose up -d` returns 0 as soon as the containers START, not when they
stay up, so step 4 does its own check. A `web` that reads a bad value out of
`.env` exits 2 before it binds and `restart: unless-stopped` restarts it without
end; the old script printed `DEPLOY OK` over a site that answered every visitor
with 502 (review round 4, finding #13). A failed step 4 leaves the new schema in
place: correct the fault and run the script again.

`scripts/deploy.sh --no-caddy` starts `web` and `worker` only and leaves `caddy`
alone. `DEPLOY_SKIP_CADDY=1 scripts/deploy.sh` does the same. Use it on a stack
that terminates TLS somewhere else, and in a test bring-up that binds no port
80. Any other argument stops the script with exit 2.

WARNING: Do not upgrade an existing stack with `docker compose up -d`. Compose
creates every container first and starts them second, so it destroys the
serving `web` and `worker` BEFORE `migrate` runs. A migration that then fails
leaves both in state `Created`. `restart: unless-stopped` gives no recovery,
because Docker never started them, and `docker compose start web` refuses while
the `service_completed_successfully` dependency is unsatisfied. The site is down
and stays down (review round 3, finding #16). The script keeps the old version
up until the new schema is in place.

If step 3 fails, read the output of `migrate`, correct the migration, and run
the script again. The site serves the old version for the whole time.

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
`.env` reaches the database on the next upgrade.
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
2. Run `scripts/deploy.sh`.

`migrate` runs `ALTER ROLE ... PASSWORD` for both roles and `web` and `worker`
start with the new DSN. No manual SQL is needed. Use the script, not
`docker compose up -d`: a changed password changes the DSN of `web` and
`worker`, so compose recreates both containers before `migrate` runs. See
"Upgrade" above.

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
4. Run `scripts/deploy.sh`.
5. Do a check: `docker compose ps` shows `migrate` exited 0, and `web` and
   `worker` up.

### Recover a rotation that ran in the wrong order

If `.env` already carries the new value and `migrate` exits 2 with
`password authentication failed for user "postgres"`, the database still holds
the OLD password. Write the new one into the role and start the stack again:

```sh
docker compose exec db psql -U postgres -c "ALTER ROLE postgres PASSWORD '<the value now in .env>'"
scripts/deploy.sh
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
- **The upgrade (D9).** `scripts/deploy.sh` runs the four steps of the
  "Upgrade" section above in order. It aborts on a `migrate` that exits
  non-zero and leaves the running site alone. It aborts on a `web` or `worker`
  that does not reach state `running` within 30 s, or on a `web` that never logs
  `listening on`, and prints the last 40 log lines of that service.
- **The ops surface (U6).** `scripts/check_ops.sh` runs the operator's own
  commands: `docker compose config` resolves `docker-compose.yml`, and
  `docker compose build` builds every service that has a `build:` section. It
  then runs a container from each built image and proves that `cadus-web`,
  `cadus-worker`, and `cadus-migrate` are on the `PATH` there. For every service
  that builds the app image it proves three more facts: the service HAS a
  `command:`, its first token names one of the three binaries and exists in the
  image, and every further token is in the allowlist of that binary
  (`cadus-migrate` takes `--admin-login`; `cadus-web` and `cadus-worker` take no
  argument). A renamed binary target, a wrong `dockerfile:` key, a deleted
  `command:`, or a mistyped flag then fails the gate instead of the operator's
  next bring-up (review round 2, finding #12; review round 4, finding #9).
- **Latency and token budgets (L\*, T\*).** The benchmarks land with M4 and M5
  and run in the same gate job. Model calls run in the worker (R4). The gate
  reads the RESOLVED dependency graph from `cargo metadata` (`tests/purity.rs`
  in `crates/web` and in `crates/worker`). It walks the normal dependency
  closure of `cadus-web` and of `cadus-worker` across every target platform and
  rejects `reqwest`, `ureq`, `isahc`, `curl`, `tokio-tungstenite`, and the model
  SDKs anywhere in it, so a client inside `cadus-store` or under a
  `[target.'cfg(...)'.dependencies]` table fails the gate too (review round 4,
  finding #6). Both tests also pin the literal list of DIRECT normal
  dependencies of `cadus-web`, `cadus-worker`, and `cadus-store`, so any new
  dependency of the three tier crates is a reviewable diff. The gate does not
  inspect handler bodies.
- **Runtime.** `/api/ready` runs one `SELECT 1` through the web pool. It reports
  the datastore only, and it reports nothing about `cadus-worker`: a 200 from
  `/api/ready` is no proof that the async layer runs. M0 gives the worker no
  monitoring endpoint; read `docker compose logs worker` instead. Caddy 404s
  `/api/ready` at the edge on purpose; scrape it over the compose network at
  `web:8080`.

## Query bound

`DB_STATEMENT_TIMEOUT_MS` (default `5000`) bounds every query of the web and
worker pools through `statement_timeout`. Set `0` to remove the bound. A value
that is not a whole number stops `cadus-web` and `cadus-worker` at start with a
configuration error. sqlx 0.9 exposes no TCP keepalive, so a socket that a
firewall drops silently is not bounded by this setting.

The bound covers the web and worker pools; `cadus-migrate` ignores it. A
migration runs without a statement bound, because a migration takes as long as
it takes. A 5 s bound cancelled a slow `CREATE INDEX` or a DDL statement that
waited for a lock, exited the one-shot 2, and took the whole stack down (review
round 3, finding #1). `docker-compose.yml` therefore does not forward
`DB_STATEMENT_TIMEOUT_MS` to the `migrate` service.

## Stop budget

`SHUTDOWN_DEADLINE_SECS` (default `10`) is ONE budget for the whole stop of
`cadus-web`. After `SIGTERM` the drain of the open requests gets the budget, and
the pool close gets what is left of it, at least 1 second. The total stop time
is therefore `SHUTDOWN_DEADLINE_SECS` + 1 s or less, which stays inside the
`stop_grace_period` of 20 s that `docker-compose.yml` sets, so Docker never
sends `SIGKILL` and the container reports exit 0. The old code spent the budget
twice and took 20.01 s at the default, which gave exit 137 on every restart
(review round 3, finding #7). Keep `SHUTDOWN_DEADLINE_SECS` below 19, or raise
`stop_grace_period` with it.

## Role lock database

`cadus-migrate --admin-login` alters cluster-scoped roles. Two migrate runs on one
cluster serialize on an advisory lock that lives in `CADUS_MAINTENANCE_DB` (default
`postgres`), because Postgres scopes an advisory lock to one database. The migrate
role must be able to connect to that database. `cadus-migrate` ignores
`DB_STATEMENT_TIMEOUT_MS` and runs every migration without a statement bound.

## Password rule and exit codes

`CADUS_APP_PASSWORD` and `CADUS_ADMIN_PASSWORD` hold 16 to 128 characters of the set
`A-Z a-z 0-9 _ -` only, because the compose DSNs carry the raw value inside a URL.
`openssl rand -hex 24` satisfies the rule. `cadus-migrate` refuses any other value
with exit code 2 before it runs a statement.

| Binary | 0 | 2 | 3 |
|---|---|---|---|
| `cadus-web` | clean stop | start error (config, connect, bind) | boot guard: the role bypasses RLS (C3) |
| `cadus-worker` | clean stop | start error | — |
| `cadus-migrate` | done | error or bad argument | stopped by signal |
