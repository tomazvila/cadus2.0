Source: the 2.0 answer grammar of M2, measured on 2026-08-26 against
`crates/core/tests/fixtures/answers/corpus_1_0.jsonl` (3,492 answers, 478 topics).

# The undecidable answers — the V2 residue for re-kinding

This document lists every curriculum answer that the 2.0 answer grammar refuses. It is
the V2 input for the owner and for M6 authoring. Each group names one cause, one topic
set, and one recommended action.

Regenerate the data with:

```sh
CADUS_RESIDUE_DUMP=/tmp/residue.jsonl \
    cargo test -p cadus-core --test answer_oracle dump_the_undecidable -- --nocapture
```

The dump writes one JSON line per refused answer: `answer`, `answer_kind`, `shape`,
`topic_id`, `kp_id`, `exemplar_index`, `source` (the normalized parser input), and
`reason` (the refusal of the grammar). The identity of the 278 answers is also a
committed fixture, `crates/core/tests/fixtures/answers/undecidable_1_0.jsonl`, and
`crates/core/tests/answer_parse.rs` fails when the set moves.

---

## 1. The counts

| Measure | Value |
|---|---:|
| corpus answers | 3,492 |
| answers inside the grammar | 3,214 (92.04%) |
| answers the grammar refuses | **278 (7.96%)** |
| topics that hold a refused answer | 124 of 478 |
| refused answers on `expression` topics | 208 |
| refused answers on `numeric` topics | 70 |

`docs/reference/checker-1.0-spec.md` section 8.3 estimated 230 refusals. The measured
number is 278. The estimate counted whole shape buckets, and the grammar decides one
answer at a time. The difference runs both ways: the `interval_ineq` bucket gave 29 of
its 34 rows to the grammar, and the `expression_symbolic` bucket lost 65 rows the
estimate had counted as covered. `crates/core/tests/answer_parse.rs` records the five
buckets that split.

### The refusal reason the grammar gives

| Reason | n |
|---|---:|
| `a name that is not a function or variable` | 212 |
| `an exponent that is not a whole number` | 28 |
| `a character outside the grammar` | 21 |
| `a number glued to a name reads as a label` | 11 |
| `an inequality with no bare variable` | 4 |
| `a fraction with a zero denominator` | 2 |

Every refusal is an `Undecidable` value. The checker never guesses, and it never panics.

---

## 2. The groups

The 17 groups below are disjoint, and they sum to 278.

| # | Group | n | topics | Recommended action |
|---|---|---:|---:|---|
| 1 | prose and single words | 169 | 78 | Add a `choice` answer kind, or re-kind to `multi-step` |
| 2 | a fractional or symbolic exponent | 28 | 12 | Extend the grammar: a rational exponent and a symbolic exponent |
| 3 | quotient and remainder | 16 | 5 | Author the answer as a tuple, or add a `q R r` production |
| 4 | a prose list with commas | 15 | 9 | Re-kind to `multi-step`; author the two vectors as tuples |
| 5 | a multi-letter variable run | 11 | 9 | Extend the lexer: split an unknown letter run into variables |
| 6 | a subscripted logarithm base | 10 | 3 | Extend the lexer: read `log_b(x)` as the two-argument `log` |
| 7 | a general inequality | 5 | 3 | Extend the grammar: an inequality between two expressions |
| 8 | infinity | 4 | 1 | Add an infinity value, or add a `choice` answer kind |
| 9 | a label set | 4 | 2 | Add a `choice` answer kind over opaque label tokens |
| 10 | an approximation marker | 4 | 2 | Author the exact value; state the rounding in the prompt |
| 11 | a value with a unit | 3 | 3 | Extend the grammar: a value and an opaque unit token |
| 12 | the `arctan` spelling | 2 | 1 | Add `arcsin`, `arccos`, `arctan` as spellings of the whitelist |
| 13 | an undefined quotient | 2 | 1 | Add a `choice` answer kind with the option `undefined` |
| 14 | derivative notation `dy/dx` | 2 | 1 | Re-kind to `multi-step` |
| 15 | a subscripted variable | 1 | 1 | Extend the lexer: read `a_1` as one variable name |
| 16 | a factorial | 1 | 1 | Add a factorial production, or re-kind |
| 17 | an unknown function | 1 | 1 | Re-kind to `multi-step` |

