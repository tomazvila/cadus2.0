# Unit04 residual repair

Source: integration snapshot `ef8f56f1`, local baseline `8a666f5`. All recipes remain **pending**. The work changes only 23 exemplar blocks in `curriculum/foundations/04-linear-graphs.yaml` and these Unit04 recipe/test artifacts. Topic settings, constraints, visuals, diagnostics, other KPs, and other units are unchanged.

## Checkpoints and counts

First checkpoint: `30464be844d5c13ea29c696fc7ad247a78cddcda`, containing the first 10 repaired KPs. Unit04 residuals fell from 34 to 24 at that checkpoint. This completed batch repairs 23 KPs in total.

| Authoritative measure | Before | First checkpoint | Final |
|---|---:|---:|---:|
| Unit04 KPs with issues / 69 | 34 | 24 | 11 |
| Foundations KPs with issues / 809 | 181 | 171 | 158 |
| Unit04 audit-clean KPs | 35 | 45 | 58 |

The exact initial residual set is in [before-audit.json](before-audit.json); the exact remaining set is in [after-audit.json](after-audit.json). Both come from the production `content_audit_facts` adapter followed by `scripts/review/foundations_content_audit.py`. Audit exit status 1 is expected because residual KPs remain; neither run refused its input, and neither found orphan recipe keys.

| Issue code | Unit04 before → after | Foundations before → after |
|---|---:|---:|
| `absent_pending_template_recipe` | 34 → 11 | 181 → 158 |
| `fewer_than_four_exemplars` | 34 → 11 | 112 → 89 |
| `missing_solution_sketch` | 16 → 6 | 36 → 26 |
| `undecidable_authored_answer` | 14 → 6 | 42 → 34 |
| `duplicate_problem_answer_family` | 2 → 0 | 4 → 2 |
| `generic_or_tautological_sketch` | 0 → 0 | 3 → 3 |
| `singleton_label_contract` | 0 → 0 | 0 → 0 |

## Retained content

Each of the 23 repaired KPs has exactly four authored exemplars, for 92 total, and exactly one pending recipe with 12 exhaustive cases, for 276 total. The recipes have 12 distinct rendered tasks and 12 distinct mathematical answers/models after normalization. Every parameter changes the answer while the other parameter is held fixed. Domains contain meaningful numeric inputs; there are no text-choice fillers, constant parameters, hidden answers, constraint padding, or author-supplied space counts.

Authored tasks include different representations, directions, sign cases, and error corrections. Their sketches explain the required calculation or geometric relationship. The table recipe uses explicitly identified numerical products as table entries so the production renderer needs no calculated-field extension.

Multi-step topics use explicit coefficient scaffolds: `(m,b)` completes `y=mx+b`, `(A,M,B)` completes `y+A=M(x+B)`, and `(A,B,C)` completes `Ax+By=C`. These retain equation-construction tasks within the snapshot's supported tuple contracts. Standard-form coefficients are primitive integers with positive `A`. Intercept plotting uses four coordinates in an explicitly stated order. Reduced-fraction tasks use the production required-form contract.

## Verification

- The real worker's `verify_kind(Kind::Template, ...)` passes for all 23 recipes using live curriculum specs. No database or model endpoint is involved. The test also calls this snapshot's `answer_for_contract` and checks its result against actual instantiation.
- All 276 cases are enumerated. The worker test exports actual rendered problems and answers for independent review.
- Independent Python rational arithmetic checks all 92 authored answers and all 276 served answers against geometric incidence, direction-vector dot products, displacements, proportionality, collinearity determinants, primitive coefficients, and reduced rise/run pairs. It never evaluates a recipe's `answer_expr`.
- Negative controls reject corrupted answers, sign/reciprocal mistakes, swapped coordinates, wrong arity, unreduced fractions, scaled standard coefficients, inconsistent tables, collapsed/cancelling outputs, inert axes, and duplicate tasks. Each recipe's intentionally incorrect sample is also refused by the production verifier.
- Numeric-normalized authored problem families are checked across Foundations. Pending template statement families are checked across the content tree. Mathematical model uniqueness additionally catches point-slope records describing the same line.
- Passed: 2 worker integration tests, 7 independent semantic tests, 9 authoritative-audit tests, targeted Clippy with warnings denied, Rust formatting, `git diff --check`, and code-size/function-size checks. Generator output is deterministic. A parsed baseline comparison confirms that exactly the 23 retained exemplar blocks changed.

The full repository `scripts/gate.sh` stopped at its mandatory `CADUS_TEST_DATABASE_URL` requirement. Database-backed checks and live UI serving were not run. No content was approved or deployed, no database was written, and no started process remains running. No parser/evaluator code or broad patch was imported.

## Reproduce

Use Rust 1.94 or newer from project-local/Nix tooling. Run from the repository root, keeping Cargo at one job and SQLx offline:
```sh
export CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true
mkdir -p target/unit04-evidence
python3 scripts/authoring/unit04_residual_recipes.py
cargo test -p cadus-worker --test unit04_residual_recipes
cargo run --quiet -p cadus-core --bin content_audit_facts -- curriculum > target/unit04-evidence/after-facts.json
python3 scripts/review/foundations_content_audit.py --facts target/unit04-evidence/after-facts.json --output target/unit04-evidence/after-audit.json
# The preceding audit returns 1 while residuals remain; continue with:
python3 scripts/authoring/test_unit04_residual.py -v
python3 -m unittest discover -s scripts/review -p test_foundations_content_audit.py
cargo clippy -p cadus-worker --test unit04_residual_recipes -- -D warnings
```

`before-audit.json` is immutable baseline evidence. Regenerating content does not replace either checked-in audit summary; rerun the real audit before updating the after-summary.

## Deferred residuals

These 11 KPs remain explicitly open. A retained recipe would need to preserve their learning objective and establish genuine variety without copying a supplied answer or padding a constant result.

| KP | Reason for deferral |
|---|---|
| `coordinate-plane/kp1` | Quadrant/axis classification needs computed categorical answers. |
| `proportional-relationships/kp2` | Recognition needs both proportional and non-proportional cases with computed decisions. |
| `graphing-proportional-relationships/kp3` | Comparing representations needs a computed choice of the faster relationship. |
| `horizontal-vertical-slopes/kp1` | Every valid horizontal-line slope is zero; varying coordinates does not vary the answer. |
| `horizontal-vertical-slopes/kp2` | Every valid vertical-line slope is undefined; coordinate changes would pad that constant outcome. |
| `horizontal-vertical-slopes/kp3` | The current constant-coordinate questions directly expose the requested number; a stronger task design is needed. |
| `slope-as-rate-of-change/kp2` | Sign interpretation needs increasing/decreasing/constant decisions tied to the context. |
| `solutions-of-two-variable-equations/kp1` | Candidate verification requires computed yes/no results, rather than changing the task to coordinate completion. |
| `graphing-from-a-table/kp2` | Preserve plotting/linearity reasoning; numeric continuation would overlap its separate pattern-extension KP. |
| `graphing-linear-equations/kp3` | Preserve horizontal/vertical graph identification and its location. |
| `slopes-of-parallel-perpendicular-lines/kp3` | Classification needs computed parallel/perpendicular/neither answers. |

The snapshot's label-answer path can select a supplied text binding, but does not compute categorical branches from numeric inputs. Supplying the correct label as a visible parameter would reveal the answer. These are design deferrals, not claims that every possible content-only redesign is impossible.
