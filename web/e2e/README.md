# The browser click-through (S13)

Three walks drive the BUILT bundle in a real Chromium: `demo.mjs` against `?demo=1`,
`journey.mjs` against `?demo=journey`, and `authed.mjs` against the M5 `cadus-web` binary.
Beside them, `packaging.sh` (S14) drives the
deployed stack itself — Caddy, the service and the database — with curl instead of a browser.

## Why this exists, concretely

The unit suite runs in jsdom. In one session the 1.0 click-through found three failures that
every one of those tests, and every HTTP-level check, passed straight through
(`/home/deploy/dev/cadus/web/e2e/README.md`):

1. **A blank page.** Vite rewrote `<link rel="stylesheet" href="/vendor/katex/katex.min.css">`
   into `import "/vendor/katex/katex.min.css"` at the top of the entry chunk, because
   `/vendor/**` is external. Chrome refuses a stylesheet as a module script, so the entry
   never evaluated. Every asset answered 200. The page was white.
2. **Every problem printed as raw LaTeX.** `auto-render.min.js` was injected ahead of
   `katex.min.js`, so the extension captured `window.katex` as `undefined` and threw on every
   render. `src/lib/katex.ts` catches that and falls back to escaped plain text — the
   documented graceful degradation — so the only symptom was `$\frac{6}{8}$` on screen. jsdom
   stubs the global entirely and cannot see the order.
3. **The demo lesson practised the wrong problem.** `POST /task/{id}/serve` is idempotent on
   the real server; the demo backend advanced its cursor on every call, so the learner
   practised a problem the server never served. No unit test makes two serve calls in a row.

All three now have gates — 1 and 2 in `scripts/check-bundle-csp.mjs`, 3 in `src/api/demo.ts`
and the `SERVE-idem` tests — **but every one of those gates was written after a browser found
the failure.** So the click-through keeps its own detector for each
(`checks.mjs`: `checkNotBlank`, `checkMathRendered`, `checkSameProblem`), and `run.mjs`
proves the detectors still work by breaking the build on purpose.

## Running it

```sh
node e2e/run.mjs demo                      # one walk against a clean build
node e2e/run.mjs journey                   # instruction through delayed retention
node e2e/run.mjs authed --api=URL          # one walk against a running cadus-web
node e2e/run.mjs acceptance                # the S13 acceptance check
```

`acceptance` is what the unit is measured by. It builds each of the three deliberately broken
trees of `breaks.mjs`, runs the demo walk against each, and requires **that** failure to be
named; then it builds the real tree and requires both the compatibility walk and the complete
2.0 journey to pass. Its output ends:

```
================ S13 ACCEPTANCE ================
  PASS  blank-page     exit=1 reported="FAIL-1 blank page"=true
  PASS  raw-latex      exit=1 reported="FAIL-2 raw LaTeX"=true
  PASS  wrong-problem  exit=1 reported="FAIL-3 wrong problem"=true
  PASS  clean          exit=0
  PASS  journey        exit=0
```

Add `--no-build` to reuse `web/dist`, and `--port=N` to move the origin off 4173.

## The browser runs in a container, always

Playwright's browser binaries install on this box without root and **do not launch**:
`chrome-headless-shell` needs system libraries and `playwright install-deps` needs root, which
this box has none of (spec section 7.3). `run.mjs` therefore drives every walk inside
`mcr.microsoft.com/playwright:v1.62.1-noble`, which carries its own browsers. Nothing here
ever launches a browser natively.

`run.mjs` PROBES how the container reaches the host. The image's own documentation says
`host.docker.internal`, and `--add-host=…:host-gateway` makes that name resolve on Linux; on a
host whose firewall drops traffic from the docker bridge the name resolves and the connection
still hangs, which surfaces only as a 45-second `page.goto` timeout. The fallback is
`--network host`. Set `E2E_NETWORK=bridge` or `E2E_NETWORK=host` to skip the probe.

## The origin

`serve.mjs` serves `dist/` and proxies `/api` to the axum service **on one origin**, which is
the shape the deployment has (Caddy in front of the same service, S14). A preview server on
one port with the API on another makes every cookie write `403 cross_origin_rejected` for a
reason the product does not have. It is a test fixture: no dependencies, no rewriting, and no
security headers of its own — the walk reads those off `/api/health`, where the service stamps
them.

## The authed path, end to end

```sh
# 1. A database with the migrations applied.
docker exec cadus2-testdb psql -U test -d postgres -c 'CREATE DATABASE cadus2_s13'
DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_s13 \
  target/release/cadus-migrate

# 2. The M5 binary. It connects as `cadus_app`, because the C3 boot guard stops a role that
#    bypasses row-level security — `test` is a superuser and does.
DATABASE_URL=postgresql://cadus_app@127.0.0.1:55434/cadus2_s13 \
BIND_ADDR=127.0.0.1:8099 \
PUBLIC_ORIGIN=http://127.0.0.1:4173 \
CADUS_WEB_INSECURE_COOKIE=1 \
  target/release/cadus-web &

# 3. The one account the walk signs in as.
web/e2e/seed.sh --api=http://127.0.0.1:8099

# 4. The walk.
cd web && node e2e/run.mjs authed --api=http://127.0.0.1:8099
```

