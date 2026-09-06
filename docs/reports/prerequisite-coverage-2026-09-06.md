# Prerequisite and diagnostic coverage — 2026-09-06

The audit reads the curriculum alone: it asks whether a prerequisite edge points at a topic a learner can practice, whether the placement can ask about a topic and grade the answer, and what evidence stands behind each topic a course seeds as mastered.

The counts come from `cadus_core::readiness::PrereqCoverage`. The audit grants no readiness: a topic with no diagnostic item stays a topic with no diagnostic item.

## Every course

| course | topics | practicable | diagnostic decidable | undecidable | missing | prerequisite edges | dangling | to unpracticable | assumed mastery | assumed with no full evidence |
|---|---|---|---|---|---|---|---|---|---|---|
| foundations | 285 | 0 | 236 | 49 | 0 | 815 | 0 | 815 | 285 | 285 |
| proofs | 92 | 0 | 3 | 89 | 0 | 268 | 0 | 268 | 0 | 0 |
| geometry | 87 | 0 | 67 | 20 | 0 | 210 | 0 | 210 | 0 | 0 |
| probability-statistics | 82 | 0 | 62 | 20 | 0 | 186 | 0 | 186 | 0 | 0 |
| precalculus | 37 | 0 | 22 | 15 | 0 | 146 | 0 | 146 | 0 | 0 |
| discrete-mathematics | 38 | 0 | 19 | 19 | 0 | 94 | 0 | 94 | 0 | 0 |
| calculus-1 | 79 | 0 | 55 | 24 | 0 | 272 | 0 | 272 | 0 | 0 |
| calculus-2 | 67 | 0 | 34 | 33 | 0 | 213 | 0 | 213 | 0 | 0 |
| linear-algebra | 75 | 0 | 19 | 56 | 0 | 210 | 0 | 210 | 0 | 0 |
| multivariable-calculus | 65 | 0 | 21 | 44 | 0 | 184 | 0 | 184 | 0 | 0 |
| differential-equations | 59 | 0 | 13 | 46 | 0 | 167 | 0 | 167 | 0 | 0 |
| abstract-algebra | 54 | 0 | 13 | 41 | 0 | 192 | 0 | 192 | 0 | 0 |
| category-theory | 70 | 0 | 0 | 70 | 0 | 324 | 0 | 324 | 0 | 0 |

## foundations

285 topics. 0 hold at least one practicable knowledge point. 815 authored prerequisite edges, of which 0 name a topic the tree does not hold and 815 point at a topic with no practice.

### Diagnostic coverage

Decidable 236, undecidable 49, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 49.**

- `divisibility-rules`
- `understanding-ratios`
- `ratio-tables-equivalent-ratios`
- `unit-rates`
- `comparing-integers`
- `equations-special-cases`
- `basic-absolute-value-equations`
- `absolute-value-equations`
- `consecutive-integer-problems`
- `coordinate-plane`
- `interpreting-graphs-qualitatively`
- `graphing-proportional-relationships`
- `horizontal-vertical-slopes`
- `graphing-linear-equations`
- `point-slope-standard-form`
- `slopes-of-parallel-perpendicular-lines`
- `interpreting-linear-models`
- `solutions-of-inequalities`
- `writing-inequalities-from-statements`
- `and-or-inequalities`
- `interval-notation`
- `basic-absolute-value-inequalities`
- `graphing-linear-inequalities`
- `systems-special-cases`
- `systems-of-linear-inequalities`
- … and 24 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 815.**

- `subtraction-facts` → `single-digit-addition` (the target has no practicable knowledge point)
- `division-facts` → `multiplication-tables` (the target has no practicable knowledge point)
- `perfect-squares` → `multiplication-tables` (the target has no practicable knowledge point)
- `comparing-ordering-whole-numbers` → `place-value` (the target has no practicable knowledge point)
- `rounding-whole-numbers` → `comparing-ordering-whole-numbers` (the target has no practicable knowledge point)
- `rounding-whole-numbers` → `place-value` (the target has no practicable knowledge point)
- `addition-with-carrying` → `place-value` (the target has no practicable knowledge point)
- `addition-with-carrying` → `single-digit-addition` (the target has no practicable knowledge point)
- `subtraction-with-borrowing` → `place-value` (the target has no practicable knowledge point)
- `subtraction-with-borrowing` → `subtraction-facts` (the target has no practicable knowledge point)
- `multi-digit-addition-subtraction` → `addition-with-carrying` (the target has no practicable knowledge point)
- `multi-digit-addition-subtraction` → `place-value` (the target has no practicable knowledge point)
- `multi-digit-addition-subtraction` → `subtraction-with-borrowing` (the target has no practicable knowledge point)
- `addition-subtraction-word-problems` → `multi-digit-addition-subtraction` (the target has no practicable knowledge point)
- `multiplying-by-powers-of-ten` → `multiplication-tables` (the target has no practicable knowledge point)
- `multiplying-by-powers-of-ten` → `place-value` (the target has no practicable knowledge point)
- `multiplying-by-one-digit` → `addition-with-carrying` (the target has no practicable knowledge point)
- `multiplying-by-one-digit` → `multiplication-tables` (the target has no practicable knowledge point)
- `multi-digit-multiplication` → `multi-digit-addition-subtraction` (the target has no practicable knowledge point)
- `multi-digit-multiplication` → `multiplying-by-one-digit` (the target has no practicable knowledge point)
- `multi-digit-multiplication` → `multiplying-by-powers-of-ten` (the target has no practicable knowledge point)
- `division-with-remainders` → `division-facts` (the target has no practicable knowledge point)
- `division-with-remainders` → `subtraction-facts` (the target has no practicable knowledge point)
- `long-division-one-digit` → `division-with-remainders` (the target has no practicable knowledge point)
- `long-division-one-digit` → `multiplying-by-one-digit` (the target has no practicable knowledge point)
- … and 790 more.

