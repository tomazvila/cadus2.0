# Framework checklist

One row per audit finding of `docs/reviews/FRAMEWORK-audit-2026-09-06.md`, and one
row per required item and acceptance check of `ASSIGNMENT.md`. The plan that owns
the units is `docs/plans/FRAMEWORK.md`.

Column rules:

- **Implementation** — the commit that lands the code. `—` means no commit yet.
- **Tests** — the test file that holds the evidence.
- **Content evidence** — the curriculum, report, or deployment artifact that shows
  the outcome on real content.
- **Residual limitation** — what the row does NOT establish after the work lands.
- **Status** — `open` until every other column of the row is filled.

Unit f1-inventory fills the measurement half of the Phase 1 rows. It changes no
`src/` file and no curriculum file, so every row stays `open`.

## 1. Audit findings (a) to (p)

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| (a) | The grade route accepts `numeric` and `expression` only | `7427409` (f4) | `crates/web/tests/grade_route_outcome.rs` | the kind block is gone; every kind reaches the checker | the per-item contract (f3) is not in place; `proof` stays ungraded | `done` |
| (b) | Foundations `multi-step` topics with ordinary numeric final answers | `4a6cde2`, `51707a3` (f1) | `crates/core/tests/answer_inventory.rs` | `docs/reports/foundations-answer-inventory.md` §3: all 78 topics, their final-answer shapes, and the proposed contract; §3.2 the 19 topics an `exact` contract unblocks | The 78 topics carry no `answer_contract` field yet (f3-contract). | `open` |
| (c) | The verdict converts `Undecidable` into incorrect | `205c5d9`, `7427409` (f4) | `crates/core/tests/events_outcome.rs`, `crates/web/tests/grade_route_outcome.rs` | an undecidable answer is `ungraded`, not incorrect, through API, fold, status, admin list and SPA | no learner data folded yet under v2; the recovery path is admin-only | `done` |
| (d) | `sqrt(2)` against `2^(1/2)` gives undecidable | `3dee1c4` (f2) | `crates/core/tests/answer_rational_exponent.rs` | `2^(1/2)` equals `sqrt(2)`; 15 corpus rows recovered | written `q` in 2..6 and `|p|` <= 12 only | `done` |
| (e) | `1/3` against `0.3` is accepted under ruling `D6-dec` | — (f3-contract) | — (f3) | `docs/reports/foundations-answer-inventory.md` §6 note 3: the `Approx` contract carries the authored digit count | `D6-dec` stays as ruled. The contract adds the authored side, not a tolerance. | `open` |
| (f) | Exemplar parsing refuses `9 R2` | `62214ad` (f2) | `crates/core/tests/answer_remainder.rs` | `9 R2` reads as the pair `(9, 2)`; 16 corpus rows recovered | no divisor is known, so a remainder larger than the divisor is not refused | `done` |
| (g) | Work-quality tiers derive from final-answer correctness only | `205c5d9` (f4, D-F4) | `crates/core/tests/events_outcome.rs` | the tier stays correctness-based; an ungraded attempt carries no tier effect and no diagnosis | reasoning assessment (structured steps) is unit f10/f16 work | `open` |
| (h) | Deployed `content_store` has zero rows | — (f8-content-foundations) | — (f8) | — | f8 needs the model key and the owner's cost decision. | `open` |
| (i) | Foundations knowledge-point and exemplar counts | `4a6cde2`, `51707a3` (f1) | `crates/core/tests/answer_inventory.rs` | `docs/reports/foundations-answer-inventory.md` §0 and §4: 809 knowledge points, 1,695 exemplars, 679 with no sketch, all reproduced by the live loader and grammar | The audit's parseability numbers run over 583 knowledge points; f1 runs over 809. §4 states both. | `open` |
| (j) | The SPA falls back to practice when teach fails | `b6a85d2` (f6) | `crates/web/tests/readiness_routes.rs`, `web/test/session.*.test.tsx` | the serve route answers `409 no_instruction` and the SPA shows "No instruction yet" and never serves practice | no approved instruction exists yet (f8) | `done` |
| (k) | `lesson.kp_pass` is not read by the gate | `2f0713c` (f12) | `crates/core/tests/projector_pass_rule.rs` | `lesson.kp_pass` is parsed and read; the default keeps the 1.0 behavior; the config hash holds | no threshold calibration | `done` |
| (l) | `xp::is_mastered` includes Learning, Placed, and Floor | `3de9b18` (f14) | `crates/core/tests/selector_confirm.rs` | practiced and inferred are counted apart; an inferred topic gets one confirmation item; the dashboard shows both | the parity fixtures run with `mastery.confirm_inferred = false` | `done` |
| (m) | The assisted-correct rework shows the solution beside the input | — (f11-rework) | — (f11) | — | Out of f1 scope. | `open` |
| (n) | Review scoring weights later answers and needs the final answer correct | — (f13-review-inconclusive) | — (f13) | — | Out of f1 scope. | `open` |
| (o) | `TaskType::MultiStep` is one independent component question per position | — (f16, f17) | — (f17) | — | Out of f1 scope. | `open` |
| (p) | The five baseline test files exist | — (f22-browser-checks) | `crates/core/tests/diagnostic.rs`, `selector_plan.rs`, `fire_attempt.rs`, `instruction_gate.rs`, `pool_source.rs` | — | The baseline command needs a rerun on the framework branch. | `open` |

