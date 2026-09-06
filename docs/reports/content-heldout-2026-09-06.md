# Foundations held-out content: deterministic teach/hint generation

## Scope and method

The readiness audit of 2026-09-06 counts 809 Foundations knowledge points,
208 pending documents over 81 (all `arithmetic-core`, `framework/content-batch`
at `a8596b1`), zero approved, and 728 knowledge points with no `teach` or
`hint_ladder` draft at all. This lane closes part of that gap without any
model call: it classifies the `Compute|Calculate|Evaluate|Simplify $expr$.`
pure-numeric-arithmetic family across every Foundations unit, evaluates each
KP's own authored exemplar with an exact-rational evaluator, and generates
a fresh same-family worked example and a generic Socratic hint ladder from
it — never copying a served answer into either.

The evaluator (`scripts/authoring/foundations_compute.py`) rewrites LaTeX
arithmetic into a whitelisted Python `ast` expression (never `eval`) and
walks it with exact `fractions.Fraction` arithmetic: integers, decimals,
fractions, mixed numbers, absolute value, thousands separators, implicit
multiplication, and integer or exact-rational exponents (`8^{2/3}`). Its
oracle test evaluates all 228 qualifying exemplars authored anywhere in the
Foundations tree and checks it reaches the exact authored answer for every
one — the curriculum is the oracle, not a hand-picked sample.

`scripts/authoring/foundations_drafts.py` classifies each knowledge point
(`classify_kp`): every exemplar must fit the pure-numeric family with a
parseable answer, and `same_shape_new_operands` must find a fresh operand
draw, seeded deterministically on the knowledge point's own serving key,
whose result differs from every answer the knowledge point already serves.
The teach page's final step states ONLY the bare new answer (never the
expression again), because the production gate
(`crates/core/src/instruction/teach.rs::check_no_other_answer`) scans the
whole last step for any token a sibling exemplar already serves — restating
the new expression risked an incidental digit collision with an unrelated
exemplar's small answer. The hint ladder is one of nine generic,
digit-free, operator-family templates (absolute value, exponent, fraction
reduce/add-sub/mul-div, decimal add-sub/mul-div, integer add-sub/mul-div);
no rung can ever name a served answer because no rung names a digit.

## What this lane generated

47 new knowledge points gained one `teach` and one `hint_ladder` draft each
(94 documents), under `docs/content-foundations/<unit>/`:

| unit | knowledge points |
| --- | --- |
| `integers-negatives` | 21 |
| `fractions-decimals` | 21 |
| `exponents-radicals` | 4 |
| `rational-trig` | 1 |

(An earlier pass of this same generator shipped 55 KPs with a wider,
unbounded operand search; see "A caught defect" below for why the count
dropped to 47 and why that is the correct outcome, not a regression.)

`arithmetic-core` (81 KPs) already holds hand-authored drafts from
`framework/content-batch` and is left untouched by this generator
(`--skip-unit arithmetic-core`, the default).

## Validation

- `scripts/authoring/test_foundations_compute.py` (14 tests) and
  `test_foundations_drafts.py` (8 tests): pure-Python, no database, no model.
- `crates/worker/tests/authoring_heldout_drafts.rs` (3 tests): every
  generated draft runs through the real `verify_kind` gate the worker's
  `author` command calls, against the real curriculum, with a dense
  synthetic answer set (0..=1000) for hint ladders; every hint ladder holds
  exactly three question rungs with no digit; every drafted KP sits inside
  its declared unit with exactly one teach and one hint_ladder.
- `crates/worker/tests/authoring_heldout_import.rs` (1 test): imports all
  four manifests into a disposable `TestDb` through
  `scripts/authoring/import_local_drafts.py` and the real `cadus-worker
  author` binary. First pass stores 94, second pass stores 0 skips 94. Model
  cost reported: 0 micro-USD for both passes. `content_store` ends at 94
  rows, all `pending`, 0 `approved`. `model_call_log` holds 94 rows, all at
  `cost_usd = 0` and `model_id = 'operator-draft-v1'`.
- `cargo fmt --all -- --check` and `cargo clippy -p cadus-worker --tests -- -D
  warnings`: clean. `jscpd` over the new files: 0 clones.
- No production or manual database was written. `CADUS_TEST_DATABASE_URL`
  points at the throwaway `cadus2-testdb` container; `TestDb` creates and
  drops a fresh database per test run.

## Root import command (same pattern as `framework/content-batch`)

```sh
DATABASE_URL=<isolated database> python3 scripts/authoring/import_local_drafts.py \
  --manifest docs/content-foundations/<unit>/manifest.json \
  --worker <path>/cadus-worker
```

for `<unit>` in `integers-negatives`, `fractions-decimals`,
`exponents-radicals`, `rational-trig`. Every row lands `pending`; nothing here
approves a row.

## Exact residual: 681 Foundations knowledge points not drafted by this pass

| unit | residual KPs |
| --- | --- |
| `polynomials-quadratics` | 102 |
| `rational-trig` | 102 |
| `functions-exponentials` | 92 |
| `systems-inequalities` | 75 |
| `exponents-radicals` | 74 |
| `fractions-decimals` | 69 |
| `linear-graphs` | 69 |
| `expressions-equations` | 67 |
| `integers-negatives` | 25 |
| `measurement-units` | 6 |

Full per-KP list: `scripts/authoring/generate_foundations_drafts.py
--residual-report <path>` regenerates it (also archived at
`/home/deploy/.cache/cadus2_orchestration/heldout-residuals.json` on the
authoring host; not committed, since it is fully reproducible from the
committed generator and curriculum). By reason:

- **605 KPs**: not every exemplar is a
  `Compute|Calculate|Evaluate|Simplify $expr$.`-shaped problem — most of
  `linear-graphs`, `expressions-equations`, `systems-inequalities`,
  `polynomials-quadratics`, `functions-exponentials` and `measurement-units`
  are word problems, "solve for x", graph-reading or factoring/expanding
  problems whose answer is an expression, not a value this evaluator checks.
  A correct generator for these families needs a symbolic (not purely
  numeric) checker; out of this lane's scope.
- **60 KPs**: an exemplar names a LaTeX construct outside the whitelist —
  almost entirely `\sqrt{}` (radicals proper, as opposed to the rational
  exponents this lane's evaluator does support) and a few percent/pi forms.
- **13 KPs**: `same_shape_new_operands` exhausted its draws without landing
  on a value both distinct from every served answer AND inside the range
  those served answers already span (a knowledge point whose few exemplars
  already densely cover their whole authored band correctly declines rather
  than manufacturing an out-of-band item — see "A caught defect" below).
- **3 KPs**: an authored answer is not a plain number, a fraction or a
  decimal (for example, a spelled-out scientific-notation answer).

## `solutions`: closed for every pure-numeric compute KP

The readiness audit's `solutions` blocker (a non-held-out decidable exemplar
with no `solution_sketch`) is a CURRICULUM fact
(`crates/core/src/readiness/facts.rs`), not a `content_store` document.
`scripts/authoring/apply_foundations_solution_sketches.py --write` added a
`solution_sketch` to every exemplar of every pure-numeric compute knowledge
point that lacked one: 105 lines across `arithmetic-core` (57),
`fractions-decimals` (25), `integers-negatives` (19) and `exponents-radicals`
(4) — no exemplar was added, removed, reordered, or had its `answer` or
`answer_contract` touched, and each sketch names only that exemplar's own
already-served problem and answer (never a different exemplar's). The
generator is `foundations_drafts.solution_sketch_for`, built on the same
verified evaluator as the teach/hint drafts; the line-level editor
(`foundations_curriculum_patch.py`) follows the same scan-and-insert
convention as the existing `foundations_visuals.py` and refuses (writing
nothing) if a target already carries an authored sketch or is not found.

Validated three ways: `cargo run --bin lint_curriculum` reports 0 findings on
the patched tree; the Python regression test asserts the regenerated fixture
has zero remaining `missing_solution_sketches` across all 92 qualifying
knowledge points; and
`crates/core/tests/foundations_heldout_solutions.rs` loads the REAL
curriculum through the REAL `ReadinessIndex` (production code, not this
lane's own classifier) and asserts `facts.solutions` now holds for all 92.

## A caught defect: a same-shape redraw needs a bounded band, not just a distinct answer

The first version of `same_shape_new_operands` redrew every bare operand up
to twice its original size and only forbade landing on an ALREADY-served
answer. An audit that additionally checked "does the new answer stay within
the range this knowledge point's own exemplars already span" caught 65 of
the 87 candidates it had accepted across the whole curriculum failing that
check — including committed regressions such as
`dividing-integers/kp1` turning a "divides evenly" knowledge point's worked
example into `19 \div (-8) = -19/8` (not an integer), and
`mixed-numbers/kp1` producing the malformed mixed number `2\frac{4}{4}`
(numerator not below denominator). Distinct-from-served is necessary but not
sufficient: nothing stopped a redraw from being easier, harder, or outright
malformed relative to the knowledge point's own authored band.

The fix adds two checks the operand redraw alone cannot make on its own,
both supplied by the caller and checked by `same_shape_new_operands` on
every draw: `extra_ok` bounds the final value to `[min(served), max(served)]`
and, when every served answer is already an integer, requires the new one to
be one too; `operand_ceiling` caps every drawn operand's SIZE at the largest
bare-integer operand size any of the knowledge point's OWN exemplars already
use anywhere (so a fraction's denominator, for example, never drifts past
every denominator its siblings ever used). A knowledge point whose served
answers already densely cover its whole authored band (for example
`single-digit-addition/kp1`, whose three exemplars are 7, 8 and 9 with no
integer left between them) now correctly declines rather than manufacturing
an out-of-band item. Applied to the whole curriculum this dropped the
teach/hint_ladder count from 55 to 47 knowledge points — the 8 lost
candidates all had no honest way to reach a new value in the same authored
band. `crates/worker/tests/authoring_heldout_drafts.rs` and
`authoring_heldout_import.rs` were re-run against the regenerated,
now-bounded drafts and stay green;
`scripts/authoring/test_foundations_drafts.py` gained a regression assertion
that no returned candidate ever crosses its knowledge point's own served
range or turns an all-integer knowledge point fractional.

The `solution_sketch` patch (the section above) never used
`same_shape_new_operands` at all — it only restates an exemplar's own
already-authored numbers — so it needed no rework and no re-import.

## Untouched by this lane: `practicable` and `assessable`

`practicable` (at least three practice items) and `assessable` (one item held
out) need a FOURTH pedagogically-distinct decidable exemplar on most of these
knowledge points (most currently author exactly two or three) —
`same_shape_new_operands` is exactly the tool such a patch would need, but
inserting a new exemplar block (versus this lane's additive-only
`solution_sketch` line) is a larger structural edit, and updating whatever
curriculum-hash-pinned fixtures it touches and re-auditing readiness end to
end is a separately-sized change this lane did not reach. It is the natural
next step for whichever lane picks this file up next.