### Assumed mastery

A course seeds 285 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

| topic | seeded by | diagnostic | confirmable | remediable |
|---|---|---|---|---|
| `single-digit-addition` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, foundations, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | no |
| `subtraction-facts` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `multiplication-tables` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, foundations, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | no |
| `division-facts` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `perfect-squares` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `place-value` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, foundations, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | no |
| `comparing-ordering-whole-numbers` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `rounding-whole-numbers` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `addition-with-carrying` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `subtraction-with-borrowing` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `multi-digit-addition-subtraction` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | no |
| `addition-subtraction-word-problems` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `multiplying-by-powers-of-ten` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `multiplying-by-one-digit` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `multi-digit-multiplication` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | no |
| `division-with-remainders` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `long-division-one-digit` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `long-division` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | no |
| `multiplication-division-word-problems` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `whole-number-exponents` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `expressions-with-parentheses` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `order-of-operations` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | no |
| `factors-and-multiples` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |
| `divisibility-rules` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | undecidable | no | no |
| `prime-composite-numbers` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | no |

… and 260 more seeded topics.

## proofs

92 topics. 0 hold at least one practicable knowledge point. 268 authored prerequisite edges, of which 0 name a topic the tree does not hold and 268 point at a topic with no practice.

### Diagnostic coverage

Decidable 3, undecidable 89, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 89.**

- `propositions-and-truth-values`
- `logical-connectives`
- `propositional-logic`
- `truth-tables-basic-connectives`
- `evaluating-compound-propositions`
- `truth-tables`
- `tautologies-and-contradictions`
- `conditional-statements`
- `necessary-sufficient-conditions`
- `de-morgan-double-negation`
- `logical-equivalence`
- `universal-existential-statements`
- `quantifiers`
- `negating-statements`
- `counterexamples-and-disproof`
- `rules-of-inference`
- `arguments-with-quantifiers`
- `direct-proof`
- `vacuous-trivial-proofs`
- `rational-number-proofs`
- `divisibility-parity-proofs`
- `proof-by-cases`
- `proof-by-exhaustion-wlog`
- `proof-by-contrapositive`
- `biconditional-proofs`
- … and 64 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 268.**

- `propositions-and-truth-values` → `evaluating-expressions` (the target has no practicable knowledge point)
- `logical-connectives` → `propositions-and-truth-values` (the target has no practicable knowledge point)
- `propositional-logic` → `logical-connectives` (the target has no practicable knowledge point)
- `propositional-logic` → `propositions-and-truth-values` (the target has no practicable knowledge point)
- `truth-tables-basic-connectives` → `logical-connectives` (the target has no practicable knowledge point)
- `evaluating-compound-propositions` → `logical-connectives` (the target has no practicable knowledge point)
- `evaluating-compound-propositions` → `truth-tables-basic-connectives` (the target has no practicable knowledge point)
- `truth-tables` → `evaluating-compound-propositions` (the target has no practicable knowledge point)
- `truth-tables` → `propositional-logic` (the target has no practicable knowledge point)
- `truth-tables` → `truth-tables-basic-connectives` (the target has no practicable knowledge point)
- `tautologies-and-contradictions` → `evaluating-compound-propositions` (the target has no practicable knowledge point)
- `tautologies-and-contradictions` → `truth-tables` (the target has no practicable knowledge point)
- `conditional-statements` → `propositional-logic` (the target has no practicable knowledge point)
- `conditional-statements` → `truth-tables` (the target has no practicable knowledge point)
- `necessary-sufficient-conditions` → `conditional-statements` (the target has no practicable knowledge point)
- `de-morgan-double-negation` → `logical-connectives` (the target has no practicable knowledge point)
- `de-morgan-double-negation` → `truth-tables` (the target has no practicable knowledge point)
- `logical-equivalence` → `conditional-statements` (the target has no practicable knowledge point)
- `logical-equivalence` → `de-morgan-double-negation` (the target has no practicable knowledge point)
- `logical-equivalence` → `truth-tables` (the target has no practicable knowledge point)
- `predicates-and-domains` → `evaluating-expressions` (the target has no practicable knowledge point)
- `predicates-and-domains` → `propositional-logic` (the target has no practicable knowledge point)
- `universal-existential-statements` → `predicates-and-domains` (the target has no practicable knowledge point)
- `universal-existential-statements` → `propositional-logic` (the target has no practicable knowledge point)
- `quantifiers` → `conditional-statements` (the target has no practicable knowledge point)
- … and 243 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## geometry

87 topics. 0 hold at least one practicable knowledge point. 210 authored prerequisite edges, of which 0 name a topic the tree does not hold and 210 point at a topic with no practice.

### Diagnostic coverage

Decidable 67, undecidable 20, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 20.**

- `points-lines-planes`
- `classifying-triangles`
- `triangle-inequality-classification`
- `triangle-inequality-applications`
- `rigid-motions-congruence`
- `sss-sas-congruence`
- `asa-aas-congruence`
- `hl-congruence`
- `triangle-congruence-criteria`
- `congruence-proofs-cpctc`
- `dilations-scale-factors`
- `triangle-similarity-criteria`
- `proving-triangles-similar`
- `equations-of-circles`
- `coordinate-geometry-proofs`
- `geometric-reasoning`
- `postulates-theorems-justifications`
- `writing-geometric-proofs`
- `straightedge-compass-constructions`
- `perpendicular-parallel-constructions`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 210.**

