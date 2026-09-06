# Foundations answer inventory (unit f1-inventory, design decision D-F1)

Date: 2026-09-06. Branch `framework/f1-inventory`.

Instrument: `crates/core/tests/answer_inventory.rs`, with the helpers
`crates/core/tests/common/inventory.rs` (the walk) and
`crates/core/tests/common/shape.rs` (the shape classifier). The test loads the
checked-in tree `curriculum/` with the real loader (`load_curriculum`), walks
every Foundations topic, knowledge point and exemplar in load order, and runs
the real answer grammar `cadus_core::answer::canonical_form` on every authored
answer.

Command:

```sh
CADUS_INVENTORY_DUMP=~/.cache/cadus2_f1/inventory.jsonl \
  cargo test -p cadus-core --test answer_inventory -- --nocapture
```

The dump writes one JSON line per exemplar with the fields `course`, `unit`,
`topic_id`, `answer_kind`, `kp_id`, `exemplar_index`, `answer`, `shape`,
`verdict`, `has_solution_sketch`. Without the env var the test asserts the
counts of `docs/reviews/FRAMEWORK-audit-2026-09-06.md` finding (i) and passes.

## 0. Headline

| Fact | Value |
|---|---:|
| Foundations knowledge points | 809 |
| Foundations exemplars | 1,695 |
| Exemplars with no solution sketch | 679 |
| Answers the grammar decides today | 1,325 (78.2 %) |
| Answers the grammar refuses today | 370 (21.8 %) |
| `multi-step` topics | 78 |
| `multi-step` exemplars | 464 |
| `multi-step` exemplars the grammar already decides | 233 (50.2 %) |
| `multi-step` topics that an `exact` contract makes gradable with NO grammar change | **19 of 78** |
| Knowledge points with 0 decidable exemplars | 146 |
| Knowledge points with 1 decidable exemplar | 58 |

The 233 decided `multi-step` answers are the direct measure of audit finding
(a): the grammar decides them, and the kind gate of
`crates/web/src/grade/route.rs:21-35` answers HTTP 409 for every one of them.

### Verdict terms

- `decided` — `canonical_form` reads the AUTHORED answer into the canonical
  form. The answer is inside the decidable grammar.
- `undecidable(<reason>)` — the grammar refuses the authored answer. The reason
  is the fixed string of `cadus_core::answer::Undecidable`.

The verdict reads the authored answer alone and reads no `answer_kind`, so it
states whether the CONTENT is gradable and never whether the current route
serves it.

## 1. Shape by answer kind

Foundations holds no `proof` topic, so the `proof` column is absent.

| shape | numeric | expression | multi-step | total |
|---|---:|---:|---:|---:|
| `integer` | 413 | 112 | 63 | 588 |
| `decimal` | 30 | 16 | 1 | 47 |
| `fraction` | 14 | 90 | 5 | 109 |
| `mixed_number` | 0 | 7 | 0 | 7 |
| `radical` | 2 | 43 | 11 | 56 |
| `rational_exponent` | 0 | 3 | 0 | 3 |
| `expression` | 9 | 288 | 3 | 300 |
| `equation_or_inequality` | 0 | 27 | 193 | 220 |
| `interval` | 0 | 0 | 6 | 6 |
| `value_with_unit` | 13 | 2 | 35 | 50 |
| `quotient_remainder` | 8 | 4 | 0 | 12 |
| `coordinates` | 0 | 17 | 35 | 52 |
| `ordered_list` | 2 | 15 | 16 | 33 |
| `set` | 0 | 4 | 0 | 4 |
| `prose` | 10 | 98 | 88 | 196 |
| `other` | 4 | 0 | 8 | 12 |
| **total** | **505** | **726** | **464** | **1,695** |

Read the table with audit finding (b) in view. `answer_kind` is the TASK
COMPLEXITY of the topic and not the shape of the answer: 63 `multi-step`
answers are plain integers, and 10 `numeric` answers are prose. The kind and the
shape are two different facts, which is the evidence for D-F1.

## 2. Shape by grammar verdict

| shape | decided | undecidable | total | decided share |
|---|---:|---:|---:|---:|
| `integer` | 588 | 0 | 588 | 100.0 % |
| `decimal` | 47 | 0 | 47 | 100.0 % |
| `fraction` | 109 | 0 | 109 | 100.0 % |
| `mixed_number` | 7 | 0 | 7 | 100.0 % |
| `radical` | 56 | 0 | 56 | 100.0 % |
| `rational_exponent` | 0 | 3 | 3 | 0.0 % |
| `expression` | 289 | 11 | 300 | 96.3 % |
| `equation_or_inequality` | 118 | 102 | 220 | 53.6 % |
| `interval` | 2 | 4 | 6 | 33.3 % |
| `value_with_unit` | 34 | 16 | 50 | 68.0 % |
| `quotient_remainder` | 0 | 12 | 12 | 0.0 % |
| `coordinates` | 52 | 0 | 52 | 100.0 % |
| `ordered_list` | 19 | 14 | 33 | 57.6 % |
| `set` | 4 | 0 | 4 | 100.0 % |
| `prose` | 0 | 196 | 196 | 0.0 % |
| `other` | 0 | 12 | 12 | 0.0 % |
| **total** | **1,325** | **370** | **1,695** | **78.2 %** |

