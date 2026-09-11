# Framework handover (2026-09-06)

The state of the math-learning framework assignment (`~/.cache/cadus2_scripts/framework/ASSIGNMENT.md`
on the box; the plan is `docs/plans/FRAMEWORK.md`, the audit is
`docs/reviews/FRAMEWORK-audit-2026-09-06.md`, the checklist is
`docs/plans/FRAMEWORK-CHECKLIST.md`). The owner stopped the work after the first swarm
of six units, with the instruction "do not pick up the next task, provide a handover".

**Read section 8 first.** Sections 1 to 6 describe the first swarm (`4f6ddb1`), and
section 7 records the historical `3f68d53` completion snapshot. Section 8 is the
current integrated closure audit and supersedes the status and next-step claims in
sections 5 to 7.

## 1. Where the code is

| Branch | Holds | State |
|---|---|---|
| `main` (2ff3c1f) | the quality-limit work: ten code limits on the whole tree, `scripts/quality.sh` PASSED, `scripts/gate.sh` exit 0 | released |
| `framework/main` | the plan, the audit, the checklist, and the six units below, merged | at 4f6ddb1 (all six units merged): `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -D warnings` clean, `cargo test --workspace` green (every test binary `ok`, 0 failed), `npm test` green (878 SPA tests), `npm run check` green at the previous merge; see section 4 for what was NOT run |
| `framework/f1-inventory` … `framework/f14-mastery-split` | the six unit branches, each merged into `framework/main` | keep until `framework/main` merges to `main`, then delete |

Worktrees: `~/.cache/cadus2_wt/framework` (the integration branch) and one per unit under
`~/.cache/cadus2_wt/f<N>-<name>`. Prompts and the shared rules: `~/.cache/cadus2_scripts/framework/`
(`unit-header.md` holds the rules every unit follows; `f<N>-<name>.md` holds each task).

## 2. The six units that landed

| Unit | Design decision | What it does | Evidence |
|---|---|---|---|
| f1-inventory (c774a32) | D-F1 | The answer-shape inventory of Foundations: 1,695 exemplar answers classified into 16 shapes with the real grammar; the checklist | `crates/core/tests/answer_inventory.rs`; `docs/reports/foundations-answer-inventory.md` |
| f2-grammar (fea794f) | D-F3 | Rational exponents, quotient-and-remainder, value-with-unit, sets versus lists, coordinates tests; the corpus residue drops from 265 to 235 of 3,492; 31 recovered rows in `recovered_2_0.jsonl` | `crates/core/tests/answer_{rational_exponent,remainder,unit,set,coordinates}.rs`; `docs/reference/undecidable-answers.md` §5 |
| f4-outcome (f60d5ee) | D-F2, D-F4, D-F9 | The third grading outcome end to end and the event contract v2; see section 3 | `crates/core/tests/events_outcome.rs` (10), `crates/web/tests/grade_route_outcome.rs` (6), `crates/web/tests/admin_ungraded.rs` (8) |
| f6-readiness (b6a85d2) | D-F5 | `crates/core/src/readiness/`: per knowledge point teachable, practicable, assessable, hints, solutions, prerequisites, visual; the selector serves a lesson only when ready and reports `SessionPlan::blocked`; the serve route answers `409 no_instruction` and the SPA shows "No instruction yet" and never falls to practice; `cadus-worker readiness` writes the report; `/api/operator/flags` carries the counts | `docs/reports/readiness-foundations-2026-09-06.md`: 809 knowledge points, 0 ready, 809 blocked on `teachable` (empty `content_store`), 274 fail a contract (the `multi-step` kind) |
| f12-pass-rule (2f0713c) | D-F7 part 1 | `lesson.kp_pass` is parsed into `PassRule` (`<n>consec`, `<k>of<m>`, `\|`); the knowledge-point rule reads it; a bad string fails the config load; the config hash stays `797575e985c12149` | `crates/core/tests/projector_pass_rule.rs` (12 tests) |
| f14-mastery-split (3de9b18) | D-F6 | `is_practiced` (Learning) versus `is_known` (Learning, Placed, Floor); selection uses known, course completion and progress use practiced; each inferred topic gets one confirmation item (a one-item review, at most two per session); `/api/status` carries `mastery { practiced, inferred, total, to_confirm }`; the dashboard shows the three numbers | `crates/core/tests/selector_confirm.rs` (13 tests); the parity fixtures fold with `mastery.confirm_inferred = false` |

