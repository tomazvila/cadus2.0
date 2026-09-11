# Unit 07 residual repair

**23 of 28 residual KPs closed.** The frozen subset contains 97 pending recipes and 408 authored exemplars. Five KPs retain only a missing-recipe finding. No content was approved and no database was written.

Base: remote integration `e92ec16c`, byte-identical local root `2eee3e6`. Scope is exclusively Unit 07 and its authoring/verification artifacts. All 707 other-unit fact records and the 74 previously retained pending recipes are unchanged.

## Exact closed KPs

| Topic | Closed KPs |
|---|---|
| `applying-the-quadratic-formula` | `kp1` |
| `choosing-factoring-strategy` | `kp2`, `kp3` |
| `completing-the-square` | `kp1` |
| `converting-to-vertex-form` | `kp1`, `kp2`, `kp3` |
| `difference-of-squares` | `kp2`, `kp3` |
| `factoring-gcf` | `kp2` |
| `parabola-vertex-form` | `kp3` |
| `perfect-square-trinomials` | `kp3` |
| `quadratic-applications` | `kp1`, `kp2` |
| `quadratic-formula` | `kp3` |
| `quadratic-graphs-vertex` | `kp2` |
| `quadratics-in-form` | `kp2` |
| `sum-difference-of-cubes` | `kp1`, `kp2`, `kp3` |
| `writing-quadratics-from-roots` | `kp1`, `kp2`, `kp3` |

Each retained KP has four authored examples with independently checked answers and a pending recipe with 12 distinct valid instances. Cube identities use meaningful shifted bases and small roots. Factoring checks preserve the requested negative GCF, squared-binomial form, and complete factorization. Root counting covers all three discriminant signs.

Eight KPs use explicit coefficient or vertex-parameter responses. Their authored prompts and exact contracts were updated together. Equation coefficients `(A,B,C)`, vertex parameters `(A,h,k)`, or square-completion constant/shift retain the full requested expression. Validation reconstructs the equation or square from every response. The production evaluator and gate are unchanged.

## Authoritative audit

Counts are affected KPs. The native `crates/core/examples/content_audit_facts.rs` and `scripts/review/foundations_content_audit.py` are byte-identical to the base.

| Counter | Whole course before | Whole course after | Unit 07 before → after |
|---|---:|---:|---:|
| Issue KPs | 205 | 182 | 28 → 5 |
| Missing pending recipe | 205 | 182 | 28 → 5 |
| Fewer than four exemplars | 124 | 124 | 0 → 0 |
| Missing solution sketch | 38 | 38 | 0 → 0 |
| Undecidable authored answer | 43 | 43 | 0 → 0 |
| Singleton label contract | 0 | 0 | 0 → 0 |
| Duplicate problem/answer family | 8 | 8 | 0 → 0 |
| Generic or tautological sketch | 3 | 3 | 0 → 0 |

Audit exit status is **1**, reflecting the remaining findings. There are no orphan pending-template keys. The old correction report describes an earlier integration; the counters above were measured on this snapshot.

## Remaining current-candidate blockers

| KP | Production rejection | Observed reason |
|---|---|---|
| `polynomial-basics/kp2` | `grammar` | Literal `trinomial` is not a valid answer-expression name. |
| `difference-of-squares/kp1` | `space-floor` | The constrained monic family has only 10 positive square constants up to 100; the production floor is 12. |
| `choosing-factoring-strategy/kp1` | `grammar` | Literal `GCF` is not a valid answer-expression name. |
| `parabola-vertex-form/kp2` | `grammar` | Literal direction name in `(upward,a)` is outside the expression grammar. |
| `quadratic-graphs-vertex/kp3` | `answer-kind` | The mixed authored multipart shapes provide no shared contract for the current candidate. |

These are reproducible rejections of the current candidates, not claims that every possible redesign is impossible. The current `answer_for_contract` supports direct text-choice labels and flat multipart assembly; the remaining literal-label recipes do not use that supported form. Further redesign stopped at the requested scope freeze. Full rejected candidates and exact messages remain in `unit07-schema-blockers.json`.

## Focused verification receipts

| Check | Result |
|---|---|
| Production curriculum lint/load and canonical answers | Pass; all 408 U07 authored answers decidable |
| Actual worker `verify_kind(Kind::Template, ...)` | 97 accepted; 5 recorded refusals |
| Exhaustive production instantiation and problem hashes | 1,164 valid distinct instances; no authored/sibling text-hash collisions |
| Independent rendered-question math | 1,164/1,164 correct |
| Repaired subset | 276 instances across 23 recipes; each recipe varies its answer |
| Retained authored math | 92/92 answers checked independently |
| Negative controls | 23 template-answer and 32 authored-answer perturbations rejected |
| Semantic algebra collision check | 228 repaired factoring/count/coefficient/conversion instances checked against authored and sibling families; no collisions |
| Production KaTeX 0.17.0 | 7,345 formulas in 4,308 fields; zero errors |
| Worker semantic regression tests | 3 passed |
| Authoritative audit regression tests | 9 passed |
| Clippy, worker example and semantic tests | Pass with `-D warnings` |
| Authoring/candidate determinism | Byte-identical repeat |
| Code limits | Files below 500 lines; functions at most 70 lines |

Raw receipts and artifact digests are in `unit07-residual-verification.json`; exact counters and closed IDs are in `unit07-summary.json`. The full audit is `unit07-audit.json`.

Verification is grounded in the loaded curriculum and actual worker output. Mathematical identities, reconstructed responses, and the listed semantic families were checked independently with exact computation. Pedagogical fit is an authored judgment. Live serving and database import were not exercised. All task-started test/build commands finished; no task-started process is left running.
