Source: the 2.0 answer grammar of M2, measured on 2026-08-27 after the review round 1
fixes (FIXM2a and FIXM2b), against
`crates/core/tests/fixtures/answers/corpus_1_0.jsonl` (3,492 answers, 478 topics).
Re-measured after the review round 2 fixes (FIXM2d and FIXM2e): the split does not move.
FIXM2d changed the mixed-number rule, the juxtaposed function argument, the space-group
rule, the times-`x` rule, and the LaTeX root, and every one of the 3,227 accepted answers
and every one of the 265 refused answers keeps its side of the line.

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
`reason` (the refusal of the grammar). The identity of the 265 answers is also a
committed fixture, `crates/core/tests/fixtures/answers/undecidable_1_0.jsonl`, and
`crates/core/tests/answer_parse.rs` fails when the set moves.

---

## 1. The counts

| Measure | Value |
|---|---:|
| corpus answers | 3,492 |
| answers inside the grammar | 3,227 (92.41%) |
| answers the grammar refuses | **265 (7.59%)** |
| topics that hold a refused answer | 116 of 478 |
| refused answers on `expression` topics | 198 |
| refused answers on `numeric` topics | 67 |

Round 1 of the review moved the split from 3,214 / 278 to 3,227 / 265. The 13 recovered
answers are the ten multi-letter variable runs of group 5 (`$12xy$`, `$3yz^2$`,
`$a^4 + 4a^3 b + 6a^2 b^2 + 4ab^3 + b^4$`, and seven more), plus `2π cm^2`, `60 km/h`,
and `50th`. Section 3.18 explains why the last three are decided but are still authored
in the wrong kind.

`docs/reference/checker-1.0-spec.md` section 8.3 estimated 230 refusals. The measured
number is 265. The estimate counted whole shape buckets, and the grammar decides one
answer at a time, so the difference runs both ways.

The estimate excluded five whole buckets: `prose_or_words` (167), `interval_ineq` (34),
`quotient_remainder` (16), `value_with_unit` (12), and `equation` (1). Those buckets give
189 refusals, not 230, because three productions of 2.0 reach into three of them:

- The **interval production** of `docs/plans/M2.md` is not in the spec section 8.1
  grammar. It gives 29 of the 34 `interval_ineq` rows to the grammar.
- The **multi-letter split** of review round 1 splits a short run of unknown letters into
  single-letter variables, so `3xy^2` is `3*x*y**2`. It gives 11 of the 12
  `value_with_unit` rows to the grammar. Section 3.18 shows why two of those 11 rows are
  a poor outcome.
- The **value label** of review findings #2, #10, and #16 reads a leading `<var> =` as
  `Assign(var, value)`. It gives the one `equation` row (`y = x`) to the grammar.

The estimate counted the other buckets as covered. They hold 76 refusals: 54 in
`expression_symbolic` (a rational or symbolic exponent, a subscripted logarithm base, a
subscripted variable, a factorial, an approximation marker, a label set), 15 in
`comma_list`, 5 in `expression_numeric`, and 2 in `fraction`. 189 + 76 = 265.

The **three-digit numerator rule** of review finding #7 narrows the mixed-number
production against the section 8.1 grammar: a mixed number needs a proper fraction in
plain digit runs (`0 < b < c`, no leading zero, no three-digit numerator), because a
three-digit run after a space is the thousands group of the V4 table. `1 000/3` is
therefore undecidable. The rule costs no corpus row — all 8 `mixed_number` rows parse —
but it is the reason the grammar is narrower than the estimate assumed.

### Refused by rule

The answers below are outside the grammar by a deliberate decision, and no corpus row
holds one of them today, so they add nothing to the 265. A curriculum author who writes
one gets an `Undecidable` and no verdict, which is the point: each shape has two readings,
and a checker that picks one of the two grades a wrong answer correct. The list stands
here because A2 must reject these spellings at authoring time as well.

