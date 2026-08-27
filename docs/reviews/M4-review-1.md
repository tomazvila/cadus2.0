# M4 adversarial review — round 1 (2026-08-27)

Run on commit 48b4ec2 (M4 U1–U5 integrated). Six lenses, two find/refute rounds, major+ only: 40 raised, 22 confirmed. Assigned to FIXM4a (template core), FIXM4b (pool, worker, store, image), FIXM4c (benchmarks).

## Orchestrator rulings (binding)

- Per-instance re-check (#1/#2/#15): `TemplateSource::fill` runs every per-instance gate
  rule (exemplar envelope, non-answer tokens, free symbols, decimal trailing zero, hint
  give-away, canonical round-trip) on EVERY instance before it enters the pool; a refusal is
  skipped and counted (`refusals()`); the refill logs and flags a KP whose refusal rate is
  above 10%. The gate's sampled walk above the limit keeps its fixed seed (reproducible)
  but the refill never trusts it.
- Hidden parameter (#4): a parameter that appears in `answer_expr` but in no rendered field
  (statement, sketch, hints, distractor notes) is a rejection: `parameter {name!r} changes
  the answer but never appears in the statement`.
- Sampled walk (#7/#12): a draw that exhausts its budget is skipped, the walk continues; the
  gate refuses only when the whole walk found no tuple, and the message names the count
  found. Coverage above the limit (#16/#22) reads the satisfying sample: edges = the extreme
  satisfying tuples per axis; the crossed-corner rule applies only when both corner tuples
  satisfy the constraints, otherwise it is skipped with the reason logged.
- Values (#8/#9/#18): a constraint literal reads `n/d` and decimals; a `Value` keeps its
  authored spelling for rendering (choice text verbatim; a new `decimal` domain kind
  `{low, high, scale}` renders as a decimal; a rational domain renders `n/d`); the renderer
  wraps a negative or non-integer value in parentheses everywhere (the author writes the
  sign in the statement when a bare form is wanted); the answer writer brackets a power that
  is the base of a power (`(x**2)**3`) and every non-atomic argument.
- Worker curriculum (#5/#6): the image carries `curriculum/` at `/app/curriculum`;
  `CADUS_CURRICULUM` defaults to it; the worker refuses to start (exit 2, message names the
  path and the first finding) when the curriculum does not load. Compose and docs updated.
- Refill starvation (#3): pairs with no fillable source (no approved template, no decidable
  exemplar) leave the target list, are flagged in `operator_flags`, and are retried with a
  backoff (worker-local map keyed by pair, 15 min); the per-tick budget goes to fillable pairs.
- Nonce (#10): the batch nonce is the UTC microsecond clock at the tick, not the tick count.
- Undecodable pool row (#19): skipped, claimed with a logged reason, counted
  (`pool_row_undecodable`); the pop continues with the remaining candidates.
- Benchmarks (#11/#20/#21): B seeds rows through the production insert and pops through the
  production pop; A's L2 half compares each corpus answer against a re-spelled equivalent that
  bypasses the string rung; the allocation bound is measured + 0.5% (a literal) with a
  per-iteration comment.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `crates/core/src/pool/source.rs:240` | FIXM4b | Above the exhaustive limit the gate checks a fixed 4,096-tuple sample while the refill serves a different draw, so no per-instance rule (envelope, non-answer, hint give-away) ever runs on the instance that reaches the pool |
| 2 | blocker | `crates/core/src/pool/source.rs:242` | FIXM4b (dup of #1) | The refill path never re-runs the per-instance gate checks, so an instance the gate would refuse reaches the pool |
| 3 | blocker | `crates/store/src/pool.rs:477` | FIXM4b | The refill target list is starved forever by pairs that can never fill |
| 4 | blocker | `crates/core/src/template/gate.rs:714` | FIXM4a | The gate accepts a template whose answer depends on a parameter the statement never shows, so one printed problem carries several different "correct" answers |
| 5 | blocker | `crates/worker/src/bin/cadus-worker.rs:130` | FIXM4b | A curriculum that does not load turns the WHOLE refill off, and the shipped image ships no curriculum |
| 6 | blocker | `crates/worker/src/bin/cadus-worker.rs:130` | FIXM4b (dup of #5) | A curriculum that does not load turns the whole D-O4 refill off, and the shipped image never carries one |
| 7 | major | `crates/core/src/template/gate.rs:859` | FIXM4a | `build_walk` abandons its tuple collection on the first `NoSatisfyingTuple`, so a sparsely constrained template is refused with the false message "the constraints refuse every tuple" |
| 8 | major | `crates/core/src/template/constraint.rs:275` | FIXM4a | A decimal constraint literal does not survive `to_body`/`from_body`: `{"lit": "0.5"}` is written back as `{"lit": "1/2"}`, which the reader refuses |
| 9 | major | `crates/core/src/template/domain.rs:106` | FIXM4a | A decimal parameter value is rendered into the statement as a fraction, so a decimals template serves text the author never wrote and the reviewer never approved |
| 10 | major | `crates/worker/src/lib.rs:227` | FIXM4b | The refill nonce is a process-local tick counter, so a worker restart replays the same seeds and inserts nothing |
| 11 | major | `crates/store/tests/bench_serve_roundtrip.rs:170` | FIXM4c | Benchmark B seeds pool rows the U4 production pop refuses to decode |
| 12 | major | `crates/core/src/template/gate.rs:859` | FIXM4a (dup of #7) | build_walk stops the whole sampled walk at the first draw that gives up, so the gate reports a false empty space and verifies a small part of an accepted one |
| 13 | major | `crates/core/src/template/eval.rs:676` | FIXM4a | The answer writer's bracket rule is unverified: no test instantiates an expression-kind template |
| 14 | major | `crates/core/src/template/eval.rs:577` | FIXM4a | The exact square root never meets a radicand that is not a whole number, so its denominator test is unverified |
| 15 | major | `crates/core/src/pool/source.rs:234` | FIXM4b (dup of #1) | The refill drops the per-instance re-check 1.0 runs at serve time, so an instance the gate refuses reaches the pool |
| 16 | major | `crates/core/src/template/gate.rs:1032` | FIXM4a | Above the exhaustive limit the gate reads the DECLARED axis ends, so a constrained template can never be approved |
| 17 | major | `crates/core/src/template/eval.rs:725` | FIXM4a | The answer writer omits the brackets around a nested power, so an expression answer leaves the decidable grammar and every instance of the template is refused |
| 18 | major | `crates/core/src/template/render.rs:101` | FIXM4a | The renderer splices a non-atomic parameter value into the statement without brackets while the answer evaluator brackets it, so the printed problem asks a different question than the stored answer answers |
| 19 | major | `crates/store/src/pool.rs:352` | FIXM4b | One undecodable pool row denies every serve for that pair, and is never retired |
| 20 | major | `crates/core/tests/bench_l1.rs:576` | FIXM4c | Benchmark A's L2 gate measures the string rung, so the checker's arithmetic is unmeasured in release |
| 21 | major | `crates/core/tests/bench_l1.rs:105` | FIXM4c | The L1 allocation bound has 1.2 allocations per iteration of headroom, so a `format!` in the hot loop passes |
| 22 | major | `crates/core/src/template/gate.rs:1154` | FIXM4a | The crossed-corner rule is unsatisfiable for a band-constrained pair, so no such template can ever be approved |

## FIXM4a

### #4 [blocker] The gate accepts a template whose answer depends on a parameter the statement never shows, so one printed problem carries several different "correct" answers

File: `crates/core/src/template/gate.rs:714` — IDs: C4, A1, C6, V2

**Claim.** check_dead_parameters counts a name in answer_expr as "use", so a parameter that appears in no rendered field is legal; the gate then walks, renders and hashes all 144 instances without noticing that many of them render the same statement with different expected answers.

**Evidence.**

```
gate.rs:701-714 — `let mut used = scan(&doc.statement).0; ... used.extend(ast_names(ast));` (a name reached only through `answer_expr` satisfies the rule).

Document: statement `"Compute ${a}$ squared."`, params a:int 1..12 and b:int 1..12, answer_expr `"a**2 + b"`, four samples at the four corners (13, 145, 2, 156).

$ cargo run (rv probe, path dep on crates/core)
GATE ACCEPTED space=Exact(144) instances=144 exhaustive=true
  a=3,b=1: text="Compute $3$ squared." answer="10" hash=b693b404c888
  a=3,b=7: text="Compute $3$ squared." answer="16" hash=b693b404c888
  a=3,b=12: text="Compute $3$ squared." answer="21" hash=b693b404c888
  fill(24) returned 12 distinct pool rows:
    row text="Compute $1$ squared." expected_answer="3"  bindings={a:1, b:2}
    row text="Compute $10$ squared." expected_answer="107" bindings={a:10, b:7}
    row text="Compute $12$ squared." expected_answer="155" bindings={a:12, b:11}

The same run with `b` moved into `solution_sketch` and a hint (still not the statement) is also ACCEPTED: `a=4,b=1 -> "17"` and `a=4,b=9 -> "25"`, both hash `4eb63e09dd1b`.
```

**Failure scenario.** An author writes `answer_expr = "a**2 + b"` for `Compute ${a}$ squared.`; every worked sample agrees with the expression, so the gate accepts and a reviewer approves. TemplateSource::fill dedups by instance_hash (the statement digest alone, document.rs:150-160), so the pool keeps exactly one of the 12 tuples per value of `a`, picked by the shuffle. A learner is served `Compute $1$ squared.` and the pool row's expected_answer is `3`. The learner answers `1`, the M5 grade path canonicalizes both and records a failure in the append-only log. No instance of this template is answerable, and nothing in the gate, the refill, or the pop can detect it, because the two halves of one instance are individually consistent. The gate already renders and hashes every satisfying tuple in the exhaustive walk, so the missing rule is "two tuples that render one statement must compute one answer" (equivalently: every parameter the answer reads must appear in the statement).

**Refuter.** I could not refute it. I reproduced the exact behavior with a probe crate that links crates/core by path.

What I checked, and what the code does:

1. /home/deploy/dev/cadus2.0/crates/core/src/template/gate.rs:701-729 — `check_dead_parameters` builds `used` from the statement, the solution sketch, the hints, and the distractor notes, then adds `used.extend(ast_names(ast))`. A parameter that only the answer expression reads counts as used, so `b` is not dead.

2. gate.rs:265-286 — the full check list. No step compares two tuples that render one statement. `check_instances` (gate.rs:1178-1250) walks every satisfying tuple, renders it, evaluates it, and inspects each instance alone: placeholder left, empty answer, non-number token, decimal trailing zero run, free symbol, envelope, hints. It never groups by `instance_hash`, and `instance_hash` is not referenced anywhere in gate.rs.

3. /home

### #7 [major] `build_walk` abandons its tuple collection on the first `NoSatisfyingTuple`, so a sparsely constrained template is refused with the false message "the constraints refuse every tuple"

File: `crates/core/src/template/gate.rs:859` — IDs: A1, C4

**Claim.** In the non-exhaustive branch `build_walk` breaks its 4,096-iteration collection loop the first time `DrawPlan::draw_satisfying` exhausts its 1,000-draw budget, so for any constraint set whose density is under roughly 1/1000 the gate collects a handful of tuples or none, and `check_space` then reports that no tuple satisfies the constraints when many do.

**Evidence.**

```
gate.rs:857-859: `for _ in 0..GATE_SAMPLES { match compiled.plan().draw_satisfying(&doc.constraints, &mut rng) { Ok(tuple) => tuples.push(tuple), Err(DrawError::NoSatisfyingTuple { .. }) => break,` — and `draw_satisfying` gives up after `MAX_REJECTIONS = 1_000` independent draws (draw.rs:65, :181-189). Run, a,b int 1..90 with constraints `eq(a,b)` and `divides(5, a)`:
  declared_space = 8100  (EXHAUSTIVE_SPACE_LIMIT = 4096)
  TRUE satisfying tuples (brute force with all_hold) = 18
  space_size() = Estimated { estimate: 17, samples: 4096, hits: 9 }
  GATE REJECTED [no-satisfying-tuple] the constraints refuse every tuple of the declared domains, so the template has no instance to serve
A sweep over the same family shows the starvation directly (tuples collected out of GATE_SAMPLES=4096): hi=90 step=5 -> 0/4096; hi=100 step=8 -> 1/4096; hi=110 step=5 -> 2/4096; hi=80 step=5 -> 32/4096.
```

**Failure scenario.** An author writes the A1 headline case — a template with inter-parameter constraints — over domains whose product is above 4,096: a,b in 1..90 with `a = b` and `5 divides a`. Eighteen tuples satisfy the constraints (a=b in {5,10,...,90}), comfortably over MIN_SPACE_SIZE=12, and `space_size` itself reports `Estimated { estimate: 17, hits: 9 }` in the very same `Walk`. The gate nonetheless returns `[no-satisfying-tuple] the constraints refuse every tuple of the declared domains, so the template has no instance to serve` — a statement the same struct contradicts — and the knowledge point stays on the A6 exemplar fallback. When the break happens a little later instead of on the first attempt (hi=100 step=8: 1 tuple, hi=110 step=5: 2 tuples), the document is accepted after `check_instances` has looked at one or two instances instead of 4,096, which multiplies the exposure of the finding above.

**Refuter.** I ran the code and the claim is correct in every part. `build_walk` (/home/deploy/dev/cadus2.0/crates/core/src/template/gate.rs:856-860) leaves the 4,096-iteration collection loop on the FIRST `DrawError::NoSatisfyingTuple`, and `draw_satisfying` (/home/deploy/dev/cadus2.0/crates/core/src/template/draw.rs:181-189) gives up after `MAX_REJECTIONS = 1_000` draws. The probability that one call fails is (1-d)^1000 for density d, so the loop stops early long before the density gets near zero. Two effects follow, and I saw both.

First effect: `check_space` (gate.rs:877-885) reports `no-satisfying-tuple` for a document whose satisfying set is large. The reviewer's own fixture reproduced number for number.

Second effect, which the reviewer calls the multiplier: a document the gate ACCEPTS gets its C4 instance check truncated. `check_instances` (gate.rs:1176) reads `walk.tuples`, so the count of

### #8 [major] A decimal constraint literal does not survive `to_body`/`from_body`: `{"lit": "0.5"}` is written back as `{"lit": "1/2"}`, which the reader refuses

File: `crates/core/src/template/constraint.rs:275` — IDs: A1, C6

**Claim.** `write_rational` writes a non-whole literal as a decimal only when the reduced denominator is exactly a power of ten and falls back to `numerator/denominator` otherwise, but `Scalar::rational`/`decimal_to_rational` never reads the `n/d` form back, so a gate-accepted template that uses a decimal constraint literal serializes to a body that `from_body` cannot read.

**Evidence.**

```
constraint.rs:305 `format!("{}/{}", number.numer(), number.denom())` is reached for every literal whose reduced denominator is not a power of ten (0.5 -> 1/2, 0.25 -> 1/4, 0.2 -> 1/5, -2.5 -> -5/2), while domain.rs:132 refuses a text that is not all digits: `if !whole.chars().all(|c| c.is_ascii_digit()) ...  return None;`. Run:
  P1 round trip: {"op":"eq","left":"a","right":{"lit":"3/2"}}
  P1 reread: Err("a lit term needs a whole number or a decimal string, not \"3/2\"")
End to end on a gate-accepted document (rational p, int a, constraint `{"op":"ge","left":"p","right":{"lit":"0.5"}}`):
  GATE ACCEPTED Verified { space: Exact(81), instances_checked: 81, exhaustive: true }
  STORED BODY (constraint part) = "constraints":[{"op":"ge","left":"p","right":{"lit":"1/2"}}],
  RE-READ FAILED: a lit term needs a whole number or a decimal string, not "1/2"
```

**Failure scenario.** An author writes a fraction-of-a-number template with the constraint `{"op":"ge","left":"p","right":{"lit":"0.5"}}`. `gate` accepts it (Exact(81)); the authoring pipeline stores what `to_body(with_space_size(...))` returns, as gate.rs:312-320 directs. That stored body carries `{"lit":"1/2"}`. A reviewer approves the digest (C6). At the next tick `refill.rs:445` calls `from_body` on that body, gets `a lit term needs a whole number or a decimal string, not "1/2"`, caches the digest under `RefillState::refused`, and the knowledge point falls back to exemplars for the life of the worker process — a template that passed every check is permanently unservable. document.rs:24 states the property this breaks: "Serde therefore round-trips the document byte for byte".

**Refuter.** The claim is demonstrable and I reproduced it end to end. `write_rational` (/home/deploy/dev/cadus2.0/crates/core/src/template/constraint.rs:275) reduces the denominator by repeated division by ten and writes a decimal only when the remainder is exactly 1. A `BigRational` in lowest terms has a power-of-ten denominator only when the numerator is coprime to 10, so 0.1, 0.3, and 1.10 survive, while 0.5, 0.2, 0.25, 0.75, 1.5, and -2.5 all fall through to line 305 and write `numerator/denominator`. The reader has no matching branch: `Scalar::rational` calls `decimal_to_rational`, which refuses any text that is not signed digits with at most one point (domain.rs:132), and `TryFrom<TermRepr> for Term` turns that `None` into the error `a lit term needs a whole number or a decimal string`. The write path and the read path therefore do not agree on the same grammar. The module header at constraint

### #9 [major] A decimal parameter value is rendered into the statement as a fraction, so a decimals template serves text the author never wrote and the reviewer never approved

File: `crates/core/src/template/domain.rs:106` — IDs: A1, C6

**Claim.** `Scalar::value` converts any text scalar that parses as a decimal into `Value::Num`, and `Value::canonical_string` writes a non-whole rational as `numerator/denominator`, so a choice value or rational parameter authored as `0.2` reaches the learner as `1/5` and no decimal can ever appear in a rendered statement.

**Evidence.**

```
domain.rs:106-111 `pub fn value(&self) -> Value { match self.rational() { Some(number) => Value::Num(number), None => Value::Text(self.text()) } }`, with domain.rs:164-169 `if number.denom().is_one() { number.numer().to_string() } else { format!("{}/{}", number.numer(), number.denom()) }`. Run, a `multiply-a-decimal` template with `"d": {"kind":"choice","values":["0.2","0.4","0.5","0.6","0.8","1.5"]}` and statement `Compute ${d} \times {n}$.`:
  GATE ACCEPTED Verified { space: Exact(48), instances_checked: 48, exhaustive: true }
    served statement = "Compute $1/2 \\times 8$."   expected = "4"
    served statement = "Compute $1/5 \\times 9$."   expected = "9/5"
    served statement = "Compute $4/5 \\times 2$."   expected = "8/5"
```

**Failure scenario.** An author writes a decimal-multiplication knowledge point, using the decimal-as-string scalar that domain.rs:71-79 exists to provide ("A decimal is written as a string, and `Scalar::rational` reads it exactly"). The gate accepts all 48 instances, and a reviewer approves a body whose choice list reads `"0.2", "0.4", "0.5"`. The pool row then carries `Compute $1/5 \times 9$.`, so the learner is asked a fraction question on a decimals knowledge point, the approved digest covers text that is never served, and `Scalar::text()` (used by the gate's own `scalar_repr` rejection messages) and `Scalar::value().canonical_string()` (used by the renderer and by `PoolProblem::bindings`) disagree on the spelling of the same parameter value.

**Refuter.** The core defect is demonstrable and I found no ruling that permits it. `Scalar::value` (crates/core/src/template/domain.rs:106-111) throws the authored spelling away: it calls `Scalar::rational`, and `decimal_to_rational` (domain.rs:119-143) accepts every signed decimal, so the text scalar `"0.2"` becomes `Value::Num(1/5)`. The renderer writes `value.canonical_string()` (crates/core/src/template/render.rs:101), and `canonical_string` (domain.rs:161-172) writes a non-whole rational as `numerator/denominator`. A choice value authored as `0.2` therefore reaches the learner as `1/5`, and `PoolProblem::bindings` (crates/core/src/pool/row.rs:217) records the same substituted spelling.

I ran the reviewer's template through `gate_body` plus `Compiled::draw`. The gate accepted it (`Verified { space: Exact(48), instances_checked: 48, exhaustive: true }`) and the drawn statements read `Compute $1/

### #12 [major] build_walk stops the whole sampled walk at the first draw that gives up, so the gate reports a false empty space and verifies a small part of an accepted one

File: `crates/core/src/template/gate.rs:859` — IDs: A2, C4, A1 — duplicate of #7

**Claim.** In the sampled branch `build_walk` breaks the whole 4,096-tuple walk at the first `draw_satisfying` that spends its 1,000 rejections, so a low-hit-rate constraint makes the gate say the template has no instance while its own `space_size` counts instances, and makes an accepted template carry a verification of a small part of its space.

**Evidence.**

```
gate.rs:854-868:
```
for _ in 0..GATE_SAMPLES {
    match compiled.plan().draw_satisfying(&doc.constraints, &mut rng) {
        Ok(tuple) => tuples.push(tuple),
        Err(DrawError::NoSatisfyingTuple { .. }) => break,
```
`draw_satisfying` gives up after `MAX_REJECTIONS = 1_000` draws (draw.rs:181-189). One give-up ends the walk.

Run against `cadus_core::template::gate`, a = 4096..8192, b = 1..10, constraint `mod(a, 4096) == 0`, 40,970 declared tuples and 20 satisfying tuples (2 values of `a` times 10 values of `b`, worked by hand):
  space_size = Estimated { estimate: 20, samples: 4096, hits: 2 }
  gate => REJECTED [no-satisfying-tuple] the constraints refuse every tuple of the declared domains, so the template has no instance to serve
The same call therefore counts 20 tuples and reports zero.

Second run, a = 430..8170, b = 1..10, constraint `mod(a, 430) == 0`, 77,410 declared tuples and 190 satisfying tuples:
  gate => ACCEPTED space Estimated { estimate: 188, ... } instances_checked 21 exhaustive false
The gate read 21 of about 190 instances and said nothing about the shortfall.

Neither path has a test. template_gate.rs:664 and :700 are the only sampled-branch tests, and both declare no constraint, so every draw is a hit and the break never runs.
```

**Failure scenario.** An author writes a template over a large domain with a selective constraint, such as `a` in 4096..8192 with `mod(a, 4096) == 0`. The gate refuses it with `the constraints refuse every tuple of the declared domains, so the template has no instance to serve`, while the same gate call computes `space_size` as 20. The message is false, and the author has nothing to act on (A2). With a less selective constraint the gate accepts instead, after it reads 21 of about 190 instances. The instance walk is the check that catches an answer expression that goes wrong on part of its space — 1.0 records the live case at `problem_templates.py:432-437`, where a 10,000-instance subtraction template was accepted on 22 of 60 seeds with 55 negative instances. A 21-instance walk over a 190-instance space reproduces that class of miss, and `Verified.instances_checked` is the only record of it.

**Refuter.** The claim is demonstrable, and I reproduced both runs byte-for-byte against the built code. `build_walk` (/home/deploy/dev/cadus2.0/crates/core/src/template/gate.rs:856-866) breaks the whole 4,096-draw loop on the first `DrawError::NoSatisfyingTuple`, and `draw_satisfying` (/home/deploy/dev/cadus2.0/crates/core/src/template/draw.rs:176-190) reports that error after 1,000 rejected draws. One give-up therefore ends the walk.

Prong 1 (false message, A2). With `a` in 4096..8192, `b` in 1..10, and the constraint `mod(a, 4096) == 0`, the hit rate is about 2/4097, so the first `draw_satisfying` spends its 1,000 draws with probability about 0.61. It did. `tuples` stayed empty, and `check_space` (gate.rs:876-882) returned the `no-satisfying-tuple` rejection. The same gate call counted the space as `Estimated { estimate: 20, samples: 4096, hits: 2 }` through `space_size` (/home/deploy/dev/cadus2.

### #13 [major] The answer writer's bracket rule is unverified: no test instantiates an expression-kind template

File: `crates/core/src/template/eval.rs:676` — IDs: C4, V2, A1

**Claim.** No test in the workspace evaluates an answer expression that needs a bracket, so a build with the bracket rule removed passes the whole suite and writes an expected answer of a different value.

**Evidence.**

```
eval.rs:675-685 `write_at`: `let bracket = level(node) < need;`. It is the only rule that keeps a sum inside a product or a power, and it runs only for an `expression` answer, because a numeric answer folds to one literal.

I replaced the line with `let bracket = false;` in a copy of the tree and ran `CADUS_BENCH=1 CADUS_TEST_DATABASE_URL=... cargo test --workspace` against the live Postgres: 49 `test result` lines, 0 failed. The mutant survives the whole workspace, the 20-template benchmark fixture included.

The reason is a coverage gap. `AnswerKind::Expression` appears twice in the M4 test files (template_gate.rs:447 and :551), and both uses are name-collision rejections that never reach the evaluator. The two expression fixtures write answers that need no bracket: `09-collect-like-terms.json` (`(a + b)*x` folds to `12*x`) and `10-distribute-over-a-sum.json` (`a*x + a*b` folds to `2*x + 20`). bench_l1.rs:400-408 only counts them: `assert_eq!(expression, 2, "two fixtures answer an expression")`.

Demonstration, run on the unmutated tree and on the mutant, with `answer_expr` `a*(x + b)` and a = 7, b = 1:
  unmutated: statement `Expand $7(x + 1)$.`  answer `7*(1 + x)`  canon Poly{const 7, x 7}
  mutant   : statement `Expand $7(x + 1)$.`  answer `7*1 + x`    canon Poly{const 7, x 1}
```

**Failure scenario.** A regression, or a later change to `level`, drops the bracket for a sum under a product. The template `Expand ${a}(x + {b})$.` with `answer_expr` `a*(x + b)` then writes the expected answer `7*1 + x` for the tuple a = 7, b = 1. That string stays inside the M2 grammar and canonicalizes, so `EvalError::NotCanonical` never fires and every V2 check passes. The pool row carries `x + 7` as the expected answer of a problem whose answer is `7x + 7`, and the M5 grade path marks the correct learner wrong on every instance of the template. The whole test suite stays green, so nothing reports the regression. The only net left is the sample-agreement row of the gate, and it speaks only for a template of this shape, of which the tree holds none.

**Refuter.** The claim is demonstrable, and I reproduced it independently. I copied the tree outside the repo, replaced line 676 of crates/core/src/template/eval.rs with `let bracket = false && level(node) < need;`, and ran the full `cadus-core` test set (every M4 core test file: template.rs, template_gate.rs, pool.rs, bench_l1.rs, plus all M0-M3 files). The result was identical to the unmutated baseline: 23 `test result` lines, 0 failed. The mutant is not equivalent: with the same tree I evaluated `a*(x + b)` for a = 7, b = 1 and got `7*(1 + x)` (canon Poly const 7, x 7) unmutated against `7*1 + x` (canon Poly const 7, x 1) mutated. Related shapes break the same way: `(x + a)/b` writes `7 + x/1`, and `-(x + a)` writes `-7 + x`. Each wrong string stays inside the M2 grammar and canonicalizes, so `EvalError::NotCanonical` never fires. The cause is the coverage gap the reviewer names. Every `answer_exp

### #14 [major] The exact square root never meets a radicand that is not a whole number, so its denominator test is unverified

File: `crates/core/src/template/eval.rs:577` — IDs: C4, V2, D6

**Claim.** Every `sqrt` case in the workspace binds a whole number, so the denominator half of the perfect-square test is unverified; a build that drops it passes the whole suite and answers `2` for `sqrt(4/3)`.

**Evidence.**

```
eval.rs:575-582 `square_root`:
```
let numerator = value.numer().sqrt();
let denominator = value.denom().sqrt();
if &(&numerator * &numerator) == value.numer()
    && &(&denominator * &denominator) == value.denom()
{
    return literal(BigRational::new(numerator, denominator));
}
```
I removed the second condition in a copy of the tree and ran `CADUS_BENCH=1 CADUS_TEST_DATABASE_URL=... cargo test --workspace` against the live Postgres: 49 `test result` lines, 0 failed. The mutant survives.

The reason is a coverage gap. The evaluator cases at template.rs:699-701 are `("sqrt(a)", a = 49, "7")` and `("sqrt(a)", a = 8, "sqrt(8)")`; both bind whole numbers, so the denominator is 1 and 1 * 1 == 1 under either version. The one sqrt fixture, `crates/core/tests/fixtures/templates/19-square-roots.json`, declares a choice domain of the twelve perfect squares 4..169.

Demonstration, run on the unmutated tree and on the mutant, `answer_expr` `sqrt(4/a)` over a in 1..12:
  unmutated: a=1 -> `2`   a=3 -> `sqrt(4/3)`  a=9 -> `2/3`  a=12 -> `sqrt(1/3)`
  mutant   : a=1 -> `2`   a=3 -> `2`          a=9 -> `2/3`  a=12 -> `1`
```

**Failure scenario.** A knowledge point about the square root of a fraction stores a template with `answer_expr` `sqrt(4/a)` and a rational or integer parameter, a shape the rational domain of 2.0 exists for. A regression that drops the denominator test writes `2` as the expected answer of the instance whose answer is `2/sqrt(3)`, about 1.1547, and `1` for the instance whose answer is `1/sqrt(3)`. Both wrong strings are whole numbers, so they canonicalize, pass V2, and reach `serving_pool.expected_answer`. Every learner who answers correctly is graded wrong. No test in the workspace fails, because no test and no fixture ever takes the square root of a value that is not a whole number.

**Refuter.** The claim holds. I tried to refute it and I failed on the load-bearing part.

1. The coverage gap is real. A grep of every `sqrt` in the workspace shows that each radicand that reaches the template evaluator is a whole number. The two evaluator cases at crates/core/tests/template.rs:701-702 bind a = 49 and a = 8. The compile fixture at crates/core/tests/template.rs:354-357 binds an int domain 1..50. The one sqrt fixture, crates/core/tests/fixtures/templates/19-square-roots.json, lists the twelve perfect squares 4..169. For a whole radicand the denominator is 1, so the second condition is always true, and for a non-square whole radicand the first condition short-circuits. The `sqrt` cases in crates/core/tests/answer_parse.rs and answer_divergence.rs, `sqrt(4/9)` included, go through crates/core/src/answer/canon.rs, not through eval.rs; `square_root` has exactly two callers, both in eval.r

### #16 [major] Above the exhaustive limit the gate reads the DECLARED axis ends, so a constrained template can never be approved

File: `crates/core/src/template/gate.rs:1032` — IDs: A1

**Claim.** axis_extremes reads the satisfying tuples only when walk.exhaustive is true; above EXHAUSTIVE_SPACE_LIMIT it falls back to the declared value list, so check_coverage demands a worked sample at a value the constraints forbid while check_coverage's own sample-constraint rule rejects that sample — no author input passes.

**Evidence.**

```
crates/core/src/template/gate.rs:1032-1039:
```rust
let seen: Vec<Value> = if walk.exhaustive {
    walk.tuples.iter().filter_map(|tuple| tuple.get(name).cloned()).collect()
} else {
    values.get(name).cloned().unwrap_or_default()
};
```
serving-1.0-spec.md section 8, trap 11: "Re-derive the crossed-corner requirement from the *satisfying* tuples, not from the declared ends, or the check silently weakens."

Run against the shipped crate, one template shape at two sizes:
```
60x60  (3600 <= 4096): Ok(Exact(1770))
100x100 satisfying ends: Some(("edge-coverage", "no worked sample uses the low end of a (1) — the edges are where an expression stops being right"))
100x100 with a=1 sample: Some(("sample-constraint", "sample 0 binds {'a': 1, 'b': 1}, which the gt constraint refuses — a sample outside the constraints verifies nothing"))
100x100 with b=100 sample: Some(("sample-constraint", "sample 1 binds {'a': 100, 'b': 100}, which the gt constraint refuses — a sample outside the constraints verifies nothing"))
```
```

**Failure scenario.** Take the A1 flagship shape: `params a: int 1..100, b: int 1..100`, `constraints [{"op":"gt","left":"a","right":"b"}]`, `answer_expr "a - b"`. 10,000 declared tuples is above the limit, so the gate reads a's declared low end 1 and demands a sample that binds a = 1. Every such sample violates `a > b`, because b is at least 1, so the sample-constraint rule refuses it. The same document with high 60 (3,600 tuples) is accepted with Exact(1770). Any constrained template above 4,096 declared tuples whose constraints exclude a declared end is permanently unapprovable, and A1 names inter-parameter constraints as the feature 1.0 lacked.

**Refuter.** The claim is demonstrable against the shipped crate. I built a probe crate with a path dependency on crates/core and ran the A1 flagship shape (params a, b int 1..N, constraint gt a b, answer_expr "a - b") at four sizes. At or under 4,096 declared tuples the gate accepts the document. Above 4,096 declared tuples the same shape is unapprovable: no sample set passes.

The mechanism is the one the reviewer names. build_walk (crates/core/src/template/gate.rs:821-869) sets exhaustive = true only under EXHAUSTIVE_SPACE_LIMIT = 4_096. axis_extremes (gate.rs:1032-1039) then reads the satisfying tuples only under that flag, and above the limit it reads values.get(name), the DECLARED value list. check_coverage (gate.rs:1109-1122) demands a worked sample at each declared end, and check_coverage's own sample-constraint rule (gate.rs:1088-1101) refuses every sample the constraints exclude. For a > b 

### #17 [major] The answer writer omits the brackets around a nested power, so an expression answer leaves the decidable grammar and every instance of the template is refused

File: `crates/core/src/template/eval.rs:725` — IDs: V2, C4, A2

**Claim.** Prec has no level above Power, so level(Ast::Pow) is Prec::Power and write_at(base, Prec::Power) never brackets a power that is itself the base of a power; write() turns Pow(Pow(x,2),3) into `x**2**3`, which the M2 parser refuses as "a tower of powers", so every instance of such a template fails canonicalization and the gate blames the instance answer instead of the writer.

**Evidence.**

```
eval.rs:670 `_ => Prec::Power,` gives Ast::Pow the top level; eval.rs:725-726 `write_at(base, Prec::Power, out)?;` then `out.push_str("**")`; write_at brackets only when `level(node) < need`, and Power < Power is false. Measured:
  parsed "(x**2)**3" -> Pow(Pow(Var("x"), 2), 3)
    write -> "x**2**3"  canon=Err(Undecidable { reason: "a tower of powers" })
A 352,102-case round-trip fuzz of `canon(evaluate(ast, b))` against `canonical_form(write(evaluate(ast, b)))` over random expressions reported: compared 352102 bad 2273 tower 2273 differs 0 - every failure is this one bracket rule, and no case writes a string that reads back as a different value. End to end, an expression template with answer_expr `a*(x**2)**3`:
  REJECTED [sample-eval] answer_expr failed on sample {'a': 2}: the answer "2*x**2**3" does not canonicalize: undecidable answer: a tower of powers
```

**Failure scenario.** An author writes an expression template whose answer keeps a power of a power: `answer_kind: expression`, statement `Simplify ${a} \left(x^{{2}}\right)^{{3}}$.`, `params: {"a": {"kind": "int", "low": 2, "high": 14}}`, answer_expr `a*(x**2)**3`, samples a=2 -> 2*x**6 and a=14 -> 14*x**6. parse_answer_expr accepts the source (the parser refuses `x**2**3` but not `(x**2)**3`). evaluate leaves Pow(Pow(Var("x"),2),3) in the tree, write() drops the brackets and produces `2*x**2**3`, canonical_form refuses it, and the gate rejects the document with a message that names the written answer, not the writer. No edit to answer_expr short of pre-simplifying the power by hand makes the document approvable.

**Refuter.** The claim reproduces exactly, end to end, and no ruling protects the present behavior. `level()` at /home/deploy/dev/cadus2.0/crates/core/src/template/eval.rs:670 gives `Ast::Pow` the top level `Prec::Power`. The `Ast::Pow` arm of `write_bare` at eval.rs:725-726 asks for `Prec::Power` on the base, and `write_at` brackets only when `level(node) < need`, so a `Pow` base never takes brackets. `parse_answer_expr` accepts `(x**2)**3` (the tower guard in /home/deploy/dev/cadus2.0/crates/core/src/answer/parse.rs:761 fires on `**` after an exponent, and the brackets keep it silent). `evaluate` holds the nested node: the `Ast::Pow` arm at eval.rs:258-263 folds only when the base evaluates to a number, and a free variable base stays a `Pow`. `write` then drops the brackets, and `canonical_form` refuses the string it wrote.

Two points of the report are stronger than the report states, and one is w

### #18 [major] The renderer splices a non-atomic parameter value into the statement without brackets while the answer evaluator brackets it, so the printed problem asks a different question than the stored answer answers

File: `crates/core/src/template/render.rs:101` — IDs: C4, A1, D6, V2

**Claim.** render() writes Value::canonical_string() raw into the statement, so a rational parameter enters as `3/2` and a negative integer as `-3`; the evaluator gives those same values Prec::Sum and brackets them (eval.rs:662-666), so the answer is computed from `(3/2)**2` while the learner reads `$3/2^{2}$`, which is 3/(2^2).

**Evidence.**

```
render.rs:99-102 — `let Some(value) = bindings.get(&name) else { ... }; out.push_str(&value.canonical_string());` (no bracketing, no atomicity rule).
eval.rs:662-666 — `Ast::Integer(value) if value.is_negative() => Prec::Sum, ... Ast::Fraction { numerator, .. } if numerator.is_negative() => Prec::Sum,` and `Ast::Fraction { .. } | Ast::Mul(_) | Ast::Div(_, _) => Prec::Product,` so a power brackets both.

$ cargo run (rv probe, path dep on crates/core)
=== C2 rational squared   (params a: rational num 1..13 over den 2..2, answer_expr "a**2", samples 0.5->0.25 and 6.5->42.25)
  ACCEPTED space=Exact(13) instances=13 exhaustive=true
    a=3/2: stmt="Compute $3/2^{2}$." answer="9/4"
    a=1/2: stmt="Compute $1/2^{2}$." answer="1/4"
=== C1 negative domain squared   (params a: int -12..12, answer_expr "a**2")
  ACCEPTED space=Exact(25) instances=25 exhaustive=true
    a=-3: stmt="Compute $-3^{2}$." answer="9"
    drawn stmt="Compute $-9^{2}$." ans="81"
```

**Failure scenario.** A fractions knowledge point declares `a` as a rational domain (a 2.0-only domain kind, domain.rs:1-21) and `answer_expr = "a**2"`. The gate walks all 13 satisfying tuples, every instance renders, canonicalizes, and lies inside the exemplar envelope, so the document is accepted and approved. The refill writes `Compute $3/2^{2}$.` with expected_answer `9/4`. KaTeX renders that as 3/2^2; a learner who reads the printed problem correctly answers `3/4` and is graded wrong, while the learner who guesses the author's intent answers `9/4`. The same splice makes `Compute $-3^{2}$.` expect `9` where the printed statement reads -9. The renderer has an explicit LaTeX grammar rule already (the doubled brace, render.rs:14-21) and the evaluator has an explicit bracketing rule for exactly these two node shapes; only the statement side is missing one, so the two halves of one instance disagree by construction.

**Refuter.** The claim holds. I reproduced it end to end and found no rule that stops it.

1. The renderer splices the raw canonical string. `/home/deploy/dev/cadus2.0/crates/core/src/template/render.rs:99-102` writes `out.push_str(&value.canonical_string())`. `Value::canonical_string` (`crates/core/src/template/domain.rs:161-173`) writes `numerator/denominator` for a fractional rational and a leading `-` for a negative whole number. The renderer adds no brackets and applies no atomicity rule. The module comment (`render.rs:29-32`) states only that the value never goes through a number formatter.

2. The evaluator applies the opposite rule on the answer side. `crates/core/src/template/eval.rs:655-670`: `level()` gives `Ast::Integer` with a negative value, `Ast::Decimal` with a negative mantissa, and `Ast::Fraction` with a negative numerator the level `Prec::Sum`, and gives every `Ast::Fraction` the l

### #22 [major] The crossed-corner rule is unsatisfiable for a band-constrained pair, so no such template can ever be approved

File: `crates/core/src/template/gate.rs:1154` — IDs: A1

**Claim.** check_crossed_corners demands a worked sample at (low_l, high_r) or (high_l, low_r) computed from the per-axis extremes of the satisfying walk, but with a two-sided (band) constraint neither of those two tuples satisfies the constraints, and check_coverage's sample-constraint rule then refuses whichever one the author writes, so the two rules deadlock and the document can never pass the gate.

**Evidence.**

```
gate.rs:1154 `(bound_l == low_l && bound_r == high_r) || (bound_l == high_l && bound_r == low_r)` — the crossed pair is built from the two axes' extremes independently, and no check asks whether any satisfying tuple realizes either pairing. Live gate runs on the band template `a,b in 1..10` with `gt(a,b)` and `lt(a, b+3)` (17 satisfying tuples, above the floor of 12):
  E2-ends-covered: REJECTED [crossed-corner] no worked sample crosses a and b — one of them at its low end WITH the other at its high end (a=2 with b=9, or a=10 with b=1). ...
  E2-corner-a-low-b-high: REJECTED [sample-constraint] sample 4 binds {'a': 2, 'b': 9}, which the gt constraint refuses — a sample outside the constraints verifies nothing
  E2-corner-a-high-b-low: REJECTED [sample-constraint] sample 4 binds {'a': 10, 'b': 1}, which the lt constraint refuses — a sample outside the constraints verifies nothing
```

**Failure scenario.** An author writes the natural A1 template for "subtract two numbers whose difference is 1 or 2": params a,b in 1..10, constraints gt(a,b) and lt(a, b+3), answer_expr `a - b`, samples covering every axis end (a=2 with b=1, a=10 with b=9, a=3 with b=1, a=10 with b=8). The gate refuses with the crossed-corner message and names the two tuples it wants. Adding either named tuple produces a sample-constraint refusal. There is no sample set that satisfies both rules, so the template is permanently unapprovable and the knowledge point falls back to A6 exemplar rotation forever. The gate's 20 committed fixtures never hit this: every constrained fixture uses a one-sided constraint (gt, divides, coprime, carries, eq on digit_sum) whose high/low corner is realizable, so no test in crates/core/tests/template_gate.rs or bench_l1.rs covers a two-sided constraint.

**Refuter.** I cannot refute the claim. The gate refuses the natural A1 band template, and it names two tuples that its own sample-constraint rule then refuses. The two rules deadlock, and no sample set clears both. This is not one bad template: for any two-sided constraint of the form b < a < b+k, the crossed corners built from the two axis extremes are unsatisfiable whenever the satisfying space is large enough to pass the floor of 12, so the whole class is permanently unapprovable and falls back to A6 exemplars. A1 names inter-parameter constraints as the feature whose absence blocked 1.0 templates, and spec trap 11 tells the gate to derive the crossed-corner requirement from the satisfying tuples. axis_extremes does that for each axis alone; check_crossed_corners then re-crosses the two axes as an independent product and never asks whether a satisfying tuple realizes either pairing. The correct r

## FIXM4b

### #1 [blocker] Above the exhaustive limit the gate checks a fixed 4,096-tuple sample while the refill serves a different draw, so no per-instance rule (envelope, non-answer, hint give-away) ever runs on the instance that reaches the pool

File: `crates/core/src/pool/source.rs:240` — IDs: C4, C6, A1

**Claim.** For a template whose declared space exceeds EXHAUSTIVE_SPACE_LIMIT, `check_instances` verifies only the 4,096 tuples `build_walk` draws from the constant `GATE_SEED`, while `TemplateSource::fill` draws from the batch seed and pushes the resulting instance into the pool with no check at all, so an instance the gate would have refused is served.

**Evidence.**

```
gate.rs:855-859 (the checked set is a fixed sample): `let mut rng = super::draw::rng_from_seed(GATE_SEED); let mut tuples = Vec::with_capacity(GATE_SAMPLES as usize);` — and source.rs:240 (the served set is unchecked): `Ok(instance) => { if seen.insert(instance.instance_hash.clone()) { out.push(instance); ... } }`. Run, template `Compute $9999 - {a} \times {b}$.` with `answer_expr` `9999 - a*b`, a,b int 1..100, exemplars `9987`/`9879`:
  envelope = Some(Envelope { non_negative: true, integral: true })
  GATE ACCEPTED: space=Estimated { estimate: 10000, samples: 4096, hits: 4096 } instances_checked=4096 exhaustive=false
  SERVED seed=1459 bindings={"a": 100, "b": 100}
    statement = "Compute $9999 - 100 \\times 100$."
    expected_answer = "-1"
The same defect is refused when the walk is exhaustive: shrinking to a,b int 1..64 with `3999 - a*b` (4,096 declared) gives `GATE REJECTED [envelope-sign] instance {'a': 63, 'b': 64} answers '-33', but every authored answer for this knowledge point is non-negative`.
```

**Failure scenario.** An author writes the 1..100 template above for a knowledge point whose exemplars are all non-negative whole numbers. `gate` accepts it (the single violating tuple a=100,b=100 is not among the 4,096 tuples drawn from GATE_SEED=0). The operator approves it (C6). `refill_target` calls `template_instances`, which re-runs `gate(&doc, &spec)` — the identical pure function, same GATE_SEED — accepts again and caches the verdict by digest (refill.rs:475-479). `TemplateSource::fill` then draws with `batch_seed(...)`; at seed 1459 it returns the instance `Compute $9999 - 100 \times 100$.` with `expected_answer` `-1`, and `insert` writes it to `serving_pool`. The learner is served a problem whose answer the knowledge point's own envelope rule declares impossible. This is the live 1.0 defect the spec records at line 306 (`a 10,000-instance subtraction template was accepted on 22 of 60 seeds with 55 negative instances`), and the refill module header at refill.rs:33-40 claims 2.0 `keeps that property` — it does not, because caching the verdict per digest means the gate never sees the tuple that is served.

**Refuter.** The claim reproduces exactly, and no code path refutes it. Above EXHAUSTIVE_SPACE_LIMIT (4,096), build_walk (crates/core/src/template/gate.rs:853-871) fills Walk.tuples with GATE_SAMPLES=4,096 tuples drawn from the constant GATE_SEED=0, and check_instances (gate.rs:1176-1250) — the only caller of check_envelope — iterates walk.tuples alone. TemplateSource::fill (crates/core/src/pool/source.rs:217-260) draws from the caller's batch seed and pushes every instance that compiled.instantiate returns; the envelope check, the not-a-number check, the free-symbol check, and the hint check exist only inside gate.rs. A grep over the non-test tree shows exactly one gate() call site outside tests: crates/worker/src/refill.rs:475, and it gates the DOCUMENT once per digest and caches the verdict in RefillState (refill.rs:453-486). refill_target then calls source.fill(...) and hands the result straight 

### #2 [blocker] The refill path never re-runs the per-instance gate checks, so an instance the gate would refuse reaches the pool

File: `crates/core/src/pool/source.rs:242` — IDs: C6, A1, D-O4 — duplicate of #1

**Claim.** Above EXHAUSTIVE_SPACE_LIMIT the gate verifies only 4,096 sampled tuples, and TemplateSource::fill inserts every candidate that renders and evaluates without re-running the exemplar-envelope, hint-give-away, non-answer-token, or numeric free-symbol checks, so an instance the gate itself would refuse becomes a serving_pool row.

**Evidence.**

```
crates/core/src/pool/source.rs:233-247 is the whole per-instance check at fill time:
    match self.compiled.instantiate(bindings) {
        Err(error) => refusal = Some(error),
        Ok(instance) => { if seen.insert(instance.instance_hash.clone()) { out.push(instance); ... } }
`Compiled::instantiate` (crates/core/src/template/document.rs:232-248) only renders, evaluates and canonicalizes. `check_envelope` (gate.rs:1279) and `check_hints` (gate.rs:1325) live in `check_instances` (gate.rs:1178) and run over `walk.tuples` alone. `TemplateSource` holds no exemplars and no `GateSpec`, so the envelope cannot run there at all. crates/worker/src/refill.rs:365-380 calls `source.fill(...)` and passes the result straight to `insert`.
The module doc at crates/worker/src/refill.rs:33-40 claims the opposite: "1.0 re-runs its whole check set at serve time ... a 10,000-instance subtraction template was accepted on 22 of 60 seeds with 55 negative instances. 2.0 keeps that property". It does not: the only thing that runs again is `gate(&doc, &spec)` (refill.rs:475), and `build_walk` seeds from the constant `GATE_SEED = 0` (gate.rs:84, gate.rs:854), so the re-gate reproduces the authoring verdict bit for bit and learns nothing about the tuples `fill` actually drew.
Demonstrated with a scratch binary linking cadus-core:
  envelope = Some(Envelope { non_negative: true, integral: true })
  GATE ACCEPTED: space=Estimated { estimate: 250500, samples: 4096, hits: 4096 } checked=4096 exhaustive=false
  seed 2421: pool row text="Compute $499 - 500$." expected_answer="-1" bindings={"a": 499, "b": 500}
  seed 4469: pool row text="Compute $499 - 500$." expected_answer="-1" ...
  seed 24261: pool row text="Compute $499 - 500$." expected_answer="-1" ...
```

**Failure scenario.** KP `subtraction-borrowing` has one exemplar, `Compute $52 - 27$.` answering `25`, so `exemplar_envelope` yields `{non_negative: true, integral: true}`. An approved template declares statement `Compute ${a} - {b}$.`, `a` int 499..999, `b` int 1..500 (250,500 declared tuples, above the 4,096 limit), `answer_expr` `a - b`, and three samples that satisfy every coverage rule ((499,1), (999,500), (999,1)) and are all non-negative. `gate` accepts: its 4,096 draws from GATE_SEED=0 miss the single negative tuple (a=499, b=500). The worker's `refill_target` computes `batch_seed(...)`; for batch seeds 2421, 4469 and 24261 `TemplateSource::fill` draws exactly that tuple and inserts a pool row with text `Compute $499 - 500$.` and `expected_answer = -1`. The gate would have refused that instance with `[envelope-sign] instance {'a': 499, 'b': 500} answers '-1', but every authored answer for this knowledge point is non-negative`. This is the 1.0 regression `problem_templates.py:432-437` records verbatim (`Compute $90 - 91$` -> `-1`), reintroduced. The same gap lets a hint rung that names the answer of an unwalked instance reach the learner.

**Refuter.** The claim is correct, and I reproduced it independently with my own fixture. Above EXHAUSTIVE_SPACE_LIMIT the gate walks only GATE_SAMPLES = 4,096 tuples drawn from the constant GATE_SEED = 0 (crates/core/src/template/gate.rs:78, :84, :846-871). The envelope check, the hint give-away check, the non-answer-token check, and the numeric free-symbol check live only in check_instances (gate.rs:1178-1252), and check_instances reads walk.tuples alone. TemplateSource::fill (crates/core/src/pool/source.rs:218-256) draws its own tuples from the batch seed and calls Compiled::instantiate, which renders, evaluates, canonicalizes, and hashes and nothing more (crates/core/src/template/document.rs:232-248). The source holds no exemplars and no GateSpec, so the envelope cannot run there. A grep over crates/ finds no other caller of check_envelope or exemplar_envelope outside gate.rs, so no later stage (

### #3 [blocker] The refill target list is starved forever by pairs that can never fill

File: `crates/store/src/pool.rs:477` — IDs: D-O4, A6, A5

**Claim.** `refill_targets` orders by ascending unclaimed depth and takes the first `targets_per_tick` pairs, so every `(user, kp)` pair that can never gain a row sits permanently at depth 0 at the head of the list and consumes the whole per-tick budget on every tick.

**Evidence.**

```
crates/store/src/pool.rs:476-478:
        HAVING count(*) FILTER (WHERE claimed_at IS NULL) < $1
        ORDER BY "depth!", user_id, kp_id
        LIMIT $2
Nothing excludes a pair the previous pass reported as `Filled::NoSource` (crates/worker/src/refill.rs:403-409) or as `failed`. Demonstration (scratchpad copy of the repo, `crates/worker/tests/rv_demo.rs::unfillable_pairs_starve_every_other_pair`, passes): 32 drained pairs whose serving keys the curriculum does not name, plus one pair with an approved template at depth 0, `RefillConfig { target_depth: 24, targets_per_tick: 32, base_seed: 0 }`. Five consecutive passes with nonces 1..5 each report `targets = 32, without_source = 32, inserted = 0`; `refill_targets(&db.admin, 24, 32)` never lists `perfect-squares/kp1`, and its unclaimed depth stays 0.
```

**Failure scenario.** A deployment has 11 learners and 3 knowledge points that hold exemplars but no approved template (the normal A6 state, and the state every exemplar-backed pair reaches permanently — see the write-once exemplar finding). That is 33 pairs pinned at depth 0. From the first tick on, all 32 slots of every refill tick go to those pairs, `inserted` is 0, and no learner's templated pool is ever filled again. Every serve is then a pool miss, which is exactly the path A6 forbids as the normal path.

**Refuter.** The claim holds. I did not refute it. Three code facts make the starvation real.

1. The target query keeps a fully drained pair and puts it first. `refill_targets` (crates/store/src/pool.rs:463-491) groups `serving_pool` by `(user_id, kp_id)`, keeps a group with `count(*) FILTER (WHERE claimed_at IS NULL) < $1`, and orders by `"depth!", user_id, kp_id` with `LIMIT $2`. A pair whose rows are all claimed has depth 0. 0 < 24 is true, so the pair stays in the list, and depth 0 sorts before every other depth. The order is deterministic, so the same head set comes back on every tick. Nothing in the query, in `refill_once` (crates/worker/src/refill.rs:277-333), or in `RefillState` (crates/worker/src/refill.rs:167-172, which holds only `accepted` and `refused` digests) excludes a pair the previous pass reported as `Filled::NoSource` or as `failed`, and no cursor or rotation moves the window.

2

### #5 [blocker] A curriculum that does not load turns the WHOLE refill off, and the shipped image ships no curriculum

File: `crates/worker/src/bin/cadus-worker.rs:130` — IDs: D-O4, A6, D-O1

**Claim.** `load_arena()` returning `None` is passed straight through as `RefillJob = None`, and `run_with` then skips the refill branch entirely, so a worker without a curriculum refills nothing at all — not even knowledge points that have an approved template — and the shipped container has no curriculum tree, so this is the default state of the deployment.

**Evidence.**

```
crates/worker/src/bin/cadus-worker.rs:125-132 documents the opposite ("the worker still refills every knowledge point that has an approved template") and then writes `let job = curriculum.as_ref().map(RefillJob::new);` — `RefillJob::templates_only`, which exists for exactly this case, is dead code (`grep -rn templates_only crates/` returns only its own definition at refill.rs:126). crates/worker/src/lib.rs:221 `let Some(job) = refill else { continue; };`. Dockerfile:65-67 copies only the three binaries into `WORKDIR /app`; docker-compose.yml:174-188 sets no `CADUS_CURRICULUM` and mounts no volume, so `load_curriculum("curriculum")` hits crates/core/src/curriculum/load.rs:105-114 `ParseError::CurriculumNotFound`. Live run against a seeded database (approved template `perfect-squares/kp1`, drained pair, depth 0):
  CADUS_CURRICULUM=/nonexistent -> "WARN cadus-worker: the curriculum did not load" then "heartbeat tick=1..6" and NOT ONE "refill tick" line; `SELECT count(*) ... claimed_at IS NULL` = 0 after 6 ticks.
  Same binary, same database, CADUS_CURRICULUM pointed at the fixture tree -> "refill tick=1 targets=1 inserted=12 from_template=12".
```

**Failure scenario.** Bring the stack up with `docker compose up` exactly as docs/SELF_HOST.md describes. `cadus-worker` logs one WARN line, then heartbeats forever and inserts zero rows into `serving_pool`. Every knowledge point with an approved template is never filled, so every D-O1 pop returns `Ok(None)` and every serve is a pool miss. The only signal is a single warn line at boot that names the A6 fallback, not the refill.

**Refuter.** The claim is demonstrable from the source and I cannot refute it. cadus-worker.rs:130 maps a failed curriculum load to a None RefillJob, and lib.rs:221 uses `let Some(job) = refill else { continue; }` to skip the entire D-O4 refill pass before refill_once ever runs. The refill does not need the curriculum to fill from an approved template: refill_target step 1 reads approved_template() from the database, and the module comment at refill.rs:42-45 states that a knowledge point the curriculum does not name is filled on its C6 approval alone. The curriculum supplies only the GateSpec and the A6 exemplars. RefillJob::templates_only (refill.rs:126) exists for exactly this case and is dead code. The doc comment at cadus-worker.rs:125-127 asserts the opposite of the code. The deployment evidence also holds: the runtime stage of the Dockerfile copies three binaries into WORKDIR /app and no curric

### #6 [blocker] A curriculum that does not load turns the whole D-O4 refill off, and the shipped image never carries one

File: `crates/worker/src/bin/cadus-worker.rs:130` — IDs: D-O4, A1, A6, A7 — duplicate of #5

**Claim.** `load_arena()` returns `None` on any curriculum load failure and the binary then passes `None` as the refill job, so `run_with` skips the refill branch entirely; the deployed image contains no `curriculum/` tree and the compose worker sets no `CADUS_CURRICULUM` and mounts no volume, so the serving pool is never filled in the shipped stack even for knowledge points that have a C6-approved template.

**Evidence.**

```
crates/worker/src/bin/cadus-worker.rs:125-130 promises the opposite of what it does:
```
    // A tree that does not load is news, not a fatal error: the worker still
    // refills every knowledge point that has an approved template, and it says
    // in the log that the fallback is off.
    let curriculum = load_arena();
    let job = curriculum.as_ref().map(RefillJob::new);
```
crates/worker/src/lib.rs:221-223 is where the pass is dropped:
```
        let Some(job) = refill else {
            continue;
        };
```
`RefillJob::templates_only` (crates/worker/src/refill.rs:126), the constructor that would keep the template half alive, has no caller anywhere in the tree (`grep -rn 'templates_only' crates` returns only its definition).

Run against a migrated database seeded with one learner, one drained `(user, kp)` pair for `perfect-squares/kp1`, and one `content_store` row with `status='approved'` holding the perfect-squares template:

CASE A, CADUS_CURRICULUM=crates/worker/tests/fixtures/pool (valid), 4 ticks:
```
SELECT count(*) FROM serving_pool WHERE claimed_at IS NULL  ->  12
```
CASE B, same database reset to 0 unclaimed rows, CADUS_CURRICULUM=/home/deploy/dev/cadus2.0/no-such-curriculum, 4 ticks:
```
WARN cadus-worker: the curriculum did not load; the A6 exemplar fallback is off path=/home/deploy/dev/cadus2.0/no-such-curriculum error=no courses.yaml under /home/deploy/dev/cadus2.0/no-such-curriculum
INFO worker: loop starts tick_ms=1000
INFO heartbeat tick=1
INFO heartbeat tick=2
INFO heartbeat tick=3
INFO heartbeat tick=4
INFO worker: loop stops after 4 ticks
SELECT count(*) FROM serving_pool WHERE claimed_at IS NULL  ->  0
```
No `refill tick=` line is ever logged, and no row is inserted, although the approved template is present. The log line names only the A6 exemplar fallback.

The deployment lands in CASE B by default. Dockerfile:63-67 copies three binaries into `/app` and nothing else:
```
WORKDIR /app
COPY --from=builder /src/target/release/cadus-web /usr/local/bin/cadus-web
COPY --from=builder /src/target/release/cadus-worker /usr/local/bin/cadus-worker
COPY --from=builder /src/target/release/cadus-migrate /usr/local/bin/cadus-migrate
```
docker-compose.yml:174-188 (`worker`) sets DATABASE_URL, DB_STATEMENT_TIMEOUT_MS, DB_CLIENT_TIMEOUT_MS and RUST_LOG, declares no `volumes:`, and sets no `CADUS_CURRICULUM`, so `load_curriculum` runs on the relative default `curriculum` (crates/worker/src/bin/cadus-worker.rs:39) inside an empty `/app`.
```

**Failure scenario.** An operator brings the stack up with `docker compose up -d --build`. The worker logs `the curriculum did not load; the A6 exemplar fallback is off`, then heartbeats forever. `refill_once` is never called, so `serving_pool` stays empty for every learner and every knowledge point, including the ones whose templates a human already approved under C6. M5's serve path then takes the pool-miss branch on every single serve. The one log line an operator reads says the exemplar fallback is off; it does not say the template refill is off too.

**Refuter.** The claim holds. I found no way to refute it.

The refutation attempts and why each one failed:

1. "The comment is correct and the template refill survives a curriculum failure." It does not. `load_arena` gives `None`, `curriculum.as_ref().map(RefillJob::new)` gives `job = None`, and `run_with` skips the whole refill pass with `let Some(job) = refill else { continue; };`. The `continue` is after the heartbeat, so the worker heartbeats forever and calls `refill_once` zero times. The comment at cadus-worker.rs:125-127 says the opposite of the code under it.

2. "The template half needs the curriculum too, so the skip loses nothing." It does not need it. `refill_target` reads the approved body from `content_store` through `cadus_store::pool::approved_template`, and `template_instances` has an explicit `known = None` branch that accepts the document on its C6 approval alone. `RefillJob::tem

### #10 [major] The refill nonce is a process-local tick counter, so a worker restart replays the same seeds and inserts nothing

File: `crates/worker/src/lib.rs:227` — IDs: D-O4, A5, A6

**Claim.** `run_with` passes the tick counter as the batch nonce and the counter restarts at 0 with every worker process, so after a restart the refill redraws batches the pool already holds and `ON CONFLICT DO NOTHING` writes no row.

**Evidence.**

```
crates/worker/src/lib.rs:178 `let mut ticks: u64 = 0;` and :227 `result = refill::refill_once(db, job, &mut state, ticks)`. `batch_seed` is pure in `(base_seed, user_id, kp_id, nonce)` (crates/worker/src/refill.rs:268-272) and `base_seed` defaults to the constant 0 (:78). The project's own test pins the consequence — crates/worker/tests/refill.rs:447-452: "A repeat of one nonce inserts nothing: the seed decides the batch." The module doc claims the opposite of what the wiring gives (crates/worker/src/refill.rs:30-31): "The nonce changes every tick, so a second refill of one pair draws a different batch and the pool grows past the tuples it already holds." Demonstration (`crates/worker/tests/rv_demo.rs::a_restart_replays_the_tick_nonces_and_the_pool_never_refills`, passes): process A at nonce 1 inserts 8; the learner serves all 8; a fresh `RefillState` at nonce 1 (the restarted process) reports `inserted = 0` and the unclaimed depth stays 0; the same pair at nonce 99 inserts 3, so the template is not out of instances and the replayed seed is the cause.
```

**Failure scenario.** A learner is served continuously while worker process A runs, so A refills that pair at its ticks 1, 2, 3, … The operator deploys, and process B starts with `ticks = 0`. B's tick 1 draws `batch_seed(0, user, kp, 1)` — the batch A already wrote and the learner already consumed — and inserts 0 rows; B's tick 2 replays A's tick-2 batch, and so on for the whole window A had already walked. The pool of an active learner stalls at depth 0 instead of refilling, and every serve in that window is a pool miss. Two worker replicas have the same defect against each other: both count ticks from 0, so both draw identical batches and the second one always inserts 0.

**Refuter.** The claim holds. I checked each link of the chain and I found no guard at any link.

1. The nonce is a process-local counter. `crates/worker/src/lib.rs:178` declares `let mut ticks: u64 = 0;` inside `run_with`, and `:227` passes that same `ticks` to `refill::refill_once`. Nothing reads the counter back from the database, from a file, or from the environment, so a new process always starts the nonce sequence at 0.

2. The base seed is the constant 0 in every deployment. `RefillConfig::default` sets `base_seed: DEFAULT_BASE_SEED` (`crates/worker/src/refill.rs:78, 96`), and the binary builds the job with `curriculum.as_ref().map(RefillJob::new)` (`crates/worker/src/bin/cadus-worker.rs:130`), which takes that default. `grep` over the binary finds no `seed` string, so no operator flag and no environment variable changes it. Two processes therefore agree on the base seed.

3. The seed is pure 

### #15 [major] The refill drops the per-instance re-check 1.0 runs at serve time, so an instance the gate refuses reaches the pool

File: `crates/core/src/pool/source.rs:234` — IDs: C4, C6, A1 — duplicate of #1

**Claim.** TemplateSource::fill calls Compiled::instantiate, which runs render, evaluate, and canonicalize only; the gate's per-instance rules (exemplar envelope, NON_ANSWERS, decimal trailing zero, hint-answer) never run on the instances the refill inserts, and above EXHAUSTIVE_SPACE_LIMIT the gate reads a 4,096-tuple sample, so a bad corner survives into the pool.

**Evidence.**

```
crates/core/src/pool/source.rs:234 `match self.compiled.instantiate(bindings) {`, whose comment on lines 236-239 cites `problem_templates.py:399-403` — the very lines that define the re-check it dropped.

1.0, cadus_web/problem_templates.py:399-403:
> "Re-check the instance HERE, not only in the gate. The gate walks a small space exhaustively, but a large one it only samples, so a bad corner can survive into the table."
and `_build(template, bindings, spec.answer_kind, envelope)` re-applies the envelope per instance.

crates/worker/src/refill.rs:33-40 claims the opposite: "2.0 keeps that property and moves the cost off the request path". It caches one verdict per DIGEST (RefillState), not per instance.

Run against the shipped crate:
```
gate verdict: Ok(Verified { space: Estimated { estimate: 10000, samples: 4096, hits: 4096 }, instances_checked: 4096, exhaustive: false })
seeds in 0..4000 whose 24-row batch carries the -1 instance: 12
```
```

**Failure scenario.** A knowledge point whose every authored exemplar answers a non-negative whole number gets the template `{"statement": "Compute ${a} - 2$.", "params": {"a": {"kind":"int","low":1,"high":10000}}, "answer_expr": "a - 2"}` with the edge samples `a = 1 -> -1` and `a = 10000 -> 9998`. 10,000 declared tuples is above the exhaustive limit, so the gate draws 4,096 tuples from GATE_SEED 0, misses a = 1, and ACCEPTS — although the document's own worked sample states the answer -1. 12 of the 4,000 refill seeds tested draw a = 1, and insert_batch writes a pool row whose statement is `Compute $1 - 2$.` and whose expected_answer is `-1`. gate::check_envelope would have refused that instance with "but every authored answer for this knowledge point is non-negative". This is the 1.0 live incident serving-1.0-spec.md section 4 records (22 of 60 seeds, 55 negative instances).

**Refuter.** The claim is demonstrable and I reproduced it exactly against the shipped crate. Three facts hold together. (1) TemplateSource::fill (crates/core/src/pool/source.rs:234) calls Compiled::instantiate, and Compiled::instantiate (crates/core/src/template/document.rs:243) does render + evaluate + canonicalize + problem_text_hash and nothing else. The gate's per-instance rules live in check_instances (crates/core/src/template/gate.rs:1178) — placeholder-left, empty-answer, NON_ANSWERS/not-a-number, decimal-answer trailing zero, free-symbol, check_envelope, check_hints — and that function reads only walk.tuples. Nothing on the fill path calls it. A grep for check_envelope/exemplar_envelope across crates/ finds them only inside gate.rs and in crates/core/tests/template_gate.rs; no store or worker code re-checks an instance. (2) build_walk (gate.rs:820) enumerates every tuple only at or under EXH

### #19 [major] One undecodable pool row denies every serve for that pair, and is never retired

File: `crates/store/src/pool.rs:352` — IDs: D-O1, A6

**Claim.** `pop_with_ring_tx` decodes all up to 8 popped rows with `?` before the candidate rule runs, so a single `serving_pool` row whose `problem`/`expected_answer` document this build refuses aborts the whole pop with `StoreError::Body` — the seven servable rows popped alongside it are discarded, and the bad row is never claimed, so it poisons every later serve of that `(user, kp)` too.

**Evidence.**

```
crates/store/src/pool.rs:350-360 `for row in popped { candidates.push(read_row(...)?); }` — the `?` converts `PoolBodyError` through `StoreError::Body` (crates/store/src/lib.rs:215) and returns before `pick` at line 362. The decoder refuses on version and on any unknown field: crates/core/src/pool/row.rs:204-211 `check_version`, `#[serde(deny_unknown_fields)]` at row.rs:109 and row.rs:161; crates/core/tests/pool_row.rs:103-122 pins both refusals. crates/core/src/pool/row.rs:47-50 claims "A bump retires every unclaimed row" — nothing retires it. Demonstrated with the exact pop SELECT of pool.rs:330-342 against one `{"v":2,...}` row created one hour earlier plus seven good rows:
  instance_hash | doc_version
  poison        | 2      <- first candidate of every pop
  good-2 .. good-5 | 1    (7 rows)
and `count(*) FILTER (WHERE claimed_at IS NULL)` = 8, so the refill sees a full pair and adds nothing.
```

**Failure scenario.** Bump `POOL_ROW_VERSION` (its own doc comment presents this as the supported retirement path), or let any writer add a field to `problem` — e.g. the solution sketch M5 needs. Every unclaimed row written by the previous build stays in `serving_pool`. On the next serve the pop reads it as candidate 1 of 8, `read_row` returns `PoolBodyError`, and the serve fails with an error instead of serving one of the seven good rows behind it. The bad row is never claimed and still counts toward `unclaimed_depth`/`refill_targets`, so the pair stays broken until an operator deletes the row by hand. The module's own rule — "A repeat is a far smaller failure than no problem" — is inverted: the learner gets no problem at all.

**Refuter.** The claim holds. I found no defense for it.

1. The control flow is exactly as stated. `/home/deploy/dev/cadus2.0/crates/store/src/pool.rs:350-360` decodes every popped row in one loop with `?`, and `pick` runs first at line 362. One row that does not decode ends the function. The other seven popped rows go away with it, and the transaction claims nothing.

2. The refusal path is real and pinned. `PoolProblem` and `PoolAnswer` both carry `#[serde(deny_unknown_fields)]`, and `from_body` calls `check_version` (crates/core/src/pool/row.rs:152-155, 195-198, 204-211). The two refusals are pinned by tests in crates/core/tests/pool_row.rs (`an_unknown_field_is_refused`, `an_unknown_version_is_refused`). `read_row` turns `PoolBodyError` into `StoreError::Body` through `#[from]` at crates/store/src/lib.rs:215.

3. Nothing retires a stale row. A grep over `migrations/`, `crates/`, `scripts/`, and 

## FIXM4c

### #11 [major] Benchmark B seeds pool rows the U4 production pop refuses to decode

File: `crates/store/tests/bench_serve_roundtrip.rs:170` — IDs: L1, D-O1, D-S5

**Claim.** The 200 fixture rows of benchmark B carry a `problem` document with no `v` field and an extra `kind` field, and an `expected_answer` that is a bare JSON string, so `cadus_store::pool::pop_with_ring_tx` — the U4 pop the serve path calls — returns `StoreError::PoolRow` on every one of them; benchmark B never notices, because it writes its own SELECT and never decodes the two documents.

**Evidence.**

```
Fixture (bench_serve_roundtrip.rs:168-176 and :242):
    serde_json::json!({"text": text, "seed": 20_260_827_u64, "bindings": {"a": index + 1}, "kind": "numeric"})
    JsonValue::String(((index + 1) * (index + 1)).to_string()),
The production documents (crates/core/src/pool/row.rs:108-123, :162-168) are `#[serde(deny_unknown_fields)]`:
    pub struct PoolProblem { pub v: u32, pub text: String, pub bindings: BTreeMap<String,String>, pub seed: u64 }
    pub struct PoolAnswer  { pub v: u32, pub answer: String }
I seeded benchmark B's fixture verbatim into a throwaway database and ran both transactions against it:
    PART 1  benchmark B's inline SQL over benchmark B's fixture: OK, claimed 8554aa90-4723-422e-aa03-bd3b4a0eabf9
    PART 1  U4 pop_with_ring_tx over the same fixture: ERROR -- pool document error: unknown field `kind`, expected one of `v`, `text`, `bindings`, `seed` at line 1 column 7
The SELECT also diverges from crates/store/src/pool.rs:339 in two more places: it orders by `created_at` alone where the pop orders by `created_at, id`, and its claim is `WHERE id = $1` where the pop is `WHERE id = $1 AND claimed_at IS NULL` with a `rows_affected` check. The fixture gives every row a distinct `created_at` (:234), while `insert_batch` writes a whole batch in one statement and every row of it shares `created_at` (documented at crates/store/src/pool.rs:192-199), so the tie sort that every production pop performs is never measured.
```

**Failure scenario.** docs/reference/l1-budget.md:93-96 states that benchmark B measures "The D-O1 transaction of spec section 7.2". It does not: the 100 ms L1 segment is measured over rows that `pop_with_ring_tx` cannot read, through SQL that no production caller runs. A regression inside `pop_with_ring_tx` — a dropped `SKIP LOCKED`, a changed ORDER BY, an added round trip, a slow `PoolProblem::from_body` — moves no number in benchmark-b.json and fails no gate. Measured on a production-shaped pool written through `insert_batch`, the real pop costs p50 1,790,689 ns against benchmark B's 1,626,493 ns for the same fixture, so the reported number is not the number of the path that serves.

**Refuter.** I tried to refute the claim and failed. Every factual part of it holds in the code, and the decode failure reproduces.

1. The fixture is not the production row shape. `problem_document` (crates/store/tests/bench_serve_roundtrip.rs:168-176) writes `{"text", "seed", "bindings", "kind"}`. The production reader `cadus_core::pool::PoolProblem` (crates/core/src/pool/row.rs:107-123) is `#[serde(deny_unknown_fields)]`, requires `v`, and types `bindings` as `BTreeMap<String, String>`. The fixture has no `v`, adds `kind`, and writes the binding value as an integer. That is three separate mismatches, one more than the reviewer names. The `expected_answer` column gets `JsonValue::String(...)` (:242), a bare JSON string, where `PoolAnswer` (row.rs:160-168) is an object of `{v, answer}`.

2. The decode fails. I built a scratch crate against `cadus-core` and called the two production readers on the ex

### #20 [major] Benchmark A's L2 gate measures the string rung, so the checker's arithmetic is unmeasured in release

File: `crates/core/tests/bench_l1.rs:576` — IDs: L2, L1

**Claim.** The L2 half of benchmark A compares every corpus answer with itself, so `check` returns at rung 2 (string-key equality) before the parser and the canonicalizer run; the p95 that docs/reference/l1-budget.md cites for the 5 ms L2 check segment measures `normalize` plus a string compare, and no script ever runs the one test that does drive the arithmetic against a release budget.

**Evidence.**

```
crates/core/tests/bench_l1.rs:576 `let outcome = check(&row.answer, &row.answer, kind_of(row));` — and crates/core/src/answer/check.rs:105-107 returns first: `// Rung 2. The two answers are the same string.` / `if expected_text.string_key == learner_text.string_key { return Outcome::decided(true); }`. The M2 suite already records the same trap at crates/core/tests/answer_check.rs:1431: `// The self-check above stops on the string rung, so it never reaches the arithmetic.` Measured on this box, release, over the same 3,492-row corpus: `check(a,a)` p50 450 ns, p95 1,500 ns, max 10,530 ns (the benchmark prints p95 1,480 ns); `canonical_form(a)` p50 919 ns, p95 10,880 ns, max 129,038 ns; `check(a, "a+0")` p50 2,620 ns, p95 22,080 ns, max 213,125 ns. A second probe: of the 3,492 rows, 265 are outside the grammar (`canonical_form` errs — e.g. "7 L/min", "18 degrees Celsius"), and `check(a,a)` returns Decided(correct) for all 265, so the benchmark's own assertion `assert_eq!(decided, CORPUS_ANSWERS)` (bench_l1.rs:582) is satisfied without one call into the parser. docs/reference/serving-1.0-spec.md:554 asks for the opposite input set: "`answer::check` on the 3,227 parsed corpus answers". `grep -rn CADUS_RELEASE_BENCH` over the repo hits only crates/core/tests/answer_check.rs:97, crates/core/tests/selector.rs, and two review documents — neither scripts/gate.sh nor scripts/bench.sh sets it, so answer_check.rs's real-arithmetic budgets always run at the 10x debug values (50 ms per check, 5 s per corpus pass) inside the parallel `cargo test --workspace` step.
```

**Failure scenario.** Make `canon` 30 times slower on rational-function answers (say a rebuilt denominator per node). The gate stays green: `benchmark_a_check_holds_the_l2_segment` still reports p95 about 1,500 ns because every one of its 3,492 checks stops at rung 2; `the_corpus_canonicalization_holds_the_l2_budget` runs in debug against a 50 ms per-check budget while today's worst debug canonicalization is about 1.3 ms (129 µs release), so it absorbs a 30x regression; and docs/reference/l1-budget.md:47,141 keeps claiming the check segment is measured at "3,205 times under its segment". The learner-side cost, which is the one a grade request pays, moves from 22 µs p95 to about 0.7 ms p95 and from 213 µs to about 6.4 ms on the worst corpus answer — past the 5 ms the same document reserves for it — with nothing in the gate red.

**Refuter.** I could not refute the claim. Every part of it is demonstrable from the source and reproduces on this box.

1. The L2 half of benchmark A is a self-check. crates/core/tests/bench_l1.rs:576 calls `check(&row.answer, &row.answer, kind_of(row))`. In crates/core/src/answer/check.rs, rung 2 (line 105-107) compares `expected_text.string_key` against `learner_text.string_key` and returns `Outcome::decided(true)` before `canonical()` runs at rung 3. `normalize` (crates/core/src/answer/normalize.rs:92) is pure string work: trim, strip `$`, strip a trailing period, collapse whitespace, casefold, map Unicode operators, strip thousands groups. No parser, no canonicalizer. For any non-blank string s inside the input cap and a decidable kind, `check(s, s, kind)` therefore always stops at rung 2. The benchmark's own assertion `assert_eq!(decided, CORPUS_ANSWERS)` (line 582) proves that all 3,492 rows t

### #21 [major] The L1 allocation bound has 1.2 allocations per iteration of headroom, so a `format!` in the hot loop passes

File: `crates/core/tests/bench_l1.rs:105` — IDs: L1

**Claim.** `ALLOCATION_BOUND` is 110,000 against a measured 107,581 for 2,000 iterations — 2,419 spare allocations, or 1.2 per instance — so a regression that adds one heap allocation per instantiation stays under the bound, although both the constant's own docstring and docs/reference/l1-budget.md name that exact regression class as the one this bound exists to catch.

**Evidence.**

```
crates/core/tests/bench_l1.rs:105 `const ALLOCATION_BOUND: u64 = 110_000;` with the claim at lines 99-104: "The bound is 110,000, which is 2.2 percent above the measured number. The mutation check chose it: one `format!` around the rendered value in `template::render` moves the count to 111,881 and fails this assertion"; docs/reference/l1-budget.md:74-77 repeats it: "The bound catches the regression a timing bound on a shared runner never catches: a `format!` in a hot loop." I copied crates/core to a scratch workspace outside the repo and made one edit, at crates/core/src/template/document.rs:249, `let instance_hash = problem_text_hash(&text);` -> `let instance_hash = problem_text_hash(&format!("{text}"));` (one `format!` per instance, in the path the benchmark times). Unmutated run: `107581 allocations, 47 blocked, ring tail "c027d35bab12"`-equivalent, p95 7,980 ns, PASS. Mutated run: `benchmark A instantiate (release): p50 3740 ns, p95 8639 ns, p99 9870 ns, max 44919 ns, 109581 allocations, 47 blocked, ring tail "c027d35bab27"` — `test result: ok. 5 passed`. The count rose by exactly 2,000 (one per iteration) and stayed 419 under the bound.
```

**Failure scenario.** A change adds one owned copy per served instance — `problem_text_hash(&format!("{text}"))`, or a `to_string()` on the rendered statement before the digest. Benchmark A, the hard gate of the L1 core segment, reports 109,581 allocations and passes; the time assertion cannot catch it either, because the p95 of 8.6 µs sits 580 times under the 5 ms segment, so both halves of the gate stay green while the per-instance allocation count grows from 53 to 54. The two pinned sequence literals (BLOCKED_IN_THE_RUN, RING_TAIL_AFTER_THE_RUN) are unchanged by the mutation as well, so nothing in the file moves.

**Refuter.** I tried to refute the claim and I failed. I reproduced both runs on this box, and the numbers match the reviewer exactly. The bound of 110,000 sits 2,419 above the measured 107,581, so the assertion fires only above 1.209 extra allocations per instance. A regression that adds one allocation per instance (2,000 for the loop) lands at 109,581 and passes. The time half of the gate passes too, and the two pinned literals (BLOCKED_IN_THE_RUN = 47, RING_TAIL_AFTER_THE_RUN = "c027d35bab27") do not move, because the mutation keeps the text identical. No other test in the repo counts allocations; my grep of crates/ found only prose about allocations, no second assertion.

Three limits of the finding, for the ruling:
1. The docstring at crates/core/tests/bench_l1.rs:99-104 names one exact mutation (a `format!` around the rendered value in `template::render`). That mutation is caught: it measures 1