- `points-lines-planes` → `coordinate-plane` (the target has no practicable knowledge point)
- `measuring-segments-angles` → `one-step-equations` (the target has no practicable knowledge point)
- `measuring-segments-angles` → `points-lines-planes` (the target has no practicable knowledge point)
- `segment-angle-addition` → `evaluating-expressions` (the target has no practicable knowledge point)
- `segment-angle-addition` → `measuring-segments-angles` (the target has no practicable knowledge point)
- `segment-angle-addition` → `one-step-equations` (the target has no practicable knowledge point)
- `complementary-supplementary-angles` → `measuring-segments-angles` (the target has no practicable knowledge point)
- `complementary-supplementary-angles` → `one-step-equations` (the target has no practicable knowledge point)
- `angle-relationships` → `complementary-supplementary-angles` (the target has no practicable knowledge point)
- `angle-relationships` → `segment-angle-addition` (the target has no practicable knowledge point)
- `angle-relationships` → `two-step-equations` (the target has no practicable knowledge point)
- `angle-bisectors` → `angle-relationships` (the target has no practicable knowledge point)
- `angle-bisectors` → `segment-angle-addition` (the target has no practicable knowledge point)
- `angle-bisectors` → `two-step-equations` (the target has no practicable knowledge point)
- `perpendicular-bisectors` → `angle-relationships` (the target has no practicable knowledge point)
- `perpendicular-bisectors` → `segment-angle-addition` (the target has no practicable knowledge point)
- `perpendicular-bisectors` → `two-step-equations` (the target has no practicable knowledge point)
- `transversal-angle-pairs` → `angle-relationships` (the target has no practicable knowledge point)
- `parallel-lines-transversals` → `multi-step-equations` (the target has no practicable knowledge point)
- `parallel-lines-transversals` → `transversal-angle-pairs` (the target has no practicable knowledge point)
- `proving-lines-parallel` → `parallel-lines-transversals` (the target has no practicable knowledge point)
- `proving-lines-parallel` → `postulates-theorems-justifications` (the target has no practicable knowledge point)
- `proving-lines-parallel` → `transversal-angle-pairs` (the target has no practicable knowledge point)
- `triangle-angle-sum` → `angle-relationships` (the target has no practicable knowledge point)
- `triangle-angle-sum` → `multi-step-equations` (the target has no practicable knowledge point)
- … and 185 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## probability-statistics

82 topics. 0 hold at least one practicable knowledge point. 186 authored prerequisite edges, of which 0 name a topic the tree does not hold and 186 point at a topic with no practice.

### Diagnostic coverage

Decidable 62, undecidable 20, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 20.**

- `reading-statistical-graphs`
- `mean-median-mode`
- `outliers-effect-on-center`
- `choosing-summary-statistics`
- `listing-sample-spaces`
- `simulating-probability`
- `binomial-distribution`
- `populations-samples`
- `sampling-methods-bias`
- `experimental-design-basics`
- `sampling-distributions`
- `scatterplots-correlation`
- `hypothesis-tests-logic`
- `type-i-ii-errors`
- `one-sample-z-test`
- `one-proportion-z-test`
- `one-sample-t-test`
- `two-sample-t-test`
- `chi-square-goodness-of-fit`
- `inference-for-slope`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 186.**

- `summation-notation` → `evaluating-expressions` (the target has no practicable knowledge point)
- `summation-notation` → `order-of-operations` (the target has no practicable knowledge point)
- `frequency-tables-histograms` → `comparing-ordering-whole-numbers` (the target has no practicable knowledge point)
- `frequency-tables-histograms` → `multi-digit-addition-subtraction` (the target has no practicable knowledge point)
- `frequency-tables-histograms` → `percentages` (the target has no practicable knowledge point)
- `dot-plots-stem-leaf` → `comparing-ordering-whole-numbers` (the target has no practicable knowledge point)
- `dot-plots-stem-leaf` → `place-value` (the target has no practicable knowledge point)
- `reading-statistical-graphs` → `dot-plots-stem-leaf` (the target has no practicable knowledge point)
- `reading-statistical-graphs` → `frequency-tables-histograms` (the target has no practicable knowledge point)
- `reading-statistical-graphs` → `percentages` (the target has no practicable knowledge point)
- `mean-of-a-data-set` → `decimal-operations` (the target has no practicable knowledge point)
- `mean-of-a-data-set` → `long-division` (the target has no practicable knowledge point)
- `median-and-mode` → `comparing-ordering-whole-numbers` (the target has no practicable knowledge point)
- `median-and-mode` → `decimal-operations` (the target has no practicable knowledge point)
- `weighted-averages` → `decimal-operations` (the target has no practicable knowledge point)
- `weighted-averages` → `mean-of-a-data-set` (the target has no practicable knowledge point)
- `weighted-averages` → `percentages` (the target has no practicable knowledge point)
- `mean-from-frequency-table` → `frequency-tables-histograms` (the target has no practicable knowledge point)
- `mean-from-frequency-table` → `mean-of-a-data-set` (the target has no practicable knowledge point)
- `mean-from-frequency-table` → `summation-notation` (the target has no practicable knowledge point)
- `mean-median-mode` → `mean-of-a-data-set` (the target has no practicable knowledge point)
- `mean-median-mode` → `median-and-mode` (the target has no practicable knowledge point)
- `mean-median-mode` → `weighted-averages` (the target has no practicable knowledge point)
- `outliers-effect-on-center` → `mean-median-mode` (the target has no practicable knowledge point)
- `percentiles` → `median-and-mode` (the target has no practicable knowledge point)
- … and 161 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## precalculus

37 topics. 0 hold at least one practicable knowledge point. 146 authored prerequisite edges, of which 0 name a topic the tree does not hold and 146 point at a topic with no practice.

### Diagnostic coverage

Decidable 22, undecidable 15, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 15.**

- `even-odd-functions`
- `rational-root-theorem`
- `fundamental-theorem-algebra-multiplicity`
- `polynomial-end-behavior-graphing`
- `polynomial-inequalities`
- `vertical-horizontal-asymptotes`
- `graphing-rational-functions`
- `rational-inequalities`
- `sinusoid-phase-shift`
- `tangent-reciprocal-graphs`
- `parabola-focus-directrix`
- `ellipses`
- `hyperbolas`
- `classifying-conics-completing-square`
- `conjugate-root-theorem`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 146.**