Two facts from f1 that change the plan:

- 19 of the 78 `multi-step` Foundations topics become gradable with an `exact` contract
  and no grammar change; 233 of their 464 exemplar answers decide today.
- The two largest missing grammar productions are `label` (147 answers: `x = 4`,
  `area = 12`) and `disjunction` (57: `x = 2 or x = 3`). D-F3 did not list them. They are
  the next grammar unit.

A fact from f6: no Foundations knowledge point reaches the bar of three distinct
decidable practice items today (146 hold 0, 58 hold 1, 548 hold 2, 57 hold 3 before f2;
138, 52, 561, 58 after f2). Content authoring, not code, closes that.

## 3. Unit f4-outcome

The event contract v2 is commit `205c5d9` on `framework/f4-outcome`; every later unit
builds on it.

- `Attempt` gains `outcome: Correct | Incorrect | Ungraded { reason }`; `correct: bool`
  stays and equals `outcome == Correct`. It also gains `item_digest`, `item_source`
  (`exemplar | template | integrated | probe`), `exposure` (`first | repeat`),
  `timing_reliable`, `skills`, `independent_after_feedback`. `ReviewResult` gains
  `inconclusive` (unit f13 sets it). `RegradedAttempt` gains `outcome`. New event type
  `retention_probe` (a no-op in the fold; unit f19 gives it a handler). `LearnerModel`
  gains `ungraded_attempts` per topic and `ungraded`, the last 20 `(attempt_id, topic, reason)`.
- Wire rule: the writer skips a field at its default and skips `outcome` when `correct`
  spells it, so a v1 row keeps its bytes (C2). `SCHEMA_VERSION` is 2; the reader
  accepts 1 and 2 and writes back the version it read; the shim is serde defaults plus
  `Attempt::normalize()`, called from `Event::from_json` and from the store's `jsonb`
  decode. A v1 row keeps `exposure: None` and `timing_reliable: None`: the reliability of
  old evidence is not reconstructible.
- `PROJECTOR_VERSION` is 4 (a full replay). The parity comparisons restamp the fold with
  the 1.0 version through `common::events::oracle_stamp`. Unit f14 had kept version 3
  for the same digests; the merge takes f4's bump and f14's flag stays.
- Fold rules for `Ungraded`: no FIRe update, no `kp_progress` step, not in the
  pass-rule sequence, no XP, no streak effect.
- Web: the answer-kind refusal (`409 undecidable_kind`) is gone; every kind reaches the
  checker; `proof` is ungraded with no checker call. The ungraded reply is
  `{outcome: "ungraded", reason, ...}` with no `correct` key, no solution, no re-solve,
  and `diagnosis.status == "not_offered"` (D-F4). `GET /api/status` carries
  `ungraded_attempts` per topic and `ungraded`. `GET /api/admin/ungraded` lists the
  recovery items; `POST /api/admin/ungraded/{attempt_id}/regrade` with
  `{"outcome": "correct" | "incorrect"}` appends a `regraded` event and replays.
  The metric label `undecidable` became `ungraded`.
- SPA: a "Not marked" feedback state with the reason and a Next button; the dashboard
  shows the ungraded count.
- Traps: a `PROJECTOR_VERSION` bump invalidates every seeded `learner_models` cache in
  tests (`crates/web/tests/common/seed.rs::seed_cached_model` is the one writer); a
  changed `sqlx::query!` text needs `rm -rf .sqlx target/sqlx` and
  `cargo sqlx prepare --workspace -- --all-targets` against a migrated database; the
  store decodes `jsonb` straight into `Event`, so a new read path must call
  `Event::normalize()`.

## 4. What was NOT verified

- The owner relaxed the coverage rule for the framework units on 2026-09-06: tests
  cover the new behavior, not every region. `scripts/quality.sh` was NOT run on
  `framework/main`; its coverage check reports gaps there. The other nine limits were
  kept per unit (files under 500 lines, complexity, dead code, clones, clippy, fmt).
- `scripts/gate.sh` was NOT run on `framework/main` (it runs the release parity tests,
  the benchmarks, `cargo sqlx prepare --check`, the migration check and the ops check).
  Run it before a merge to `main`.
