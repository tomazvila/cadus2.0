# Prerequisite and diagnostic coverage — 2026-09-06

The audit reads the curriculum alone: it asks whether a prerequisite edge points at a topic a learner can practice, whether the placement can ask about a topic and grade the answer, and what evidence stands behind each topic a course seeds as mastered.

The counts come from `cadus_core::readiness::PrereqCoverage`. The audit grants no readiness: a topic with no diagnostic item stays a topic with no diagnostic item.

## Every course

| course | topics | practicable | diagnostic decidable | undecidable | missing | prerequisite edges | dangling | to unpracticable | assumed mastery | assumed with no full evidence |
|---|---|---|---|---|---|---|---|---|---|---|
| foundations | 259 | 259 | 220 | 39 | 0 | 771 | 0 | 0 | 259 | 39 |
| proofs | 92 | 92 | 3 | 89 | 0 | 268 | 0 | 0 | 0 | 0 |
| geometry | 87 | 87 | 71 | 16 | 0 | 210 | 0 | 0 | 0 | 0 |
| probability-statistics | 82 | 82 | 64 | 18 | 0 | 186 | 0 | 0 | 0 | 0 |
| precalculus | 37 | 37 | 27 | 10 | 0 | 146 | 0 | 0 | 0 | 0 |
| discrete-mathematics | 38 | 38 | 27 | 11 | 0 | 94 | 0 | 0 | 0 | 0 |
| calculus-1 | 79 | 79 | 57 | 22 | 0 | 271 | 0 | 0 | 0 | 0 |
| calculus-2 | 67 | 67 | 34 | 33 | 0 | 213 | 0 | 0 | 0 | 0 |
| linear-algebra | 75 | 75 | 24 | 51 | 0 | 210 | 0 | 0 | 0 | 0 |
| multivariable-calculus | 65 | 65 | 21 | 44 | 0 | 184 | 0 | 0 | 0 | 0 |
| differential-equations | 59 | 59 | 13 | 46 | 0 | 167 | 0 | 0 | 0 | 0 |
| abstract-algebra | 54 | 54 | 13 | 41 | 0 | 192 | 0 | 0 | 0 | 0 |
| category-theory | 70 | 70 | 0 | 70 | 0 | 324 | 0 | 0 | 0 | 0 |

## foundations

259 topics. 259 hold at least one practicable knowledge point. 771 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

### Diagnostic coverage

Decidable 220, undecidable 39, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 39.**

- `fraction-word-problems`
- `percentages`
- `rearranging-formulas`
- `literal-equations`
- `basic-absolute-value-equations`
- `absolute-value-equations`
- `translating-sentences-to-equations`
- `money-geometry-problems`
- `equation-word-problems`
- `graphing-from-a-table`
- `slope-intercept-form`
- `point-slope-form`
- `parallel-perpendicular-lines`
- `compound-inequalities`
- `inequality-word-problems`
- `graphing-systems`
- `substitution-with-isolated-variable`
- `systems-substitution`
- `systems-elimination`
- `pythagorean-theorem`
- `radical-equations-basic`
- `quadratic-equations-factoring`
- `square-root-property`
- `completing-the-square`
- `quadratic-formula`
- … and 14 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 259 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

| topic | seeded by | diagnostic | confirmable | remediable |
|---|---|---|---|---|
| `single-digit-addition` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, foundations, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | yes |
| `subtraction-facts` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `multiplication-tables` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, foundations, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | yes |
| `division-facts` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `place-value` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, foundations, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | yes |
| `comparing-ordering-whole-numbers` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `rounding-whole-numbers` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `multi-digit-addition-subtraction` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | yes |
| `addition-subtraction-word-problems` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `multiplying-by-powers-of-ten` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `multi-digit-multiplication` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | yes |
| `long-division-one-digit` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `multiplication-division-word-problems` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `whole-number-exponents` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `expressions-with-parentheses` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `order-of-operations` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics, proofs | decidable | yes | yes |
| `factors-and-multiples` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `divisibility-rules` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `prime-composite-numbers` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `prime-factorization` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `greatest-common-factor` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `gcf-lcm` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `rounding-estimation` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `fraction-basics` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |
| `fractions-on-number-line` | abstract-algebra, calculus-1, calculus-2, category-theory, differential-equations, discrete-mathematics, geometry, linear-algebra, multivariable-calculus, precalculus, probability-statistics | decidable | yes | yes |

… and 234 more seeded topics.

## proofs

