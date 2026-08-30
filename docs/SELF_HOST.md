# Self-host Cadus 2.0

One server, one `docker compose` stack. No cloud vendor, no managed service.

## Bring-up

1. Install Docker Engine with the Compose plugin, then clone this repository.
2. `cp .env.example .env`. The copied file carries the http pair
   `SITE_ADDRESS=:80` and `CADUS_WEB_INSECURE_COOKIE=1`, which serves http only
   and suits a test on a bare IP. For a real site, set `SITE_ADDRESS` to your
   domain AND set `CADUS_WEB_INSECURE_COOKIE=0`. The two keys are one decision;
   see "The cookie posture" below. Every key in the `Required` block of `.env`
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
   Caddy then gets a Let's Encrypt certificate by itself. If another stack on
   this server already holds port 80 or port 443, move the HOST ports of Caddy
   with `CADDY_HTTP_PORT` and `CADDY_HTTPS_PORT`. See "Proxy ports" below.
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
   over. The script then applies the Caddyfile of this commit (below) and waits
   up to 30 s for these facts: `web`, `worker`, and `caddy` each report container
   state `running`; none of the three restarts while the check runs;
   `docker compose logs web` holds the line `listening on`; and a recreated
   `caddy` logs `serving initial configuration`. The facts must hold twice, 2 s
   apart. If the deadline passes, the script names the CONTAINER that failed,
   prints its last 40 log lines, and exits 1.

`docker compose up -d` returns 0 as soon as the containers START, not when they
stay up, so step 4 does its own check. A `web` that reads a bad value out of
`.env` exits 2 before it binds and `restart: unless-stopped` restarts it without
end; the old script printed `DEPLOY OK` over a site that answered every visitor
with 502 (review round 4, finding #13). The check covers `caddy` for the same
reason: `caddy` carries no healthcheck, so a Caddyfile that Caddy refuses left
the sole ingress in a restart loop under a `DEPLOY OK` line (M6 review, finding
F11). A failed step 4 leaves the new schema in place: correct the fault and run
the script again.

The script has NO flag that skips `caddy`. The `spa` image bakes the built SPA
bundle into that container (M6 S14), so `caddy` serves every byte of the front
end. An upgrade that starts the new `web` and leaves `caddy` alone pins the
browser to the previous commit's bundle against the new API (M6 review, finding
F24). `scripts/deploy.sh --no-caddy` and `DEPLOY_SKIP_CADDY=1` are gone, and each
stops the script with exit 2. Any other argument stops it with exit 2 as well.

`deploy/Caddyfile` is a bind mount, and Caddy reads its configuration once, at
start. A commit that edits that file alone changes no image and no service spec,
so compose keeps the container and the proxy goes on with the configuration it
loaded at its own start (M6 review, finding F12). Step 4 therefore reads the
container id of `caddy` before and after the `up`. A NEW id is a new container
that read the file at start. The SAME id gets
`docker compose exec caddy caddy reload --config /etc/caddy/Caddyfile
--adapter caddyfile`, which hands the running process the file and drops no
connection. A configuration that Caddy refuses fails that reload, and the script
then recreates the container, where the start check reports the failure.

Step 4 prints the check command with the port that Caddy really publishes, so a
moved `CADDY_HTTP_PORT` gives the right URL. If `docker compose port caddy 80`
prints nothing, the script prints that command instead of a guessed URL.

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
  `CADUS_TEST_DATABASE_URL`, `docker`, and `shellcheck`: it exits 2 and runs no
  check when the variable is unset, it fails with
  `GATE FAILED: docker is required` when docker is absent, and with
  `GATE FAILED: shellcheck is required` when shellcheck is absent. See
  `README.md` for the one-time database setup and for the shellcheck install.
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
  `docker compose build` builds every service that has a `build:` section. Every
  such service names its image in a `target:` key -- `runtime` for the app image,
  `spa` for the edge image -- and the checks below read that key, because the two
  images carry different things.

  In each APP image it proves that `cadus-web`, `cadus-worker`, and
  `cadus-migrate` are on the `PATH`, and that `/app/curriculum/courses.yaml`
  exists. For every service that builds the app image it proves three more facts:
  the service HAS a `command:`, its first token names one of the three binaries
  and exists in the image, and every further token is in the allowlist of that
  binary (`cadus-migrate` takes `--admin-login`; `cadus-web` and `cadus-worker`
  take no argument). A renamed binary target, a wrong `dockerfile:` key, a
  deleted `command:`, or a mistyped flag then fails the gate instead of the
  operator's next bring-up (review round 2, finding #12; review round 4,
  finding #9).

  In each EDGE image it proves that the built bundle is under `/srv` --
  `index.html`, the hashed `assets/`, and the vendored KaTeX -- and that `caddy`
  runs it, and that the service declares NO `command:`, because a command
  replaces the Caddy entrypoint. In BOTH images it proves that `node`, `npm` and
  `npx` are absent: node builds the bundle in a stage that ships nothing.
  Finally it reads `deploy/Caddyfile`, which is a bind mount that no image check
  can see, and proves that `/api/*` proxies to `web:8080`, that every other path
  serves the bundle out of the Dockerfile's own `/srv` with an `index.html`
  fallback, and that its five security headers equal `SECURITY_HEADERS` of
  `crates/web/src/security.rs` character for character.