### What each action buys

- **A `choice` answer kind** (groups 1, 8, 9, 13) covers **179 answers on 82 topics**.
  Authoring gives a closed option set per exemplar, and the checker decides by set
  membership after casefolding. The check is exact, it runs no arithmetic, and it closes
  the largest 1.0 correctness gap: 1.0 reads `yes` as the product `e*s*y`, so every
  anagram of a word answer passes (`docs/reference/checker-1.0-spec.md` section 7.6).
- **Seven grammar extensions** (groups 2, 5, 6, 7, 11, 12, 15) recover **58 answers on 30
  topics**. Every one of them stays decidable: no search, no float, no simplification.
- **Re-kinding to `multi-step`** covers the remaining **41 answers on about 20 topics**.
  A `multi-step` topic gets the model grader, which is what the 1.0 design intended for an
  answer that is not a value.

The grammar extensions lift grammar coverage from 92.04% to 93.7%. The `choice` kind adds
a further 5.1%. The 41 re-kinded answers then claim no deterministic verdict, which is
the correct outcome for them.

---

## 3. The groups in detail

### 3.1 Prose and single words — 169 answers, 78 topics

`answer_kind` is `expression` on 122 of them and `numeric` on 47. There are 70 distinct
answers, compared without case. The most frequent:

| Answer | n |
|---|---:|
| `yes` | 38 |
| `no` | 25 |
| `diverges` | 9 |
| `DNE` | 7 |
| `no solution` | 4 |
| `all real numbers` | 4 |
| `negative` | 4 |
| `decreasing` | 3 |
| `increasing` | 3 |
| `undefined` | 3 |
| `infinitely many` | 3 |

The tail holds one or two of each: `equal`, `open`, `closed`, `left`, `right`, `down`,
`prime`, `composite`, `parallel`, `perpendicular`, `obtuse`, `square`, `rhombus`,
`rectangle`, `trinomial`, `binomial`, `bisect`, `washers`, `growth`, `decay`, `jump`,
`removable`, `infinite`, `smaller`, `more`, `none`, `neither`, `even`, `true`, `false`,
`ASA`, `50th`, `65th percentile`, `85th percentile`, `II`, `III`, `exactly one`,
`both negative`, `quadrant II`, `the y-axis`, `the left sum`, `Interpolation`,
`Extrapolation`, `vertices`, `sides`, `liters`, `18 degrees Celsius`, `overestimate`,
`infinitely many solutions`, and ten graded sentences such as
`underestimate — chords lie below a concave-down curve`,
`yes — compositions of continuous functions are continuous`, and
`the object traveled approximately 51 meters during the first 6 seconds`.

**Action.** Add a `choice` answer kind. About 160 of the 169 are one short option and fit
a closed option set directly. The ten graded sentences do not; re-kind those topics to
`multi-step`.

**Why this group matters most.** 1.0 claims a deterministic verdict on all of them and
gets it wrong. Prose parses into a product of one-letter symbols, and multiplication
commutes, so `sey` passes for `yes`. 2.0 refuses them, which is right, but it leaves 78
topics with no fast path.

### 3.2 A fractional or symbolic exponent — 28 answers, 12 topics

Topics: `antiderivatives`, `derivative-power-rule`, `derivatives-exp-log`,
`improper-integrals-discontinuous`, `integration-substitution`, `limits-of-sequences`,
`logarithmic-differentiation`, `radical-exponent-conversion`, `rational-exponents`,
`recursive-sequences`, `reverse-power-rule`, `u-substitution-basics`.

Two shapes hide here:

- A **rational exponent**: `x^(1/2)`, `x^(2/3)`, `x^(5/6)`, `(5/2)x^(3/2)`,
  `(2/3)x^(-1/3)`, `(2/9)(x^3 + 1)^(3/2) + C`, `3 + 3*2^(1/3)`.
- A **symbolic exponent**: `2^x ln 2`, `10^x ln 10`, `3^x (1 + x ln 3)`, `x^x (ln x + 1)`,
  `x^(sin x) (cos(x) ln(x) + sin(x)/x)`, `3*2^(n-1)`, `$1/2^n$`.

**Action.** Extend the grammar in two steps. A rational exponent `p/q` over a
non-negative rational base becomes a radical, and the canonical form already holds
radicals. A symbolic exponent becomes an opaque power atom that compares structurally on
its canonical base and its canonical exponent — the rule the whitelisted functions
already follow. Both steps stay exact and run no search.

### 3.3 Quotient and remainder — 16 answers, 5 topics

Topics: `division-with-remainders`, `long-division`, `long-division-one-digit`,
`polynomial-division`, `synthetic-division`.

Two spellings: `9 R2`, `6 R2`, `5 R3`, `8 R2`, `23 R14`, `41 R16`, `41 R8`, `241 R2`,
`71 R3`, `152 R3`; and `x + 2 remainder 3`, `2x + 3 remainder 5`,
`x^2 + x + 1 remainder 6`, `2x^2 + 4x + 5 remainder 11`, `x^2 - 2x remainder 6`.

**Action.** Author the answer as the tuple `(quotient, remainder)`. The tuple production
already exists, and it compares element by element. If the authored spelling must stay,
add a `q R r` production that reads both spellings into the same tuple.

### 3.4 A prose list with commas — 15 answers, 9 topics

The 1.0 shape classifier calls these `comma_list`, and each item is prose:
`slope 3, y-intercept -5`, `degree 3, leading coefficient 4`, `rise 2, run 3`,
`initial value 7, growth factor 3`, `closed circle at 0, ray to the left`,
`left 3, right 4, two-sided DNE`,
`shift right 1, stretch vertically by 2, reflect across the x-axis`,
`long leg 3√3, hypotenuse 6`, `no — the left side suggests 2, the right side 7`,
`$\langle -1, -1, -1 \rangle$`, `$\langle -y, -z, -x \rangle$`.

Topics: `computing-div-curl`, `exponential-functions`, `function-stretches-reflections`,
`graphing-inequalities-number-line`, `limits-from-tables`, `limits-piecewise-functions`,
`polynomial-basics`, `reading-slope-intercept-equations`, `special-right-triangles`.

**Action.** Re-kind these topics to `multi-step`. Each answer names two or three labeled
quantities, and a label is prose. The two `\langle … \rangle` vector answers of
`computing-div-curl` are the exception: author them as tuples and they enter the grammar
at once.

### 3.5 A multi-letter variable run — 11 answers, 9 topics

The lexer reads `xy` as one name, `xy` is not a whitelisted variable, and the answer is
refused. Topics: `binomial-expansion`, `boolean-algebra-basics`,
`differentials-error-propagation`, `dividing-polynomials-by-monomials`,
`first-order-partial-derivatives`, `gcf-of-monomials`, `implicit-differentiation`,
`partial-derivatives`, `three-dimensional-coordinates`.

The answers: `$xy$`, `$xz$`, `$12xy$`, `6xy^3`, `$3yz^2$`, `3xy^2 + 2y`, `-3xy + 2`,
`$yz/(x + z)^2$`, `-2xy/(x^2 + 2y)`, `$a^4 + 4a^3 b + 6a^2 b^2 + 4ab^3 + b^4$`, and
`3x^2 dx`.

1.0 splits such a run with SymPy's `implicit_multiplication_application`
(`docs/reference/checker-1.0-spec.md` section 3.1), so `3xy^2` reads as `3*x*y**2`.

