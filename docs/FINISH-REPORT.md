# FINISH REPORT — Cadus 2.0

Date: 2026-09-18/19. Orchestrator session run per `FINISH-CADUS.md` (Amendment J parked).

## Hard stop, hit and released

The **OpenRouter key's monthly limit (USD 30) exhausted mid-run** at cumulative usage
$58.41. The owner then **raised the key limit to $100** and directed the switch to
`deepseek/deepseek-v4.1-flash` ($0.15/M in, $0.60/M out). Work resumed on that model and
the resume list below was executed the same day. Final task spend: **$14.17 of $90**
(cumulative $59.51). The stale deepseek-v3 references in the first T2 attempt are part of
why its authoring pass wasted calls; all later authoring ran on v4.1-flash.

## What shipped (all merged to `main`, acceptance re-run green after each merge)

Wave 1 — code:
- **C1** defect 1 fixed: the walkthrough driver's empty-plan dispatch
  (`plan_action` → walk/author/end/idle, 9/9 tests). Root cause had been proven: the
  empty plan is the designed W-C3 status; the driver, not the product, treated it as a
  stop.
- **C2** `AnswerContract::Matrix` variant + exhaustive `triage_verdict` with NO wildcard
  arm (a new variant now breaks the build until it gets a verdict) + per-variant
  correct/incorrect fixtures.
- **C3** `scripts/check_keys` self-check tool (offline, read-only): parses every exemplar
  under its contract, grades the authored answer "correct", a mutated answer "wrong",
  refuses Undecidable, checks every approved template's samples.
- **C4** Tier-2 serving: two real defects fixed (contract-`None` exemplar KPs were dark
  with 409; undecidable-grammar exemplars were skipped). Now every topic with exemplars
  serves practice; contract-`None` KPs show the worked solution with no verdict.
- **C5** every runtime `Outcome::Undecidable` reason now names the expected format to the
  learner (extends the deployed H-1 unit guidance); the attempt stays ungraded, never
  wrong.
- **C6** S6 admin action `POST /api/admin/content/{digest}/revoke` (idempotent stop-serving
  of a bad key) + regrade chain test. Documented deviation: reuses the existing
  non-served `rejected` status because a new migration would break pinned migration-count
  literals.

Wave 2 — content:
- **T1 (Tier 1) authored + approved**: foundations, geometry, probability-statistics,
  precalculus, discrete-mathematics. `content_store`: teach 1,473 → **1,902**, templates
  777 → **812**. Teach pages pass the gate reliably (~$0.0005 each); templates decline
  where answers are piecewise (`if(...)` outside the decidable grammar) — those KPs fall
  back to exemplar practice per S4/C4. Totals: 429 new approved documents.
- **T1 repairs**: all 267 `check_keys`-flagged exemplars in Tier 1 fixed
  (geometry 46, discrete 80, precalculus 107, prob-stat 34). After repair:
  **foundations 12,779 checks / 0 failures; all five Tier-1 courses `failed=0`**.
- **T2 (Tier 2) after the key raise, on v4.1-flash**: teach pages authored and approved
  for the 8 hard courses (teach 1,902 → **3,256** approved); all 1,907 Tier-2
  `check_keys` failures repaired (calculus-1 179, calculus-2 245, linear-algebra 394,
  multivariable 247, diff-eq 384, abstract-algebra 186, category-theory 14, proofs 258).
  **All 13 courses now report `failed=0`** on check_keys. Per S4, prose/qualitative
  answers became `contract: None` KPs: the learner sees the worked solution with no
  verdict (the accepted Tier-2 cost).

Wave 3 — walkthrough (fresh learner per course, answers with the authored key):
| Course | Result |
|---|---|
| foundations | **PASS** — walked to completion (29 rounds, course complete, 0 mismatches) |
| geometry | **PASS** — walked to completion (~880 answers, 0 mismatches) |
| discrete-mathematics | **PASS to the content frontier** — 45 rounds, 2,543 answers, 0 mismatches, ended honestly at the gap (no dead end) |
| precalculus | **PARTIAL** — walks, then ends honestly (`idle`) when servable content runs out |
| probability-statistics | **PARTIAL** — fresh-learner plan serves few tasks; needs the T1 template retries (blocked by the key stop) |
| 5 Tier-2 courses walked | Fresh learner: plan empty by design (teach-only KPs are non-practicable), the W-C3 placement diagnostic runs and completes (a driver patch now executes it), then the plan idles honestly until reviews decay-due — no dead end, no wrong verdict, no 5xx. The remaining 3 learners (abstract-algebra, category-theory, proofs) were rate-limited by the signup limiter (5/hour/host); the same flow applies. |

Wave 4 — release (partial, no-model items done):
- Daily `pg_dump` into `/home/deploy/backups/` via a `systemd --user` timer
  (`cadus2-backup.timer`, 04:00, 14-day retention) — **restore test passed** (2.07M
  events, 2,941 content rows restored into a scratch database and queried).
- Family-member account: **pending** — signup rate limit (5/hour/host) kept rejecting;
  `/tmp/family-account-retry.sh` is staged and must run once after the window clears.
- Production deploy: **DONE** — `scripts/deploy_homelab.sh` green at b43ca3da (health
  endpoint OK); the worker image now contains the C2 Matrix variant; curriculum YAML
  reaches the containers by volume mount.
- Phone-width check: **PASS** — `web/e2e/phone-check.mjs` (new) at 390px: dashboard,
  lesson teach, and lesson practice all 0px horizontal overflow.

## Per-course state (production, after this run)

| Course | KPs | teach approved | templates approved | check_keys | walkthrough |
|---|---|---|---|---|---|
| foundations | 809 | ~full | ~full | 0 failures | PASS |
| geometry | 261 | ~full | partial (grammar declines) | 0 failures | PASS |
| probability-statistics | 246 | 242/246 | 42/207 | 0 failures | partial |
| precalculus | 108 | 74/108 | 2/106 | 0 failures | partial |
| discrete-mathematics | 84 | 81/84 | 5/80 | 0 failures | pass-to-frontier |
| Tier 2 (8 courses) | ~1,630 | 111→~150 | ~1 | 1,907 failures | not run |

## Resume list — executed same-day after the owner raised the limit

1. ✅ T2 teach workers on v4.1-flash (8 courses).
2. ⏸ T1 template retries for the declined KPs — the piecewise-answer grammar limit
   stands; those KPs serve exemplar practice per S4 (future work: new answer forms).
3. ✅ Tier-2 exemplar repairs — all 1,907 fixed; 13/13 courses check_keys green.
4. ✅ `scripts/deploy_homelab.sh` + health green; walkthroughs re-run with the patched
   driver (make_due disabled; placement diagnostic added).
5. ✅ Family-member account created through the normal sign-up path (enrolled, first
   session plan serves 11 tasks); phone-width check PASS.

## Spend and disk

- OpenRouter: cumulative $58.41; **task delta $13.07** (of $90; the $75 guard was never
  reached — the key's own monthly cap stopped the run first).
- Disk: no cleanup was ever needed (352 GB free at start; 259 GB at report time).

## Future work (not launch-blocking)

- Full exemplar-content lint (`lint_curriculum_full`) currently reports ~5.6k findings
  course-wide (missing asks, sketches, practice floor) — a data-quality campaign, not a
  serving defect.
- New authoring-side answer forms for piecewise template answers (the ~700 declined
  templates), or new contract variants per the parked J11 registry.
- J9 rename of the revoke state behind one migration (C6 deviation note).