- **The shell scripts.** `scripts/check_ops.sh` runs
  `shellcheck -S warning scripts/*.sh`. `scripts/deploy.sh` is THE upgrade
  procedure, so an unquoted expansion or a lost exit code in it lands on the
  operator's server. Fix a finding; do not silence it.
- **The CI workflow.** The last check of `scripts/check_ops.sh` proves that
  every published port of `.github/workflows/ci.yml` binds `127.0.0.1`. The gate
  database in CI runs with trust auth, so a `5432:5432` line puts a superuser
  port on every interface of the runner for the length of the job.
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
- **Runtime.** `/api/ready` answers
  `{"ok", "db", "worker": {"claim_age_secs", "stale"}}` (ruling D-M5-6). `db` is
  the verdict of one `SELECT 1` through the web pool, and it alone decides the
  status code: `ok` with 200, `down` with 503. `claim_age_secs` is the age in
  seconds of the oldest diagnosis job that still waits for a claim, and `null`
  means no job waits. A backlog older than 60 s sets `stale` and adds
  `"warnings": ["worker_claim_stale"]`, and it NEVER gives a 503: the whole grade
  verdict is local CPU work, so a learner studies through a worker outage and
  only the diagnosis prose waits. Caddy 404s `/api/ready` at the edge on purpose;
  scrape it over the compose network at `web:8080`.
- **The CSRF origin check.** `cadus-web` refuses a cross-origin write that
  carries the session cookie with `403 cross_origin_rejected`. Set
  `PUBLIC_ORIGIN` in `.env` to the origin you serve, such as
  `https://tutor.example`. Unset, the check rebuilds the origin from
  `X-Forwarded-Proto` and `Host`, and a proxy that stops sending
  `X-Forwarded-Proto` then makes every browser write answer 403. A value with a
  path or a trailing slash matches no `Origin` header at all, so `cadus-web`
  exits 2 rather than refuse every write.

## The curriculum in the image

The image carries the reviewed curriculum tree at `/app/curriculum`, and the
`worker` service reads it there. `CADUS_CURRICULUM` names the path, and the
image sets that variable to `/app/curriculum`, so a normal deployment sets
nothing.

The worker needs the tree for two jobs of the pool refill (D-O4):

- the A6 exemplar fallback, which fills the pool of a knowledge point that has no
  approved template;
- the knowledge-point half of the verification gate, which reads the topic answer
  kind and the authored exemplars.

**`cadus-worker` exits 2 when the tree does not load.** The message names the
path and the first finding, for example:

```
cadus-worker: configuration error: the curriculum at /app/curriculum did not load: no courses.yaml under /app/curriculum
```

