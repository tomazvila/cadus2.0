# Legacy multi-step statement audit

## Complete reviewed scope

All 78 Foundations topics labeled `multi-step` were inspected statement by statement: 464 knowledge-point exemplars plus all 78 diagnostic exemplars, 542 statements total. The classification preserves every question and answer. The source digest covers topic, KP, exemplar index, problem, and answer; the manifest digest also covers each review decision and rationale.

| Statement class | Count | Meaning |
|---|---:|---|
| Component exercise | 342 | An isolated symbolic, numeric, geometric, or conceptual exercise. A sequence of these supplies no shared application scenario. |
| Coherent model | 161 | One verbal or physical situation links the quantities and requested result or interpretation. Preserve this existing modeling content. |
| Linked outputs | 35 | Multiple outputs or a verification concern the same mathematical object. Preserve their internal relationship. |
| Context fragment | 4 | A model or its variable meanings depend on surrounding material. Repair context before standalone reuse. |

At topic level, 35 contain only component exercises, 18 contain only coherent modeling statements, and 25 mix categories. This is a statement-coherence review, not a claim that every word problem combines several skills: some coherent contexts ask one arithmetic operation. Conversely, a purely mathematical question can have linked outputs without a real-world setting.

The legacy format supplies one flat expected answer per exemplar. None of these 78 topic documents is itself a new `IntegratedItem` with authored step contracts, method alternatives, and a final interpretation. The 161 coherent models and 35 linked-output statements are preservation/adaptation candidates; the `multi-step` topic label alone provides no evidence that its independent exemplars form one integrated scenario. No content was replaced or reclassified in the curriculum.

## Context-dependent statements

- `linear-word-problems/kp1/1`: “Using that model” depends on the preceding taxi equation.
- `interpreting-linear-models/kp1/1`: the tank model omits the contextual variable units.
- `interpreting-linear-models/kp2/0`: the bare linear model relies on the earlier tank interpretation.
- `interpreting-linear-models/kp2/1`: the bare cost expression relies on the earlier repair interpretation.

## Reproduction and pins

Run from the repository root:

```sh
cargo run -p cadus-core --bin dump_curriculum -- curriculum | python3 scripts/review/legacy_multistep.py > ~/.cache/legacy-multistep-statements.jsonl
cmp docs/reports/legacy-multistep-statements.jsonl ~/.cache/legacy-multistep-statements.jsonl
cargo test -p cadus-core --test legacy_statement_audit
```

The Python script contains the reviewed rubric, topic decisions, and explicit statement exceptions. It refuses changed source text until that content is reviewed. The Rust regression independently loads the actual curriculum, reconstructs every source row, and verifies complete coverage and the manifest decisions against their pins.

- Source rows SHA-256: `432d59e238c00ffe8d83bac6e3e0e9277689c55f21818249577c96d6476ab00d`.
- Manifest SHA-256: `bb7c2a471bc05786a13bc702a8f9a694e722dbbdfad96ab5e36f8c8329b97599`.

## All 78 topics

Counts include each topic's diagnostic statement. C = component; M = coherent model; L = linked outputs; F = context fragment.