- `function-transformations-shifts` → `coordinate-plane` (the target has no practicable knowledge point)
- `function-transformations-shifts` → `domain-range` (the target has no practicable knowledge point)
- `function-transformations-shifts` → `function-notation` (the target has no practicable knowledge point)
- `function-transformations-shifts` → `quadratic-graphs-vertex` (the target has no practicable knowledge point)
- `function-stretches-reflections` → `function-notation` (the target has no practicable knowledge point)
- `function-stretches-reflections` → `function-transformations-shifts` (the target has no practicable knowledge point)
- `function-stretches-reflections` → `quadratic-graphs-vertex` (the target has no practicable knowledge point)
- `even-odd-functions` → `evaluating-expressions` (the target has no practicable knowledge point)
- `even-odd-functions` → `function-notation` (the target has no practicable knowledge point)
- `even-odd-functions` → `function-stretches-reflections` (the target has no practicable knowledge point)
- `even-odd-functions` → `polynomial-addition-subtraction` (the target has no practicable knowledge point)
- `piecewise-absolute-value-graphs` → `absolute-value` (the target has no practicable knowledge point)
- `piecewise-absolute-value-graphs` → `domain-range` (the target has no practicable knowledge point)
- `piecewise-absolute-value-graphs` → `function-notation` (the target has no practicable knowledge point)
- `piecewise-absolute-value-graphs` → `function-transformations-shifts` (the target has no practicable knowledge point)
- `piecewise-absolute-value-graphs` → `graphing-linear-equations` (the target has no practicable knowledge point)
- `average-rate-of-change` → `evaluating-expressions` (the target has no practicable knowledge point)
- `average-rate-of-change` → `function-notation` (the target has no practicable knowledge point)
- `average-rate-of-change` → `slope` (the target has no practicable knowledge point)
- `remainder-factor-theorems` → `evaluating-expressions` (the target has no practicable knowledge point)
- `remainder-factor-theorems` → `factoring-trinomials` (the target has no practicable knowledge point)
- `remainder-factor-theorems` → `function-notation` (the target has no practicable knowledge point)
- `remainder-factor-theorems` → `polynomial-division` (the target has no practicable knowledge point)
- `rational-root-theorem` → `divisibility-rules` (the target has no practicable knowledge point)
- `rational-root-theorem` → `polynomial-division` (the target has no practicable knowledge point)
- … and 121 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## discrete-mathematics

38 topics. 0 hold at least one practicable knowledge point. 94 authored prerequisite edges, of which 0 name a topic the tree does not hold and 94 point at a topic with no practice.

### Diagnostic coverage

Decidable 19, undecidable 19, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 19.**

- `division-algorithm`
- `euclidean-algorithm`
- `rsa-cryptography`
- `pigeonhole-applications`
- `combinatorial-proofs`
- `iteration-telescoping`
- `linear-homogeneous-recurrences`
- `nonhomogeneous-recurrences`
- `degree-handshake`
- `graph-isomorphism`
- `euler-trails-circuits`
- `hamilton-paths-cycles`
- `trees-characterizations`
- `bipartite-graphs-matchings`
- `normal-forms-dnf-cnf`
- `logic-circuits`
- `big-o-notation`
- `growth-of-functions`
- `master-theorem`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 94.**

- `division-algorithm` → `divisibility-parity-proofs` (the target has no practicable knowledge point)
- `division-algorithm` → `divisibility-rules` (the target has no practicable knowledge point)
- `division-algorithm` → `long-division` (the target has no practicable knowledge point)
- `euclidean-algorithm` → `division-algorithm` (the target has no practicable knowledge point)
- `euclidean-algorithm` → `gcf-lcm` (the target has no practicable knowledge point)
- `modular-arithmetic` → `division-algorithm` (the target has no practicable knowledge point)
- `modular-arithmetic` → `integer-exponents-intro` (the target has no practicable knowledge point)
- `linear-congruences` → `euclidean-algorithm` (the target has no practicable knowledge point)
- `linear-congruences` → `modular-arithmetic` (the target has no practicable knowledge point)
- `chinese-remainder-theorem` → `linear-congruences` (the target has no practicable knowledge point)
- `fermat-euler-theorems` → `linear-congruences` (the target has no practicable knowledge point)
- `fermat-euler-theorems` → `modular-arithmetic` (the target has no practicable knowledge point)
- `fermat-euler-theorems` → `prime-factorization` (the target has no practicable knowledge point)
- `rsa-cryptography` → `fermat-euler-theorems` (the target has no practicable knowledge point)
- `rsa-cryptography` → `linear-congruences` (the target has no practicable knowledge point)
- `inclusion-exclusion` → `divisibility-rules` (the target has no practicable knowledge point)
- `inclusion-exclusion` → `multiplication-principle` (the target has no practicable knowledge point)
- `inclusion-exclusion` → `set-operations` (the target has no practicable knowledge point)
- `stars-and-bars` → `combinations` (the target has no practicable knowledge point)
- `stars-and-bars` → `multiplication-principle` (the target has no practicable knowledge point)
- `permutations-of-multisets` → `combinations` (the target has no practicable knowledge point)
- `permutations-of-multisets` → `permutations` (the target has no practicable knowledge point)
- `derangements` → `inclusion-exclusion` (the target has no practicable knowledge point)
- `derangements` → `permutations` (the target has no practicable knowledge point)
- `pigeonhole-applications` → `multiplication-principle` (the target has no practicable knowledge point)
- … and 69 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## calculus-1

79 topics. 0 hold at least one practicable knowledge point. 272 authored prerequisite edges, of which 0 name a topic the tree does not hold and 272 point at a topic with no practice.

### Diagnostic coverage

Decidable 55, undecidable 24, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 24.**