| Answer | Refusal reason | Rule |
|---|---|---|
| `1 000/3` | `two numbers stand side by side` | A mixed number needs `0 < b < c` in plain digit runs. A three-digit run after a space is a thousands group of the V4 table (round 1, finding #7). |
| `2\frac{3}{2}` | `a mixed number whose fraction is not proper` | The same rule, for the literal-fraction token. `2\frac{3}{2}` is neither the mixed number 7/2 nor the product 3 (round 2, finding #1). |
| `x 2½` | `a fraction stands after a number that is no whole part` | A number token in front of a fraction is a mixed number or the answer is undecidable. The term in front here is the product `x*2` and not a bare whole number (round 2, findings #2, #3). |
| `2.5½` | `a fraction stands after a number that is no whole part` | The same rule. A whole part is a bare whole number, and a decimal is not one (round 2, finding #2). |
| `1/2 ½` | `a fraction stands after a number that is no whole part` | The same rule. The term in front is the fraction `1/2`, so two fractions side by side take no reading (round 2, finding #2). |
| `x/1 000` | `a space-grouped number stands after a factor` | A space-grouped number is a whole answer or a full operand at the top level. After a factor it takes no reading, and the old parser read `x/1 * 0` (round 2, finding #11). |
| `2\frac{x+1}{2}` | `a mixed number whose fraction is not proper` | A `\frac` after a number is the fractional part of a mixed number, and a mixed number needs two plain digit runs. A brace body that holds an expression takes no reading (round 3, the structural ruling). |
| `2\frac{+1}{2}` | `a mixed number whose fraction is not proper` | The same rule. A sign is not a plain digit run, so `+1` is no numerator of a mixed number. |
| `√√16` | `a root with no argument` | The glyph `√` is a lexer token that takes one primary. A second `√` is no primary, so the outer root has no argument (round 3, the structural ruling). |
| `2^50%` | `an exponent that is not a whole number` | A `%` is a postfix on the primary it follows, so the exponent is 1/2 and not a whole number. The old string rewrite wrote `2**(50)/100`, which is 5 and grades a wrong answer correct (round 3, findings #3, #4). |
| `50%%` | `two percent signs on one number` | One primary takes at most one percent postfix. Two readings stand behind the second sign — 0.5% and 0.005 — and the grammar picks neither. |
| `1,500%` | `a comma-grouped number stands in a longer answer` | The comma thousands group is a FULL match of the whole string (1.0 `_COMMA_GROUPS_RE`). A grouped number inside a longer answer keeps two readings, the number 1500 and the tuple `1, 500` (round 3, finding #6). |
| `3 + 1,500` | `a comma-grouped number stands in a longer answer` | The same rule, without a percent. |
| `1 500%` | `two numbers stand side by side` | The space thousands group is a full match too, so `1 500` inside a longer answer is two numbers. The old percent rewrite inserted a bracket that bypassed the refusal, and `1 500%` then meant two different values (round 3, finding #6). |
| `t/4 3/4` | `a fraction stands after a number that is no whole part` | A `/` takes the number token into the quotient `t/4`, so that token is inside a factor and it is no whole part. The old parser read `((t/4)*3)/4` = 3t/16, and the mixed-number rule reads `t/(4 + 3/4)` = 4t/19, so a wrong answer graded correct (round 4, finding #1). |
| `x/2 1/2` | `a fraction stands after a number that is no whole part` | The same rule, with a variable numerator. |
| `cos(x)/2 1/2` | `a fraction stands after a number that is no whole part` | The same rule, with a function call in front of the `/`. |
| `pi/2 1/2` | `a fraction stands after a number that is no whole part` | The same rule, with a constant in front of the `/`. |
| `x^2 1/2` | `a fraction stands after a number that is no whole part` | A `^` takes the number token into the power `x**2`, so that token is no whole part either (round 4, finding #1). |
| `x 2^3 1/2` | `a fraction stands after a number that is no whole part` | The same rule, with the power as the last factor of a longer product. |

The mirror spellings `x 3 1/2`, `2.5 1/2`, `1/2 1/2`, `3 3/2`, `9/2 1/2`, and `x 2 1/2`
take the round 1 refusal `two numbers stand side by side`, so the `a b/c` spelling and the
token spelling of one shape give one answer. The glyph and the `\frac` spellings of the six
round-4 rows above (`t/4 ¾`, `t/4 \frac{3}{4}`, `x^2 ½`, `pi/2 ½`) take the same reason as
the row itself, which is the point of the round-4 fix: one shape, one verdict.
`crates/core/tests/answer_parse.rs` pins every reason above as a literal,
`crates/core/tests/answer_check.rs` pins the round-4 rows at the verdict level, and
`crates/core/tests/answer_divergence.rs` pins the `x/1 000` refusal and the `2\frac{3}{2}`
refusal.

None of the twenty-one answers above is a corpus answer, so the split of the section below
does not move. Re-verified on 2026-08-27, after FIXM2j: 3,492 corpus answers, 3,227 parsed,
265 refused, and the committed `undecidable_1_0.jsonl` holds the same 265 rows as before
round 3.

### The refusal reason the grammar gives

| Reason | n |
|---|---:|
| `a name that is not a function or variable` | 199 |
| `an exponent that is not a whole number` | 28 |
| `a character outside the grammar` | 21 |
| `a number glued to a name reads as a label` | 11 |
| `an inequality with no bare variable` | 4 |
| `a fraction with a zero denominator` | 2 |

Every refusal is an `Undecidable` value. The checker never guesses, and it never panics.

---

## 2. The groups

The 17 groups below are disjoint, and they sum to 265.

| # | Group | n | topics | Recommended action |
|---|---|---:|---:|---|
| 1 | prose and single words | 168 | 78 | Add a `choice` answer kind, or re-kind to `multi-step` |
| 2 | a fractional or symbolic exponent | 28 | 12 | Extend the grammar: a rational exponent and a symbolic exponent |
| 3 | quotient and remainder | 16 | 5 | Author the answer as a tuple, or add a `q R r` production |
| 4 | a prose list with commas | 15 | 9 | Re-kind to `multi-step`; author the two vectors as tuples |
| 5 | a differential | 1 | 1 | Keep the refusal; re-kind the topic to `multi-step` |
| 6 | a subscripted logarithm base | 10 | 3 | Extend the lexer: read `log_b(x)` as the two-argument `log` |
| 7 | a general inequality | 5 | 3 | Extend the grammar: an inequality between two expressions |
| 8 | infinity | 4 | 1 | Add an infinity value, or add a `choice` answer kind |
| 9 | a label set | 4 | 2 | Add a `choice` answer kind over opaque label tokens |
| 10 | an approximation marker | 4 | 2 | Author the exact value; state the rounding in the prompt |
| 11 | a value with a unit | 1 | 1 | Extend the grammar: a value and an opaque unit token |
| 12 | the `arctan` spelling | 2 | 1 | Add `arcsin`, `arccos`, `arctan` as spellings of the whitelist |
| 13 | an undefined quotient | 2 | 1 | Add a `choice` answer kind with the option `undefined` |
| 14 | derivative notation `dy/dx` | 2 | 1 | Re-kind to `multi-step` |
| 15 | a subscripted variable | 1 | 1 | Extend the lexer: read `a_1` as one variable name |
| 16 | a factorial | 1 | 1 | Add a factorial production, or re-kind |
| 17 | an unknown function | 1 | 1 | Re-kind to `multi-step` |

### What each action buys

- **A `choice` answer kind** (groups 1, 8, 9, 13) covers **178 answers on 82 topics**.
  Authoring gives a closed option set per exemplar, and the checker decides by set
  membership after casefolding. The check is exact, it runs no arithmetic, and it closes
  the largest 1.0 correctness gap: 1.0 reads `yes` as the product `e*s*y`, so every
  anagram of a word answer passes (`docs/reference/checker-1.0-spec.md` section 7.6).
- **Six grammar extensions** (groups 2, 6, 7, 11, 12, 15) recover **45 answers on 21
  topics**. Every one of them stays decidable: no search, no float, no simplification.
  Two of the five rows of group 7 stay out, because their answers are prose and an
  integral sign.
- **Re-kinding to `multi-step`** covers the remaining **42 answers on about 20 topics**:
  groups 3, 4, 5, 10, 14, 16, 17, and the two rows of group 7 that no extension reaches.
  A `multi-step` topic gets the model grader, which is what the 1.0 design intended for
  an answer that is not a value.

The grammar extensions lift grammar coverage from 92.41% to 93.70%. The `choice` kind
adds a further 5.10%. The 42 re-kinded answers then claim no deterministic verdict, which
is the correct outcome for them.

---

## 3. The groups in detail

### 3.1 Prose and single words — 168 answers, 78 topics

`answer_kind` is `expression` on 122 of them and `numeric` on 46. There are 69 distinct
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

The tail holds one or two of each: `18 degrees Celsius`, `2 down`, `65th percentile`,
`85th percentile`, `ASA`, `binomial`, `bisect`, `both negative`, `closed`, `composite`,
`decay`, `equal`, `even`, `exactly one`, `Extrapolation`, `false`, `growth`, `II`, `III`,
`infinite`, `infinitely many solutions`, `Interpolation`, `jump`, `left`, `liters`,
`more`, `neither`, `none`, `obtuse`, `open`, `overestimate`, `parallel`, `perpendicular`,
`prime`, `quadrant II`, `rectangle`, `removable`, `rhombus`, `right`, `sides`, `smaller`,
`square`, `the left sum`, `the y-axis`, `trinomial`, `true`, `vertices`, `washers`.

Ten graded sentences close the group: `2 both ways`, `2x - 2 (both ways)`,
`7 (the values are 6.7, 6.97, 7.03, 7.3)`, `Both equal 5`, `GCF (factor out 5 first)`,
`the definite integral ∫_a^b f(x) dx`,
`the object traveled approximately 51 meters during the first 6 seconds`,
`the temperature stayed constant`,
`underestimate — chords lie below a concave-down curve`, and
`yes — compositions of continuous functions are continuous`.

**Action.** Add a `choice` answer kind. 158 of the 168 are one short option and fit a
closed option set directly. The ten graded sentences do not; re-kind those topics to
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
`71 R3`, `152 R3`; and `x + 2 remainder 3` (twice), `2x + 3 remainder 5`,
`x^2 + x + 1 remainder 6`, `2x^2 + 4x + 5 remainder 11`, `x^2 - 2x remainder 6`.

**Action.** Author the answer as the tuple `(quotient, remainder)`. The tuple production
already exists, and it compares element by element. If the authored spelling must stay,
add a `q R r` production that reads both spellings into the same tuple.

### 3.4 A prose list with commas — 15 answers, 9 topics

The 1.0 shape classifier calls these `comma_list`, and each item is prose:
`slope 3, y-intercept -5`, `slope 0, y-intercept 5`, `slope 1, y-intercept 0`,
`degree 3, leading coefficient 4`, `degree 3, leading coefficient -5`, `rise 2, run 3`,
`initial value 7, growth factor 3`, `initial value 200, growth factor 1/2`,
`closed circle at 0, ray to the left`, `left 3, right 4, two-sided DNE`,
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

### 3.5 A differential — 1 answer, 1 topic

`3x^2 dx` on `differentials-error-propagation`.

Review round 1 added the multi-letter split, so a short run of unknown letters becomes one
variable per letter and `3xy^2` is `3*x*y**2`. That rule recovered the other ten answers
of this group (`$xy$`, `$xz$`, `$12xy$`, `6xy^3`, `$3yz^2$`, `3xy^2 + 2y`, `-3xy + 2`,
`$yz/(x + z)^2$`, `-2xy/(x^2 + 2y)`, `$a^4 + 4a^3 b + 6a^2 b^2 + 4ab^3 + b^4$`).

The split stops at a differential on purpose. It would read `3x^2 dx` as `3*x**2*d*x`,
which is the silent semantic corruption of spec section 7.8. The rule therefore refuses a
run that starts with `d` and one more letter (`dx`, `dy`, `dt`).

**Action.** Keep the refusal, and re-kind `differentials-error-propagation` to
`multi-step`. A decidable reading of `dx` needs a differential atom, and one topic does
not pay for it.

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
through the expression grammar: an upper-case run stays out of the multi-letter split of
group 5 today, and a lower-case reading of it would make `HT` and `TH` the same outcome.

### 3.10 An approximation marker — 4 answers, 2 topics

`≈ 36.9°` and `≈ 58.0°` (`right-triangle-trig`), `≈ 14` and `≈ 20`
(`solving-right-triangles-sides`). The `≈` character is outside the grammar.

**Action.** Author the exact value and state the rounding in the prompt. Do not add a
tolerance rung. D6 forbids a float in an equality decision, and a tolerance is exactly the
1.0 rung that makes `1/3` equal `0.333333` and not equal `0.33333` (spec section 3.2). If
a rounded answer must stay authored, give the exemplar the rounded number as the exact
expected value and drop the `≈`.

### 3.11 A value with a unit — 1 answer, 1 topic

`7 L/min` (`average-instantaneous-rate`). The upper-case `L` is not a variable letter of
the multi-letter split, so the answer stays out of the grammar.

The other two answers of this group now parse, and section 3.18 explains why that is not
a good outcome.

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

### 3.18 Decided, but better re-kinded — 3 answers, 3 topics

These three answers are inside the grammar, so they are not part of the 265. The grammar
decides them, and the decision is honest arithmetic on a reading the author did not mean.
The list is here because a decided answer with the wrong meaning is harder to find than a
refused one.

| Answer | Topic | What 2.0 reads |
|---|---|---|
| `50th` | `percentiles` | `50*t*h` |
| `60 km/h` | `estimating-derivatives` | `60*k*m/h` |
| `2π cm^2` | `differentials-error-propagation` | `2*pi*c*m**2` |

The multi-letter split gives every unknown letter run a variable per letter, so a unit and
an ordinal suffix become a product of variables. The consequences:

- `50th` against the learner answer `50` is **false**, because `50*t*h` is not 50. The
  learner who writes the number is marked wrong.
- `50th` against `50ht` is **true**, because multiplication commutes.
- `60 km/h` against `60 h/km` is false, which is right by accident, but `2π cm^2` against
  `2π m^2c` is **true**, which is right under no reading a learner intends.

**Action.** Re-kind `percentiles` to `multi-step`, or author `50` and put the ordinal in
the prompt. Author the two unit answers against the `value unit` production of group 11
when it lands, and author the value alone until then. A2 must reject an authored answer
whose letter run is a unit, an ordinal suffix, or any other non-variable spelling.

---

## 4. What to do first

1. **Add the `choice` answer kind** (A2, V2). It covers 178 answers on 82 topics, it
   closes the largest 1.0 correctness gap, and it runs no arithmetic.
2. **Add the rational exponent and the `value unit` production** (groups 2 and 11, and
   section 3.18). Together they recover 29 answers and they close the accidental unit
   reading that the multi-letter split opened.
3. **Add `log_b`, the general inequality, `arctan`, and `a_1`** (groups 6, 7, 12, 15).
   They recover 16 answers on 8 topics.
4. **Re-kind the rest** (groups 3, 4, 5, 10, 14, 16, 17, and the ten graded sentences of
   group 1). About 42 answers are not values, and `multi-step` is the honest kind for
   them.

Until then, A2 must reject the 265 answers at authoring time, and it must reject the
three answers of section 3.18 as well. A topic that carries one of them claims a
deterministic verdict the checker cannot support, and that claim is the 1.0 defect the M2
plan set out to remove.