An earlier build treated a missing tree as a warning and kept the tick loop
running. The refill then filled nothing for any learner, and the one log line
named the exemplar fallback alone (review round 1, findings #5 and #6). An exit
is the honest report: `docker compose ps` shows the worker in a restart loop, and
`docker compose logs worker` names the path to fix.

To run a tree the image does not carry, bind-mount it and point the variable at
the mount:

```yaml
services:
  worker:
    environment:
      CADUS_CURRICULUM: /srv/curriculum
    volumes:
      - ./my-curriculum:/srv/curriculum:ro
```

`scripts/check_ops.sh` asserts that `/app/curriculum` is a directory in every
image the compose file builds, so a `COPY` line that goes missing fails the gate
and not the deployment.

## The operator flags (A6, C6)

`cadus_store::pool::operator_flags` gives one row per knowledge point. M5 U12
puts it on `GET /api/operator/flags`, a read-only route that serves an ADMIN
account only: a session on an account with `users.is_admin = false` gets
`403 forbidden`, and a request with no session gets `401 unauthorized`. Each row
carries seven fields:

| Field | Meaning |
|---|---|
| `kp_id` | The serving key, `"<topic_id>/<kp_id>"`. |
| `approved_templates` | The count of `content_store` template rows with `status = 'approved'` (C6). |
| `pool_depth` | The count of unclaimed `serving_pool` rows of that knowledge point. |
| `last_source` | The source of the newest served row: `template`, `exemplar`, or `generator` (A7). |
| `last_exemplar_at` | When the knowledge point last fell back to an exemplar (A6). |
| `needs_template` | `true` when `approved_templates` is 0. Every serve of this knowledge point is an exemplar rotation. |
| `source_exhausted` | `true` when the refill filled a pair of this knowledge point twice in a row and wrote no new statement. |

`needs_template` and `source_exhausted` are two different faults, and the cure
differs:

- **`needs_template = true`** — nobody approved a template. Author one, put it
  through the gate, and approve it (C6). Until then the learner sees the authored
  exemplars in rotation, and the pool of that knowledge point holds at most one
  row per exemplar.
- **`source_exhausted = true`** — a source exists and it produces nothing the pool
  does not hold. An exemplar list of 3 under a target depth of 24 does this after
  the learner works through all 3, and so does a template whose whole space is in
  the pool. Widen the parameter domains of the template, or author more
  exemplars. A restart of the worker does not repair it.

The exhausted pair leaves the refill target list for 60 minutes, so the per-tick
budget goes to the pairs that still grow. A pair with no source at all leaves it
for 15 minutes.

### What the endpoint reads, and what it does not

Two limits, and both are in the answer:

- **`pool_depth` and `last_source` are the CALLING account's.** `serving_pool`
  is under row-level security, and the route binds the tenant of the admin who
  called it (C3). `approved_templates` and `needs_template` read
  `content_store`, which holds curriculum content and stands outside row-level
  security, so those two are deployment-wide.
- **`source_exhausted` is always `false` here.** The backoff map lives in the
  worker process, and 2.0 has no table that carries it across processes. The
  worker log line is where that flag is true.

### The gate block

The answer carries a second block, `gate`. It runs
`cadus_core::template::gate` over the approved template of a knowledge point and
reports `Verified::notes` — the checks the gate SKIPPED and why. The core writes
no log, so the caller of the gate reports them, and the refill worker drops them.
An empty `notes` list is the good case.

One gate call walks up to 4,096 instances, about 10 ms of CPU. The route
therefore gates at most 20 templates per request, in `kp_id` order, and sets
`gate_truncated` when it left some template ungated. Add `?kp=<topic>/<kp>` to
scope the whole answer to one knowledge point and read that template's notes.

A row with `gated: false` names a serving key the loaded curriculum does not
hold: the gate needs the topic's answer kind and the point's exemplars, and that
key has neither. A row with a `rejected` object names a template that
`content_store` carries as `approved` and the gate refuses today — a
disagreement worth an operator's attention.

### The refill log line

The worker writes one line per tick at info level:

```
refill tick=42 targets=3 inserted=27 from_template=24 from_exemplar=3 without_source=0 failed=0 refused_instances=0 flagged_refusals=0 skipped_starved=1 exhausted=0 retired_unapproved=0 nonce=1772150400123456
```

Read three of those fields first:

- `without_source` counts the pairs with no approved template and no exemplar.
- `exhausted` counts the pairs this tick put on the 60-minute backoff.
- `retired_unapproved` counts the pool rows this tick took out of the pool
  because their digest lost its approval.

`source_exhausted` lives in the worker process and in no table, so a reader
outside that process gets `false`. The `exhausted` count of the log line is the
signal a deployment reads today.

### Revoke a wrong template (C6)

If the log or a learner report names a template that computes a wrong answer, do
these steps:

1. Reject the digest through the review surface below:

   ```sh
   curl -fsS -X POST -H 'content-type: application/json' \
     -b "$COOKIE" -d '{"reason":"computes a wrong answer"}' \
     https://<host>/api/admin/content/<digest>/reject
   ```

2. Wait one worker tick. The refill claims every unclaimed pool row of that
   digest, writes one warn line per row, and counts them in `retired_unapproved`.
3. Read `docker compose logs worker` and do a check of the count. It is the
   number of wrong problems that never reached a learner.
4. Author the corrected template and approve it. The next tick fills the pool
   from the new digest.

The pop reads the approval on every serve, so step 1 alone stops the wrong
problems. Step 2 is what lets the corrected template take their place: the pool
holds one row per statement (A5), and a corrected template inserts nothing over
rows that are still unclaimed.

**Do not delete the `content_store` row.** `serving_pool.content_digest`
references it, and the approval record is the C6 audit trail.

## The authoring run (A2, T3, C6)

M6 R8 puts the offline authoring pipeline behind one subcommand of the worker
binary. The pass reads the curriculum, authors documents, gates them, and stores
each accepted document as `pending`. It never runs on a request path (R4), and
`cadus-web` never reaches a model endpoint (L6).

```sh
cadus-worker                       # the tick loop: refill and diagnosis
cadus-worker author [OPTIONS]      # one authoring pass
cadus-worker --help                # the option list
```

| Option | What it does |
|---|---|
| `--kp <topic_id/kp_id>` | Author for this knowledge point. Repeat it for more. The default is every knowledge point of the tree. |
| `--kind <kind>` | Author this kind: `template`, `teach`, `hint_ladder`, or `diagnosis`. Repeat it for more. The default is all four. |
| `--dry-run` | Print the plan. Make no model call and no write. |
| `--stale` | List the approved documents an older prompt wrote. Make no model call and no write. |

The pass reads `DATABASE_URL` (the `cadus_admin` connection),
`CADUS_CURRICULUM` (the tree; the default is `./curriculum`), and the endpoint
variables of the section "The diagnosis worker" below: `OPENAI_API_KEY`,
`OPENAI_BASE_URL`, `OPENAI_MODEL` and `OPENROUTER_PROVIDER_ORDER`. A dry run
needs no model variable. A run that is not a dry run needs `OPENAI_API_KEY`, and
an empty key ends the process with exit code 2.

### The authoring token budget

The pass does NOT read the two `DIAGNOSIS_*` token variables. It reads its own
two, and it keeps them apart on purpose: an authored document is a whole
template, teach page, hint ladder or distractor set, and the diagnosis ceiling of
600 output tokens with a reasoning ceiling of 600 beside it leaves zero visible
tokens for one.

| Variable | Default | Meaning |
|---|---|---|
| `AUTHORING_OUTPUT_TOKENS` | `4000` | The output ceiling of one authoring call. The pass is offline and no learner waits for it, so this is not a latency bound. |
| `AUTHORING_REASONING_MAX_TOKENS` | `2000` | The reasoning ceiling of one authoring call (T5). The reasoning budget is part of the output budget, so this default keeps half of the output ceiling for the document. |

The configuration line of the pass names both values. Read it before you spend
tokens:

```
cadus-worker: the authoring pass is configured model=deepseek/deepseek-v4-pro base_url=https://openrouter.ai/api/v1 output_tokens=4000 reasoning_max_tokens=2000
```

`docker-compose.yml` does not forward the two variables yet, so pass them on the
command line of an authoring run in a container:

```sh
docker compose exec -e AUTHORING_OUTPUT_TOKENS=6000 worker \
  cadus-worker author --kp perfect-squares/kp1 --kind template
```

### Read the plan first

A run spends model tokens (T3), so read the plan before you spend them:

```sh
docker compose exec worker cadus-worker author \
  --kp perfect-squares/kp1 --kind template --kind teach --dry-run
```

```
authoring plan
kp_id kind taken target author
perfect-squares/kp1 template 0 3 1
perfect-squares/kp1 teach 0 1 1
plan: pairs 2, documents 2, model calls 2 to 10
dry run: no model call and no write
```

Read the columns like this:

- `taken` — the approved and the pending documents this pair holds now. Both
  occupy a bank slot, so a pending document stops a second copy of itself.
- `target` — the documents a full bank holds: 3 for `template`, 1 for the other
  three kinds.
- `author` — the documents THIS pass writes for the pair: 1 for a short bank, 0
  for a full one.

**One pass authors at most one document per pair.** A template bank of three
therefore fills over three passes, and a human reviews what each pass stored
before the next pass runs (C6). The last line states the bill: one model call per
document when the gate accepts the first reply, and five calls when every attempt
is refused.

### Run the pass

```sh
docker compose exec worker cadus-worker author --kp perfect-squares/kp1 --kind template
```

```
authoring plan
kp_id kind taken target author
perfect-squares/kp1 template 0 3 1
plan: pairs 1, documents 1, model calls 1 to 5
template: stored 1 skipped 0 declined 0 calls 1 alerts 0
```

The plan prints first, then one result line per kind:

- `stored` — the knowledge points that hold a `pending` document after the pass.
- `skipped` — the knowledge points the pass made no call for, because the bank
  was full.
- `declined` — the knowledge points that used all five attempts. A decline stores
  nothing, and a `declined ...` line follows with the last refusal of the gate.
  A knowledge point of an answer kind the gate can never accept declines with
  ZERO calls: `template` and `diagnosis` both need a symbolically decidable
  answer kind (`numeric` or `expression`), so a `proof` topic costs nothing for
  either kind. `teach` and `hint_ladder` carry no answer expression, so the pass
  authors them for every answer kind.
- `calls` and `alerts` — the model calls of the pass, and the passes above three
  attempts (T3).

The process exits 0 after a pass and 2 after a configuration error. An unknown
knowledge point, an unknown kind, an unreadable curriculum, and an empty
`OPENAI_API_KEY` are all exit code 2, and the one line on stderr names the cause.

### The LaTeX escape repair (trap T1)

A model that writes its own JSON with ONE backslash emits `"$\times$"`. That is
valid JSON, and `\t` is a valid JSON escape, so the decoded value is
`$<TAB>imes$` and the LaTeX command is gone. No gate reads a control character,
so the mangled text used to reach `content_store` on every kind.

The pass now repairs every text field of the model's reply before any gate runs:
the statement, the solution sketch, every hint, the concept, every worked step,
and every distractor answer and note. Two fields are never repaired —
`answer_expr` and the `expected` of a worked sample — because the server
COMPUTES over both and a rewrite changes the arithmetic.

A control character that the repair cannot name refuses the whole body. The
refusal reads:

```
the text carries a control character that is not a LaTeX command — a JSON string eats the first letter of \times when the backslash is written once, so write EVERY backslash of a LaTeX command twice
```

That sentence goes to the model as the feedback of the next attempt, so a
refusal costs one attempt of the five and usually buys a clean document. A
newline stays legal text; every other control character refuses.

### After a prompt edit (C6)

Every stored row names the prompt that authored it, in
`content_store.prompt_digest`. The digest covers the system prompt, the tool
name and the tool schema of the kind. An edit to any of the three gives the kind
a new digest, and every row of the old digest becomes STALE.

A stale row keeps its approval. C6 binds an approval to the CONTENT, and a
prompt edit changes no content, so a stale approved document goes on serving
until a reviewer approves its replacement. The edit marks the row; it never
unapproves it.

List the marked rows:

```sh
docker compose exec worker cadus-worker author --kind teach --stale
```

```
stale documents
kp_id kind digest prompt_digest
perfect-squares/kp1 teach sha256:0f3c1a94b7e2d508 sha256:a1b2c3d4e5f60718
stale: rows 1
```

The listing names APPROVED rows alone. A `pending` row of an older prompt is
already in front of a reviewer, and a reviewer reads the body and not the
prompt.

The next pass then re-authors the marked pairs first, and a stale slot does NOT
fill the bank: a knowledge point with one stale approved teach page authors one
new teach page, although the teach bank is 1. Approve the new document, and
reject the old one when it is no longer wanted.

A row with an EMPTY `prompt_digest` is never stale. NULL means "the prompt is
not recorded", which is what every row written before migration
`0012_content_prompt_digest` carries.

### After the pass

Every stored document is `pending` and serves nothing. Approve it on the review
surface below, or reject it with a reason. The 8 rendered instances of the show
route are where a wrong template is visible (C6).

### Run it against a test database

The commands above run against any database the migrations built. Point
`DATABASE_URL` at it, point `CADUS_CURRICULUM` at a tree, and run the dry run
first:

```sh
DATABASE_URL=postgresql://…/cadus_test \
CADUS_CURRICULUM=crates/worker/tests/fixtures/pool \
  cadus-worker author --kp perfect-squares/kp1 --kind template --dry-run
```

A dry run makes no call and writes no row, so it is safe on any database.

## The review surface (C6)

M6 R5 puts the four review routes on `/api/admin/content*`. All four serve an
ADMIN account only: a session on an account with `users.is_admin = false` gets
`403 forbidden`, and a request with no session gets `401 unauthorized`.

| Method and path | What it does |
|---|---|
| `GET /api/admin/content?status=&kind=&kp=` | The review queue. Each line carries the digest, the serving key, the kind, the status, `authoring_attempts`, `authoring_cost_usd`, `created_at`, a 64-character summary, `approved_templates`, and `bank_warning`. |
| `GET /api/admin/content/{digest}` | One document: the body, the gate block, and 8 rendered instances with their computed answers. |
| `POST /api/admin/content/{digest}/approve` | Approve that digest. The body is `{}`. The call is idempotent. |
| `POST /api/admin/content/{digest}/reject` | Reject that digest. The body is `{"reason": "..."}`, and a request with no reason is `422`. |

Three rules an operator must know:

- **Approval binds to the digest (C6).** An edited body is a new digest and a
  new row, so no approval carries over. Do not edit a body in `psql`.
- **`bank_warning` is `true` below three approved templates.** A knowledge
  point serves from its approved slots alone, so a bank of two repeats a
  smaller set of problem shapes than the bank was sized for.
- **The 8 instances are the point.** A template that computes correctly and
  asks the wrong question is obvious in the instances and invisible in the
  expression. Read them before you approve.

### The admin connection

`cadus_app` holds SELECT on `content_store` and nothing else, so the two WRITE
routes need a connection of `cadus_admin`. `CADUS_ADMIN_DATABASE_URL` names it,
and the compose file sets it from `CADUS_ADMIN_PASSWORD` already. Unset, both
writes answer `503 admin_path_unavailable`, the two read routes still serve, and
every other route of the process is unchanged. The C3 boot guard runs on
`DATABASE_URL` alone: `cadus_admin` bypasses row-level security by design, and no
learner route takes this pool.

## The authoring bill (T3)

The offline authoring pipeline is the second spender of model tokens, and T2
names no third one. Every attempt of it writes one `model_call_log` row with
`purpose = 'authoring'` and no `user_id`: an authoring call belongs to a
knowledge point, not to a learner.

The stored document carries the bill of the pass that wrote it:

| Column | Meaning |
|---|---|
| `content_store.authoring_attempts` | The model calls that pass spent. The pipeline stops at 5 (`docs/reference/authoring-and-spa-1.0-spec.md`, section 2.2). |
| `content_store.authoring_cost_usd` | The sum of the prices of those calls. Each price is rounded to six decimal places first, the way its own ledger row holds it, so the total on the row is the sum of the call rows of that pass. |

`authoring_cost_usd` is NULL when no call of the pass reported a price. The
attempt count stays exact in that case: read the count when the money is NULL.

**The T3 alert.** A pass above 3 attempts writes an `error` log line with the
knowledge point, the kind, the attempts and the bill:

```
T3 alert: this knowledge point spent more than 3 authoring attempts
```

Two passes raise it: a pass the gate accepted late, and a pass that used all five
attempts and stored nothing. The alert stops no job.

List the documents the alert covers:

```sh
docker compose exec db psql -U cadus_admin -d cadus -c \
  "SELECT kp_id, kind, authoring_attempts, authoring_cost_usd
     FROM content_store WHERE authoring_attempts > 3
    ORDER BY authoring_attempts DESC, kp_id"
```

A declined knowledge point holds no row there, because a decline stores nothing.
It has no approved template, so the operator flags name it with
`needs_template = true`, and its spend is in the ledger under
`purpose = 'authoring'`.

## The diagnosis worker (A4, T4, T5)

The worker claims one `diagnosis_jobs` row per tick, calls the model, filters the
error tags, writes `result`, and sends `NOTIFY diagnosis_done`. The learner never
waits on it: the verdict, the worked solution and the re-solve instruction are
all deterministic and already on screen (A3, L2). A diagnosis that fails costs
prose and nothing else.

### The seven variables

`OPENAI_API_KEY` is the switch. If it is empty or absent, the worker runs its
refill job, leaves the queue standing, and calls no model. If it holds a value,
every variable below is binding and a bad one exits the process with code 2.

Set the seven keys in `.env`. `docker-compose.yml` forwards them to the `worker`
service and to no other service: a model call runs in `cadus-worker` and never on
a request path (R4, L6). Compose reads `.env` for interpolation only, so a key
that no `environment:` block names never reaches a container. A blank value takes
the default of the table below. Run `scripts/deploy.sh` after a change, and do a
check of the result inside the container:

```sh
docker compose exec worker env | sort | grep -E "OPENAI|DIAGNOSIS"
```

A worker that gets a key writes one info line at start. The line names the model,
the endpoint and the cap:

```
cadus-worker: the diagnosis job is configured
```

A worker that gets no key writes this line instead:

```
cadus-worker: OPENAI_API_KEY is empty; the diagnosis queue waits and no model is called
```

| Variable | Default | Meaning |
|---|---|---|
| `OPENAI_API_KEY` | none | The bearer token. Empty means: call no model. |
| `OPENAI_BASE_URL` | `https://openrouter.ai/api/v1` | The endpoint. A local OpenAI-compatible server, for example `http://10.8.0.3:8080/v1`, is the same code path. |
| `OPENAI_MODEL` | `deepseek/deepseek-v4-pro` | The model id the request names (O2). |
| `OPENROUTER_PROVIDER_ORDER` | none | A comma-separated provider list. T5 pins the order, so an OpenRouter endpoint with an empty list is a configuration error. |
| `DIAGNOSIS_OUTPUT_TOKENS` | `600` | The output ceiling of one diagnosis call. It is a latency bound, not a spend cap. The authoring pass has its own ceiling; see "The authoring token budget". |
| `DIAGNOSIS_REASONING_MAX_TOKENS` | `600` | The reasoning ceiling of one diagnosis call (T5). |
| `DIAGNOSIS_CALLS_PER_SESSION` | `0` | The T4 call cap per session. `0` is unlimited (O2). |

`provider` and `reasoning` are OpenRouter extensions. The client puts them in the
body only when the PARSED host of `OPENAI_BASE_URL` is `openrouter.ai`, so a
local endpoint gets the plain OpenAI shape and a look-alike host such as
`openrouter.ai.attacker.example` gets neither the extensions nor the routing
behavior.

### The four end states of a row

| `status` | What it means | What the operator does |
|---|---|---|
| `pending` | The row waits for a worker. | Nothing. |
| `running` | A worker holds it. A lease past 5 minutes goes back to `pending`. | Nothing. |
| `done` | `result` holds the diagnosis. | Nothing. |
| `failed` | Three attempts failed, or the payload does not read. | Read the warn lines; the learner kept the verdict. |
| `capped` | A configured `DIAGNOSIS_CALLS_PER_SESSION` refused the call. | Raise the cap, or leave it. |

A row goes back on the queue after each failed attempt and dead-letters on the
third. The sweep runs every 5 minutes and does two things: it returns a stale
lease to `pending`, and it dead-letters a stale row that already used its three
attempts.

### The diagnosis log line

The worker writes one line per pass that did something, at info level:

```
diagnosis tick=42 outcome=Done job=Some(0f0e...) http_attempts=2
```

`http_attempts` above 1 is a retry: a truncated reply, a 429, a 5xx, or a
transport error. A truncated reply repeats with a 4× output ceiling, because a
reasoning model shares its completion budget with its hidden reasoning and an
identical retry reproduces the truncation exactly.

### The model-call ledger (T6)

Every HTTP attempt the worker makes writes one row of `model_call_log`: the
purpose, the model id, the provider, the cached and uncached input tokens, the
output tokens, the reasoning tokens, the wall clock in milliseconds, the cost and
the provider's request id. A truncation retry is two calls and two bills, so it
writes two rows.

Read the ledger through an ADMIN connection. `cadus_app`, the role the web
service uses, holds no privilege on the table and none on its sequence, and that
is why the table needs no tenant policy.

```sh
docker compose exec db psql -U cadus_admin -d cadus -c \
  "SELECT date_trunc('day', ts) AS day, purpose, count(*) AS calls,
          sum(input_tokens_uncached) AS input, sum(output_tokens) AS output,
          sum(cost_usd) AS usd
     FROM model_call_log GROUP BY 1, 2 ORDER BY 1 DESC"
```

Two columns need a word:

- `cost_usd` is NULL when the provider priced nothing. An unmeasured call is
  visible AS unmeasured; it is never a dropped row and never a guess.
- `output_tokens` counts the VISIBLE output. A provider that reports its hidden
  reasoning inside `completion_tokens` has that part moved to
  `reasoning_tokens`, so the two columns count different tokens and their sum is
  what the provider billed as completion.

### The metrics series

`GET /metrics` reports four series beyond the two request series:

| Series | Labels | Source |
|---|---|---|
| `cadus_deterministic_grade_total` | `result` = `correct`, `notation`, `blank`, `incorrect`, `undecidable` | The web process. It counts grade decisions taken with NO model call, and it starts at zero on every restart. |
| `cadus_diagnosis_jobs_total` | `result` = `ready_preauthored`, `enqueued`, `done`, `failed`, `capped` | `ready_preauthored` is a hit in the authored bank, which writes no job row, so the web process counts it. The other four are the `diagnosis_jobs` rows themselves. |
| `cadus_model_call_tokens_total` | `purpose`, `kind` = `cached`, `uncached`, `output`, `reasoning` | `model_call_log`. |
| `cadus_model_call_latency_seconds` | `purpose` | `model_call_log`, as a `_sum` and a `_count`. |

The model calls run in the worker, a different process from the one that answers
`/metrics`, so the last two series and three labels of the second come from the
tables and not from a counter in memory. They survive a restart of either
process. The scrape reads them with two SECURITY DEFINER aggregates
(`migrations/0009_metrics_readers.sql`) that return sums by purpose and by status
and no row of either table. A datastore that answers nothing drops those series
from one scrape; the request series and the grade counter still answer 200.

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

`DB_CLIENT_TIMEOUT_MS` (default `10000`) is the second bound and works on the
CLIENT side: it bounds the whole call in `cadus-web` and `cadus-worker`, not the
statement in Postgres. Set `0` to remove it. The value is a whole number of
milliseconds, like `DB_STATEMENT_TIMEOUT_MS`.

The two bounds cover two different faults:

| Key | Side | Ends |
|---|---|---|
| `DB_STATEMENT_TIMEOUT_MS` | server | a statement that RUNS too long |
| `DB_CLIENT_TIMEOUT_MS` | client | any call that does not come back, a dropped socket included |

Keep `DB_CLIENT_TIMEOUT_MS` above `DB_STATEMENT_TIMEOUT_MS`. The server then
reports the timeout first and names the statement, and the client bound stays
the backstop for the case the server never answers. sqlx 0.9 exposes no TCP
keepalive, so this client bound is what ends a socket that a firewall drops
silently. `cadus-migrate` ignores `DB_CLIENT_TIMEOUT_MS` for the same reason it
ignores the server bound, and `docker-compose.yml` does not forward it to the
`migrate` service. `scripts/check_ops.sh` check (f) fails the gate if either key
reaches `migrate`.

## The cookie posture

`SITE_ADDRESS` and `CADUS_WEB_INSECURE_COOKIE` are ONE decision. Set them
together:

| Deployment | `SITE_ADDRESS` | `CADUS_WEB_INSECURE_COOKIE` | Session cookie |
|---|---|---|---|
| http, on a bare IP | `:80` | `1` | `cadus_session`, `HttpOnly`, `SameSite=Lax`, `Path=/` |
| https, on a domain | `math.example.com` | `0` (the default) | `__Host-cadus_session`, and `Secure` with it |

At the default `0`, `cadus-web` writes `__Host-cadus_session` with `Secure`. A
browser DISCARDS such a cookie on an `http://` origin, and it reports nothing:
the login answers 200, the SPA shows a signed-in screen, and the next authed
write answers 401. `1` serves the plain name `cadus_session` without `Secure`,
which is the only posture an http origin can hold. The value `1` belongs to an
http deployment alone: it drops `Secure` from the session cookie, so a public
site on https keeps the `0` (M6 review, finding F14).

`.env.example` ships the http pair, so a copied file holds a session on a bare
IP. `scripts/check_ops.sh` check (k) reads `.env.example` and fails the gate on
any other pairing of the two keys. The only values are `0` and `1`; `cadus-web`
exits 2 on anything else.

## The edge configuration

`deploy/Caddyfile` is the whole edge: it serves the SPA bundle from `/srv` and
proxies `/api/*` to `web:8080`, so the bundle and the API share ONE origin. The
`caddy` service bind-mounts the file at `/etc/caddy/Caddyfile`, which is the only
path from the repository to the running proxy: the `spa` image copies the bundle
and no Caddyfile, so a container without that mount runs the stock `caddy:2`
configuration and serves the Caddy welcome page.

`scripts/check_ops.sh` check (j) holds the file to six facts before a commit
lands: `caddy validate` in the edge image accepts it; the compose file mounts it
at `/etc/caddy/Caddyfile`; `/api/*` proxies to `web:8080`; the `@ops` block
answers 404 for `/api/ready` and `/metrics`; the SPA handle falls back to
`index.html` under the root the Dockerfile copies the bundle to; and its five
security headers equal `SECURITY_HEADERS` of `crates/web/src/security.rs`,
character for character. `caddy validate` is in that list because the other
facts are text facts: a file that Caddy REFUSES passed the whole gate before, and
the operator met the fault as a restart loop on the sole ingress (M6 review,
finding F13).

## Proxy ports

`CADDY_HTTP_PORT` (default `80`) and `CADDY_HTTPS_PORT` (default `443`) set the
HOST ports of the `caddy` service. Caddy keeps ports 80 and 443 INSIDE the
container, so the Caddyfile and `SITE_ADDRESS` do not change with these keys.

Keep the defaults for a real site. Let's Encrypt reaches port 80 and port 443
only, so a moved port gets no certificate. Move the ports for a test bring-up on
a server where another stack already holds 80 and 443, and set
`SITE_ADDRESS=:80` for that bring-up:

```sh
# .env
SITE_ADDRESS=:80
CADDY_HTTP_PORT=18080
CADDY_HTTPS_PORT=18443
```

The stack then answers on `http://127.0.0.1:18080/api/health`. `scripts/deploy.sh`
prints that URL in its check command, because it reads the published port from
`docker compose port caddy 80`.

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
`DB_STATEMENT_TIMEOUT_MS` and `DB_CLIENT_TIMEOUT_MS` and runs every migration
without a query bound.

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