- `limits-graphical-numerical`
- `one-sided-limits`
- `limits-at-infinity-rational`
- `one-sided-infinite-limits`
- `continuity-at-a-point`
- `intermediate-value-theorem`
- `differentiability`
- `derivatives-exp-log`
- `derivatives-piecewise-functions`
- `derivatives-inverse-trig`
- `logarithmic-differentiation`
- `motion-along-a-line`
- `related-rates-setup`
- `related-rates`
- `newtons-method`
- `critical-points-extrema`
- `rolles-theorem`
- `closed-interval-method`
- `increasing-decreasing-intervals`
- `concavity-inflection-points`
- `second-derivative-test`
- `curve-analysis-derivatives`
- `basic-optimization`
- `marginal-analysis`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 272.**

- `limits-from-tables` → `comparing-ordering-decimals` (the target has no practicable knowledge point)
- `limits-from-tables` → `evaluating-functions` (the target has no practicable knowledge point)
- `limits-from-tables` → `function-notation` (the target has no practicable knowledge point)
- `limits-graphical-numerical` → `coordinate-plane` (the target has no practicable knowledge point)
- `limits-graphical-numerical` → `domain-range` (the target has no practicable knowledge point)
- `limits-graphical-numerical` → `function-notation` (the target has no practicable knowledge point)
- `limits-graphical-numerical` → `limits-from-tables` (the target has no practicable knowledge point)
- `limit-laws` → `evaluating-expressions` (the target has no practicable knowledge point)
- `limit-laws` → `limits-graphical-numerical` (the target has no practicable knowledge point)
- `limit-laws` → `order-of-operations` (the target has no practicable knowledge point)
- `one-sided-limits` → `absolute-value` (the target has no practicable knowledge point)
- `one-sided-limits` → `limits-graphical-numerical` (the target has no practicable knowledge point)
- `one-sided-limits` → `piecewise-absolute-value-graphs` (the target has no practicable knowledge point)
- `limits-piecewise-functions` → `evaluating-functions` (the target has no practicable knowledge point)
- `limits-piecewise-functions` → `one-sided-limits` (the target has no practicable knowledge point)
- `limits-piecewise-functions` → `piecewise-absolute-value-graphs` (the target has no practicable knowledge point)
- `factoring-limits` → `difference-of-squares` (the target has no practicable knowledge point)
- `factoring-limits` → `factoring-gcf` (the target has no practicable knowledge point)
- `factoring-limits` → `factoring-trinomials` (the target has no practicable knowledge point)
- `factoring-limits` → `limit-laws` (the target has no practicable knowledge point)
- `limits-algebraic` → `complex-fractions` (the target has no practicable knowledge point)
- `limits-algebraic` → `factoring-limits` (the target has no practicable knowledge point)
- `limits-algebraic` → `rational-expressions` (the target has no practicable knowledge point)
- `limits-algebraic` → `simplifying-radicals` (the target has no practicable knowledge point)
- `limits-at-infinity-rational` → `limit-laws` (the target has no practicable knowledge point)
- … and 247 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## calculus-2

67 topics. 0 hold at least one practicable knowledge point. 213 authored prerequisite edges, of which 0 name a topic the tree does not hold and 213 point at a topic with no practice.

### Diagnostic coverage

Decidable 34, undecidable 33, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 33.**

- `completing-the-square-integrands`
- `partial-fractions-integration`
- `weierstrass-substitution`
- `improper-integrals`
- `choosing-integration-technique`
- `consumer-producer-surplus`
- `work-springs`
- `average-value-work`
- `fluid-force`
- `series-partial-sums`
- `nth-term-divergence-test`
- `integral-p-series-tests`
- `direct-comparison-test`
- `comparison-tests`
- `ratio-test`
- `ratio-root-tests`
- `alternating-series-test`
- `alternating-series`
- `alternating-series-estimation`
- `strategy-for-testing-series`
- `radius-of-convergence`
- `geometric-power-series`
- `differentiating-integrating-power-series`
- `standard-maclaurin-series`
- `binomial-series`
- … and 8 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 213.**

- `integration-by-parts-single` → `antiderivatives` (the target has no practicable knowledge point)
- `integration-by-parts-single` → `derivatives-exp-log` (the target has no practicable knowledge point)
- `integration-by-parts-single` → `integration-substitution` (the target has no practicable knowledge point)
- `integration-by-parts-single` → `product-rule` (the target has no practicable knowledge point)
- `integration-by-parts` → `definite-integrals-ftc` (the target has no practicable knowledge point)
- `integration-by-parts` → `derivatives-exp-log` (the target has no practicable knowledge point)
- `integration-by-parts` → `integration-by-parts-single` (the target has no practicable knowledge point)
- `integration-by-parts` → `integration-substitution` (the target has no practicable knowledge point)
- `tabular-integration-by-parts` → `derivatives-of-polynomials` (the target has no practicable knowledge point)
- `tabular-integration-by-parts` → `integration-by-parts` (the target has no practicable knowledge point)
- `trig-integrals-odd-powers` → `derivatives-trig` (the target has no practicable knowledge point)
- `trig-integrals-odd-powers` → `integration-substitution` (the target has no practicable knowledge point)
- `trig-integrals-odd-powers` → `trig-identities-basic` (the target has no practicable knowledge point)
- `tangent-secant-integrals` → `derivatives-trig` (the target has no practicable knowledge point)
- `tangent-secant-integrals` → `integration-substitution` (the target has no practicable knowledge point)
- `tangent-secant-integrals` → `reciprocal-trig-functions` (the target has no practicable knowledge point)
- `tangent-secant-integrals` → `trig-integrals-odd-powers` (the target has no practicable knowledge point)
- `trigonometric-integrals` → `double-half-angle-identities` (the target has no practicable knowledge point)
- `trigonometric-integrals` → `integration-substitution` (the target has no practicable knowledge point)
- `trigonometric-integrals` → `product-to-sum-identities` (the target has no practicable knowledge point)
- `trigonometric-integrals` → `tangent-secant-integrals` (the target has no practicable knowledge point)
- `trigonometric-integrals` → `trig-identities-basic` (the target has no practicable knowledge point)
- `trigonometric-integrals` → `trig-integrals-odd-powers` (the target has no practicable knowledge point)
- `completing-the-square-integrands` → `completing-the-square` (the target has no practicable knowledge point)
- `completing-the-square-integrands` → `derivatives-inverse-trig` (the target has no practicable knowledge point)
- … and 188 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## linear-algebra