Five shapes are complete today: `integer`, `decimal`, `fraction`,
`mixed_number`, `radical`, `coordinates` and `set`. Three shapes are empty
today: `rational_exponent`, `quotient_remainder`, `prose`.

The refusal reasons, as the grammar spells them:

| count | reason | shapes |
|---:|---|---|
| 272 | a name that is not a function or variable | prose 183, equation 62, list 14, unit 7, quotient 4, interval 2 |
| 57 | a character outside the grammar | prose 13, other 12, expression 11, equation 10, unit 9, interval 2 |
| 25 | trailing text after the answer | equation 25 |
| 8 | a number glued to a name reads as a label | quotient 8 |
| 3 | an exponent that is not a whole number | rational exponent 3 |
| 2 | a symbol where a value belongs | equation 2 |
| 2 | an inequality with no bare variable | equation 2 |
| 1 | a chained inequality needs one variable in the middle | equation 1 |

## 3. The 78 `multi-step` topics

Column `decided` counts the exemplars of the topic that the grammar decides
today. Column `exact now` is `yes` when every exemplar of the topic is decided:
that topic becomes gradable with an `exact` contract and NO grammar change.

**19 of the 78 topics are `exact now`.**

| topic | unit | final-answer shapes | decided | proposed contract | exact now |
|---|---|---|---:|---|---|
| `fraction-word-problems` | fractions-decimals | integer x4, fraction x2 | 6/6 | `exact` | yes |
| `percentages` | fractions-decimals | value_with_unit x3, integer x2, prose | 5/6 | `unit` | no |
| `understanding-ratios` | fractions-decimals | other x5, integer, prose | 1/7 | `exact` | no |
| `ratio-tables-equivalent-ratios` | fractions-decimals | integer x4, prose x2 | 4/6 | `exact` | no |
| `unit-rates` | fractions-decimals | integer x6, prose | 6/7 | `exact` | no |
| `percent-applications` | fractions-decimals | integer x5, decimal | 6/6 | `exact` | yes |
| `comparing-integers` | integers-negatives | equation_or_inequality x3, prose | 0/4 | `exact` | no |
| `integer-word-problems` | integers-negatives | integer x4 | 4/4 | `exact` | yes |
| `rearranging-formulas` | expressions-equations | equation_or_inequality x4 | 4/4 | `exact` | yes |
| `literal-equations` | expressions-equations | equation_or_inequality x6 | 6/6 | `exact` | yes |
| `basic-absolute-value-equations` | expressions-equations | equation_or_inequality x4 | 0/4 | `exact` | no |
| `absolute-value-equations` | expressions-equations | equation_or_inequality x4, prose x2 | 0/6 | `exact` | no |
| `translating-sentences-to-equations` | expressions-equations | integer x2, equation_or_inequality x2 | 2/4 | `exact` | no |
| `consecutive-integer-problems` | expressions-equations | ordered_list x3, integer x2 | 2/5 | `exact` | no |
| `money-geometry-problems` | expressions-equations | integer x4 | 4/4 | `exact` | yes |
| `equation-word-problems` | expressions-equations | integer x6 | 6/6 | `exact` | yes |
| `interpreting-graphs-qualitatively` | linear-graphs | prose x6 | 0/6 | `none` | no |
| `graphing-proportional-relationships` | linear-graphs | prose x3, ordered_list, integer, fraction | 2/6 | `exact` | no |
| `graphing-from-a-table` | linear-graphs | ordered_list x2, integer x2, prose, coordinates | 5/6 | `exact` | no |
| `slope-intercept-form` | linear-graphs | equation_or_inequality x6 | 6/6 | `exact` | yes |
| `graphing-linear-equations` | linear-graphs | ordered_list x4, prose x2 | 0/6 | `exact` | no |
| `point-slope-form` | linear-graphs | equation_or_inequality x4, prose, coordinates | 3/6 | `exact` | no |
| `point-slope-standard-form` | linear-graphs | equation_or_inequality x5, fraction | 2/6 | `exact` | no |
| `parallel-perpendicular-lines` | linear-graphs | equation_or_inequality x5, prose | 5/6 | `exact` | no |
| `interpreting-linear-models` | linear-graphs | prose x4, value_with_unit, integer | 2/6 | `none` | no |
| `linear-word-problems` | linear-graphs | equation_or_inequality x3, integer x2, value_with_unit | 5/6 | `exact` | no |
| `and-or-inequalities` | systems-inequalities | equation_or_inequality x4, prose x2 | 2/6 | `exact` | no |
| `compound-inequalities` | systems-inequalities | equation_or_inequality x6 | 4/6 | `exact` | no |
| `interval-notation` | systems-inequalities | interval x6, equation_or_inequality x2, prose | 3/9 | `exact` | no |
| `inequality-word-problems` | systems-inequalities | prose x4, integer x2 | 2/6 | `none` | no |
| `basic-absolute-value-inequalities` | systems-inequalities | equation_or_inequality x4, prose x2 | 2/6 | `exact` | no |
| `absolute-value-inequalities` | systems-inequalities | equation_or_inequality x6 | 3/6 | `exact` | no |
| `graphing-linear-inequalities` | systems-inequalities | prose x6 | 0/6 | `none` | no |
| `graphing-systems` | systems-inequalities | coordinates x4, prose, equation_or_inequality | 4/6 | `coordinates` | no |
| `substitution-with-isolated-variable` | systems-inequalities | coordinates x6 | 6/6 | `coordinates` | yes |
| `systems-substitution` | systems-inequalities | coordinates x6 | 6/6 | `coordinates` | yes |
| `elimination-with-addition` | systems-inequalities | coordinates x6 | 6/6 | `coordinates` | yes |
| `systems-elimination` | systems-inequalities | coordinates x6 | 6/6 | `coordinates` | yes |
| `systems-of-linear-inequalities` | systems-inequalities | prose x4, equation_or_inequality, coordinates | 1/6 | `none` | no |
| `systems-money-problems` | systems-inequalities | prose x4, integer x2 | 2/6 | `none` | no |
| `systems-mixture-problems` | systems-inequalities | prose x4, value_with_unit, equation_or_inequality | 1/6 | `none` | no |
| `systems-rate-problems` | systems-inequalities | prose x4, ordered_list x2 | 0/6 | `none` | no |
| `systems-word-problems` | systems-inequalities | ordered_list x3, prose x3 | 0/6 | `exact` | no |
| `pythagorean-theorem` | exponents-radicals | integer x4, radical x2 | 6/6 | `exact` | yes |
| `pythagorean-converse` | exponents-radicals | prose x6 | 0/6 | `none` | no |
| `radical-equations-basic` | exponents-radicals | equation_or_inequality x5, prose | 5/6 | `exact` | no |
| `zero-product-property` | polynomials-quadratics | equation_or_inequality x6 | 0/6 | `exact` | no |
| `quadratic-equations-factoring` | polynomials-quadratics | equation_or_inequality x6 | 0/6 | `exact` | no |
| `square-root-property` | polynomials-quadratics | equation_or_inequality x6 | 0/6 | `exact` | no |
| `writing-quadratics-from-roots` | polynomials-quadratics | equation_or_inequality x6 | 0/6 | `exact` | no |
| `completing-the-square` | polynomials-quadratics | equation_or_inequality x5, prose | 0/6 | `exact` | no |
| `completing-square-leading-coefficient` | polynomials-quadratics | equation_or_inequality x6 | 0/6 | `exact` | no |
| `applying-the-quadratic-formula` | polynomials-quadratics | equation_or_inequality x6 | 0/6 | `exact` | no |
| `quadratic-formula` | polynomials-quadratics | equation_or_inequality x5, prose | 0/6 | `exact` | no |
| `parabola-vertex-form` | polynomials-quadratics | coordinates x2, prose x2, equation_or_inequality, integer | 3/6 | `multipart` | no |
| `quadratic-graphs-vertex` | polynomials-quadratics | prose x4, coordinates x2 | 2/6 | `none` | no |
| `converting-to-vertex-form` | polynomials-quadratics | equation_or_inequality x6 | 4/6 | `exact` | no |
| `quadratic-applications` | polynomials-quadratics | value_with_unit x2, integer, ordered_list | 2/4 | `unit` | no |
| `domain-range` | functions-exponentials | equation_or_inequality x5, prose x2 | 4/7 | `exact` | no |
| `exponential-growth-decay` | functions-exponentials | integer x3, value_with_unit x3 | 4/6 | `unit` | no |
| `compound-interest` | functions-exponentials | value_with_unit x4, equation_or_inequality x2 | 2/6 | `unit` | no |
| `exponential-equations-same-base` | functions-exponentials | equation_or_inequality x6 | 6/6 | `exact` | yes |
| `exponential-equations` | functions-exponentials | equation_or_inequality x6 | 6/6 | `exact` | yes |
| `logarithm-basics` | functions-exponentials | equation_or_inequality x5, fraction | 2/6 | `exact` | no |
| `logarithmic-equations` | functions-exponentials | equation_or_inequality x6 | 6/6 | `exact` | yes |
| `exponential-equations-with-logarithms` | functions-exponentials | equation_or_inequality x7 | 4/7 | `exact` | no |
| `continuous-growth-model` | functions-exponentials | value_with_unit x4, equation_or_inequality x3, integer, expression | 3/9 | `exact` | no |
| `rational-expression-restrictions` | rational-trig | equation_or_inequality x6 | 4/6 | `exact` | no |
| `basic-rational-equations` | rational-trig | equation_or_inequality x6 | 6/6 | `exact` | yes |
| `rational-equations` | rational-trig | equation_or_inequality x4, prose x2 | 2/6 | `exact` | no |
| `work-rate-problems` | rational-trig | value_with_unit x5, equation_or_inequality | 1/6 | `unit` | no |
| `angle-of-elevation-depression` | rational-trig | value_with_unit x4, radical x2 | 6/6 | `unit` | yes |
| `trig-applications` | rational-trig | value_with_unit x3, other x2, equation_or_inequality | 3/6 | `approx` | no |
| `trig-graphs-basic` | rational-trig | prose x3, expression x2, integer | 3/6 | `exact` | no |
| `trig-graphs-midline` | rational-trig | equation_or_inequality x4, prose x2 | 2/6 | `exact` | no |
| `law-of-sines` | rational-trig | radical x2, other, integer, equation_or_inequality, value_with_unit | 4/6 | `exact` | no |
| `law-of-cosines` | rational-trig | radical x3, value_with_unit x2, integer | 6/6 | `exact` | yes |
| `law-of-sines-cosines` | rational-trig | prose x3, radical x2, equation_or_inequality, value_with_unit | 3/7 | `exact` | no |