**Action.** Extend the lexer. When a letter run is not a whitelisted function name, not a
Greek name, and not a single letter, split it into single-letter variables. That recovers
10 of the 11.

**Caution.** The split reads `3x^2 dx` as `3*x**2*d*x`, which is the silent semantic
corruption of spec section 7.8. Refuse a run that starts with a differential (`dx`, `dy`,
`dt`) instead of splitting it, and leave `differentials-error-propagation` in the residue.

### 3.6 A subscripted logarithm base — 10 answers, 3 topics

Topics: `log-power-rule`, `log-product-quotient-rules`, `logarithm-properties`.

The answers: `3·log_b(x)`, `5·log_2(x)`, `log_3(x^4)`, `log_3(5x^2)`, `log_b(x^3/y^2)`,
`log_b(x) + log_b(y)`, `log_b(x) - log_b(y)`, `log_b(x) + log_b(y) - log_b(z)`,
`2·log_b(x) + log_b(y) - log_b(z)`, `3·log_b(x) + 2·log_b(y)`.

1.0 reads `log_b(x)` as the product `log_b*x` — a silent corruption (spec section 7.8).

**Action.** Extend the lexer to read `log_<base>` as the two-argument `log`. The AST
already accepts `log` with one or two arguments, and the canonical form already compares
a function application structurally on its canonical arguments. The base `b` becomes a
variable and `2` becomes a rational, so `log_b(x) + log_b(y)` and `log_b(y) + log_b(x)`
are one answer while `log_2(x)` and `log_3(x)` stay two.

### 3.7 A general inequality — 5 answers, 3 topics

`2x + 5 <= 17`, `y/3 > 6`, `n - 3 > 10` (`writing-inequalities-from-statements`),
`5 <= 7, so it holds` (`vectors-in-rn`), and `6 ≤ ∫ ≤ 15`
(`riemann-sums-definite-integral`).

The grammar accepts an inequality only with a bare variable on one side, so `x <= 4`
parses and `2x + 5 <= 17` does not.

**Action.** Extend the inequality production to two expressions. Canonicalize
`lhs - rhs`, normalize the leading coefficient of the variable to `1`, and flip the
operator when that coefficient is negative. `2x + 5 <= 17` and `x <= 6` then become one
answer, and three of the five enter the grammar. Re-kind `vectors-in-rn`, whose answer is
prose, and `riemann-sums-definite-integral`, whose answer carries an integral sign.

### 3.8 Infinity — 4 answers, 1 topic

`∞` and `-∞` (three times) on `limits-at-infinity-rational`.

1.0 accepts `oo` and `∞` because every SymPy name is in scope of the learner answer box.
That same namespace is how a learner reaches `zoo`, `integrate`, and `factorint`
(spec section 3.1). 2.0 has no such namespace.

**Action.** Add an infinity value with the spellings `∞`, `-∞`, `oo`, and `-oo`, or put
the four exemplars on a `choice` kind. An infinity value is a small, decidable addition;
a SymPy namespace is not.

### 3.9 A label set — 4 answers, 2 topics

`$\{HH, HT, TH, TT\}$`, `$\{HA, HB, HC, TA, TB, TC\}$`,
`$\{H1, H2, H3, H4, T1, T2, T3, T4\}$` (`listing-sample-spaces`), and
`$\{AB, AC, BD\}$` (`spanning-trees-mst`).

The members are labels, not values. `HT` is one outcome, not `H` times `T`.

**Action.** Add a `choice` answer kind whose option is an unordered set of opaque label
tokens, and compare the two sets after casefolding each token. Do not send a label
through the expression grammar: the multi-letter split of group 5 reads `HT` as `H*T` and
makes `HT` and `TH` the same outcome.

### 3.10 An approximation marker — 4 answers, 2 topics

`≈ 36.9°` and `≈ 58.0°` (`right-triangle-trig`), `≈ 14` and `≈ 20`
(`solving-right-triangles-sides`). The `≈` character is outside the grammar.