`PUBLIC_ORIGIN` must be the origin the BROWSER loads, or the CSRF layer refuses every cookie
write. `CADUS_WEB_INSECURE_COOKIE=1` is required over plain http: a `__Host-` cookie without
`Secure` is refused by the browser, and the cookie-posture guard stops the process rather than
serve one.

## What each walk covers

**`demo.mjs`** — the canned backend, no account, no network. The dashboard (one primary
action, W-C2), the curriculum map (a painted Cytoscape canvas, the readout, the accessible
list view), a lesson end to end (worked example → practice → **SERVE-idem** → hint → grade →
1400 ms auto-advance → the next knowledge point), the placement (three ground rules, and
**DIAG-nosol**: a mark and nothing else), and a 390 px viewport checked for horizontal
overflow.

**`journey.mjs`** — the deterministic 2.0 browser fixture. It walks approved instruction →
integrated application → field-level feedback → a distinct unseen seven-day assessment → the
retention report. It checks rendered KaTeX on both tasks, including the MathML accessibility
tree and the hidden visual presentation, and proves the delayed assessment repeats no
instruction. This is browser/UI evidence; the Rust journey and recovery suites own database
persistence, replay, and scheduler time.

**`authed.mjs`** — the only path that exercises the session cookie, the CSRF origin layer and
the service's own payloads. Sign-in, the `HttpOnly` cookie with empty web storage
(**SEC-cookie**), the service CSP, the map over the real 1090-topic curriculum, the placement,
and sign-out. M5 mounts no `/api/diag/*` route, so the walk asserts the screen's stated
"not available" branch rather than skipping the screen.

Both wait for a view's **content**, never for its shell. `GET /api/status` and `GET /api/graph`
are round trips, and a read taken as soon as the section appears reads an empty spinner — two
of the four false failures in 1.0's first run were exactly that.

Both exit non-zero on any console error, page error, failed request or 4xx/5xx, and write
screenshots to `e2e/shots/`.

## The packaging check (S14)

`packaging.sh` is the third script here, and the only one that starts the real deployment:

```sh
web/e2e/packaging.sh              # build, bring up, check, tear down
web/e2e/packaging.sh --no-build   # reuse the images that are already built
web/e2e/packaging.sh --keep       # leave the stack up on http://127.0.0.1:18080
```

It runs `docker-compose.yml` under its own project name (`cadus2s14acc`) and its own port
(18080), so it touches no other stack on the box, and it removes the project and its volumes
on the way out.

`scripts/check_ops.sh` reads the Dockerfile, the compose file and the Caddyfile and proves
what they SAY. This script proves what the stack DOES, and it holds the four facts of spec
section 7.1 row S14:

```
PASS: nonode   -- neither the app image nor the edge image carries node, npm, or npx
PASS: csp      -- the bundle inside the edge image passes the audit
PASS: edge     -- one origin: / and /ops serve the bundle with the five security headers,
                  /api/health answers {"ok":true} from the service, /api/ready and /metrics
                  answer 404
PASS: csrf     -- through Caddy, the cookie POST from the deployment's own origin is 200 and
                  the same cookie POST from https://evil.example is 403 cross_origin_rejected
```

The last one is why the whole stack has to run. The `403` comes from the CSRF origin layer of
the SERVICE (`crates/web/src/origin.rs`) and never from Caddy, so only a real proxy in front
of a real service on ONE origin tells the two POSTs apart. `serve.mjs` reproduces that shape
for the click-through; this script is the shape itself.

The CSP audit reads the bundle out of the EDGE IMAGE and not out of a host build:
`packaging.sh` replaces `web/dist` with `docker cp` from the image and runs `npm run csp`
over those bytes. A host build that passes says nothing about what the image ships.

It is not part of `npm run check` and not part of the Rust gate. It builds two images and
brings a database up, so it runs when the packaging changes.

## In CI

`.github/workflows/ci.yml` runs three SPA steps in the ONE gate job, in this order:

1. `npm ci` in `web/`;
2. `npm run check` — the unit gate, native, no docker;
3. `node e2e/run.mjs acceptance` — the click-through, in the Playwright container;

then it uploads `web/e2e/shots/*.png` as the `click-through` artifact with `if: always()`, so
a red run keeps the screenshots that explain it.

Steps 2 and 3 are SEPARATE STEPS on purpose (spec section 7.3): a docker outage then fails the
click-through with the unit gate already green and reported. They stay in the one job because
`scripts/check_ops.sh` check (i) refuses a second `runs-on:` — the budget benchmarks run
inside `scripts/gate.sh` and must not run beside another suite. Steps of one job run one at a
time, so nothing there contends with a benchmark.