### 3.1 The contract distribution of the 78

| proposed contract | topics |
|---|---:|
| `exact` | 55 |
| `none` | 10 |
| `unit` | 6 |
| `coordinates` | 5 |
| `multipart` | 1 |
| `approx` | 1 |

The rule that assigns the contract reads the shapes of the topic's exemplars, in
this order: any `quotient_remainder` shape gives `quotient_remainder`; a quarter
or more of the answers with the `≈` marker gives `approx`; a majority of `prose`
gives `none`; half or more of `value_with_unit` gives `unit`; half or more of
`coordinates` gives `coordinates`; half or more with a `;` separator gives
`multipart`; half or more of `set` gives `set`; the rest is `exact`.

19 of the 55 `exact` topics are gradable today. 41 `exact` topics still hold at
least one answer the grammar refuses, and section 5 names the productions that
recover them. 4 of the 78 topics carry a contract other than `exact` and are
therefore blocked on the contract work of f3-contract, not on the grammar.

### 3.2 The 19 topics that an `exact` contract unblocks today

`fraction-word-problems`, `percent-applications`, `integer-word-problems`,
`rearranging-formulas`, `literal-equations`, `money-geometry-problems`,
`equation-word-problems`, `slope-intercept-form`,
`substitution-with-isolated-variable`, `systems-substitution`,
`elimination-with-addition`, `systems-elimination`, `pythagorean-theorem`,
`exponential-equations-same-base`, `exponential-equations`,
`logarithmic-equations`, `basic-rational-equations`,
`angle-of-elevation-depression`, `law-of-cosines`.

