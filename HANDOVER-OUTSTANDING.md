# Handover — outstanding issues from the CADUS 2.0 grind evaluation

> **STATUS (2026-09-15):** H-1, H-3, H-4, H-5, H-6, H-7 and H-8 are FIXED
> (see the "Handover fixes (H-1…H-8)" section at the end of `ISSUES.md` for
> what shipped where). H-2 received a documented decision (option c) plus the
> dashboard UX text. `REQUIREMENTS.md` still wins over everything; `ISSUES.md`
> is the log of record; this file keeps the repro detail.

**To:** the next agent on this box.
**From:** the grind evaluation of 2026-09-14/15 (fresh-learner walkthrough + full
automated grind). 11 numbered issues were found and are ALL fixed, deployed and
verified — see `ISSUES.md` and commits `fc93a25c`, `adda454b`, `9d935270`,
`8bdc8063`, `fa6d18fa`.

**Authority:** `REQUIREMENTS.md` wins over everything; `ISSUES.md` is the log
of record; this file adds repro detail the log entries lack.

---

## 1. Runtime state you inherit

| Thing | Where |
|---|---|
| Learner under test | `learner.eval@example.com` / `Learn!Pass2026x` (is_admin=true) |
| Deployment | Docker compose project `homelab`, workdir `/home/deploy/homelab` (services `cadus2-db/-migrate/-web/-worker/-report-worker/-edge`), `.env` at `/home/deploy/homelab/.env` |
| Site | `https://cadus.homelab.tomazvi.la` (caddy publishes 80/443) |
| Grind driver | `/tmp/orchestrator.py` (loops: sessions → author blocked topics → approve → re-stamp → make_due → next course), supervised as `systemd --user` unit `cadus-grind3`, log `/tmp/orchestrator.log` |
| Eval DB for tests | `postgresql://test:test@127.0.0.1:55434/cadus2_evalgate` (migrated) |
| Prod DB | `docker exec cadus2-db psql -U cadus_admin -d cadus` (owner role; the runtime `cadus_app` role must never be used for admin writes) |

**Build/test on this box:** there is NO system `cc`. Run everything through
`nix shell nixpkgs#gcc -c bash -c '...'`, with `SQLX_OFFLINE=true` and
`CADUS_TEST_DATABASE_URL` set for tests. `cargo sqlx prepare --check` needs the
live DSN. The gate's `scripts/check_ops.sh` needs docker+python3+shellcheck.

