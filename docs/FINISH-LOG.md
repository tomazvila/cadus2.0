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