Each of these topics needs one change and one change only: the grade route must
grade by CONTRACT and not by `answer_kind` (D-F1, unit f4-outcome). No grammar
change, no content change.

## 4. Knowledge points with 0 or 1 decidable exemplars

The unit of readiness is the knowledge point, not the topic (D-F5,
"practicable: >= 3 decidable distinct items"). The test counts DISTINCT
decidable exemplars per knowledge point: an exemplar counts when the grammar
decides its authored answer, and two exemplars with the same problem text and
the same answer text count once.

Distribution over the 809 knowledge points: `{0: 146, 1: 58, 2: 548, 3: 57}`.
No Foundations knowledge point holds a duplicate decidable exemplar, so the
distinct count equals the decided count for every knowledge point.

**No Foundations knowledge point reaches the D-F5 bar of 3 distinct decidable
items today.** 57 knowledge points hold 3, and every one of those 57 needs a
held-out fourth item before an assessment item exists that practice never
served.

| unit file | knowledge points | 0 decidable | 1 decidable | share at risk |
|---|---:|---:|---:|---:|
| `00-arithmetic-core.yaml` | 81 | 9 | 3 | 15% |
| `01-fractions-decimals.yaml` | 90 | 4 | 7 | 12% |
| `02-integers-negatives.yaml` | 46 | 2 | 1 | 7% |
| `03-expressions-equations.yaml` | 67 | 11 | 2 | 19% |
| `04-linear-graphs.yaml` | 69 | 19 | 13 | 46% |
| `05-systems-inequalities.yaml` | 75 | 38 | 4 | 56% |
| `06-exponents-radicals.yaml` | 78 | 3 | 4 | 9% |
| `07-polynomials-quadratics.yaml` | 102 | 31 | 6 | 36% |
| `08-functions-exponentials.yaml` | 92 | 18 | 8 | 28% |
| `09-rational-trig.yaml` | 103 | 11 | 10 | 20% |
| `10-measurement-units.yaml` | 6 | 0 | 0 | 0% |

The audit's finding (i) reports `{0: 48, 1: 33, 2: 447, 3: 55}` over a 583
knowledge-point subset, because it joins the committed corpus fixtures
`corpus_1_0.jsonl` and `undecidable_1_0.jsonl`, which cover `numeric` and
`expression` topics only. This inventory runs the live grammar over all 809
knowledge points, the 226 knowledge points of the 78 `multi-step` topics
included. The two numbers measure two different sets and both hold.