**Deploy order:** `docker build -t cadus2:latest --target runtime .`,
`docker build -t cadus2-edge:latest --target spa .`, then from
`/home/deploy/homelab`: `docker compose run --rm cadus2-migrate`, then
`docker compose up -d --no-deps cadus2-web cadus2-worker cadus2-report-worker
cadus2-edge`. `scripts/deploy.sh` does NOT work on this box (it expects the
repo's own compose + `.env`; see H-7).

**Grind progress at handover:** foundations 285/285 (100 %), geometry enrolled,
wave 1 (6 topics) authoring. `right-triangle-trig` etc. are geometry topics.

---

## 2. Outstanding issues, in priority order

### H-1 — Ungraded attempt with a unit contract: verify the learner gets format guidance
- **Evidence:** event `s_2026-09-14bc-lesson-right-triangle-trig-1`
  (topic `right-triangle-trig`, problem "An acute angle θ satisfies
  cos θ = √3/2 …") came back `outcome.ungraded.reason = "a unit is missing"`.
  Caveat: the submitted answer was junk injected by the grind driver ("0"/"1"),
  so the refusal itself is likely CORRECT behavior.
- **What to check:** a legitimate learner answer WITHOUT the degree/unit mark
  (e.g. `30` vs `30°`, or `pi/3` vs `π/3`) on a unit-contract problem — does the
  reply teach the format (like ISSUE-2/3 guidance), or is it a silent
  `Not marked` loop? Query: `SELECT payload->>'submitted',
  payload->'outcome'->'ungraded'->>'reason' FROM events WHERE
  payload->>'attempt_id' = 's_2026-09-14bc-lesson-right-triangle-trig-1';`
- **Fix if confirmed:** unit-contract refusals should name the missing unit in
  the reply (`feedback` / `re_solve` path), same spirit as the ISSUE-2 fix.
- **Acceptance:** a test in `crates/web/tests/` (or a core answer-grammar test)
  proving a unitless-but-correct answer gets actionable feedback; the
  `right-triangle-trig` ungraded counter can be cleared via the admin path.

### H-2 — Ungraded attempts have no learner-side resolution (NOTE D)
- 3 attempts sit in `status.ungraded` / `ungraded_attempts`; the learner sees a
  "not marked" tile and can do nothing. The only repair path is
  `POST /api/admin/ungraded/{attempt_id}/regrade` (admin, D-F2 recovery).
- **Fix (needs a product decision first):** either (a) ungraded attempts
  expire/are re-asked on the next session of that topic, or (b) the learner
  gets a "re-answer" action, or (c) they stay admin-only but the dashboard
  explains what "not marked" means. `REQUIREMENTS.md` D-F2 owns the semantics —
  read it before coding.
- **Acceptance:** a learner-visible path (or documented decision + UX text)
  covering the 3 known attempts; `status.ungraded` reachable to 0 without SQL.

### H-3 — Hint-ladder coverage is partial (NOTE B)
- 55 approved hint ladders total against ~800 foundations KPs (and ~3,000 KPs
  curriculum-wide). `POST /api/task/{id}/hint` answers `no_hint_ladder` for
  most review/drill tasks. The waves author hints only for frontier topics.
- **Fix options:** enable a bulk `cadus-worker author --kind hint_ladder`
  backfill (model env now works: `deepseek/deepseek-chat` +
  `OPENROUTER_PROVIDER_ORDER=deepinfra,fireworks` in `/home/deploy/homelab/.env`),
  or gate the SPA's hint affordance on availability.
- **Acceptance:** `cadus-worker readiness --course <id>` reports the hint
  blocker count for every course the learner can reach, and the hint endpoint
  answers 200 on the learner's scheduled tasks.

### H-4 — The `readiness` report disagrees with the selector (unformalized finding)
- Observed 2026-09-14: `cadus-worker readiness --course foundations` reported
  `knowledge points ready 0, blocked 809, blocker teachable 809` while the web
  selector was serving lessons for hundreds of those same KPs. One of the two
  reads the content store wrongly. Suspects: the worker's readiness pass uses a
  different `ContentIndex`/review-context than the web's
  `approved_index_current` (`crates/store/src/content/index.rs` vs whatever
  `crates/worker/src/authoring/cli.rs` feeds), or it misses the boot
  re-stamp (`crates/web/src/bin/cadus-web/main.rs`, F-grind-10).
- **Why it matters:** the report is the operator's map of what content to
  author next; a report that says everything is blocked is worse than none.
- **Acceptance:** `cadus-worker readiness --course foundations` agrees with
  the session plan's `blocked` list for a live learner (write a test that
  builds both from the same fixture); investigate `regate_with_policy` /
  `approved_index` vs `approved_index_current` first.

### H-5 — A drill's final answer reports `task_status: "continue"` (NOTE A)
- The last answer of every drill returns `task_status: continue, next: null`
  while the plan already marks the task `done: true`; the SPA can never show
  the completion moment for drills. Look at `crates/web/src/grade/route.rs`
  (the drill close path, `task_moved_on`) vs the review close path which does
  return `task_passed`.
- **Acceptance:** a drill's 20th answer returns `task_status: "task_passed"`
  (or an explicit drill-closed status) with a test in the drill lifecycle
  suite.

### H-6 — Honest-ETA framing (NOTE E)
- `velocity.eta` extrapolates a hard date from a 28-day window; early learners
  see multi-year dates. Product/UX decision, not a bug. Options: range, pace
  wording, or hide until N topics practiced. `REQUIREMENTS.md` D-F6 owns the
  honesty framing.

### H-7 — `scripts/deploy.sh` does not match this server (NOTE C)
- The script assumes the repo's `docker-compose.yml` + repo `.env`; the live
  stack runs from `/home/deploy/homelab` (project `homelab`) with hand-built
  images and `container_name`s. Deploys here are manual (see §1 order), which
  is how the box once served a build one commit behind HEAD (ISSUE-8's stale
  404). **Fix:** either parametrize `deploy.sh` (compose dir + env file as
  args) or replace it with the documented manual order in
  `docs/SELF_HOST.md`. **Acceptance:** one command deploys HEAD on this box
  and fails loudly when the tree is dirty.

### H-8 — Orchestrator/driver hardening (evaluation infra, not product)
- `/tmp/orchestrator.py` is throwaway-quality but load-bearing for the grind.
  Known gaps: (a) on `unknown_task` serve errors it skips instead of re-fetching
  the plan (962 such errors in one log); (b) it is not in git; (c) `make_due`
  must keep using the stdin path for the model rewrite (`db_stdin`) — the argv
  path crashes past ~128 KiB (Errno 7). If the grind continues, move the script
  into the repo under `scripts/` (it contains no secrets) and fix (a).

---

## 3. Standing rules (do not break)

- **Events are append-only** (C3). The grind deleted events exactly twice,
  both times to un-poison the throwaway eval user (`learner.eval@example.com`)
  — never for a real account, and never as a fix for product behavior.
- **Approvals are currency-stamped**: `approved_review_engine_digest` +
  `approved_curriculum_digest` + per-row `approved_template_context_digest`.
  A deploy that moves either digest re-stamps at boot (F-grind-10 sweep in
  `crates/web/src/bin/cadus-web/main.rs`). If you change the review-engine
  source, expect the sweep to log a count at boot — it is NOT an error.
- **Template authoring grammar limits** (learned the hard way): variable
  exponents are outside the decidable grammar ("an exponent that is not a whole
  number"); literal LaTeX braces must be doubled (`{{`/`}}`); placeholders are
  `{param}` (TRIPLE braces `{{{a}}}` when a literal brace must wrap a value);
  the sample grid must yield ≥ 12 distinct problems (space-floor); every
  sample's `expected` must equal `answer_expr` evaluated at those params
  (sample-agreement). Validate via `GET /api/admin/content/{digest}` — its
  `gate.rejected` and `sample_instances` fields are the fastest feedback loop.
- **One full gate suite at a time** on this box; tests need the nix gcc wrapper
  and `CADUS_TEST_DATABASE_URL`.
- The provider rate limits (429 `upstream_provider_shared_pool`) are the
  grind's throughput ceiling. Rotation across pools is implemented in
  `author_kp()`; the next lever is a BYOK OpenRouter key (the 429 remedy hint
  says keys accumulate limits).

---

## 4. Where the log of record lives

- `ISSUES.md` — 11 numbered issues (all fixed, status header at top) + notes.
- `/tmp/orchestrator.log` — the grind log (copy it somewhere durable if this
  box reboots; `/tmp` does not survive).
- Geometry wave 1 is in flight at handover: 6 topics authoring
  (`triangle-area`, `points-lines-planes`, `distance-midpoint-formulas`,
  `circle-circumference`, `scale-drawings`, `translations-coordinate-plane`).
  11 more courses follow geometry in `COURSES` order inside the orchestrator.