- No browser check, no staging deployment, no run against the live containers.
- The content pass (unit f8) did not run. The live worker container carries a model
  endpoint and a key; `docker exec cadus2-worker cadus-worker author --dry-run` prints
  the plan at no cost. A real pass spends the owner's model credits and lands `pending`
  rows that a human approves at `/review`.

## 5. How to resume

1. Merge check: in `~/.cache/cadus2_wt/framework`, run
   `CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate scripts/gate.sh`
   and `scripts/quality.sh` (both take about an hour). Fix what fails, then merge
   `framework/main` into `main`.
2. Next units, in the plan's order (`docs/plans/FRAMEWORK.md` §Units): f3-contract
   (the `answer_contract` field per exemplar and template, and the contract-based grade
   path: 19 topics unblock at once), a grammar unit for `label` and `disjunction`,
   f5-acceptance-1 (the Phase 1 regression list as tests), f8 content (the dry run,
   then one unit slice, then the owner's decision), f11-rework, f13-review-inconclusive,
   f9, f10, f15, then Phases 4 and 5.
3. Each unit: a worktree from `framework/main`, a prompt file after the pattern of
   `~/.cache/cadus2_scripts/framework/f<N>-<name>.md` with `unit-header.md` pasted in,
   an Opus agent, a merge after clippy and the crate tests.
4. Update `docs/plans/FRAMEWORK-CHECKLIST.md` rows as units land: implementation
   commit, test file, content evidence, residual, status.

## 6. Owner decisions pending

- The model spend for the Foundations content pass (unit f8).
- Content approvals at `/review` after that pass.
- Whether the parity fixtures keep the 1.0 rules behind the two new flags
  (`readiness.enforce`, `mastery.confirm_inferred`) or move to the 2.0 rules with new
  digests (the merge took f4's `PROJECTOR_VERSION` 4 with the restamp helper, see §3).

## Codex answer-contract milestone (2026-09-06)

`framework/codex-answer-contract` contains the exact/approximate f3 slice,
contract-aware template authoring and grading, and 108 explicit exact exemplars
across the 19 inventory topics. See
[`answer-contract-milestone-2026-09-06.md`](../reports/answer-contract-milestone-2026-09-06.md)
for scope and verification. It is isolated from `framework/main` and production;
f3 and the full assignment remain open pending the remaining contracts and
acceptance evidence.

## 7. Historical completion snapshot (`3f68d53`, 2026-09-06)

Branch `framework/completion-docs` at `3f68d53`, 43 commits after `699fa89`. The
checklist reflects this snapshot: 45 rows `done`, 15 `open`, 6 `pending integration`.
This section replaces the unit order of §5 and the decisions of §6.

### 7.1 What landed after the six units

| Area | Commits | Evidence |
|---|---|---|
| Answer contracts (f3, f5): `exact`, `approx {decimals or tolerance}`, `unit`, `quotient_remainder {divisor}`, `coordinates`, `set`, `label`, `multipart`, `required_form`, `list`, `inequality_union`, `none`; finite disjunctions `x = 7 or x = -7`; 314 Foundations exemplars annotated with independent recomputation | `63abbf1`, `7016987`, `56437e1`, `316bdfb`, `003de6c` | `crates/core/tests/answer_contract*.rs`, `answer_disjunction.rs`, `crates/web/tests/grade_route_contract.rs`, `crates/worker/tests/authoring_contract.rs`; `docs/reports/answer-contract-{milestone,shapes,forms,cohorts}-2026-09-06.md` |
| Placement under the third outcome (f4): an ungraded diagnostic answer is recorded without penalty and moves no placement | `9b6c66f` | `crates/core/tests/diagnostic_outcome.rs`, `crates/web/tests/diag_route_outcome.rs`, `web/test/placement.outcome.test.tsx` |
| Content authoring (f8): shared cost ledger and request reserve, fail-fast on permanent endpoint errors, portable schema and decline artifacts, course-scoped rounds, missing-only repair, the zero-cost pilot (3 templates, 3 teach pages, 3 hint ladders, `pending` only, isolated database) | `0c8a11b`, `6ea6753`, `754eb4c`, `4a39a24`, `1daf1a8`, `54b59b7`, `69be5a2`, `2d1dd4b` | `crates/worker/tests/authoring_{budget,fail_fast,portable,course,pilot_drafts,pilot_import}.rs`, `crates/core/tests/instruction_gate_served.rs`; `docs/reports/content-pilot-2026-09-06.md`, `docs/reports/foundations-content-execution-2026-09-06.md`, `docs/content-pilot/*.json` |
| Visuals (f9): number line, fraction, coordinate plane, geometry; exact checks, accessible text, deterministic SVG, sanitizer, the practice card; `visual_present` and `broken_visuals` in readiness | `958b8be`, `8fb5ec0`, `664f4f8`, `f8a6e35` | `crates/core/tests/visual_render.rs` (15), `visual_readiness.rs` (8), `web/test/math-visual.test.tsx` (9) |
| Prerequisite and diagnostic audit (f10): `PrereqCoverage`, the assumed-mastery inventory, the report over 13 courses | `30f0563` | `crates/core/tests/prereq_coverage.rs`, `crates/worker/tests/prereq_coverage.rs`; `docs/reports/prerequisite-coverage-2026-09-06.md` |
| Independent practice and review evidence (f11, f13): the study step hides the solution, a fresh same-KP item, `independent_after_feedback`, the confirmation after three task closes; per-KP review attribution, `inconclusive` reviews with confirmation tasks; server-owned `feedback_practice`; quiz `QuizResult`, gated reveal, fresh practice after the reveal | `03481a3`, `d16d5f5`, `71c9ace`, `3f68d53` | `crates/core/tests/review_evidence.rs` (10), `crates/web/tests/{grade_route_next,review_close,feedback_assessment,quiz_result}.rs`, `crates/store/tests/review_inconclusive.rs`, `web/test/session.feedback-practice.test.tsx`, `web/test/quiz.results.test.tsx`; `docs/reviews/framework-progression-2026-09-06.md` |
| Speed (f15): `cadus_core::timing` (fluent, slow-correct, unreliable), pure and unwired | `f686d7b` | unit tests in `crates/core/src/timing/mod.rs` (6) |
| Integrated tasks (f16, f17, f18): the model, validation, secrecy view, per-field hint ladder, per-step grading; three routes under `/api/task/{id}/integrated`; `integrated_served` and `integrated_attempt` idempotent events; the React screen; seven hand-authored Foundations items including two workforce items; the guarded session wiring with the exact `409 no_integrated_item` fallback | `cbe1543`, `ed800ac`, `6c6515e`, `20f07bd`, `20888d4`, `f901d32`, `0cedaee`, `d243336` | `crates/core/tests/integrated_{item,grade,content}.rs` (21), `crates/web/tests/integrated_routes.rs` (9), `web/test/integrated.test.tsx` (7), `web/test/session.integrated.test.tsx` (8); `curriculum/foundations/integrated/*.yaml`; `docs/reports/session-integrated-wiring-2026-09-06.md` |
| Retention and policy version (f19, f20): `crates/core/src/retention/`, `Config.policy_version` and `policy_digest`, the probe fold with provenance, the 7/30/90-day schedule with one probe per session, the lifetime unseen index, `GET /api/report/retention`, the dashboard card, `PROJECTOR_VERSION` 5 | `d3b9f61`, `5f74846`, `80aea88`, `a904a00`, `3854761`, `38fa607`, `5dc4653`, `438bbfb`, `f2b31b7`, `a4089b7` | `crates/core/tests/retention_probe.rs` (8), `events_outcome.rs`, `parity_events.rs`, `projector.rs`; `web/test/dashboard.retention.test.tsx` (5); `docs/plans/CALIBRATION.md` |

`config_hash` stays `797575e985c12149`. `SCHEMA_VERSION` stays 2. `PROJECTOR_VERSION`
is 5 (`38fa607`): a cached model stamped 4 replays in full on first read.

### 7.2 Verification performed, and what was not

- Per lane, before integration (the lane reports and commit bodies): core, web and
  worker suites green against disposable test databases, Clippy `-D warnings`, fmt,
  the ten quality limits except coverage, SPA suite (880 to 888 tests), `tsc`,
  `eslint`, `npm run invariants`. See `docs/reports/*-2026-09-06.md` §Verification and
  `docs/reviews/framework-progression-2026-09-06.md`.
- On this snapshot (`3f68d53`, the documentation lane, 2026-09-06): `cargo fmt --all
  --check` clean. `cargo test -p cadus-core --no-fail-fast`: 120 binaries, 115 `ok`,
  1,353 tests passed, 5 failed. Every failure is a constant pinned before integration:

  | Test | Pinned | Actual | Cause |
  |---|---|---|---|
  | `events_outcome.rs::a_model_with_no_ungraded_attempt_keeps_the_1_0_wire_shape` | `PROJECTOR_VERSION == 4` | 5 | `38fa607` |
  | `projector.rs::the_fold_stamps_the_projector_version_and_the_config_hash` | stamp `Some(4)` | `Some(5)` | `38fa607` (the same test already asserts the constant is 5) |
  | `projector_regrade.rs::the_projector_version_is_stamped_on_a_minimal_fold` | stamp `Some(4)` | `Some(5)` | `38fa607` |
  | `loader_tree.rs::the_checked_in_tree_uses_no_yaml_1_1_form` | 89 YAML files | 96 | the 7 files of `curriculum/foundations/integrated/` (`0cedaee`) |
  | `parity_oracle.rs::the_binary_writes_the_dump_and_the_hash` | `DUMP_LEN` 4,056,661 | 4,066,943 | the 314 policies (`56437e1`, `316bdfb`; the cohorts report states the new length) |

  Root updates the five pins; no behavior changed. The five baseline binaries of
  audit row (p) all pass.
- NOT run on this snapshot: `cargo test --workspace` (web, store and worker suites need
  `CADUS_TEST_DATABASE_URL`), Clippy, the SPA suite, `scripts/quality.sh`,
  `scripts/gate.sh`, `cargo sqlx prepare --check`, any browser walk, any staging or
  production run, any readiness rerun against a database.

### 7.3 Pending integration (root cherry-picks after this snapshot)

Four patches were finished on sibling lanes while this snapshot was cut. The checklist
marks the rows that depend on them `pending integration`; root replaces the marker
with the hash.

1. Server-authoritative integrated hints, receipt replay, durable task completion:
   the hint route persists the rung exposure and grading reads the server count; a
   retried answer returns the stored receipt; completion survives reload and replan.
   Rows: (o), P4.1, A4.2; residuals of P5.3 and A4.1.
2. `IntegratedAttempt` projector skill credit: the fold credits `skills_credited`
   (decided fields only) into topic state. Rows: (g), (o), P4.1, A4.3; residuals of
   P3.3 and P5.1.
3. Terminal lesson practice closure and drill completion: the configured fail-after
   close and the drill end require fresh independent practice like the other task
   types. Row: P3.2; residual of (m) and A3.2.
4. Arithmetic-core zero-API content batch: 81 teach pages and 81 hint ladders for
   `curriculum/foundations/00-arithmetic-core.yaml` under
   `docs/content-foundations/arithmetic-core/`, the generic importer
   `scripts/authoring/import_local_drafts.py`, and their tests (none of these paths
   exist on this branch yet). Rows stay `open` ((h), P2.3, A2.2): the batch lands
   `pending` rows, and approval is a human step.

### 7.4 Remaining blockers, in order

1. The five stale test pins of §7.2, then `cargo test --workspace`, Clippy, the SPA
   suite, `scripts/quality.sh` and `scripts/gate.sh` on the integrated branch. Nothing
   merges to `main` before that.
2. Content: the deployed `content_store` holds zero rows. The pilot and the batch
   write `pending` rows only. A human approves at `/review`; a paid Foundations pass
   needs the owner's allocation (`docs/reports/foundations-content-execution-2026-09-06.md`).
   Until then every Foundations lesson is blocked by `readiness.enforce`.
3. Readiness rerun: `docs/reports/readiness-foundations-2026-09-06.md` predates the
   contracts, the visuals and the pilot rows. Rerun `cadus-worker readiness --course
   foundations` after integration and approval.
4. The timing module is unwired (P3.1, P3.3, P3.6): one call of
   `cadus_core::timing::read` in the grade path, and `timing_reliable` stops being
   `None`.
5. Visuals need content: no Foundations `visuals:` block is authored, and
   `content_store.kind` has no `visual` kind (P2.5).
6. No held-out assessment family exists on any knowledge point (P2.4, 752 blocked
   on `assessable`); 49 Foundations diagnostic answers are refused by the grammar
   (P2.6); the current curriculum has 1,445 exemplars without an explicit policy.
   `foundations-contract-candidates.jsonl` is the immutable 1,695-row review snapshot,
   in which 1,381 exemplars lacked an explicit policy at that milestone.
7. Units with no owner: f21 recovery flows (P5.3), f22 browser checks (P5.4), f23
   ops and rollback (P5.5), a v1-miss recovery scan (A3.8), a statement-level
   inspection of the 78 `multi-step` topics (P4.4).
8. Calibration: every versioned number is `v1 (uncalibrated)`; the protocol in
   `docs/plans/CALIBRATION.md` needs real delayed outcomes.
9. Owner decision still open from §6: whether the parity fixtures keep the 1.0 rules
   behind `readiness.enforce` and `mastery.confirm_inferred` or move to 2.0 digests.

## 8. Integrated closure audit (receipts through `c647a67c`, repairs through `1e27b2f8`, 2026-09-07)
The reconciled checklist has 62 `done`, 4 `open`, and zero `pending integration`
rows. The isolated post-zero content and readiness receipts are exact for
`c647a67c`. This documentation patch starts from candidate source commit
`1e27b2f8`, which includes the later fail-closed inventory, parity, manifest and
startup repairs. Bundle, exact-head replay, quality and merge-gate receipts remain
release boundaries.

### 8.1 Definitively closed since section 7
- Integrated behavior: `d750e83` persists server-owned hint progression,
  immutable retry receipts and durable completion; `dd5ca39` projects only
  explicitly credited, decided-correct fields; `ee6dcac` excludes component
  fallback from integrated performance.
- Progression and timing: `fe02eac` preserves terminal lesson failure through
  fresh practice; `8c02109` makes drill completion a distinct replayable close;
  `7d99be4` persists reliable ordinary and integrated timing evidence. The
  thresholds remain uncalibrated.
- Diagnostics: `8d3a671` makes all 285 Foundations topic diagnostics decidable;
  zero are missing or grammar-refused, and zero prerequisite IDs dangle. P2.6
  remains open until the final remediation/readiness receipt is attached.
- Contract closure: `1d355b69` adds explicit policies to the final 30
  multi-step exemplars across 8 KPs. Its end-to-end readiness test reconstructs
  every affected served instance, grades its expected answer through the captured
  policy, and requires the full Foundations contract-failure set to be empty.
- Visuals: `67c10ed` closes all visual-readiness blockers. All 153 KPs that
  require a visual have reviewed curriculum visuals, 164 figures validate and
  render deterministically with accessible equivalents, and the
  `money-geometry-problems/kp1` heuristic false-positive is removed. This closes
  P2.5.

### 8.2 Content evidence at this checkpoint
- The authoritative Foundations content audit reports zero
  `fewer_than_four_exemplars`, `missing_solution_sketch`,
  `undecidable_authored_answer`, `singleton_label_contract`,
  `duplicate_problem_answer_family`, `absent_pending_template_recipe`,
  `generic_or_tautological_sketch` and
  `pending_template_production_gate_declined` KPs, plus zero orphan template keys, on
  `20648c5a`.
- The non-importable-recipe repair covers exactly 36 KPs. Its production worker
  gate accepts all 36 recipes and all 432 exhaustive instances; exact collision
  screening finds zero matches across 157,678 authored and pending problem
  instances. `20648c5a` adds the final 19 production-gated KPs. The reconciled
  inventory is 809 of 809: 754 previously gated, the repaired 36 and the final
  19.
- `86cbd2a2`, `d3536588` and `96332af4` add and harden exactly 735 Teach
  documents for the KPs that lacked one. The checked-in manifest records 735
  accepted pending rows, 0 rejected and 0 residual against 809 canonical KPs;
  together with the 74 existing Teach documents, checked-in Teach coverage is
  809 of 809. `crates/worker/tests/whole_course_teach.rs` binds every draft and
  independent review to the canonical coverage and selected-template inputs,
  replays the unchanged production Teach gate, checks exemplar/template-instance
  collisions, and enforces pending-only side effects: 0 API calls, approvals,
  database connections and imports.
- `c647a67c` aligns `parallel-perpendicular-lines/kp1` with the authored
  polynomial-relation contract: the template now asks for and generates the
  complete slope-intercept equation, the evaluator validates that relation through
  `answer_for_contract`, and the template gate admits its symbolic free variable.
  The selected-template input, manifest and bound Teach review were refreshed
  together.
- Independent integrated semantic review passed all 30 repaired exemplars across
  8 KPs, the exact parallel-relation template semantics and the evaluator/gate's
  fail-closed scope. Focused verification passed template36 2/2 over 432
  exhaustive instances, readiness regression 1/1 and whole-course Teach 1/1 over
  all 735 additions. Review receipt SHA-256 is
  `140d08a373b8ed1731709cda5c04fc7ec438f20ce6f16228b8c7383588928efe`.
- The exact `c647a67c` post-zero run imports 809 pending templates, 809 pending
  Teach pages and 809 pending hint ladders into an isolated database: 2,427 total,
  0 approved, 0 rejected and $0 model cost. The production gate accepted 885
  candidates across all 809 KPs and selected 809 valid template drafts with
  multiplicity 734×1, 74×2 and 1×3. Its fail-closed aggregate tests passed 3/3.
- Database-backed readiness inspects 809 contracts with 0 grade failures, missing
  problems, empty statements or refusals. All 809 are practicable, assessable and
  complete for solutions, prerequisites and visuals. Ready remains 0 and blocked
  remains 809 solely because Teach and hint content awaits human approval.
- Receipt hashes: pipeline script
  `0c39bb13d22f36f06c32438bfad6b674956c676519a0076efe7e5eb5f13fae13`;
  execution log
  `76172d94efb667de875f452cb69777dba6874be9bba15c6bd883d22eff621b07`;
  inventory
  `1d5b1633008fe18833cff9dbbc6f567fc9df30f909d317d0005cdbe8207ac199`;
  selected templates
  `cc9ed51165e8c950a61294f30da274a47d73844a5daf08105a5de90a068d2a6d`;
  completion manifest
  `98fdf9757ea381ba28081742390ad66222250e927596c0e2dc06899f3372e3f3`;
  readiness JSON
  `bfaff1d999c5e956cac48cbe63433b603e60f434ba4c768ef03e12e20f78b9eb`;
  readiness Markdown
  `dcd37c6d2707202777c36f298c88509476710ea28c81e489f339a35e137799aa`.
- Candidate-head maintenance after the `c647a67c` receipts separates immutable
  review evidence from live inventories: `21053c5d`, `6e83e906`, `a6fb0d2f`,
  `eda14426` and `e448c085`. The live Foundations tree has 809 knowledge points,
  3,247 exemplars, 1,802 explicit policies and zero missing solution sketches;
  the live multi-step census is 987 statements, while the historical 1,695-row
  contract inventory and 542-row multi-step review remain unchanged.
- `3f755fe7` and `36c14b3d` close rational-tolerance and coefficient-form
  regressions. `89030a26` refreshes the canonical whole-tree pin to 8,352
  exemplars, 4,639,826 bytes and SHA-256
  `d7387d1f83c29c264f321713596faa071ca319773b58e1b5fa37cc1ddd0bc235`.
  `36caccc6`, `609d78f0`, `199a6c3c` and `1e27b2f8` respectively scope the
  Foundations instruction census, pin the complete template-helper registry,
  refresh the visual manifest and preserve shutdown during startup loading.
  These focused repairs do not substitute for the final exact-head gates.
- Generated/imported documents remain `pending` in isolated databases. A human
  must approve every digest intended for serving; no automated step in this work
  grants approval.

### 8.3 Browser, web quality and production preflight
- `f0e7b4b` runs the seven-step 2.0 journey and the full acceptance runner in
  Playwright 1.62.1 with real Chromium. The run had no console, page, request or
  HTTP failures and observed KaTeX MathML plus its aria-hidden visual HTML layer.
  This closes P5.4; the deterministic API fixture does not claim deployment.
- `717a591` raises the web quality gate to 100% coverage across all 68 source
  files. Types, lint, 976 tests and `scripts/quality.sh --web` pass.
- Production preflight `d1f37d5` and replay probe `b0f1750` inspected
  production read-only, streamed an encrypted backup into a disposable restore,
  and matched the production/restored fingerprint
  `36|36|8442802e0407e6d1d9d00d074e44d0b5`. The first normal state read advanced
  the sole account from projector v3 to v7 through sequence 36; the second read
  produced byte-identical model and report JSON and changed no event.
- The complete production history scan found 6 v1 attempts, no incorrect attempt,
  no explicit uncertain outcome and no recovery candidate. Together with the
  replay tests, this closes A3.8 for the observed production history. Uncertainty
  omitted by v1 is intrinsically unrecoverable, but this snapshot contains no
  affected miss.
- `0a2f229b`, `bc8ee93a` and `42061aea` add the exact homelab release path.
  `foundations_release_bundle.py` refuses count, KP-set, overlap, lifecycle,
  release-head or hash drift and binds the ordered 809 template + 809 Teach + 809
  hint-ladder bundle (2,427 unique pending documents). `homelab_release.sh`
  verifies that bundle, the exact-head encrypted recovery receipt, clean checkout,
  unchanged migrations, live Compose topology and image pairing before any
  production mutation. Deploy, pending import and rollback require `--execute`;
  rollback stops edge, web and worker before restoring the recorded prior images.

### 8.4 Open checklist rows and release boundaries
The open IDs are `(h)`, P2.3, P5.5 and A2.2.
- `(h)`, P2.3 and A2.2 have complete production-gated pending content but still
  require authorized production import and a human decision for every serving
  digest.
- The exact post-zero and readiness receipts close P2.4 and P2.6 at the
  authored-content boundary.
- P5.5 has encrypted production-backup restore, restored-production replay and
  fail-closed release tooling. It still requires exact-head recovery and final-gate
  receipts, explicit deployment/import authorization, deploy smoke, human review
  and rollback execution.

### 8.5 Final verification placeholders
- Final integration: `[ROOT-IDENTITY: commit=<sha>; clean=<yes/no>; timestamp=<ISO-8601>]`.
- Post-zero pipeline on `c647a67c`: exit 0; template 809; Teach 809;
  hint-ladder 809; total pending 2,427; approved 0; rejected 0; model cost $0.
  Pipeline-script SHA-256 is `0c39bb13d22f36f06c32438bfad6b674956c676519a0076efe7e5eb5f13fae13`;
  log SHA-256 is `76172d94efb667de875f452cb69777dba6874be9bba15c6bd883d22eff621b07`;
  inventory SHA-256 is `1d5b1633008fe18833cff9dbbc6f567fc9df30f909d317d0005cdbe8207ac199`.
- Release bundle: `[ROOT-BUNDLE: command=<exact>; exit=<n>; documents=<n>;
  bundle_sha256=<sha>; verify_exit=<n>]`.
- Exact-head restored-production replay: `[ROOT-REPLAY: command=<exact>; exit=<n>;
  receipt_sha256=<sha>; production_fingerprint=<value>;
  restored_before=<value>; restored_after=<value>; disposable_removed=<yes/no>]`.
- Database-backed readiness on `c647a67c`: exit 0; ready 0; blocked 809;
  blockers `{teachable: 809, hints: 809}` with all other blocker counts 0;
  contract failures 0. JSON SHA-256 is
  `bfaff1d999c5e956cac48cbe63433b603e60f434ba4c768ef03e12e20f78b9eb`;
  Markdown SHA-256 is
  `dcd37c6d2707202777c36f298c88509476710ea28c81e489f339a35e137799aa`.
- Run the two repository gates sequentially from the exact final integration commit
  so they share compiled artifacts and never contend:
  ```sh
  CARGO_BUILD_JOBS=1 CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate scripts/quality.sh >target/final-quality.log 2>&1
  CARGO_BUILD_JOBS=1 CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate scripts/gate.sh >target/final-gate.log 2>&1
  sha256sum target/final-quality.log target/final-gate.log
  ```
  Quality result: `[ROOT-QUALITY: candidate_commit=<sha>; command=<exact>;
  exit=<n>; final=<line>; elapsed=<duration>; log_sha256=<sha>]`.
  Merge-gate result: `[ROOT-GATE: candidate_commit=<sha>; command=<exact>;
  exit=<n>; final=<line>; elapsed=<duration>; log_sha256=<sha>]`.
- Human content approval, deployment, rollback execution and real-data calibration
  remain unperformed.