92 topics. 92 hold at least one practicable knowledge point. 268 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## geometry

87 topics. 87 hold at least one practicable knowledge point. 210 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

### Diagnostic coverage

Decidable 71, undecidable 16, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 16.**

- `angle-relationships`
- `perpendicular-bisectors`
- `transversal-angle-pairs`
- `triangle-inequality-classification`
- `triangle-inequality-applications`
- `rigid-motions-congruence`
- `sss-sas-congruence`
- `asa-aas-congruence`
- `hl-congruence`
- `triangle-congruence-criteria`
- `dilations-scale-factors`
- `equations-of-circles`
- `coordinate-geometry-proofs`
- `geometric-reasoning`
- `postulates-theorems-justifications`
- `straightedge-compass-constructions`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## probability-statistics

82 topics. 82 hold at least one practicable knowledge point. 186 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

### Diagnostic coverage

Decidable 64, undecidable 18, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 18.**

- `mean-median-mode`
- `outliers-effect-on-center`
- `choosing-summary-statistics`
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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## precalculus

37 topics. 37 hold at least one practicable knowledge point. 146 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

### Diagnostic coverage

Decidable 27, undecidable 10, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 10.**

- `even-odd-functions`
- `fundamental-theorem-algebra-multiplicity`
- `polynomial-end-behavior-graphing`
- `vertical-horizontal-asymptotes`
- `graphing-rational-functions`
- `sinusoid-phase-shift`
- `tangent-reciprocal-graphs`
- `parabola-focus-directrix`
- `hyperbolas`
- `conjugate-root-theorem`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## discrete-mathematics

38 topics. 38 hold at least one practicable knowledge point. 94 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

### Diagnostic coverage

Decidable 27, undecidable 11, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 11.**

- `pigeonhole-applications`
- `combinatorial-proofs`
- `linear-homogeneous-recurrences`
- `nonhomogeneous-recurrences`
- `graph-isomorphism`
- `euler-trails-circuits`
- `trees-characterizations`
- `normal-forms-dnf-cnf`
- `logic-circuits`
- `growth-of-functions`
- `master-theorem`

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## calculus-1

79 topics. 79 hold at least one practicable knowledge point. 271 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

### Diagnostic coverage

Decidable 57, undecidable 22, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 22.**

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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## calculus-2

67 topics. 67 hold at least one practicable knowledge point. 213 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## linear-algebra

75 topics. 75 hold at least one practicable knowledge point. 210 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

### Diagnostic coverage

Decidable 24, undecidable 51, missing 0. A topic with no decidable item never enters the probe set, so the placement infers its state and never measures it.

**Topics with no diagnostic item: 0.**

None.

**Topics whose diagnostic answer the grammar refuses: 51.**

- `dot-product`
- `elementary-row-operations`
- `echelon-form-recognition`
- `consistency-of-linear-systems`
- `solution-sets-free-variables`
- `matrix-vector-equations`
- `homogeneous-systems`
- `linear-systems-applications`
- `matrix-addition-scalar-multiplication`
- `matrix-multiplication`
- `identity-zero-matrices`
- `diagonal-triangular-matrices`
- `matrix-powers`
- `transpose-of-a-matrix`
- `transpose-symmetric-matrices`
- `matrix-inverse-2x2-formula`
- `matrix-inverses`
- `matrix-equations`
- `elementary-matrices-invertibility`
- `determinant-properties-cramers-rule`
- `finding-eigenvectors`
- `eigenvalues-eigenvectors`
- `eigenspaces-multiplicities`
- `complex-eigenvalues-2x2`
- `similar-matrices`
- … and 26 more.

### Prerequisite edges that lead nowhere

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## multivariable-calculus

65 topics. 65 hold at least one practicable knowledge point. 184 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## differential-equations

59 topics. 59 hold at least one practicable knowledge point. 167 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## abstract-algebra

54 topics. 54 hold at least one practicable knowledge point. 192 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

## category-theory

70 topics. 70 hold at least one practicable knowledge point. 324 authored prerequisite edges, of which 0 name a topic the tree does not hold and 0 point at a topic with no practice.

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

**Edges that lead nowhere: 0.**

None.

### Assumed mastery

A course seeds 0 topics of this course as mastered, through `mastery_floor` or `mastery_floor_course`. A seeded topic needs two pieces of evidence: a decidable diagnostic item to CONFIRM the assumption, and a practicable knowledge point to REMEDIATE a failed confirmation.

No course seeds a topic of this course as mastered.