**Action.** Author the exact value and state the rounding in the prompt. Do not add a
tolerance rung. D6 forbids a float in an equality decision, and a tolerance is exactly the
1.0 rung that makes `1/3` equal `0.333333` and not equal `0.33333` (spec section 3.2). If
a rounded answer must stay authored, give the exemplar the rounded number as the exact
expected value and drop the `≈`.

### 3.11 A value with a unit — 3 answers, 3 topics

`7 L/min` (`average-instantaneous-rate`), `2π cm^2` (`differentials-error-propagation`),
`60 km/h` (`estimating-derivatives`).

A one-letter unit already parses as a variable, so `5 m/s` is inside the grammar today —
by accident. It compares `m` and `s` as variables, and `5 m/s` therefore equals `5 s/m`
under no reading a learner intends.

**Action.** Extend the grammar with a `value unit` production. Read the trailing unit
token as an opaque casefolded string, compare it separately from the value, and give a
unit mismatch its own verdict. The same rule removes the accidental reading of `5 m/s`.

### 3.12 The `arctan` spelling — 2 answers, 1 topic

`arctan x + x/(1 + x^2)` and `arctan(2x) + 2x/(1 + 4x^2)` (`derivatives-inverse-trig`).
The whitelist holds `atan`, not `arctan`.

**Action.** Add `arcsin`, `arccos`, and `arctan` as spellings of `asin`, `acos`, and
`atan` in the normalizer. That is one rewrite table entry each, and the canonical form
does not change.

### 3.13 An undefined quotient — 2 answers, 1 topic

`0/0` and `3/0` (`factoring-limits`). The grammar refuses a division by zero, which is
correct: neither string names a value.

**Action.** Add a `choice` answer kind with the options `undefined` and `does not exist`,
and author these two exemplars against it.

### 3.14 Derivative notation — 2 answers, 1 topic

`4y^3 · dy/dx` and `2y · dy/dx + 3x^2` (`implicit-differentiation-basic`).

1.0 reads `dy/dx` as `d*y/d*x`, which left-associative division turns into `x*y`
(spec section 7.8). The derivative is silently multiplied out, and nothing detects it.

**Action.** Re-kind the topic to `multi-step`. A decidable reading of `dy/dx` needs a
derivative atom, and one topic does not pay for it.

### 3.15 A subscripted variable — 1 answer, 1 topic

`$3a_1 - a_2$` (`computing-matrix-vector-products`).

**Action.** Extend the lexer to read `a_1` as one variable name. The canonical form
already holds a variable atom by name, so nothing else changes.

### 3.16 A factorial — 1 answer, 1 topic

`n!` (`higher-order-derivatives`).

**Action.** Add a factorial production, or re-kind the topic. One answer does not pay for
a new production; re-kinding is the cheaper choice today.

### 3.17 An unknown function — 1 answer, 1 topic

`$sY(s) - y(0)$` (`laplace-of-derivatives`). `Y(s)` is an unknown function of `s`, not a
value, and `sY` is also a multi-letter run.

**Action.** Re-kind the topic to `multi-step`.

---

## 4. What to do first

1. **Add the `choice` answer kind** (A2, V2). It covers 179 answers on 82 topics, it
   closes the largest 1.0 correctness gap, and it runs no arithmetic.
2. **Add the multi-letter split and the rational exponent** (groups 2 and 5). Together
   they recover 38 answers on 20 topics for two lexer rules and one canonical-form rule.
3. **Add `log_b`, the general inequality, the unit token, `arctan`, and `a_1`**
   (groups 6, 7, 11, 12, 15). They recover 20 answers on 10 topics.
4. **Re-kind the rest** (groups 3, 4, 14, 16, 17, and the tail of group 1). About 41
   answers are not values, and `multi-step` is the honest kind for them.

Until then, A2 must reject the 278 answers at authoring time. A topic that carries one of
them claims a deterministic verdict the checker cannot support, and that claim is the 1.0
defect the M2 plan set out to remove.
