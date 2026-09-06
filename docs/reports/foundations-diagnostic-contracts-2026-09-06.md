# Foundations diagnostic answer contracts — 2026-09-06
This continuation closes the diagnostic-grammar residual in P2.6. It changes reviewed curriculum policies and bounded answer forms. It grants no content approval and touches no database.

## Exact result
The prerequisite audit originally reported 49 Foundations diagnostic answers outside the grammar. The preceding quotient/remainder review made eight decidable. This continuation classified and remediated the remaining 41.

The regenerated real-curriculum audit now reports 285 Foundations topics, 285 decidable diagnostics, zero undecidable diagnostics, zero missing diagnostics, and zero dangling prerequisite edges. `crates/worker/tests/prereq_coverage.rs::every_foundations_diagnostic_is_grammar_decidable` pins those totals against the loaded curriculum.

## Classification of the remaining 41
Each policy follows the mathematical answer shape of the prompt. No item uses an unrestricted prose parser.

| Policy | Count | Topics |
|---|---:|---|
| Closed labels | 13 | `divisibility-rules`, `ratio-tables-equivalent-ratios`, `unit-rates`, `equations-special-cases`, `coordinate-plane`, `horizontal-vertical-slopes`, `slopes-of-parallel-perpendicular-lines`, `solutions-of-inequalities`, `systems-special-cases`, `systems-of-linear-inequalities`, `pythagorean-converse`, `identifying-functions-vertical-line-test`, `one-to-one-functions` |
| Bounded lists | 5 | `consecutive-integer-problems`, `interpreting-graphs-qualitatively`, `graphing-linear-equations`, `systems-word-problems`, `rational-expression-restrictions` |
| Named multipart answers | 11 | `graphing-proportional-relationships`, `interpreting-linear-models`, `graphing-linear-inequalities`, `systems-mixture-problems`, `systems-rate-problems`, `polynomial-basics`, `parabola-vertex-form`, `quadratic-graphs-vertex`, `domain-range`, `trig-graphs-basic`, `trig-graphs-midline` |
| Reduced ratio | 1 | `understanding-ratios` |
| Ascending chain | 1 | `comparing-integers` |
| Polynomial relation | 3 | `point-slope-standard-form`, `writing-inequalities-from-statements`, `writing-quadratics-from-roots` |
| Inequality union | 3 | `and-or-inequalities`, `interval-notation`, `basic-absolute-value-inequalities` |
| Exact finite alternatives | 1 | `zero-product-property` |
| Authored decimal precision | 1 | `continuous-growth-model` |
| Measured units | 2 | `work-rate-problems`, `trig-applications` |

The eight predecessor fixes use `quotient_remainder` policies with their actual divisors. Together the two changes account for all 49 original refusals.

## Verification
- `cargo test -p cadus-core --test answer_contract_diagnostic`: 5 passed.
- `CADUS_PREREQ_BLESS=1 cargo test -p cadus-worker --test prereq_coverage`: 6 passed and regenerated `docs/reports/prerequisite-coverage-2026-09-06.md`.
- The regenerated report is the source of the 285 decidable, zero undecidable, zero missing counts.