### 4.1 The list, per unit file


#### `00-arithmetic-core.yaml`

0 decidable (9): `division-with-remainders/kp1`, `division-with-remainders/kp2`, `long-division-one-digit/kp3`, `long-division/kp3`, `factors-and-multiples/kp3`, `divisibility-rules/kp1`, `divisibility-rules/kp2`, `divisibility-rules/kp3`, `prime-composite-numbers/kp1`

1 decidable (3): `perfect-squares/kp2`, `prime-composite-numbers/kp3`, `rounding-estimation/kp3`

#### `01-fractions-decimals.yaml`

0 decidable (4): `comparing-ordering-decimals/kp2`, `understanding-ratios/kp1`, `understanding-ratios/kp2`, `ratio-tables-equivalent-ratios/kp3`

1 decidable (7): `fractions-on-number-line/kp3`, `equivalent-fractions/kp3`, `comparing-ordering-fractions/kp3`, `improper-fractions-mixed-numbers/kp3`, `percentages/kp3`, `understanding-ratios/kp3`, `unit-rates/kp2`

#### `02-integers-negatives.yaml`

0 decidable (2): `comparing-integers/kp1`, `comparing-integers/kp2`

1 decidable (1): `integer-multiplication-division/kp3`

#### `03-expressions-equations.yaml`

0 decidable (11): `equivalent-expressions/kp1`, `checking-a-solution/kp1`, `equations-special-cases/kp1`, `equations-special-cases/kp2`, `basic-absolute-value-equations/kp1`, `basic-absolute-value-equations/kp2`, `absolute-value-equations/kp1`, `absolute-value-equations/kp2`, `absolute-value-equations/kp3`, `translating-sentences-to-equations/kp2`, `consecutive-integer-problems/kp2`

1 decidable (2): `parts-of-an-expression/kp2`, `equations-special-cases/kp3`

#### `04-linear-graphs.yaml`

0 decidable (19): `coordinate-plane/kp1`, `interpreting-graphs-qualitatively/kp1`, `interpreting-graphs-qualitatively/kp2`, `interpreting-graphs-qualitatively/kp3`, `graphing-proportional-relationships/kp1`, `graphing-proportional-relationships/kp3`, `horizontal-vertical-slopes/kp2`, `slope-as-rate-of-change/kp2`, `solutions-of-two-variable-equations/kp1`, `reading-slope-intercept-equations/kp3`, `graphing-linear-equations/kp1`, `graphing-linear-equations/kp2`, `graphing-linear-equations/kp3`, `point-slope-form/kp1`, `point-slope-standard-form/kp1`, `point-slope-standard-form/kp3`, `slopes-of-parallel-perpendicular-lines/kp3`, `interpreting-linear-models/kp1`, `interpreting-linear-models/kp2`

1 decidable (13): `plotting-points/kp2`, `coordinate-plane/kp2`, `proportional-relationships/kp2`, `slope-from-a-graph/kp2`, `slope-from-two-points/kp3`, `slope/kp3`, `graphing-from-a-table/kp2`, `reading-slope-intercept-equations/kp1`, `reading-slope-intercept-equations/kp2`, `point-slope-form/kp2`, `slopes-of-parallel-perpendicular-lines/kp1`, `parallel-perpendicular-lines/kp3`, `linear-word-problems/kp1`

#### `05-systems-inequalities.yaml`

0 decidable (38): `solutions-of-inequalities/kp1`, `solutions-of-inequalities/kp2`, `solutions-of-inequalities/kp3`, `graphing-inequalities-number-line/kp1`, `graphing-inequalities-number-line/kp2`, `one-step-inequalities/kp3`, `two-step-inequalities/kp3`, `writing-inequalities-from-statements/kp2`, `and-or-inequalities/kp1`, `and-or-inequalities/kp3`, `compound-inequalities/kp3`, `interval-notation/kp3`, `inequality-word-problems/kp1`, `inequality-word-problems/kp2`, `basic-absolute-value-inequalities/kp2`, `basic-absolute-value-inequalities/kp3`, `absolute-value-inequalities/kp2`, `graphing-linear-inequalities/kp1`, `graphing-linear-inequalities/kp2`, `graphing-linear-inequalities/kp3`, `checking-systems-solutions/kp1`, `checking-systems-solutions/kp2`, `graphing-systems/kp3`, `systems-special-cases/kp1`, `systems-special-cases/kp2`, `systems-special-cases/kp3`, `systems-of-linear-inequalities/kp1`, `systems-of-linear-inequalities/kp2`, `systems-money-problems/kp1`, `systems-money-problems/kp2`, `systems-mixture-problems/kp2`, `systems-mixture-problems/kp3`, `systems-rate-problems/kp1`, `systems-rate-problems/kp2`, `systems-rate-problems/kp3`, `systems-word-problems/kp1`, `systems-word-problems/kp2`, `systems-word-problems/kp3`

1 decidable (4): `interval-notation/kp2`, `absolute-value-inequalities/kp3`, `systems-of-linear-inequalities/kp3`, `systems-mixture-problems/kp1`