75 topics. 0 hold at least one practicable knowledge point. 210 authored prerequisite edges, of which 0 name a topic the tree does not hold and 210 point at a topic with no practice.

### Diagnostic coverage

Decidable 19, undecidable 56, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 56.**

- `dot-product`
- `augmented-matrix-representation`
- `elementary-row-operations`
- `back-substitution-triangular-systems`
- `echelon-form-recognition`
- `row-reduction-echelon-forms`
- `consistency-of-linear-systems`
- `solution-sets-free-variables`
- `matrix-vector-equations`
- `homogeneous-systems`
- `linear-systems-applications`
- `matrix-addition-scalar-multiplication`
- `matrix-multiplication`
- `identity-zero-matrices`
- `diagonal-triangular-matrices`
- `matrix-operations`
- `matrix-powers`
- `transpose-of-a-matrix`
- `transpose-symmetric-matrices`
- `matrix-inverse-2x2-formula`
- `matrix-inverses`
- `matrix-equations`
- `elementary-matrices-invertibility`
- `determinant-properties-cramers-rule`
- `characteristic-polynomial`
- … and 31 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 210.**

- `component-form-of-vectors` → `coordinate-plane` (the target has no practicable knowledge point)
- `component-form-of-vectors` → `integer-addition-subtraction` (the target has no practicable knowledge point)
- `component-form-of-vectors` → `plotting-points` (the target has no practicable knowledge point)
- `vector-arithmetic` → `component-form-of-vectors` (the target has no practicable knowledge point)
- `vector-arithmetic` → `integer-addition-subtraction` (the target has no practicable knowledge point)
- `vector-arithmetic` → `integer-multiplication-division` (the target has no practicable knowledge point)
- `linear-combinations-of-vectors` → `order-of-operations` (the target has no practicable knowledge point)
- `linear-combinations-of-vectors` → `systems-elimination` (the target has no practicable knowledge point)
- `linear-combinations-of-vectors` → `vector-arithmetic` (the target has no practicable knowledge point)
- `vector-norms-unit-vectors` → `pythagorean-theorem` (the target has no practicable knowledge point)
- `vector-norms-unit-vectors` → `square-roots` (the target has no practicable knowledge point)
- `vector-norms-unit-vectors` → `vector-arithmetic` (the target has no practicable knowledge point)
- `vector-distance` → `component-form-of-vectors` (the target has no practicable knowledge point)
- `vector-distance` → `distance-midpoint-formulas` (the target has no practicable knowledge point)
- `vector-distance` → `vector-norms-unit-vectors` (the target has no practicable knowledge point)
- `vectors-in-rn` → `linear-combinations-of-vectors` (the target has no practicable knowledge point)
- `vectors-in-rn` → `vector-arithmetic` (the target has no practicable knowledge point)
- `vectors-in-rn` → `vector-distance` (the target has no practicable knowledge point)
- `vectors-in-rn` → `vector-norms-unit-vectors` (the target has no practicable knowledge point)
- `computing-dot-products` → `integer-multiplication-division` (the target has no practicable knowledge point)
- `computing-dot-products` → `vector-arithmetic` (the target has no practicable knowledge point)
- `dot-product` → `computing-dot-products` (the target has no practicable knowledge point)
- `dot-product` → `right-triangle-trig` (the target has no practicable knowledge point)
- `dot-product` → `vectors-in-rn` (the target has no practicable knowledge point)
- `dot-product-properties` → `computing-dot-products` (the target has no practicable knowledge point)
- … and 185 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## multivariable-calculus

65 topics. 0 hold at least one practicable knowledge point. 184 authored prerequisite edges, of which 0 name a topic the tree does not hold and 184 point at a topic with no practice.

### Diagnostic coverage

Decidable 21, undecidable 44, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 44.**

- `spheres-in-space`
- `cylinders-and-traces`
- `coordinates-surfaces-3d`
- `lines-in-space`
- `planes-in-space`
- `lines-planes-space`
- `vector-functions-basics`
- `vector-valued-functions`
- `motion-in-space`
- `unit-tangent-normal-vectors`
- `curvature`
- `evaluating-multivariable-functions`
- `functions-several-variables`
- `level-surfaces`
- `limits-along-paths`
- `multivariable-limits-continuity`
- `multivariable-limits-polar`
- `estimating-partial-derivatives`
- `differentials-error-estimation`
- `chain-rule-one-parameter`
- `multivariable-chain-rule`
- `computing-the-gradient`
- `gradient-directional-derivatives`
- `gradients-and-level-curves`
- `tangent-planes-level-surfaces`
- … and 19 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 184.**