## 2. Phase 1 — reliable answers and trustworthy learning evidence

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| P1.1 | Separate problem complexity from answer representation; inventory the representations Foundations requires; support contracts for exact, approximate, unit, quotient/remainder, coordinate, set, and multipart answers | `4a6cde2`, `51707a3` (f1) (inventory); — (f3-contract, f4-outcome) | `crates/core/tests/answer_inventory.rs` | `docs/reports/foundations-answer-inventory.md` §1 the shape x kind table, §3 the 78 topics, §6 the contract enum with one Foundations example per variant | f1 proposes the enum. No loader field, no lint rule, and no grade path reads it yet. | `open` |
| P1.2 | Preserve an ungraded outcome through API, UI, storage, projection, diagnostics, and analytics | — (f4-outcome, D-F2) | — (f4) | `docs/reports/foundations-answer-inventory.md` §2: the size of the ungraded set on authored content is 370 answers | No `Outcome::Ungraded` in `Attempt` yet; `SCHEMA_VERSION` is still 1. | `open` |
| P1.3 | Make mathematical acceptance policies explicit: exactness, tolerance, rounding, units, required form; bound parser resources; no numeric sampling as proof | — (f2-grammar, f3-contract) | — (f2, f3) | `docs/reports/foundations-answer-inventory.md` §5 the thirteen productions ranked, §6 the contract enum | The input cap and the no-search rule already hold (R3, L2). The unit table is not built. | `open` |
| P1.4 | Separate correctness from reasoning assessment; document the model role; no AI explanation on an unresolved grade | — (f4-outcome, D-F4) | — (f4) | — | Out of f1 scope. | `open` |
| P1.5 | Preserve historical integrity: version the contracts, safe migration and replay, keep original observations | — (f4-outcome, f23-ops) | — (f4) | — | Out of f1 scope. | `open` |

## 3. Phase 2 — complete instruction and executable content readiness

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| P2.1 | Build an executable readiness audit over serve, render and grade contracts; report blockers per course, topic and knowledge point | — (f6, f7) | — (f7) | `docs/reports/foundations-answer-inventory.md` §4: the decidable-item count per knowledge point, per unit file | f1 covers the answer-decidability input of readiness only. Instruction, hints, solutions and visuals are unmeasured. | `open` |
| P2.2 | Use readiness in task selection; a lesson must be teachable, practicable and assessable; teaching failure never becomes unaided practice | — (f6-readiness-core) | — (f6) | `docs/reports/foundations-answer-inventory.md` §4: 0 of 809 Foundations knowledge points reach 3 distinct decidable items today | The selector still serves a lesson with no decidable practice item. | `open` |
| P2.3 | Complete instructional content throughout Foundations: concept, worked example, hints, solutions, practice, assessment | — (f8-content-foundations) | — (f8) | `docs/reports/foundations-answer-inventory.md` §0: 679 of 1,695 exemplars carry no solution sketch | `content_store` is empty. f8 needs the model key and the cost decision. | `open` |
| P2.4 | Item variety: validated templates, item identity, provenance, exposure, held-out assessment families | — (f9-item-variety) | — (f9) | `docs/reports/foundations-answer-inventory.md` §4: 57 knowledge points hold 3 decidable items and none holds a fourth for a held-out family | Out of f1 scope beyond the count. | `open` |
| P2.5 | Validate generation and mathematical visuals; preserve the approval lifecycle; accessible equivalents | — (f9-item-variety) | — (f9) | — | Out of f1 scope. | `open` |
| P2.6 | Audit prerequisites and diagnostic coverage; inventory assumed mastery; assess and remediate it | — (f10-prereq-audit) | — (f10) | — | Out of f1 scope. | `open` |

