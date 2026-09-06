# Unit07 correction

## Result

The unit owns 102 knowledge points and 408 exemplars. All 102 are clean in the six authored-content audit categories. Seventy-four are clean across all seven codes and have checked-in pending template drafts. Twenty-eight retain only a missing-template finding, including 11 semantic-family exclusions and 17 exact production-worker rejections recorded in [the blocker packet](unit07-schema-blockers.json).

- Against integration `d4be3956`, whole-course duplicate KPs are **35 → 26** and all other audit codes are non-regressing.
- Changed **284 exemplar rows** and **54 answers** relative to the initial dirty YAML. Fifty-one expanded factoring answers were replaced with factored answers; this includes the out-of-range square-difference example, corrected from 121 to 100. The other answer changes correct the square-garden calculation and two examples whose KP requires shifted squares.
- Added **74 pending templates**, with exactly **12** distinct valid instances each: **888 total**.
- Preserved the existing dirty improvement, with further revisions to make contexts concrete and sketches instructional. Already-correct factored answers retain their existing spelling.
- All **707 other-unit KP fact records are unchanged**, including their problems, answers, contracts, constraints, and sketches.

## Exact audit counts

Counts are affected KPs, not individual duplicate pairs or template instances.

| Code | Reproduced dirty baseline, whole course | Final whole course | Final unit07 |
|---|---:|---:|---:|
| `fewer_than_four_exemplars` | 329 | 245 | 0 |
| `missing_solution_sketch` | 175 | 115 | 0 |
| `undecidable_authored_answer` | 73 | 70 | 0 |
| `singleton_label_contract` | 0 | 0 | 0 |
| `duplicate_problem_answer_family` | 35 | 26 | 0 |
| `absent_pending_template_recipe` | 438 | 375 | 28 |
| `generic_or_tautological_sketch` | 4 | 4 | 0 |

The final whole-course audit has 375 issue KPs; unit07 has only its 28 explicitly blocked pending-template recipes. Its exit status is 1 because these residuals remain. See [the complete live audit](unit07-audit.json) and [machine-readable summary and digests](unit07-summary.json).

**Baseline discrepancy:** the supplied `audit.json` says 28 generic-sketch KPs. Replaying the untouched audit on a curriculum copy containing the exact initial dirty YAML gives 22. The final audit also gives 22. No generic-count reduction is attributed to this correction. The original `audit.json` is preserved.

## Production schema blockers

The worker uses a uniform reviewed curriculum contract when present and otherwise can validate a supported explicit candidate contract. The recorded blockers include every authored contract and the inferred contract, so the failure can be reproduced without a database.

| KP | Exact rejection code | Required capability or constraint |
|---|---|---|
| `polynomial-basics/kp2` | `grammar` | Emit a classification label from the numeric/symbolic template evaluator. |
| `difference-of-squares/kp1` | `space-floor` | The monic square minus a positive perfect-square constant up to 100 has 10 numeric cases; the unchanged gate requires 12. Existing authored cases reduce the unseen set further. |
| `choosing-factoring-strategy/kp1` | `grammar` | Emit the first-move strategy label. |
| `writing-quadratics-from-roots/kp1` | `unknown-names` | Allow symbolic `x` in a contracted multi-step template answer. |
| `writing-quadratics-from-roots/kp2` | `unknown-names` | Same symbolic-output restriction. |
| `writing-quadratics-from-roots/kp3` | `unknown-names` | Same symbolic-output restriction. |
| `completing-the-square/kp1` | `unknown-names` | Symbolic squared-binomial output plus named multipart assembly. |
| `applying-the-quadratic-formula/kp1` | `grammar` | The current template DSL supports flat two-part multipart answers; this KP requires three named parts. |
| `parabola-vertex-form/kp2` | `grammar` | Emit the direction label within the named multipart answer. |
| `parabola-vertex-form/kp3` | `answer-kind` | Mixed authored contracts produce no shared template contract for this multi-step KP. |
| `quadratic-graphs-vertex/kp2` | `answer-kind` | Different authored multipart shapes produce no shared template contract. |
| `quadratic-graphs-vertex/kp3` | `answer-kind` | Different authored multipart shapes produce no shared template contract; direction also needs label output. |
| `converting-to-vertex-form/kp1` | `unknown-names` | Allow symbolic `x` in a contracted multi-step answer. |
| `converting-to-vertex-form/kp2` | `unknown-names` | Same symbolic-output restriction. |
| `converting-to-vertex-form/kp3` | `answer-kind` | Different authored multipart shapes produce no shared template contract; symbolic output is also required. |
| `quadratic-applications/kp1` | `answer-kind` | Numeric and named multipart exemplars have no shared template contract. |
| `quadratic-applications/kp2` | `answer-kind` | Time and length unit contracts differ, so the worker infers no shared template contract. |

