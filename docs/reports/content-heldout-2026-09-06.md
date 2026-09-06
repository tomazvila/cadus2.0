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

55 new knowledge points gained one `teach` and one `hint_ladder` draft each
(110 documents), under `docs/content-foundations/<unit>/`:

| unit | knowledge points |
| --- | --- |
| `integers-negatives` | 27 |
| `fractions-decimals` | 22 |
| `exponents-radicals` | 5 |
| `rational-trig` | 1 |

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
  author` binary. First pass stores 110, second pass stores 0 skips 110.
  Model cost reported: 0 micro-USD for both passes. `content_store` ends at
  110 rows, all `pending`, 0 `approved`. `model_call_log` holds 110 rows, all
  at `cost_usd = 0` and `model_id = 'operator-draft-v1'`.
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

## Exact residual: 673 Foundations knowledge points not drafted by this pass

| unit | residual KPs |
| --- | --- |
| `polynomials-quadratics` | 102 |
| `rational-trig` | 102 |
| `functions-exponentials` | 92 |
| `systems-inequalities` | 75 |
| `exponents-radicals` | 73 |
| `linear-graphs` | 69 |
| `fractions-decimals` | 68 |
| `expressions-equations` | 67 |
| `integers-negatives` | 19 |
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
- **5 KPs**: `same_shape_new_operands` exhausted 200 draws without landing on
  a value distinct from every served answer (small, tightly constrained
  answer ranges).
- **3 KPs**: an authored answer is not a plain number, a fraction or a
  decimal (for example, a spelled-out scientific-notation answer).

## Untouched by this lane: `solutions`, `practicable`, `assessable`

The readiness audit's `solutions` blocker (a non-held-out decidable exemplar
with no `solution_sketch`) and its `practicable`/`assessable` blockers (fewer
than three practice items, or no item held out for assessment) are CURRICULUM
facts (`crates/core/src/readiness/facts.rs`), not `content_store` documents —
fixing them means adding a `solution_sketch` and, for many KPs, a fourth
pedagogically-distinct decidable exemplar to `curriculum/foundations/*.yaml`
itself. This lane's evaluator and `same_shape_new_operands` are exactly the
tool such a patch would need (verified arithmetic, no served-answer
collision), but authoring that patch, updating every curriculum-hash-pinned
test fixture it touches, and re-auditing readiness end to end is a second,
separately-sized change that this lane did not reach. It is the natural next
step for whichever lane picks this file up next.