## 4. Phase 3 — independent practice and defensible progression

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| P3.1 | Distinguish readiness, delayed retention, routine fluency and application, with operational meaning | — (f14, f19) | — (f14, f19) | — | Out of f1 scope. | `open` |
| P3.2 | Repair the feedback-to-independent-practice loop; hide the solution; serve a fresh similar item; delayed confirmation | — (f11-rework) | — (f11) | — | Out of f1 scope. | `open` |
| P3.3 | Record evidence at the skill level: skills exercised, assistance, exposure, item identity, timing reliability | — (f9, D-F9) | — (f9) | — | Out of f1 scope. | `open` |
| P3.4 | Make progression policy explicit: read `lesson.kp_pass`, fix brittle review decisions, document threshold rationale | — (f12, f13) | — (f12, f13) | — | Out of f1 scope. | `open` |
| P3.5 | Audit implicit credit and remediation; direct checks of inferred knowledge | — (f14-mastery-split) | — (f14) | — | Out of f1 scope. | `open` |
| P3.6 | Interpret speed appropriately; separate fluency from reasoning; handle unreliable timings | — (f15-speed) | — (f15) | — | Out of f1 scope. | `open` |

## 5. Phase 4 — genuine integrated mathematical application

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| P4.1 | Integrated tasks: scenario, quantities, constraints, method choice, intermediate reasoning, final answer and interpretation, faded hints | — (f16, f17) | — (f17) | — | Out of f1 scope. | `open` |
| P4.2 | Foundations applications: rates and units, percentages and ratios, algebraic constraints, graph interpretation, geometry | — (f18) | — (f18) | `docs/reports/foundations-answer-inventory.md` §3: `systems-mixture-problems`, `understanding-ratios`, `interpreting-graphs-qualitatively` and `law-of-cosines` are the existing seeds | The existing topics are per-component. f18 authors the integrated set. | `open` |
| P4.3 | A workforce and capacity set: person-minutes, service windows, staffing lower bounds, feasibility assumptions | — (f18) | — (f18) | — | No such content exists in `curriculum/foundations/`. | `open` |
| P4.4 | Inspect the already-integrated authored questions before replacing them | — (f16) | — (f16) | `docs/reports/foundations-answer-inventory.md` §3: the 78 `multi-step` topics with their shapes are the inspection list | f1 classifies the answers, not the problem statements. | `open` |

## 6. Phase 5 — validation, calibration, and operational robustness

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| P5.1 | Measure learning outcomes: delayed unseen-item assessment, retained accuracy, assistance dependence, placement error, integrated-task performance | — (f19-retention) | — (f19) | — | Out of f1 scope. | `open` |
| P5.2 | Make adaptive policies inspectable: version intervals, weights and pass rules; a calibration protocol on real delayed outcomes | — (f20) | — (f20) | — | Out of f1 scope. | `open` |
| P5.3 | Verify failure and recovery flows: resumption, retries, duplicate submissions, worker failure, missing content, API and UI agreement | — (f21) | — (f21) | — | Out of f1 scope. | `open` |
| P5.4 | Run automated and browser verification, including rendered mathematical output | — (f22) | — (f22) | — | Out of f1 scope. | `open` |
| P5.5 | Prepare safe operation and deployment: reversible migrations, readiness checks, rollback instructions, replay against fixtures | — (f23-ops) | — (f23) | — | Out of f1 scope. | `open` |

## 7. Acceptance checks