#### `06-exponents-radicals.yaml`

0 decidable (3): `pythagorean-converse/kp1`, `pythagorean-converse/kp2`, `pythagorean-converse/kp3`

1 decidable (4): `radical-exponent-conversion/kp1`, `radical-exponent-conversion/kp2`, `rational-exponents/kp3`, `radical-equations-basic/kp3`

#### `07-polynomials-quadratics.yaml`

0 decidable (31): `polynomial-basics/kp2`, `polynomial-division/kp2`, `synthetic-division/kp2`, `zero-product-property/kp1`, `zero-product-property/kp2`, `zero-product-property/kp3`, `quadratic-equations-factoring/kp1`, `quadratic-equations-factoring/kp2`, `quadratic-equations-factoring/kp3`, `square-root-property/kp1`, `square-root-property/kp2`, `square-root-property/kp3`, `writing-quadratics-from-roots/kp1`, `writing-quadratics-from-roots/kp2`, `writing-quadratics-from-roots/kp3`, `completing-the-square/kp1`, `completing-the-square/kp2`, `completing-the-square/kp3`, `completing-square-leading-coefficient/kp1`, `completing-square-leading-coefficient/kp2`, `completing-square-leading-coefficient/kp3`, `applying-the-quadratic-formula/kp1`, `applying-the-quadratic-formula/kp2`, `applying-the-quadratic-formula/kp3`, `quadratic-formula/kp1`, `quadratic-formula/kp2`, `quadratic-formula/kp3`, `parabola-vertex-form/kp2`, `quadratic-graphs-vertex/kp2`, `quadratic-graphs-vertex/kp3`, `converting-to-vertex-form/kp3`

1 decidable (6): `polynomial-basics/kp1`, `perfect-square-trinomials/kp1`, `choosing-factoring-strategy/kp1`, `parabola-vertex-form/kp3`, `quadratic-applications/kp1`, `quadratic-applications/kp2`

#### `08-functions-exponentials.yaml`

0 decidable (18): `identifying-functions-vertical-line-test/kp1`, `identifying-functions-vertical-line-test/kp2`, `domain-range/kp1`, `one-to-one-functions/kp1`, `one-to-one-functions/kp2`, `one-to-one-functions/kp3`, `exponential-functions/kp1`, `exponential-functions/kp2`, `graphs-of-exponential-functions/kp3`, `compound-interest/kp1`, `compound-interest/kp2`, `logarithm-basics/kp1`, `logarithm-basics/kp2`, `log-power-rule/kp1`, `logarithm-properties/kp1`, `logarithm-properties/kp2`, `exponential-equations-with-logarithms/kp1`, `continuous-growth-model/kp3`

1 decidable (8): `exponential-growth-decay/kp2`, `exponential-growth-decay/kp3`, `log-product-quotient-rules/kp1`, `log-product-quotient-rules/kp2`, `log-product-quotient-rules/kp3`, `log-power-rule/kp3`, `exponential-equations-with-logarithms/kp2`, `continuous-growth-model/kp1`

#### `09-rational-trig.yaml`

0 decidable (11): `rational-expression-restrictions/kp3`, `rational-equations/kp2`, `rational-equations/kp3`, `work-rate-problems/kp1`, `work-rate-problems/kp2`, `right-triangle-trig/kp2`, `trig-applications/kp2`, `trig-graphs-basic/kp3`, `trig-graphs-midline/kp2`, `trig-graphs-midline/kp3`, `law-of-sines-cosines/kp1`

1 decidable (10): `work-rate-problems/kp3`, `solving-right-triangles-sides/kp2`, `solving-right-triangles-sides/kp3`, `special-right-triangles/kp2`, `trig-applications/kp3`, `unit-circle/kp1`, `trig-graphs-basic/kp1`, `law-of-sines/kp1`, `law-of-sines/kp3`, `law-of-sines-cosines/kp2`

#### `10-measurement-units.yaml`

0 decidable (0): none

1 decidable (0): none

## 5. The grammar productions the refused answers need (feeds f2-grammar)

Each of the 370 refused answers names the set of productions it needs. The
table below adds the productions one at a time, in the order that recovers the
most answers at each step. An answer counts as recovered when the grammar holds
every production it needs.

- **answers that need it** — refused answers whose need set holds the production.
- **recovered alone** — refused answers whose need set is exactly that one
  production. Adding the production alone decides them.
- **recovered at this step** — answers the step decides, with every earlier
  production of the table in place.