- `three-dimensional-coordinates` → `coordinate-plane` (the target has no practicable knowledge point)
- `three-dimensional-coordinates` → `distance-midpoint-formulas` (the target has no practicable knowledge point)
- `three-dimensional-coordinates` → `pythagorean-theorem` (the target has no practicable knowledge point)
- `spheres-in-space` → `completing-the-square` (the target has no practicable knowledge point)
- `spheres-in-space` → `equations-of-circles` (the target has no practicable knowledge point)
- `spheres-in-space` → `three-dimensional-coordinates` (the target has no practicable knowledge point)
- `cylinders-and-traces` → `ellipses` (the target has no practicable knowledge point)
- `cylinders-and-traces` → `quadratic-graphs-vertex` (the target has no practicable knowledge point)
- `cylinders-and-traces` → `three-dimensional-coordinates` (the target has no practicable knowledge point)
- `coordinates-surfaces-3d` → `cylinders-and-traces` (the target has no practicable knowledge point)
- `coordinates-surfaces-3d` → `hyperbolas` (the target has no practicable knowledge point)
- `coordinates-surfaces-3d` → `spheres-in-space` (the target has no practicable knowledge point)
- `lines-in-space` → `parametric-equations` (the target has no practicable knowledge point)
- `lines-in-space` → `three-dimensional-coordinates` (the target has no practicable knowledge point)
- `lines-in-space` → `vectors-in-rn` (the target has no practicable knowledge point)
- `planes-in-space` → `cross-product` (the target has no practicable knowledge point)
- `planes-in-space` → `dot-product` (the target has no practicable knowledge point)
- `planes-in-space` → `lines-in-space` (the target has no practicable knowledge point)
- `lines-planes-space` → `lines-in-space` (the target has no practicable knowledge point)
- `lines-planes-space` → `planes-in-space` (the target has no practicable knowledge point)
- `lines-planes-space` → `vector-projections` (the target has no practicable knowledge point)
- `vector-functions-basics` → `domain-range` (the target has no practicable knowledge point)
- `vector-functions-basics` → `parametric-equations` (the target has no practicable knowledge point)
- `vector-functions-basics` → `vectors-in-rn` (the target has no practicable knowledge point)
- `vector-valued-functions` → `definite-integrals-ftc` (the target has no practicable knowledge point)
- … and 159 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## differential-equations

59 topics. 0 hold at least one practicable knowledge point. 167 authored prerequisite edges, of which 0 name a topic the tree does not hold and 167 point at a topic with no practice.

### Diagnostic coverage

Decidable 13, undecidable 46, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 46.**

- `ode-order-linearity`
- `ode-classification-verification`
- `slope-fields`
- `slope-fields-equilibria`
- `separation-of-variables`
- `separable-equations`
- `integrating-factor-computation`
- `bernoulli-equations`
- `exact-equations`
- `integrating-factors-for-exactness`
- `natural-growth-decay-models`
- `newtons-law-of-cooling`
- `mixing-tank-models`
- `logistic-population-models`
- `first-order-applications`
- `orthogonal-trajectories`
- `linear-independence-wronskian`
- `reduction-of-order`
- `undetermined-coefficients-basic`
- `undetermined-coefficients`
- `variation-of-parameters`
- `free-undamped-oscillations`
- `spring-mass-oscillations`
- `rlc-circuit-equations`
- `boundary-value-problems-intro`
- … and 21 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 167.**

- `ode-order-linearity` → `derivatives-trig` (the target has no practicable knowledge point)
- `ode-order-linearity` → `higher-order-derivatives` (the target has no practicable knowledge point)
- `ode-classification-verification` → `chain-rule` (the target has no practicable knowledge point)
- `ode-classification-verification` → `derivatives-exp-log` (the target has no practicable knowledge point)
- `ode-classification-verification` → `implicit-differentiation` (the target has no practicable knowledge point)
- `ode-classification-verification` → `ode-order-linearity` (the target has no practicable knowledge point)
- `existence-uniqueness-first-order` → `continuity` (the target has no practicable knowledge point)
- `existence-uniqueness-first-order` → `ode-classification-verification` (the target has no practicable knowledge point)
- `existence-uniqueness-first-order` → `partial-derivatives` (the target has no practicable knowledge point)
- `slope-fields` → `ode-classification-verification` (the target has no practicable knowledge point)
- `slope-fields` → `slope` (the target has no practicable knowledge point)
- `slope-fields-equilibria` → `factoring-trinomials` (the target has no practicable knowledge point)
- `slope-fields-equilibria` → `slope-fields` (the target has no practicable knowledge point)
- `eulers-method` → `evaluating-expressions` (the target has no practicable knowledge point)
- `eulers-method` → `slope-fields-equilibria` (the target has no practicable knowledge point)
- `separation-of-variables` → `antiderivatives` (the target has no practicable knowledge point)
- `separation-of-variables` → `integration-substitution` (the target has no practicable knowledge point)
- `separation-of-variables` → `ode-classification-verification` (the target has no practicable knowledge point)
- `separable-equations` → `definite-integrals-ftc` (the target has no practicable knowledge point)
- `separable-equations` → `integration-substitution` (the target has no practicable knowledge point)
- `separable-equations` → `separation-of-variables` (the target has no practicable knowledge point)
- `integrating-factor-computation` → `derivatives-exp-log` (the target has no practicable knowledge point)
- `integrating-factor-computation` → `logarithm-properties` (the target has no practicable knowledge point)
- `integrating-factor-computation` → `separation-of-variables` (the target has no practicable knowledge point)
- `linear-first-order-integrating-factor` → `integrating-factor-computation` (the target has no practicable knowledge point)
- … and 142 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## abstract-algebra

54 topics. 0 hold at least one practicable knowledge point. 192 authored prerequisite edges, of which 0 name a topic the tree does not hold and 192 point at a topic with no practice.

### Diagnostic coverage

Decidable 13, undecidable 41, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 41.**

- `elementary-group-properties`
- `subgroups-subgroup-tests`
- `cyclic-groups-generators`
- `permutations-cycle-notation`
- `even-odd-permutations`
- `general-linear-group`
- `cosets`
- `lagranges-theorem`
- `consequences-of-lagrange`
- `normal-subgroups`
- `internal-direct-products`
- `commutator-subgroup`
- `group-isomorphisms`
- `isomorphism-invariants`
- `kernels-images`
- `first-isomorphism-theorem`
- `isomorphism-theorems-2-3`
- `classifying-small-groups`
- `free-groups-presentations`
- `group-actions`
- `orbits-stabilizers`
- `orbit-stabilizer-theorem`
- `cayleys-theorem`
- `ring-axioms-examples`
- `integral-domains-zero-divisors`
- … and 16 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 192.**

