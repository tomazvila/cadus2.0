# Pending-template retirement, 2026-10-06
The Foundations content commits of 2026-10-06 (dddf5ba3, 42e35bce, 3590afd6, 61311a0c, f3edb45f, 30f75056, 854e21cb, 7c25bed7) rewrote the exemplars of the exponents-radicals and polynomials-quadratics knowledge points below, and 39230f7a, db5bcad1 and 7d757a51 added the factored and expanded polynomial forms they use. The pending templates below followed the old answer formats or now generate curated problems, so they were removed from their pending sets. The curated exemplars are the source for these knowledge points. No template was approved or imported, and no database row changed.

The reports use the fields of the 2026-10-05 retirement (`body`, `previous_digest`, `source`, `superseded_by`, `current_gate`, `refusal_code`, `refusal`, `retirement_reason`). A unit06 row whose draft and fixture candidate are distinct lineages also keeps the removed candidate under `candidate`, with its own digest and gate verdict.

| Set | Knowledge point | Current gate | Rewritten by |
|---|---|---|---|
| accepted-template-recipes | evaluating-polynomials/kp1 | refused: answer-contract | 7c25bed7 |
| accepted-template-recipes | evaluating-polynomials/kp2 | refused: answer-contract | 7c25bed7 |
| accepted-template-recipes | radical-operations/kp2 | refused: answer-contract | 30f75056 |
| unit06-correction | exponent-quotient-rule/kp2 | draft accepted; candidate refused: sample-eval | dddf5ba3 |
| unit06-correction | power-of-a-power-rule/kp2 | draft accepted; candidate refused: sample-eval | dddf5ba3 |
| unit06-correction | zero-exponent-rule/kp2 | refused: answer-contract (draft and candidate) | 42e35bce |
| unit06-correction | negative-zero-exponents/kp1 | refused: answer-contract (draft and candidate) | 42e35bce |
| unit06-correction | negative-zero-exponents/kp2 | refused: answer-contract | 42e35bce |
| unit06-correction | negative-zero-exponents/kp3 | refused: answer-contract | 42e35bce |
| unit06-correction | scientific-notation/kp1 | refused: answer-contract (draft and candidate) | 42e35bce |
| unit06-correction | scientific-notation/kp2 | refused: answer-contract (draft and candidate) | 42e35bce |
| unit06-correction | scientific-notation/kp3 | draft accepted; candidate refused: sample-agreement | 42e35bce |
| unit06-correction | cube-roots/kp1 | accepted; two instances are now curated exemplars without a rehearsal role | f3edb45f |
| unit06-correction | pythagorean-theorem/kp1 | refused: answer-kind | f3edb45f |
| unit06-correction | pythagorean-theorem/kp2 | refused: answer-contract | f3edb45f |
| unit06-correction | pythagorean-theorem/kp3 | refused: answer-kind | f3edb45f |
| unit06-correction | radical-operations/kp2 | refused: answer-contract | 30f75056 |
| unit06-correction | radical-exponent-conversion/kp3 | refused: answer-contract | 30f75056 |
| unit06-correction | rational-exponents/kp1 | refused: answer-contract | 30f75056 |
| unit07-complement | difference-of-squares/kp1 | refused: space-floor | 61311a0c |

The unit06 rows were removed from both `unit06-correction/drafts.json` and `crates/worker/tests/fixtures/unit06-template-candidates.json`, and `unit06-correction/current-technical.json` was regenerated with `cargo run -p cadus-worker --example unit06_templates`. The receipt's negative-control helper now changes the coefficient of a simplest-radical answer, because six remaining candidates inherit the new `required_simplest_radical` contract. `accepted-template-recipes/manifest.json` counts 36 recipes.

The whole-course Teach source set retired 34 more templates (21 answer-contract, 7 answer-kind, 5 sample-agreement, 1 space-floor) into `whole-course-teach-retired-source-templates.json`; `inputs/templates.json` holds 760. Their Teach pages are gated against the curated exemplars alone. `scientific-notation/kp1` and `kp2` have no Teach page, so their `previous_digest` is the canonical SHA-256 of the removed row. The technical evidence was regenerated with the refresh example and `refresh_whole_course_teach_v2.py --update` against curriculum hash 22e69d47.