### 7.1 Phase 1

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| A1.1 | Equivalent radical notation | — (f2, f5) | — (f5-acceptance-1) | `docs/reports/foundations-answer-inventory.md` §5.1 `rational_exponent`: `x^(1/2)` in `radical-exponent-conversion` | The pair `sqrt(2)` against `2^(1/2)` needs the f2 production first. | `open` |
| A1.2 | Exact versus approximate thirds | — (f5) | — (f5) | `docs/reports/foundations-answer-inventory.md` §6 note 3 | Ruling `D6-dec` holds; the case needs a regression test. | `open` |
| A1.3 | Division with remainders | — (f2, f5) | — (f5) | `docs/reports/foundations-answer-inventory.md` §5.1: 12 answers, `9 R2` in `division-with-remainders` | No production yet. | `open` |
| A1.4 | Units and conversions | — (f2, f3, f5) | — (f5) | `docs/reports/foundations-answer-inventory.md` §5.1 `unit` and `currency`: 47 answers, `4.2 L` and `€1081.60` | No unit table and no conversion rule yet. | `open` |
| A1.5 | Unordered solution sets | — (f5) | — (f5) | `docs/reports/foundations-answer-inventory.md` §2: 4 `set` answers, all decided; `{2, 4, 6}` in `domain-range-of-relations` | The canon holds `Canon::Set` already; the contract must select it. | `open` |
| A1.6 | Numeric-answer word problems previously blocked by `multi-step` | — (f4, f5) | — (f5) | `docs/reports/foundations-answer-inventory.md` §3.2: the 19 topics, and §1: 63 `multi-step` answers are plain integers | Blocked on the contract gate of f4. | `open` |
| A1.7 | Alternate correct answers, plausible wrong answers, malformed input, domain boundaries | — (f5) | — (f5) | — | Out of f1 scope. | `open` |
| A1.8 | Ungraded submissions leave mastery unchanged | — (f4, f5) | — (f5) | — | Out of f1 scope. | `open` |
| A1.9 | Quiz and diagnostic answers stay hidden until the permitted reveal point | — (f5) | — (f5) | — | Out of f1 scope. | `open` |

### 7.2 Phase 2

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| A2.1 | Run readiness checks across the whole Foundations course | — (f7) | — (f7) | `docs/reports/foundations-answer-inventory.md` §4: the per-unit table over all 11 unit files | Answers only; instruction and hints are unmeasured. | `open` |
| A2.2 | Every required knowledge point has usable, approved teaching, practice, feedback and assessment | — (f8) | — (f8) | `docs/reports/foundations-answer-inventory.md` §4: 146 knowledge points hold 0 decidable items and 58 hold 1 | `content_store` is empty; approvals are pending work. | `open` |
| A2.3 | Validate deterministic content exhaustively; validate templates by boundary and property checks | — (f9) | — (f9) | `crates/core/tests/answer_inventory.rs` runs the grammar over every Foundations exemplar | Templates are not in the f1 walk. | `open` |
| A2.4 | A hand-picked lesson does not establish whole-course readiness; pending approvals remain pending | — (f7, f8) | — (f7) | `docs/reports/foundations-answer-inventory.md` §0: the counts run over all 809 knowledge points | — | `open` |

### 7.3 Phase 3

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| A3.1 | Fresh versus repeated questions | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |
| A3.2 | Assisted attempts and rework | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |
| A3.3 | Delayed reviews | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |
| A3.4 | Reviews that mix skills and difficulties | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |
| A3.5 | Placement inference and direct confirmation | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |
| A3.6 | Learner-state persistence and replay | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |
| A3.7 | Displayed mastery claims match their supporting evidence | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |
| A3.8 | Historical ambiguous answers keep their uncertainty after migration | — (f11 to f15) | — (f11 to f15) | — | Out of f1 scope. | `open` |

### 7.4 Phase 4

| ID | Item | Implementation | Tests | Content evidence | Residual limitation | Status |
|---|---|---|---|---|---|---|
| A4.1 | Demonstrate one integrated task from instruction to independent application and delayed assessment | — (f17, f18) | — (f17) | — | Out of f1 scope. | `open` |
| A4.2 | Verify step-level feedback, alternate solution paths, and answer secrecy | — (f17) | — (f17) | — | Out of f1 scope. | `open` |
| A4.3 | A sequence of unrelated component exercises does not satisfy integrated application | — (f16) | — (f17) | `docs/reports/foundations-answer-inventory.md` §3: the shapes show which `multi-step` topics carry one final answer | — | `open` |

## 8. Row count

| group | rows |
|---|---:|
| audit findings (a) to (p) | 16 |
| Phase 1 required items | 5 |
| Phase 2 required items | 6 |
| Phase 3 required items | 6 |
| Phase 4 required items | 4 |
| Phase 5 required items | 5 |
| acceptance checks | 24 |
| **total** | **66** |

Every row is `open`. f1-inventory fills the Implementation, Tests and Content
evidence columns of the rows it covers: (b), (i), P1.1, and the measurement half
of (a), (c), (d), (f), P1.2, P1.3, P2.1, P2.2, A1.1 to A1.6, A2.1 to A2.4, A4.3.