| step | production | answers that need it | recovered alone | recovered at this step | cumulative decided |
|---:|---|---:|---:|---:|---:|
| 1 | `label` | 147 | 147 | 147 | 1,472 (86.8 %) |
| 2 | `list` | 71 | 38 | 38 | 1,510 (89.1 %) |
| 3 | `equation` | 93 | 22 | 30 | 1,540 (90.9 %) |
| 4 | `disjunction` | 57 | 5 | 57 | 1,597 (94.2 %) |
| 5 | `unit` | 32 | 11 | 21 | 1,618 (95.5 %) |
| 6 | `currency` | 15 | 3 | 15 | 1,633 (96.3 %) |
| 7 | `log_base` | 15 | 10 | 15 | 1,648 (97.2 %) |
| 8 | `quotient_remainder` | 12 | 12 | 12 | 1,660 (97.9 %) |
| 9 | `multipart` | 11 | 2 | 11 | 1,671 (98.6 %) |
| 10 | `approx_marker` | 7 | 6 | 7 | 1,678 (99.0 %) |
| 11 | `ratio` | 5 | 5 | 5 | 1,683 (99.3 %) |
| 12 | `interval_union` | 4 | 0 | 4 | 1,687 (99.5 %) |
| 13 | `rational_exponent` | 3 | 3 | 3 | 1,690 (99.7 %) |

**The top five by recovered answers: `label` 147, `disjunction` 57, `list` 38,
`equation` 30, `unit` 21. Together they recover 293 of the 370 refused answers
and take Foundations from 78.2 % to 95.5 % decided.**

Five answers stay refused after all thirteen productions: `<`, `>`,
`-4 < -1 < 3`, `y/3 > 6`, `P_0`. The first two are a comparison SYMBOL as the
whole answer, the third is a true statement with no variable, the fourth is an
inequality whose variable is not bare, and the fifth is a subscripted parameter
name. All five are content defects or `label` answers, not grammar gaps. Fix
them in the curriculum, not in the parser.

### 5.1 What each production is

| production | what it reads | example answer | topic |
|---|---|---|---|
| `label` | one word or phrase from a closed authored vocabulary, compared as a canonical token | `yes` | `perfect-squares` |
| `list` | two or more values joined by `,` or ` and `, ordered or unordered by the contract | `13 and 14` | `consecutive-integer-problems` |
| `equation` | a general `lhs = rhs`, both sides expressions, not only the `x = value` label the canon holds today | `x^2 + 2x - 15 = 0` | `writing-quadratics-from-roots` |
| `disjunction` | `A or B` over two relations or two values | `x = 7 or x = -7` | `basic-absolute-value-equations` |
| `unit` | a value with a unit from a unit table, with the quantity of the table | `4.2 L` | `systems-mixture-problems` |
| `currency` | a currency sign as a unit prefix | `€1081.60` | `compound-interest` |
| `log_base` | `log_b(x)` and `log_3(81)`: the `_` subscript as the base of the log | `log_b(x) + log_b(y)` | `log-product-quotient-rules` |
| `quotient_remainder` | `q Rr` and `q remainder r`, both into one tuple | `9 R2` | `division-with-remainders` |
| `multipart` | a `;` that separates the parts of one answer, each part with its own contract | `x = 2; y = 3` | `parabola-vertex-form` |
| `approx_marker` | a leading `≈` that marks the authored value as a rounding | `≈ 14.14` | `law-of-sines` |
| `ratio` | `a:b` as an ordered pair, reduced by the contract | `2:3` | `understanding-ratios` |
| `interval_union` | `∞` as an open end and `∪` as the union of two ranges | `(-∞, -2) ∪ (4, ∞)` | `interval-notation` |
| `rational_exponent` | `a^(p/q)` into the radical canon, with a bounded `q` | `x^(1/2)` | `radical-exponent-conversion` |

D-F3 names five of the thirteen: rational exponents, `q R r`, units,
coordinates, and sets. Coordinates and sets already decide (52 and 4 answers,
both 100 %), so D-F3 buys 15 answers of the 370 as written. **The two
productions the plan does not name, `label` (147) and `disjunction` (57), carry
more than half of the whole residue.** Take that as the correction f2-grammar
needs.

### 5.2 What the productions do for the 78 `multi-step` topics

The count of `multi-step` topics whose every exemplar decides, as the
productions land in the order of the table:

| step | added | topics fully decided |
|---:|---|---:|
| 0 | today | 19 of 78 |
| 1 | `label` | 25 |
| 2 | `list` | 31 |
| 3 | `equation` | 40 |
| 4 | `disjunction` | 53 |
| 5 | `unit` | 58 |
| 6 | `currency` | 65 |
| 7 | `log_base` | 67 |
| 9 | `multipart` | 72 |
| 10 | `approx_marker` | 74 |
| 11 | `ratio` | 75 |
| 12 | `interval_union` | 76 |

Two of the 78 topics stay undecided after every production: `comparing-integers`
(the answers `<`, `>` and `-4 < -1 < 3`) and `continuous-growth-model` (the
answer `P_0`). Both hold a content defect of section 5, not a grammar gap. Fix
them in the YAML: author `<` as the `label` answer `less than`, author
`-4 < -1 < 3` as `true`, and author `P_0` as `the starting amount`.

## 6. The answer contract this inventory proposes for D-F1

D-F1 puts the contract on the item, not on the topic. `answer_kind` keeps its
meaning as the TASK COMPLEXITY of a topic. The contract below is the shape of
`answer_contract`, one per exemplar and one per template answer. The grade route
grades by contract.

