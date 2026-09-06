# Unit 07 residual repair

Snapshot: integration `e92ec16c`, local root `2eee3e6`. All recipes remain pending. No database operation or approval is involved.

## Checkpoint 1

Eight residual KPs repaired: negative GCF, both advanced differences of squares, nonmonic perfect squares, fully split quadratic form, both multi-step factoring strategies, and quadratic-formula root counting.

- Unit 07 missing recipes: 28 → 20; pending recipes: 74 → 82.
- Whole-course issue KPs and missing recipes: 205 → 197.
- Other whole-course counters unchanged: fewer than four exemplars 124; missing sketch 38; undecidable answer 43; singleton label 0; duplicate family 8; generic sketch 3.
- Real worker gate: 82 passes, 20 retained blockers; all accepted domains exhaustively instantiated (984 instances).
- Independent checks: 96 repaired instances, 32 retained authored answers, eight rejected negative controls, plus all 984 rendered template answers.
- KaTeX 0.17.0: 6,295 formulas in 3,768 fields; zero errors.
- Worker semantic regression tests: 3 passed.

The authoritative audit was rerun with the snapshot's native `content_audit_facts` example and unchanged `foundations_content_audit.py`. Its exit status remains 1 because residual course findings remain. The older correction report's whole-course totals describe an earlier integration state; this checkpoint uses the live snapshot baseline.

Verification is grounded in the loaded curriculum and actual worker output. Factoring shape and completeness were checked independently, including rejection of mathematically equivalent expanded answers. Live serving and database import were not exercised.

## Checkpoint 2

Seven more residual KPs repaired: all three cube identities, vertex-form axis/evaluation, standard-form vertex plus intercept, and both quadratic application KPs.

- Unit 07 missing recipes: 20 → 13; total repaired: 15; pending recipes: 89.
- Whole-course issue KPs and missing recipes: 197 → 190; every other counter remains at baseline.
- Worker gate: 89 passes and 13 current rejections; 1,068 exhaustively instantiated answers independently checked.
- Independent retained-authored checks: 60 answers. New-family checks: 180 instances, 15 rejected negative controls, and semantic collision checks on all 132 new algebra/count instances against authored and sibling families.
- The root-count domain excludes a scaled copy of the authored discriminant equation with repeated root 3. Cube roots stay small through meaningful shifted bases rather than extra dummy parameters.
- KaTeX: 6,708 formulas, zero errors. Worker semantic tests: 3 passed.