### Semantic-family exclusions

Eleven additional candidates fail closed despite passing the mechanical gate: `factoring-gcf/kp2`, `difference-of-squares/kp2`, `difference-of-squares/kp3`, `perfect-square-trinomials/kp3`, `sum-difference-of-cubes/kp1`, `sum-difference-of-cubes/kp2`, `sum-difference-of-cubes/kp3`, `quadratics-in-form/kp2`, `choosing-factoring-strategy/kp2`, `choosing-factoring-strategy/kp3`, and `quadratic-formula/kp3`. Their generated answers normalize to an unfactored or incomplete form, move the requested negative GCF, or remain constant across the whole family. The verifier and regression tests preserve these exclusions.

These are observed refusals, not approved or stored templates. The proposed capability explanations are grounded in `AuthoringSpec::template_contract`, worker assembly, and the core template field/instance gates. Resolving them requires a response-contract/schema decision outside this correction. No contract was weakened to manufacture a passing row.

## Verification

| Check | Result |
|---|---|
| Production curriculum lint and load | 0 findings |
| Authored expected-answer canonicalization | All 408 owned exemplars decidable |
| Real worker `verify_kind(Kind::Template, ...)` | 74 accepted; 17 gate rejections and 11 semantic-family exclusions |
| Exhaustive production domain walk and instantiation | 888 distinct valid instances; every accepted template has 12 |
| Production problem-hash collision checks | 0 against authored exemplars/diagnostics across the curriculum or any accepted sibling instance |
| Independent SymPy solving of rendered question text | 888/888 correct; uses the rendered questions, not `answer_expr` as the oracle |
| Production vendored KaTeX 0.17.0 | 6,204 formulas in 3,876 text fields; 0 errors |
| Template generation determinism | Byte-identical repeat |
| Curriculum revision idempotence | Byte-identical repeat |
| Clippy, worker verification example | Pass with `-D warnings` |
| Source size and function length | 14 new code files below 500 lines; all functions at most 70 lines |
| Other-unit facts | 707/707 unchanged |

Domains exclude reviewed authored calculations and zero coefficients that would make requested standard-form answers retain a zero term. Arithmetic, factorizations, roots, and graph coordinates were additionally inspected against each KP's constraints. The native hash check establishes exact production-text collision freedom; domain selection supplies the semantic separation from authored calculations.

No fixture generator, fixture result, or replacement audit implementation was used as verification. The audit and native fact emitter were taken byte-for-byte from commit `200c0726926cb5affad7bb024144d4409f657fb8`:

- Audit SHA-256: `a052658b07f1c064f0a5ecac8e00d0b016424b1ef9a01e8f38ece8561c787cd0`.
- Fact emitter SHA-256: `32a7402ae7c0f828e3c0e93a6b68f908f309921b07d7e711d0c9408002ecf74c`.
- The temporary unchanged fact-emitter source was removed after use; audit logic is absent from this change.

## Reproduction

Run from this worktree. The Python helpers use Nix-provided dependencies. Rust checks used the existing Nix Rust 1.95.0 toolchain, `CARGO_BUILD_JOBS=1`, `SQLX_OFFLINE=true`, and `--locked --offline`; repository dependency/toolchain pins are unchanged.

```sh
export PATH=/nix/store/zy8vh0h5k74ga7v95mg5afvscb91fj60-rust-default-1.95.0/bin:$PATH
export CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true
nix-shell -p python3Packages.pyyaml python3Packages.sympy --run 'python3 scripts/authoring/unit07/revise.py'
nix-shell -p python3Packages.pyyaml python3Packages.sympy --run 'python3 scripts/authoring/unit07/templates.py'
cargo run -p cadus-worker --example unit07_verify --locked --offline
nix-shell -p python3Packages.sympy --run 'python3 scripts/authoring/unit07/check_math.py'
nix-shell -p nodejs --run 'node scripts/authoring/unit07/check_render.cjs'
cargo clippy -p cadus-worker --example unit07_verify --locked --offline -- -D warnings
```

`check_render.cjs` also reads `target/unit07/facts.json`, emitted from the live curriculum by the unchanged native fact emitter. To reproduce the seven-code audit, use that emitter and the audit script from the cited commit with the current curriculum and `docs/content-foundations`.

The pending files are import-ready local drafts; no database import was performed. No API/model calls, production changes, deployments, approval changes, or changes to other units were made. All started commands finished; no task-started process remains running. Live serving, browser interaction, and database import were not exercised.