| Topic | C | M | L | F |
|---|---:|---:|---:|---:|
| `absolute-value-equations` | 7 | 0 | 0 | 0 |
| `absolute-value-inequalities` | 7 | 0 | 0 | 0 |
| `and-or-inequalities` | 7 | 0 | 0 | 0 |
| `angle-of-elevation-depression` | 0 | 7 | 0 | 0 |
| `applying-the-quadratic-formula` | 6 | 0 | 1 | 0 |
| `basic-absolute-value-equations` | 5 | 0 | 0 | 0 |
| `basic-absolute-value-inequalities` | 7 | 0 | 0 | 0 |
| `basic-rational-equations` | 7 | 0 | 0 | 0 |
| `comparing-integers` | 5 | 0 | 0 | 0 |
| `completing-square-leading-coefficient` | 7 | 0 | 0 | 0 |
| `completing-the-square` | 6 | 0 | 1 | 0 |
| `compound-inequalities` | 7 | 0 | 0 | 0 |
| `compound-interest` | 0 | 7 | 0 | 0 |
| `consecutive-integer-problems` | 0 | 6 | 0 | 0 |
| `continuous-growth-model` | 1 | 9 | 0 | 0 |
| `converting-to-vertex-form` | 5 | 0 | 2 | 0 |
| `domain-range` | 7 | 0 | 1 | 0 |
| `elimination-with-addition` | 6 | 0 | 1 | 0 |
| `equation-word-problems` | 0 | 7 | 0 | 0 |
| `exponential-equations` | 7 | 0 | 0 | 0 |
| `exponential-equations-same-base` | 7 | 0 | 0 | 0 |
| `exponential-equations-with-logarithms` | 8 | 0 | 0 | 0 |
| `exponential-growth-decay` | 0 | 7 | 0 | 0 |
| `fraction-word-problems` | 0 | 7 | 0 | 0 |
| `graphing-from-a-table` | 7 | 0 | 0 | 0 |
| `graphing-linear-equations` | 5 | 0 | 2 | 0 |
| `graphing-linear-inequalities` | 6 | 0 | 1 | 0 |
| `graphing-proportional-relationships` | 6 | 0 | 1 | 0 |
| `graphing-systems` | 7 | 0 | 0 | 0 |
| `inequality-word-problems` | 0 | 7 | 0 | 0 |
| `integer-word-problems` | 0 | 5 | 0 | 0 |
| `interpreting-graphs-qualitatively` | 1 | 6 | 0 | 0 |
| `interpreting-linear-models` | 0 | 4 | 0 | 3 |
| `interval-notation` | 10 | 0 | 0 | 0 |
| `law-of-cosines` | 7 | 0 | 0 | 0 |
| `law-of-sines` | 6 | 0 | 1 | 0 |
| `law-of-sines-cosines` | 4 | 3 | 1 | 0 |
| `linear-word-problems` | 0 | 6 | 0 | 1 |
| `literal-equations` | 7 | 0 | 0 | 0 |
| `logarithm-basics` | 7 | 0 | 0 | 0 |
| `logarithmic-equations` | 7 | 0 | 0 | 0 |
| `money-geometry-problems` | 0 | 5 | 0 | 0 |
| `parabola-vertex-form` | 3 | 0 | 4 | 0 |
| `parallel-perpendicular-lines` | 7 | 0 | 0 | 0 |
| `percent-applications` | 0 | 7 | 0 | 0 |
| `percentages` | 3 | 4 | 0 | 0 |
| `point-slope-form` | 5 | 0 | 2 | 0 |
| `point-slope-standard-form` | 7 | 0 | 0 | 0 |
| `pythagorean-converse` | 6 | 0 | 1 | 0 |
| `pythagorean-theorem` | 7 | 0 | 0 | 0 |
| `quadratic-applications` | 0 | 5 | 0 | 0 |
| `quadratic-equations-factoring` | 7 | 0 | 0 | 0 |
| `quadratic-formula` | 7 | 0 | 0 | 0 |
| `quadratic-graphs-vertex` | 2 | 0 | 5 | 0 |
| `radical-equations-basic` | 6 | 0 | 1 | 0 |
| `ratio-tables-equivalent-ratios` | 1 | 6 | 0 | 0 |
| `rational-equations` | 7 | 0 | 0 | 0 |
| `rational-expression-restrictions` | 7 | 0 | 0 | 0 |
| `rearranging-formulas` | 5 | 0 | 0 | 0 |
| `slope-intercept-form` | 7 | 0 | 0 | 0 |
| `square-root-property` | 7 | 0 | 0 | 0 |
| `substitution-with-isolated-variable` | 6 | 0 | 1 | 0 |
| `systems-elimination` | 7 | 0 | 0 | 0 |
| `systems-mixture-problems` | 0 | 7 | 0 | 0 |
| `systems-money-problems` | 0 | 7 | 0 | 0 |
| `systems-of-linear-inequalities` | 7 | 0 | 0 | 0 |
| `systems-rate-problems` | 0 | 7 | 0 | 0 |
| `systems-substitution` | 7 | 0 | 0 | 0 |
| `systems-word-problems` | 0 | 7 | 0 | 0 |
| `translating-sentences-to-equations` | 5 | 0 | 0 | 0 |
| `trig-applications` | 0 | 7 | 0 | 0 |
| `trig-graphs-basic` | 3 | 0 | 4 | 0 |
| `trig-graphs-midline` | 2 | 0 | 5 | 0 |
| `understanding-ratios` | 5 | 3 | 0 | 0 |
| `unit-rates` | 0 | 8 | 0 | 0 |
| `work-rate-problems` | 0 | 7 | 0 | 0 |
| `writing-quadratics-from-roots` | 7 | 0 | 0 | 0 |
| `zero-product-property` | 7 | 0 | 0 | 0 |

## Historical uncertainty replay (A3.8)

`legacy_uncertainty_replay.rs` verifies explicit ungraded outcomes in both supported version envelopes across repeated normalization, canonical serialization, full replay, and every incremental split. The reason and recovery-list entry survive, conditional placement is preserved, and no XP appears. A v1 envelope carrying explicit uncertainty is a compatibility fixture; ordinary historical v1 producers did not write this field.

A separate regression preserves an actual v1-shaped miss and its original canonical bytes. Its prose response remains undecidable under an exact checker, but the original recorded outcome is still `incorrect`: current grammar uncertainty alone cannot establish what the historical grader knew. Appending an explicit review correction with an ungraded outcome restores uncertainty on full reprojection without rewriting the source event.

A3.8 remains open. No production v1-miss scan or recovery-list population was performed, and missing historical uncertainty cannot be reconstructed with certainty from `correct: false`. P4.4 is complete as an inspection and preservation audit; neither item grants content-store approval or authorizes deployment.

## Verification

Three focused regressions passed: complete statement/manifest correspondence and two historical-uncertainty replay tests. Targeted Clippy passed with warnings denied. Formatting, changed-file LOC, whole-repository Rust dead-code scan, and new-test complexity checks passed. The Python review script reproduced the checked-in 542-row manifest byte for byte. No curriculum, production event, runtime migration, approval, or deployment was changed.