The inventory adds one variant to the D-F1 draft: **`label`**. 147 of the 370
refused answers are one word or one short phrase from a closed vocabulary, which
is more than every other production. `none` cannot carry them, because `none`
means "ungraded" and the 147 answers are exactly decidable against an authored
vocabulary. `exact` cannot carry them either, because the words are not values.

```rust
/// The contract that decides how the grade route reads one answer (D-F1).
pub enum AnswerContract {
    /// The learner answer equals the authored value, under the canonical forms
    /// of `cadus_core::answer::canon`. The default.
    Exact,
    /// The learner answer rounds to the authored value. `decimals` is the count
    /// of decimal digits the author fixed; `tolerance` is an exact rational
    /// bound. Exactly one of the two.
    Approx { decimals: Option<u32>, tolerance: Option<Ratio> },
    /// The learner answer is a value and a unit. The unit table names the
    /// quantity, and a convertible unit of the same quantity is correct with a
    /// `notation` tag.
    Unit { quantity: Quantity, unit: UnitId },
    /// The learner answer is a quotient and a remainder, in either spelling.
    QuotientRemainder,
    /// The learner answer is an ordered tuple of coordinates.
    Coordinates { arity: u8 },
    /// The learner answer is an unordered set. Order and repeats do not count.
    Set,
    /// The learner answer is an ordered or an unordered list of values, each
    /// under its own contract.
    List { ordered: bool, member: Box<AnswerContract> },
    /// The learner answer is one word or one phrase of a closed vocabulary.
    /// `synonyms` holds the accepted spellings of each option, folded to lower
    /// case. NEW in this report; D-F1 as drafted has no variant for it.
    Label { options: Vec<Vec<String>> },
    /// The learner answer holds several parts, one contract per part. The parts
    /// are labeled and the grade route grades each part.
    Multipart { parts: Vec<(String, AnswerContract)> },
    /// The item has no deterministic verdict. The attempt records
    /// `Outcome::Ungraded` (D-F2) and no FIRe update follows.
    None,
}
```

One authored Foundations answer per variant:

| variant | example answer | unit file | topic | knowledge point |
|---|---|---|---|---|
| `Exact` | `x^2 - 8x + 16` | `07-polynomials-quadratics.yaml` | `special-products` | `kp1` |
| `Approx { decimals: 2 }` | `≈ 14.14` | `09-rational-trig.yaml` | `law-of-sines` | `kp1` |
| `Unit { quantity: Volume, unit: L }` | `4.2 L` | `05-systems-inequalities.yaml` | `systems-mixture-problems` | `kp1` |
| `QuotientRemainder` | `9 R2` | `00-arithmetic-core.yaml` | `division-with-remainders` | `kp1` |
| `Coordinates { arity: 2 }` | `(3, 2)` | `04-linear-graphs.yaml` | `coordinate-plane` | `kp2` |
| `Set` | `{2, 4, 6}` | `08-functions-exponentials.yaml` | `domain-range-of-relations` | `kp2` |
| `List { ordered: true }` | `13 and 14` | `03-expressions-equations.yaml` | `consecutive-integer-problems` | `kp2` |
| `Label` | `yes` | `00-arithmetic-core.yaml` | `perfect-squares` | `kp2` |
| `Multipart` | `x = 2; y = 3` | `07-polynomials-quadratics.yaml` | `parabola-vertex-form` | `kp3` |
| `None` | `Law of Cosines` | `09-rational-trig.yaml` | `law-of-sines-cosines` | `kp1` |

Four notes on the enum.

1. `Exact` stays the default, and the loader gives it to every exemplar that
   carries no `answer_contract`. 1,325 of the 1,695 Foundations answers need
   nothing else.
2. `Unit` also covers `°`, `%` and a currency sign, which are 50 answers
   together with the word units. Four radical answers carry a unit as well
   (`5√3 m`, `50√3 m`, `10√13 km`, `20√37 m`), so `Unit` must hold any exact
   value and not a rational alone.
3. `Approx` needs the exact-arithmetic rule of `D6-dec` and no float. The
   authored `≈` marker sets the digit count; ruling `D6-dec` already decides the
   learner side.
4. `None` is for the answer that no contract decides. In Foundations that is a
   description of a graph or a method name. `None` is NOT the bucket for
   `yes`/`no`: those get `Label`.

## 7. What f2, f3 and f4 take from this report

- **f2-grammar.** Build the thirteen productions of section 5 in the order of
  the table. `label` and `disjunction` come first and are not in D-F3 today.
- **f3-contract.** Annotate the Foundations YAML from the dump. 1,325 answers
  take the default `exact`. The rest take the contract of section 3.1 per topic,
  then a per-exemplar override where the topic is mixed.
- **f4-outcome.** The kind gate goes away and the contract gate takes its place.
  19 `multi-step` topics become gradable at that commit with no other change.
- **f6-readiness-core.** No Foundations knowledge point holds 3 distinct
  decidable items today. 146 hold none. The readiness report of f7 must open on
  that number, and f8-content-foundations must author against it.

