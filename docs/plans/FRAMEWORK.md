# Cadus 2.0 framework plan

Input: `ASSIGNMENT.md` (the owner's text), `audit-2026-09-06.md` (the revalidation),
`REQUIREMENTS.md`, `docs/DECISIONS.md`, `docs/reference/undecidable-answers.md`, the
book pages 209-211, 219-221, 278-279, 375-380. The plan becomes `docs/plans/FRAMEWORK.md`
on the branch `framework/main` (from `main` after the quality gate lands).

## Ground rules

- Branch `framework/main` from `main`; units in worktrees `~/.cache/cadus2_wt/f<N>-<name>`
  on branches `framework/f<N>-<name>`; merge into `framework/main`; `scripts/gate.sh` and
  `scripts/quality.sh` both green before a merge to `main`.
- Every unit keeps the ten quality limits (`docs/plans/QUALITY.md`).
- Events: a changed contract bumps `SCHEMA_VERSION` (2) with a read shim for v1 rows; a
  changed fold bumps `PROJECTOR_VERSION` (4) and replays. Original rows never change.
- Content approval: `pending` never serves; a human approves by digest. No code writes
  `approved`.
- Write tests run on the throwaway cluster `cadus2-testdb*`; the live `cadus2-db` stays.
- The checklist `docs/plans/FRAMEWORK-CHECKLIST.md` maps each finding (a)-(p) and each
  required item to: implementation (commit), tests (file), content evidence, residual.

## Design decisions (proposed, cite the audit)

D-F1 Answer contract per item, not per topic. Keep `answer_kind` as the TASK COMPLEXITY
of a topic (`numeric`, `expression`, `multi-step`, `proof`). Add `answer_contract` per
exemplar and per template answer: `exact` (default), `approx {decimals|tolerance}`,
`unit {quantity, unit}`, `quotient_remainder`, `coordinates`, `set`, `multipart [contracts]`,
`none` (proof: ungraded). The grade route grades by CONTRACT (audit 3a, 3b). A `multi-step`
topic with an `exact` final answer is gradable. Inventory Foundations first (a script that
lists the shapes of the 1,695 exemplar answers per topic).

D-F2 Third outcome. `Attempt` gains `outcome: Correct | Incorrect | Ungraded { reason }`;
`correct: bool` stays for v1 readers (derived). Ungraded attempts: no FIRe update, no
`kp_progress` step, not in `kp_passed` sequences, no XP, no misconception diagnosis, an
`ungraded` counter on the dashboard, an admin list with a "regrade" path (a `regraded`
event exists already). Web verdict maps `Outcome::Undecidable` to `Ungraded` (audit 3c).
SPA shows "not marked" with the reason and the next task; no AI explanation fires.

D-F3 Grammar. Add rational exponents (`a^(p/q)` -> radical canon, bounded q), the `q R r`
production into the tuple, `value unit` with a unit table for Foundations, coordinates
`(x, y)`, unordered sets `{a, b}`. Keep the input cap and the no-search rule (R3).
Equivalence stays symbolic (canonical forms), never numeric sampling (assignment 1.3).

D-F4 Reasoning. Structured steps only where a contract asks (`multipart`, integrated
tasks). `work` text stays stored; the model-assisted diagnosis (A4) runs only on
`Incorrect`, never on `Ungraded`; its output is labeled "model diagnosis, uncertain".

D-F5 Readiness. A core function `readiness(kp, content_index) -> Readiness { teachable,
practicable (>= 3 decidable distinct items), assessable (>= 1 held-out item), hints,
solutions, visuals_needed/present, prerequisites_ok }` and a worker command
`cadus-worker readiness [--course]` that writes a report per course/topic/KP (JSON +
Markdown) by exercising serve, render and grade contracts. The selector serves a lesson
only when its KP is teachable+practicable+assessable; otherwise the task is `blocked`
with a reason and the plan takes the next ready task. `TeachDoc` absent => the SPA shows
a "no instruction yet" card and does NOT serve practice (audit 3j).

D-F6 Mastery claims. Split `is_mastered` into `is_practiced` (Learning) and `is_known`
(Learning | Placed | Floor). Selection uses `is_known`; course completion and the
progress display use `is_practiced` plus "inferred" counts; each inferred topic gets one
direct confirmation item before it counts as practiced (book p.377 conditional
completion; audit 3l).

D-F7 Progression policy. Parse `lesson.kp_pass` ("2consec|3of4") into `PassRule`; the
gate reads it (audit 3k). Review grading: per-question skill attribution; when the
weighted score and the last answer disagree, the review ends `Inconclusive` and schedules
one targeted confirmation item per skill in doubt instead of pass/fail (audit 3n). Every
threshold gets a doc line "why" and a `calibrate: yes|no` mark.

D-F8 Independent practice. After an assisted-correct or an incorrect attempt: the SPA
shows the solution, then a "Done studying" step hides it; the server serves a FRESH
structurally similar item (another exemplar or template instance of the same KP, never
the same problem digest); a correct fresh answer records `assisted: false,
independent_after_feedback: true`; the same digest counts as `repeat` and gives no fresh
credit; a delayed confirmation item is scheduled after >= 3 intervening tasks (audit 3m).

D-F9 Evidence at skill level. `Attempt` gains `item_digest`, `item_source`
(exemplar|template|integrated|probe), `exposure` (first|repeat), `timing_reliable: bool`
(reuse the existing timer safeguards), `skills: [kp ids exercised]`.

D-F10 Integrated tasks. New content kind `integrated` (scenario, quantities, method
choice, steps each with a contract, final answer with interpretation, hint ladder). The
serve route serves one integrated task as ONE problem with step fields; the grade route
grades per step and final; `TaskType::MultiStep` keeps its name and serves an integrated
item when one exists for the component set, else the old per-component path (audit 3o).
A hand-authored Foundations set in `curriculum/foundations/integrated/*.yaml`: rates and
units, percentages and ratios, algebraic constraints, graph reading, geometry, and the
workforce set (person-minutes, service windows, staffing lower bounds, feasibility).

D-F11 Delayed retention. New event `retention_probe` {kp, delay_days, item_digest,
correct/ungraded, assisted, exposure}; the selector schedules an unseen item per KP at
7, 30 (and 90) days after the lesson pass, one per session at most; a report route
`/api/report/retention` and a dashboard card: retained accuracy by delay, assistance
dependence, placement error (inferred topics that failed confirmation), integrated-task
performance. Every number carries its provenance (first exposure, assisted).

D-F12 Policies versioned. `Config` gains `policy_version`; review intervals, inference
weights, pass rules, probe delays live under it; the config hash already detects drift.
`docs/plans/CALIBRATION.md` states the protocol: which numbers, which delayed outcomes,
the minimum sample, and that software tests are engineering evidence only.

## Units

Phase 1 (contracts and the third outcome)
- f1-inventory: script + report of Foundations answer shapes per topic; the contract
  table; the list of the 78 multi-step topics with their final-answer contracts.
- f2-grammar: rational exponents, `q R r`, units, coordinates, sets; fixtures; the
  residue document regenerated.
- f3-contract: `answer_contract` in the curriculum model and loader; the lint rule;
  Foundations YAML annotated by the inventory (default `exact`).
- f4-outcome: `Attempt.outcome`, `SCHEMA_VERSION` 2 + shim, projector rules,
  `PROJECTOR_VERSION` 4, the web verdict, the kind gate removed (contract gate instead),
  the SPA "not marked" state, the admin ungraded list, the analytics counters.
- f5-acceptance-1: the regression cases of the Phase 1 list as tests.

Phase 2 (readiness and content)
- f6-readiness-core: `Readiness`, the selector eligibility, blocked tasks, the SPA card.
- f7-readiness-cli: `cadus-worker readiness`, the report, the operator flags.
- f8-content-foundations: an authoring pass against a model. The live worker container
  `cadus2-worker` carries `OPENAI_BASE_URL` (OpenRouter), `OPENAI_MODEL`
  (`deepseek/deepseek-v4-pro`) and a key; `cadus2-web` carries none (checked 2026-09-06,
  values not printed). So `docker exec cadus2-worker cadus-worker author --dry-run`
  prints the plan at no cost, and a real pass writes `pending` rows into the live
  `content_store`. Cost control: run one Foundations unit slice first (about 80 KPs), read
  the T3 spend line, then ask the owner before the whole course (809 KPs x 4 kinds). The
  human approval queue at `/review` stays the gate; approvals are pending work.
- f9-item-variety: item identity, exposure, held-out assessment families; template
  validation (boundaries, degeneracies); visuals (number line, fraction, coordinate,
  geometry) as SVG components with text equivalents.
- f10-prereq-audit: prerequisite and diagnostic coverage check; assumed-mastery
  inventory; confirmation items.

Phase 3 (independent practice and progression): f11-rework, f12-pass-rule,
f13-review-inconclusive, f14-mastery-split, f15-speed.

Phase 4 (integrated tasks): f16-integrated-model, f17-integrated-serve-grade,
f18-foundations-integrated-content.

Phase 5 (measurement and operations): f19-retention, f20-policy-version + calibration
doc, f21-recovery-flows tests, f22-browser-checks (Playwright container), f23-ops
(migration 0013+, rollback notes, readiness in `/api/ready`).

## Order

f1 -> f2, f3 (parallel) -> f4 -> f5; then f6, f7, f14 (parallel); f8 needs the key;
f9, f10, f11, f12, f13, f15 in pairs; f16 -> f17 -> f18; f19, f20, f21, f22, f23 last.
First integration milestone: a Foundations slice (one unit, e.g. `01-fractions-decimals`
topics 1-10) end to end with f1-f7 + f11 + f16-f17 on a staging deployment.