- `group-examples-cayley-tables` → `abelian-groups` (the target has no practicable knowledge point)
- `group-examples-cayley-tables` → `binary-operations` (the target has no practicable knowledge point)
- `group-examples-cayley-tables` → `gcf-lcm` (the target has no practicable knowledge point)
- `group-examples-cayley-tables` → `groups-intro` (the target has no practicable knowledge point)
- `group-examples-cayley-tables` → `modular-arithmetic` (the target has no practicable knowledge point)
- `elementary-group-properties` → `abelian-groups` (the target has no practicable knowledge point)
- `elementary-group-properties` → `direct-proof` (the target has no practicable knowledge point)
- `elementary-group-properties` → `group-examples-cayley-tables` (the target has no practicable knowledge point)
- `elementary-group-properties` → `groups-intro` (the target has no practicable knowledge point)
- `order-of-an-element` → `gcf-lcm` (the target has no practicable knowledge point)
- `order-of-an-element` → `group-examples-cayley-tables` (the target has no practicable knowledge point)
- `order-of-an-element` → `modular-arithmetic` (the target has no practicable knowledge point)
- `subgroups-subgroup-tests` → `elementary-group-properties` (the target has no practicable knowledge point)
- `subgroups-subgroup-tests` → `order-of-an-element` (the target has no practicable knowledge point)
- `subgroups-subgroup-tests` → `set-operations` (the target has no practicable knowledge point)
- `cyclic-groups-generators` → `abelian-groups` (the target has no practicable knowledge point)
- `cyclic-groups-generators` → `fermat-euler-theorems` (the target has no practicable knowledge point)
- `cyclic-groups-generators` → `order-of-an-element` (the target has no practicable knowledge point)
- `cyclic-groups-generators` → `subgroups-subgroup-tests` (the target has no practicable knowledge point)
- `subgroups-of-cyclic-groups` → `cyclic-groups-generators` (the target has no practicable knowledge point)
- `subgroups-of-cyclic-groups` → `euclidean-algorithm` (the target has no practicable knowledge point)
- `dihedral-groups` → `group-examples-cayley-tables` (the target has no practicable knowledge point)
- `dihedral-groups` → `order-of-an-element` (the target has no practicable knowledge point)
- `permutations-cycle-notation` → `function-composition-inverse` (the target has no practicable knowledge point)
- `permutations-cycle-notation` → `functions-injective-surjective` (the target has no practicable knowledge point)
- … and 167 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## category-theory

70 topics. 0 hold at least one practicable knowledge point. 324 authored prerequisite edges, of which 0 name a topic the tree does not hold and 324 point at a topic with no practice.

### Diagnostic coverage

Decidable 0, undecidable 70, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 70.**

- `composition-associativity-identity`
- `category-definition`
- `poset-monoid-categories`
- `discrete-indiscrete-categories`
- `examples-of-categories`
- `isomorphism-vs-equality`
- `commutative-diagrams`
- `diagram-chasing`
- `mono-epi-iso`
- `groupoids`
- `sections-retractions`
- `epi-mono-factorizations`
- `initial-terminal-objects`
- `duality-opposite-categories`
- `slice-categories`
- `functor-definition-laws`
- `functors`
- `functor-composition-identity`
- `functor-examples`
- `covariant-contravariant-functors`
- `full-faithful-functors`
- `subcategories`
- `product-categories`
- `hom-functors`
- `comma-categories`
- … and 45 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 324.**

- `composition-associativity-identity` → `binary-operations` (the target has no practicable knowledge point)
- `composition-associativity-identity` → `computing-compositions` (the target has no practicable knowledge point)
- `composition-associativity-identity` → `function-composition` (the target has no practicable knowledge point)
- `composition-associativity-identity` → `function-composition-inverse` (the target has no practicable knowledge point)
- `category-definition` → `binary-operations` (the target has no practicable knowledge point)
- `category-definition` → `composition-associativity-identity` (the target has no practicable knowledge point)
- `category-definition` → `divisibility-rules` (the target has no practicable knowledge point)
- `category-definition` → `function-composition` (the target has no practicable knowledge point)
- `category-definition` → `function-composition-inverse` (the target has no practicable knowledge point)
- `category-definition` → `monoids` (the target has no practicable knowledge point)
- `category-definition` → `partial-orders` (the target has no practicable knowledge point)
- `category-definition` → `relations-equivalence` (the target has no practicable knowledge point)
- `category-definition` → `set-notation` (the target has no practicable knowledge point)
- `poset-monoid-categories` → `category-definition` (the target has no practicable knowledge point)
- `poset-monoid-categories` → `groups-intro` (the target has no practicable knowledge point)
- `poset-monoid-categories` → `hasse-diagrams` (the target has no practicable knowledge point)
- `poset-monoid-categories` → `monoids` (the target has no practicable knowledge point)
- `poset-monoid-categories` → `partial-orders` (the target has no practicable knowledge point)
- `discrete-indiscrete-categories` → `category-definition` (the target has no practicable knowledge point)
- `discrete-indiscrete-categories` → `commutative-diagrams` (the target has no practicable knowledge point)
- `discrete-indiscrete-categories` → `indexed-families` (the target has no practicable knowledge point)
- `discrete-indiscrete-categories` → `poset-monoid-categories` (the target has no practicable knowledge point)
- `discrete-indiscrete-categories` → `set-notation` (the target has no practicable knowledge point)
- `examples-of-categories` → `category-definition` (the target has no practicable knowledge point)
- `examples-of-categories` → `groups-intro` (the target has no practicable knowledge point)
- … and 299 more.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

