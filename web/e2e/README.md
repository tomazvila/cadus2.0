# The browser click-through (S13)

Two walks that drive the BUILT bundle in a real Chromium: `demo.mjs` against `?demo=1`, and
`authed.mjs` against the M5 `cadus-web` binary.

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
node e2e/run.mjs authed --api=URL          # one walk against a running cadus-web
node e2e/run.mjs acceptance                # the S13 acceptance check
```

`acceptance` is what the unit is measured by. It builds each of the three deliberately broken
trees of `breaks.mjs`, runs the demo walk against each, and requires **that** failure to be
named; then it builds the real tree and requires a clean pass. Its output ends:

```
================ S13 ACCEPTANCE ================
  PASS  blank-page     exit=1 reported="FAIL-1 blank page"=true
  PASS  raw-latex      exit=1 reported="FAIL-2 raw LaTeX"=true
  PASS  wrong-problem  exit=1 reported="FAIL-3 wrong problem"=true
  PASS  clean          exit=0
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

## Still owed

`e2e/shots/` is written to be uploaded as a CI artifact, and the CI step is **not** wired
here: `.github/workflows/ci.yml` belongs to S14, and `scripts/check_ops.sh` fails a workflow
that declares a second `runs-on:`. The step S14 needs, after its TypeScript job runs
`node e2e/run.mjs acceptance`:

```yaml
      - name: Upload the click-through screenshots
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: click-through
          path: web/e2e/shots/*.png
          if-no-files-found: warn
```

Keep it a step of the TypeScript job and out of the Rust gate job: the browser walk needs
docker, and a docker outage must never block the unit gate (spec section 7.3).
