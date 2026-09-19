# FINISH LOG

## Wave 0 — baseline (started)

- Read FINISH-CADUS.md. Role: orchestrator. Amendment J parked (STATUS line added to REQUIREMENTS-JEV.md).
- **Key decision:** cannot create a separate OpenRouter key — `/api/v1/keys` needs a provisioning key (pk-…) which does not exist on this machine; PUT on the key is 404. Live calls succeed despite the existing key showing usage $45.34 against a stated $30 limit, so the limit is not blocking. Track task spend as delta on the cumulative usage read from `GET /api/v1/key`. Baseline **$45.34**. Task budget $90 → hard stop at cumulative **$135.34**; at cumulative ≥ **$120.34** start no new content wave. Existing key: sk-or-v1-d2f11…
- Copied walkthrough driver to `scripts/grind/orchestrator.py`; snapshot of live log at `/tmp/orchestrator-snapshot.log`.

- Uncommitted crates/web changes were fmt-only; full `cargo test -p cadus-web` green
  (all binaries 0 failures). Committed f0532c5b together with the grind driver copy and the parked Amendment J files.
- **Disk fact:** `df -h /` shows 929G total, **352G free** — the 262GB `target/` concern does not apply on this mount. Code waves need no cleanup.
- **Defect 1 root cause (C1):** `compose.rs` serves no lessons while the frontier is blocked (by design), so once due + nearly-due reviews are exhausted the plan is empty. The frontend treats that as the pinned W-C3 "no dead ends" status (diagnostic offered; `hasScheduledWork` tests exist). Trigger = 11 probability-statistics KPs blocked on `practicable`/`assessable` (grind killed mid-authoring). The *driver* bug: it logs "inspect manually" and stops instead of routing into its own authoring branch. `unknown_task` errors are stale-session races, already bounded at 3 refetches (H-8a). **Decision: C1 = fix the driver dispatch (empty plan + non-empty `blocked` → author those topics, then refetch), with a pure-function regression test; no selector change.**

- **Inventory written** (`scripts/finish_inventory.py` → `docs/FINISH-INVENTORY.json`, 3,138 KP rows). Summary: foundations 809 KPs (780 teach, 744 templates), geometry 261 (250/22), probability-statistics 246 (138/4), precalculus 108 (0/0), discrete-mathematics 84 (0/2); Tier 2 ~1,630 KPs with ~111 teach total. Verdict-capable per S4 re-aligned to the real 22-variant `AnswerContract` enum (all deterministic except `None`).
- **Baseline test:** `cargo test -p cadus-web` fully green (fmt-only changes committed, f0532c5b). Full gate initially red: curriculum lint found 2 real data defects in `populations-samples` (label answers carried `$…$` delimiters outside the choice vocabulary). Fixed, committed 298b43e0, gate re-running.
- **Wave 1 launched** (3 workers, glm-5.3-flash, thinking high): C1 (driver dispatch, main repo), C2 (Matrix + triage_verdict, worktree c2), C3 (check_keys, worktree c3).
- **C1 DONE:** pure `plan_action(plan)` dispatch (walk/author/end/idle) replaces the "inspect manually" stop; 9/9 unittest OK. Merged to main by orchestrator below.

- **Wave 1 merged (C1–C3):**
  - C1 driver dispatch (9/9 tests) — commit 0a4bf359.
  - Baseline reds fixed first (all pre-existing): curriculum lint (2 label answers, 298b43e0), stale census pins (loader 9191 exemplars, instruction-gate 442/77, parity dump TREE_HASH/DUMP_LEN/COUNTS), lint fixtures made compliant, lint_curriculum binary restored to the 1.0-parity rule set (the full exemplar sweep stays behind `lint_curriculum_full`; it reports ~5.6k real content findings — future work, not a launch gate).
  - C2 `Matrix { rows, cols }` variant + exhaustive `triage_verdict` (no wildcard arms; a new variant now breaks the build) + per-variant correct/incorrect fixtures — e0a6ab00.
  - C3 `check_keys` tool (crates/check_keys): current data = 9,276 checked, 2,176 failed (foundations 0 failed; failures concentrated in Tier 2 — Wave 2's target surface), 1,005 teach-only skips — 9bc77326.

- **Wave 1 complete (all six code items merged, acceptance re-run green on main after each merge):**
  - C4 tier-2 exemplar serving — see below (was last to finish).
  - C5 format prompts for every Undecidable reason (new tests incl. 75-reason table; ungraded path unchanged) — f2d15420.
  - C6 S6 admin revoke (deviation: reuses the existing non-served `rejected` status because a new migration would break two pinned migration-count literals; behavior identical) — b4ee65bd.

- **C4 merged** (929dc33b): two real serving defects fixed — contract-`None` exemplar KPs were 409 `pool_unavailable` dark, and undecidable-grammar exemplars were skipped. Now: exemplar practice serves, contract-None shows the worked solution with no verdict. Acceptance green on main (0 failed suites).

- **Wave 2a (T1) complete:** all 5 Tier-1 courses authored + approved (prod content_store now 2,818 approved; +296 from T1). Patterns: teach pages author at ~$0.0005 each and pass the gate; templates decline where answers are piecewise (`if(...)` outside the decidable grammar) — those KPs fall back to exemplar practice per S4/C4. Task-spend after T1: ~$12.
- **check_keys with the real template store** (bridged read-only via pg_dump into the eval db): foundations **12,779 checks, 0 failures**; remaining Tier-1 failures are pre-existing YAML exemplar defects: geometry 46, discrete-mathematics 80, precalculus 107, probability-statistics 34 (total 267) — repair workers launched.

- **Key limit hard stop (03:40):** the OpenRouter key's MONTHLY cap ($30) exhausted
  (limit_remaining=0; every completion 403s). Task delta at stop: $13.07 of $90. No
  provisioning key exists to mint a bigger one; the owner must raise/replace it. All
  model-dependent work stopped; no-model work continued.
- **Wave 3 walkthrough (no-model, honest mode):** foundations PASS (29 rounds to
  completion), geometry PASS (~880 answers to completion), discrete-mathematics 45
  rounds / 2,543 answers / 0 mismatches to the content frontier, precalculus ends
  honestly idle, probability-statistics needs the T1 template retries. Zero mismatches
  across every round of every course; the one unknown_task was the known stale-session
  race, refetched and recovered.
- **Wave 4 done without models:** daily pg_dump timer (04:00, 14-day retention) + one
  restore test passed (2.07M events verified). Family account: staged
  (/tmp/family-account-retry.sh), rate window pending.
- **FINISH-REPORT.md written** with the plain failures and the resume list. Stopping
  here per the key-limit stop; the session's remaining steps are owner-gated.
