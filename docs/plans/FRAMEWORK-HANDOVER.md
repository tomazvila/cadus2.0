# Framework handover (2026-09-06)

The state of the math-learning framework assignment (`~/.cache/cadus2_scripts/framework/ASSIGNMENT.md`
on the box; the plan is `docs/plans/FRAMEWORK.md`, the audit is
`docs/reviews/FRAMEWORK-audit-2026-09-06.md`, the checklist is
`docs/plans/FRAMEWORK-CHECKLIST.md`). The owner stopped the work after the first swarm
of six units, with the instruction "do not pick up the next task, provide a handover".

## 1. Where the code is

| Branch | Holds | State |
|---|---|---|
| `main` (2ff3c1f) | the quality-limit work: ten code limits on the whole tree, `scripts/quality.sh` PASSED, `scripts/gate.sh` exit 0 | released |
| `framework/main` | the plan, the audit, the checklist, and the six units below, merged | clippy clean, `cargo test --workspace` green, `npm run check` green (874 SPA tests) at the last merge; see section 4 for what was NOT run |
| `framework/f1-inventory` … `framework/f14-mastery-split` | the six unit branches, each merged into `framework/main` | keep until `framework/main` merges to `main`, then delete |

Worktrees: `~/.cache/cadus2_wt/framework` (the integration branch) and one per unit under
`~/.cache/cadus2_wt/f<N>-<name>`. Prompts and the shared rules: `~/.cache/cadus2_scripts/framework/`
(`unit-header.md` holds the rules every unit follows; `f<N>-<name>.md` holds each task).

## 2. The six units that landed

| Unit | Design decision | What it does | Evidence |
|---|---|---|---|
| f1-inventory (c774a32) | D-F1 | The answer-shape inventory of Foundations: 1,695 exemplar answers classified into 16 shapes with the real grammar; the checklist | `crates/core/tests/answer_inventory.rs`; `docs/reports/foundations-answer-inventory.md` |
| f2-grammar (fea794f) | D-F3 | Rational exponents, quotient-and-remainder, value-with-unit, sets versus lists, coordinates tests; the corpus residue drops from 265 to 235 of 3,492; 31 recovered rows in `recovered_2_0.jsonl` | `crates/core/tests/answer_{rational_exponent,remainder,unit,set,coordinates}.rs`; `docs/reference/undecidable-answers.md` §5 |
| f4-outcome | D-F2, D-F4, D-F9 | See section 3 | |
| f6-readiness (b6a85d2) | D-F5 | `crates/core/src/readiness/`: per knowledge point teachable, practicable, assessable, hints, solutions, prerequisites, visual; the selector serves a lesson only when ready and reports `SessionPlan::blocked`; the serve route answers `409 no_instruction` and the SPA shows "No instruction yet" and never falls to practice; `cadus-worker readiness` writes the report; `/api/operator/flags` carries the counts | `docs/reports/readiness-foundations-2026-09-06.md`: 809 knowledge points, 0 ready, 809 blocked on `teachable` (empty `content_store`), 274 fail a contract (the `multi-step` kind) |
| f12-pass-rule (2f0713c) | D-F7 part 1 | `lesson.kp_pass` is parsed into `PassRule` (`<n>consec`, `<k>of<m>`, `|`); the knowledge-point rule reads it; a bad string fails the config load; the config hash stays `797575e985c12149` | `crates/core/tests/projector_pass_rule.rs` (12 tests) |
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

(Filled from the unit's report when it lands.)

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
  digests (f14 kept `PROJECTOR_VERSION` at 3 for that reason; f4 bumps it, see §3).
