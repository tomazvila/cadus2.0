# M4 adversarial review — round 2 (2026-08-27)

Run on commit 3209073 (after FIXM4a–c). One find/refute round: 18 raised, 11 confirmed, 0 blockers. Assigned to FIXM4d (gate/template/pool source), FIXM4e (store pop + refill), FIXM4f (benchmarks). M4 closes after this wave on the two-round cap.

## Orchestrator rulings (binding)

- Statement uniqueness (#1): the gate groups every walked instance by `instance_hash`; a
  digest that carries two different canonical answers is a rejection with the message
  `statement {text!r} renders from {n} tuples with different answers`; the same check runs on
  the sampled walk above the limit; `TemplateSource::fill` keeps a digest→answer map per
  batch and refuses (and counts) a colliding digest with a different answer.
- One estimator (#2/#5): above the limit `space_size` is the count of DISTINCT satisfying
  tuples the walk found within `GATE_DRAW_BUDGET`; `check_space` reads that count. The
  4,096-draw estimator is deleted.
- Coverage on constrained axes (#3/#9/#10): a choice axis reads the set of choice values in
  the satisfying sample; an ordered axis with NO constraint that names it reads its declared
  ends; an axis that a constraint names reads the sample's extremes, and a worked sample at
  a declared end is accepted as covering that end even when the sample missed it.
- Approval on serve (#4): `pop_with_ring_tx` joins `content_store` on `content_digest` and
  serves a template row only when `status = 'approved'` (exemplar rows have no digest and
  are served); the refill job retires (claims with a logged reason) rows whose digest is no
  longer approved.
- Exhausted source (#8): a pair whose fill inserted 0 rows twice in a row is backed off
  (60 min) and flagged `source_exhausted` in `operator_flags`.
- Benchmark literals (#6/#7/#11): pin the rendered text and the hash of three iterations
  (fixtures 1, 10, 20); re-measure allocations on the merged tree and re-pin the bound at
  measured + 0.5% (107,680 → 108,218) in the test and in `docs/reference/l1-budget.md`.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | major | `crates/core/src/template/gate.rs:786` | FIXM4d | The hidden-parameter rule is a strictly weaker proxy for the C4 rule it was ruled to implement: two tuples that render ONE statement may still compute DIFFERENT answers, and the pool keeps only one of them |
| 2 | major | `crates/core/src/template/domain.rs:704` | FIXM4d | space_size still estimates from 4,096 draws while the walk now spends 262,144, so the Hard-Rule-4 space floor both accepts a template with 4 distinct instances and refuses one with 20 |
| 3 | major | `crates/core/src/template/gate.rs:1182` | FIXM4d | The choice-coverage rule still reads the DECLARED choice list, so a constrained choice axis deadlocks the gate and the template can never be approved |
| 4 | major | `/home/deploy/dev/cadus2.0/crates/store/src/pool.rs:368` | FIXM4e | A pool row is never re-checked against content_store, so a revoked approval keeps serving and a corrected template cannot displace the wrong rows |
| 5 | major | `/home/deploy/dev/cadus2.0/crates/core/src/template/gate.rs:987` | FIXM4d (dup of #2) | check_space rejects a template on the 4,096-draw estimator while the same call's 262,144-draw walk holds satisfying tuples, so a Pythagorean-triple template is refused as "only 0 distinct problem(s)" |
| 6 | major | `crates/core/tests/bench_l1.rs:117` | FIXM4f | Benchmark A's two pinned literals pin neither the rendered text nor the hash, so a renderer or digest regression on 19 of the 20 fixtures passes the L1 gate untouched |
| 7 | major | `crates/core/tests/bench_l1.rs:93` | FIXM4f | The recorded allocation measurement is stale by 99, so the L1 budget document reports a number benchmark A does not produce and the bound's margin is not the documented one |
| 8 | major | `crates/worker/src/refill.rs:405` | FIXM4e | A pair whose source is exhausted returns Rows{inserted:0} forever, keeps the head of the depth-ordered target list, and re-creates the starvation finding #3 was ruled fixed |
| 9 | major | `crates/core/src/template/gate.rs:1182` | FIXM4d (dup of #3) | The choice-coverage rule still reads the DECLARED choice list, so a choice axis whose constraints forbid a value deadlocks with the sample-constraint rule — finding #16's defect, unfixed for choice domains |
| 10 | major | `crates/core/src/template/gate.rs:1115` | FIXM4d | Above the exhaustive limit the edge-coverage rule reads the PRNG sample's extremes only, so it rejects worked samples at the true declared ends and stops verifying the real edge |
| 11 | major | `docs/reference/l1-budget.md:176` | FIXM4f (dup of #7) | The L1 allocation bound is not the measured count plus 0.5 percent: HEAD allocates 107,680, while docs/reference/l1-budget.md and bench_l1.rs both record 107,581 |

## FIXM4d

### #1 [major] The hidden-parameter rule is a strictly weaker proxy for the C4 rule it was ruled to implement: two tuples that render ONE statement may still compute DIFFERENT answers, and the pool keeps only one of them

File: `crates/core/src/template/gate.rs:786` — IDs: C4, A1, C6

**Claim.** check_hidden_parameters only refuses a parameter the answer reads that appears in no rendered field, so a template whose parameters ALL appear in the statement can still render one statement from several tuples with several different answers; the gate renders and hashes all 144 instances and never groups them, and TemplateSource::fill then dedups by instance_hash and stores one of the answers.

**Evidence.**

```
gate.rs:785-795 — the whole rule is `for name in doc.params.keys() { if answer_names.contains(name) && !rendered.contains(name) { ... } }`. gate.rs:1319-1331 `check_instances` walks every satisfying tuple, renders it and inspects each instance ALONE; `instance_hash` is never grouped in gate.rs.

Probe (crate with a path dep on crates/core, /tmp/claude-1000/-home-deploy-dev-cadus2-0/423a634f-40c8-4ad8-9fdd-67df2281434b/scratchpad/rv_c4/src/main.rs):

  statement: "A code is made by writing one number next to another: ${a}{b}$. Multiply the two numbers that were written. What is the product?"
  params: a,b int 1..12   answer_expr: "a*b"
  samples: (1,1)=1 (1,12)=12 (12,1)=12 (12,12)=144

  [adjacent] GATE ACCEPTED space=Exact(144) instances=144 exhaustive=true notes=[]
  [adjacent] COLLISION hash=f1769a41cd1d text="A code is made by writing one number next to another: $112$. Multiply the two numbers that were written. What is the product?"
        a=1,b=12  -> answer 12
        a=11,b=2  -> answer 22
  [adjacent] distinct statements=142 colliding-with-different-answers=1
  [adjacent] fill -> 142 rows, 0 refusals
     row text="...: $112$. ..." answer="22" bindings=["a=11", "b=2"]
```

**Failure scenario.** An author writes the statement above; every worked sample agrees with a*b, so the gate accepts and a reviewer approves. TemplateSource::fill draws all 144 satisfying tuples, dedups by instance_hash (source.rs:417), and keeps a=11,b=2 for the digest f1769a41cd1d. A learner is served "...: $112$. Multiply the two numbers that were written...", reads the code as 1 and 12, answers 12, and the M5 grade path canonicalizes 12 against the stored 22 and records a failure in the append-only log. The gate already holds the rendered statement and the computed answer of all 144 tuples in check_instances, so grouping them by instance_hash and refusing a digest with two answers costs one BTreeMap in the exhaustive branch.

**Refuter.** The claim holds. I could not run the probe this session (Bash command classification was rate-limited for every attempt), so I verified each mechanical step in the source. Every part of the claim is true of the code, and I found a second path to the same defect that needs no adjacency.

1. The rule is a name-presence rule, not an answer-uniqueness rule. check_hidden_parameters (crates/core/src/template/gate.rs:781-796) refuses a parameter only when answer_names.contains(name) && !rendered.contains(name). It compares NAMES. It never compares two tuples, and it never reads a computed answer.

2. The gate never groups instances. gate (gate.rs:286-309) runs 14 steps: check_document, check_params, check_constraint_shape, check_rendered_fields, compile, check_dead_parameters, check_answer_names, build_walk, check_space, check_samples, check_distractors, axis_extremes, check_coverage, check_ins

### #2 [major] space_size still estimates from 4,096 draws while the walk now spends 262,144, so the Hard-Rule-4 space floor both accepts a template with 4 distinct instances and refuses one with 20

File: `crates/core/src/template/domain.rs:704` — IDs: A1, A5, D5, C4, C6

**Claim.** The FIXM4a fix for findings 7/12 raised the gate walk to GATE_DRAW_BUDGET = 262,144 draws but left space_size on ESTIMATE_SAMPLES = 4,096, so above EXHAUSTIVE_SPACE_LIMIT the count check_space enforces against MIN_SPACE_SIZE is off by one to two orders of magnitude in both directions: a template with 4 satisfying tuples is accepted with a recorded space of 244, and a template with 20 satisfying tuples is refused with a recorded space of 0.

**Evidence.**

```
domain.rs:701-715 draws `for _ in 0..ESTIMATE_SAMPLES` (ESTIMATE_SAMPLES = 4_096, domain.rs:67) while gate.rs:937-944 now draws up to GATE_DRAW_BUDGET = 262_144. check_space (gate.rs:963) reads walk.space, which is that estimate.

FALSE ACCEPT (probe src/bin/overest2.rs): a,b int 1..1000; constraints eq(a,b) and divides(202,a); samples (202,202) and (808,808).
  true satisfying tuples: [202, 404, 606, 808] (count 4)
  GATE ACCEPTED space=Estimated { estimate: 244, samples: 4096, hits: 1 } instances_checked=1 exhaustive=false
     notes=["the sampled walk found 1 satisfying tuple(s) in 262144 draw(s), and the instance check read those 1"]
  with_space_size writes Some(Estimated { estimate: 244, samples: 4096, hits: 1 }) into the body
  fill(seed=2, need=24) -> 1 rows, 1 distinct statements: ["Compute $606 \\times 606$."]
  fill(seed=1) -> no tuple of the declared domains satisfies the constraints
  fill(seed=3) -> no tuple of the declared domains satisfies the constraints

FALSE REJECT (probe src/bin/sparse.rs): same shape with divides(50,a).
  declared=1000000 true satisfying tuples=20
  space_size() = Estimated { estimate: 0, samples: 4096, hits: 0 }
  GATE REJECTED [space-floor] the declared domains produce only 0 distinct problem(s); at least 12 are needed ...
  (check_space tests walk.tuples.is_empty() FIRST at gate.rs:964, so reaching space-floor proves the walk itself held satisfying tuples while the stored count said 0.)
```

**Failure scenario.** An author writes an A1 template with inter-parameter constraints over domains whose product is above 4,096. Case A: the constraints admit exactly 4 tuples. The estimator's 4,096 draws happen to hit one of them, so the gate reports 244 distinct problems, clears MIN_SPACE_SIZE = 12, checks exactly ONE of the four instances, and writes Estimated{244} into the body a human approves under C6. The D5 ring holds 20 digests and the pool can never hold more than 4, so every serve of that knowledge point after the fourth is a ring hit and pool_exhausted fires forever; most refill ticks insert nothing at all (2 of the 3 seeds above return an error, not rows). Case B: the constraints admit 20 tuples, comfortably over the floor, and the gate refuses the document with "only 0 distinct problem(s)" — a number the same Walk contradicts — so the knowledge point stays on the A6 exemplar fallback forever. The fix is the same one applied to build_walk: count the estimator's hits over the draw budget the walk already spends, or reuse the walk's own hit rate instead of a second, smaller sample.

**Refuter.** The claim is correct in every part, and I reproduced both prongs against the built code.

The two draw budgets disagree. `build_walk` (/home/deploy/dev/cadus2.0/crates/core/src/template/gate.rs:930-947) now draws up to `GATE_DRAW_BUDGET = 262_144` (gate.rs:93). `space_size` (/home/deploy/dev/cadus2.0/crates/core/src/template/domain.rs:690-716) still draws `ESTIMATE_SAMPLES = 4_096` (domain.rs:67). `build_walk` calls `space_size` at gate.rs:923 and stores the result in `Walk.space`; `check_space` (gate.rs:963-994) reads `walk.space.count()` and compares it to `MIN_SPACE_SIZE = 12`.

The arithmetic alone makes the floor unenforceable above `EXHAUSTIVE_SPACE_LIMIT`. The estimate is `declared * hits / 4096` (domain.rs:710), so its step is `declared / 4096`. With a declared space of 1,000,000 the step is 244. The stored count is therefore 0, or 244, or 488 — never a number near 12. The floor 

### #3 [major] The choice-coverage rule still reads the DECLARED choice list, so a constrained choice axis deadlocks the gate and the template can never be approved

File: `crates/core/src/template/gate.rs:1182` — IDs: A1, A2, C6, A6

**Claim.** The FIXM4a fix moved the ordered-axis edges onto the satisfying walk (axis_extremes) and made the crossed-corner rule skip an unreachable corner, but the choice branch of check_coverage three lines away still demands a worked sample for EVERY declared choice, including choices the constraints admit no tuple for, and the sample-constraint rule then refuses exactly the sample the message asks for.

**Evidence.**

```
gate.rs:1181-1197 — `if matches!(domain, Domain::Choice { .. }) { let missing: Vec<String> = values.get(name).into_iter().flatten().filter(|value| !seen.contains(*value)) ...` — `values` is the DECLARED value list built by `check_params`/`domain_values` (gate.rs:459), never `walk.tuples`. Contrast gate.rs:1115-1134 `axis_extremes`, which the fix changed to read `walk.tuples`, and gate.rs:1240-1252, which the fix changed to skip an unreachable corner.

Probe crate with a path dependency on crates/core, document: statement `Divide ${n}$ by ${d}$.`, params `n: int 1..12`, `d: choice [2,3,4,5,6,8,10,12]`, constraints `divides(d,n)` and `ne(n,d)`, answer_expr `n/d`. 12 satisfying tuples (d=2:5, d=3:3, d=4:2, d=5:1, d=6:1, d=8/10/12:0), which clears MIN_SPACE_SIZE = 12.

$ cargo run
== times-table-no-8-10-12: REJECTED [choice-coverage] no worked sample uses d=['8', '10', '12'] — every choice must appear in a sample, or the expression is unverified for it
== times-table-with-8-10-12: REJECTED [sample-constraint] sample 6 binds {'d': 8, 'n': 8}, which the ne constraint refuses — a sample outside the constraints verifies nothing

A second, larger shape gives the same pair of refusals (n: int 1..100, d: choice [2,3,101], `divides(d,n)`, 83 satisfying tuples): [choice-coverage] asks for d=101, and every sample that binds it earns [sample-domain] or [sample-constraint].
```

**Failure scenario.** An author writes the A1 headline shape — a template with inter-parameter constraints — over a choice axis: "Divide ${n}$ by ${d}$" with n in 1..12, a divisor list [2,3,4,5,6,8,10,12], and the constraints `d divides n` and `n != d` (a proper multiple, not the trivial n/n = 1). Twelve tuples satisfy the constraints, so the space floor passes and the gate walks all twelve instances. check_coverage then refuses with `no worked sample uses d=['8', '10', '12']`. The author does what the message says and adds the samples (n=8,d=8), (n=10,d=10), (n=12,d=12) — the only tuples in the declared domains that bind those three choices at all — and the gate answers `sample 6 binds {'d': 8, 'n': 8}, which the ne constraint refuses`. No sample list clears both rules, because the constraints admit no tuple with d in {8,10,12}. The document is permanently unapprovable, `approved_template` keeps returning None for that knowledge point, and refill.rs:544-573 puts it on the A6 exemplar rotation forever. This is the third member of the family round 1 confirmed as #16 (declared int ends) and #22 (crossed corners); both of those were fixed by reading the satisfying set, and the choice branch was left reading the declared set.

**Refuter.** The claim is demonstrable, and my probe reproduced both quoted rejection messages verbatim. The code path is confirmed: `gate()` (crates/core/src/template/gate.rs:291-301) builds `values` with `check_params` (line 291) and passes that same map, unfiltered, to `check_coverage` (line 301). For a choice domain, `domain_values` returns the DECLARED list verbatim (crates/core/src/template/domain.rs:484-490). The choice branch of `check_coverage` (gate.rs:1181-1197) reads `values.get(name)` and demands a sample for every declared choice, while `axis_extremes` (gate.rs:1115-1136) and the crossed-corner rule (gate.rs:1240-1262) read `walk.tuples`, the satisfying set. The two rules in the same function therefore deadlock: the sample-constraint loop (gate.rs:1155-1170) and the sample-domain loop (gate.rs:1141-1154) both run BEFORE the choice branch and refuse exactly the sample the choice-coverage

### #5 [major] check_space rejects a template on the 4,096-draw estimator while the same call's 262,144-draw walk holds satisfying tuples, so a Pythagorean-triple template is refused as "only 0 distinct problem(s)"

File: `/home/deploy/dev/cadus2.0/crates/core/src/template/gate.rs:987` — IDs: A1, A2, C4, A6 — duplicate of #2

**Claim.** The #7/#12 fix gave `build_walk` a budget of `GATE_DRAW_BUDGET = 262,144` draws, but `walk.space` above the exhaustive limit is still `space_size(...)`, which draws `ESTIMATE_SAMPLES = 4,096` times; `check_space` then reads `walk.space.count()` for the `MIN_SPACE_SIZE` floor, so a constraint set whose density is under about 1/4,096 is refused with a count of 0 that the walk in the same `Walk` struct contradicts.

**Evidence.**

```
gate.rs:987-995 — `let count = walk.space.count(); if count < MIN_SPACE_SIZE { ... "the declared domains produce only {count} distinct problem(s)" ... }`, where `walk.space` above the limit is `space_size(&doc.params, &doc.constraints)` (gate.rs:917) and `space_size` draws `ESTIMATE_SAMPLES = 4_096` tuples (domain.rs:66).

Probe: the textbook A1 case — a Pythagorean-triple template, `a`, `b`, `c` each `int 1..100`, constraints `a**2 + b**2 = c**2` and `a < b`, `answer_expr` `c`, samples (3,4,5) and (60,80,100):

  true satisfying tuples (brute force with all_hold) = 52
  declared=Ok(1000000) space_size=Ok(Estimated { estimate: 0, samples: 4096, hits: 0 })
  REJECTED [space-floor] the declared domains produce only 0 distinct problem(s); at least 12 are needed for randomized values and for avoidance of a recently-served problem to mean anything (Hard Rule 4)

The rejection is `space-floor` and not `no-satisfying-tuple`, and `check_space` reaches the floor only after `walk.tuples.is_empty()` is false (gate.rs:965), so the same call collected satisfying tuples and reported the count as 0. A denser control of the same shape is accepted: `a`, `b` in 1..200 with `a = b` gives `Estimated { estimate: 244, samples: 4096, hits: 25 }` and ACCEPTED, while `a`, `b` in 1..10000 with `a = b` (10,000 satisfying tuples) gives the same `hits: 0` refusal.
```

**Failure scenario.** An author writes the A1 headline template — inter-parameter constraints with an exact answer — for right triangles: three integer axes over 1..100 with `a**2 + b**2 = c**2` and `a < b`. Fifty-two tuples satisfy the constraints, four times the `MIN_SPACE_SIZE` floor of 12, and the gate's own sampled walk finds satisfying tuples (proved by which rejection fires). The gate nonetheless returns `[space-floor] the declared domains produce only 0 distinct problem(s)`. The authoring pipeline of A2 feeds that message back to the model, which cannot act on it: no change to the domains or the constraints raises the estimator's hit count, because the estimator's 4,096 draws are the problem and widening the domains lowers the density further. The template is never stored, the knowledge point keeps `needs_template = true`, and it serves its exemplars forever (A6). The defect is the surviving half of the round-1 rulings on #7 and #12: `build_walk` no longer stops on a sparse stretch, and `space_size` still decides the verdict on 4,096 draws.

**Refuter.** The claim holds. I read the code and I could not break the mechanism.

1. The two counts come from two different draw budgets. `build_walk` above the exhaustive limit sets `space` from `space_size(&doc.params, &doc.constraints)` (gate.rs:917), and only afterward runs its own loop of up to `GATE_DRAW_BUDGET = 262_144` draws to fill `tuples` (gate.rs:937-944). `space_size` (domain.rs:690-713) draws `ESTIMATE_SAMPLES = 4_096` tuples from `ESTIMATE_SEED = 0` and computes `estimate = declared * hits / 4096`. The 262,144-draw walk therefore learns 64 times more about the density than the number the gate then reports, and the walk's result is discarded for counting.

2. `check_space` decides the floor on the weaker number. Line 987 reads `walk.space.count()`, not `walk.tuples.len()` and not the walk's hit rate. The `no-satisfying-tuple` guard at gate.rs:965 tests `walk.tuples.is_empty()`, so th

### #9 [major] The choice-coverage rule still reads the DECLARED choice list, so a choice axis whose constraints forbid a value deadlocks with the sample-constraint rule — finding #16's defect, unfixed for choice domains

File: `crates/core/src/template/gate.rs:1182` — IDs: A1, A2, C4, C6 — duplicate of #3

**Claim.** FIXM4a rewrote `axis_extremes` to read the satisfying walk, but left `check_coverage`'s choice branch reading `values.get(name)` — the declared choice list — so the gate demands a worked sample at a choice value the constraints refuse, and its own sample-constraint rule then refuses that sample; no author input passes, at any space size.

**Evidence.**

```
gate.rs:1181-1199 — `if matches!(domain, Domain::Choice { .. }) { let missing: Vec<String> = values.get(name).into_iter().flatten().filter(|value| !seen.contains(*value)) ... }`. `values` is the DECLARED per-axis value list built by `check_params`; the satisfying walk is never consulted for a choice axis (`axis_extremes` at gate.rs:1116-1122 skips `Domain::Choice` entirely). gate.rs:1163-1174 is the sample-constraint rule that refuses any sample the constraints exclude.

Probe (scratchpad `tq/src/bin/choicecov.rs`, path dep on crates/core; params `a` choice [1..12], `b` int 1..12, constraint `gt(a,b)`, answer_expr `a - b`, 144 declared tuples so the walk is EXHAUSTIVE):

  A no-sample-for-a=1: REJECTED [choice-coverage] no worked sample uses a=['1'] — every choice must appear in a sample, or the expression is unverified for it
  B sample-for-a=1:    REJECTED [sample-constraint] sample 0 binds {'a': 1, 'b': 1}, which the gt constraint refuses — a sample outside the constraints verifies nothing

The two rules name each other's refusal. `a = 1` is satisfiable by no tuple, because `b >= 1` and `a > b`.
```

**Failure scenario.** An author writes the A1 inter-parameter shape with the larger operand as an explicit choice list: `a` choice [1,2,...,12], `b` int 1..12, `constraints [{"op":"gt","left":"a","right":"b"}]`, `answer_expr "a - b"`. Eleven of the twelve choices are reachable and 66 tuples satisfy the constraints, well over MIN_SPACE_SIZE. Without a sample at a=1 the gate returns choice-coverage naming a=['1']; adding that sample returns sample-constraint refusing the same tuple. There is no sample set that clears both rules, so the knowledge point is permanently unapprovable and falls back to A6 exemplar rotation forever — the same outcome finding #16 was raised for, reached through the choice branch the fix did not touch.

**Refuter.** I cannot refute the claim. The code reading confirms every step of it.

`gate()` at gate.rs:287-301 builds `values` from `check_params` and hands that same map to `check_coverage`. `check_params` fills it from `domain_values`, which returns the DECLARED value list of each domain. FIXM4a rewrote `axis_extremes` (gate.rs:1115-1134) to read `walk.tuples`, but that function starts with a filter that keeps only `Int`, `Rational`, and `Decimal` domains and skips `Choice`. The choice branch at gate.rs:1177-1200 therefore reads `values.get(name)` — the declared list — and demands a worked sample at every declared choice value. The sample-constraint rule at gate.rs:1160-1176 refuses any sample the constraints exclude. The two rules meet head-on the moment a declared choice value lies in no satisfying tuple.

The gate's own comment states the rule the branch breaks. gate.rs:1108-1114 says the ends

### #10 [major] Above the exhaustive limit the edge-coverage rule reads the PRNG sample's extremes only, so it rejects worked samples at the true declared ends and stops verifying the real edge

File: `crates/core/src/template/gate.rs:1115` — IDs: A1, C4, A2, V2

**Claim.** The fix to finding #16 made `axis_extremes` read `walk.tuples` in BOTH branches, so above `EXHAUSTIVE_SPACE_LIMIT` the low and high end of an ordered axis are the minimum and maximum of the gate's 4,096 fixed-seed draws, even when the axis carries no constraint at all and its declared ends are freely reachable; `check_coverage` then refuses a document whose worked samples sit at the true declared ends and demands a sample at a value that is an artifact of `GATE_SEED`.

**Evidence.**

```
crates/core/src/template/gate.rs:1115-1134 — the `walk.exhaustive` branch is gone: `let seen: Vec<Value> = walk.tuples.iter().filter_map(|tuple| tuple.get(name).cloned()).collect();` then `(seen.iter().min(), seen.iter().max())`.

Probe over `cadus_core::template::gate`, one unconstrained axis `a: {"kind":"int","low":1,"high":10000}` (declared space 10,000 > 4,096), statement `Compute ${a}^{{2}}$.`, `answer_expr` `a**2`:

  samples a=1 and a=10000   -> REJECT [edge-coverage] no worked sample uses the low end of a (2) — the edges are where an expression stops being right
  samples a=2 and a=10000   -> ACCEPT instances=4096 space=Estimated { estimate: 10000, samples: 4096, hits: 4096 } notes=[]
  samples a=2 and a=9999    -> REJECT [edge-coverage] no worked sample uses the high end of a (10000)

The accepted document has no worked sample at a=1. The gate's own comment at gate.rs:1530-1534 names a=1 of this exact template as the degenerate instance that matters.
```

**Failure scenario.** An author writes the plainest template above the exhaustive limit: square a whole number, a in 1..10000, worked samples at the two declared ends a=1 and a=10000. The gate refuses it with `[edge-coverage] no worked sample uses the low end of a (2)`. Nothing in the document, the domain, or the constraints names 2; it is the smallest value the 4,096 draws from `GATE_SEED` happened to hit, so the author cannot write a correct document without running the gate and copying the number back, and any change to `GATE_SEED`, `GATE_SAMPLES`, or the draw code invalidates every such stored document. The reason the fix gives for dropping the declared ends — "a declared end the constraints forbid asks the author for a worked sample the sample-constraint rule then refuses" — does not apply here, because this axis has no constraint. Worse for C4: after the author complies with a sample at a=2, the gate ACCEPTS a document whose worked-sample oracle never covers a=1 or, on a different seed, never covers a=10000. The rule whose own message reads "the edges are where an expression stops being right" no longer requires a verified answer at either true edge of any axis above the limit.

**Refuter.** I cannot refute the claim. Every load-bearing part of it reproduces against the unmodified crate.

1. The code. `axis_extremes` (gate.rs:1115-1135) reads `walk.tuples` unconditionally. The branch on `walk.exhaustive` is gone. Above `EXHAUSTIVE_SPACE_LIMIT` = 4,096, `walk.tuples` holds the 4,096 draws of `GATE_SEED` = 0 and nothing else, for a constrained axis and an unconstrained axis alike.

2. The rejection. A probe crate with a path dependency on crates/core ran the plainest template above the limit: one axis `a` int 1..10000, no constraints, statement `Compute ${a}^{{2}}$.`, `answer_expr` `a**2`, worked samples at the two declared ends. The gate refused it with `[edge-coverage] no worked sample uses the low end of a (2)`. The number 2 appears nowhere in the document, the domain, or the constraints. It is the minimum of the seeded draws.

3. The acceptance. The same document with a sa

## FIXM4e

### #4 [major] A pool row is never re-checked against content_store, so a revoked approval keeps serving and a corrected template cannot displace the wrong rows

File: `/home/deploy/dev/cadus2.0/crates/store/src/pool.rs:368` — IDs: C6, C4, A1

**Claim.** `pop_with_ring_tx` selects on `user_id`, `kp_id`, and `claimed_at IS NULL` alone: it never joins `content_store` and never reads `content_store.status`, so a row whose approval an operator revoked is still served; and because `insert_batch` writes `ON CONFLICT ... DO NOTHING` on the statement digest, a newly approved corrected template inserts nothing over the unclaimed wrong rows of the digest it replaces.

**Evidence.**

```
crates/store/src/pool.rs:367-371 — `FROM serving_pool WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL ORDER BY created_at, id FOR UPDATE SKIP LOCKED LIMIT $3`. `content_digest` is written at pool.rs:246 and read back at pool.rs:387, and no code path uses it for anything.
migrations/0005_content.sql:16 — `status ... CHECK (status IN ('pending','approved','rejected'))  -- C6: 'pending' is never served`.

Probe against the throwaway cluster, using the production `insert_batch_for_user`, `approved_template`, and `pop_with_ring`:

  step 1: inserted 1 row(s) from the approved digest d1
  step 2: approved_template after the revocation = None
  step 3: SERVED text="Compute $7 \\times 8$." expected_answer="15" content_digest=Some("d1") source=template
  step 4: seeded 1 more unclaimed wrong row(s) from d1
  step 5: the corrected batch from d2 inserted 0 row(s)
  step 6: SERVED expected_answer="15" content_digest=Some("d1")
```

**Failure scenario.** An operator reads the refill log, sees that template digest d1 for perfect-squares/kp1 computes a wrong answer, and does the one thing C6 gives him: he sets `content_store.status = 'rejected'` for d1. `approved_template` then returns None, so the refill stops using d1 — and the up-to-24 unclaimed rows d1 already wrote keep being served, one per serve, with the wrong `expected_answer`, until the learner has worked through all of them. The M5 grade path canonicalizes the learner's correct answer against the wrong stored one and writes an append-only wrong `attempt` for each of them. The second half needs no revocation at all: the operator authors a corrected template, it passes the gate, a human approves it as digest d2, and it renders the same statements, so `insert_batch` matches every corrected row on `(user_id, kp_id, instance_hash)` and writes 0 rows (step 5 above). The pool is at its target depth, so `refill_targets` does not even list the pair. The correction reaches no learner who already has the wrong rows queued.

**Refuter.** Both halves of the claim are demonstrable from the statements themselves, and no ruling in docs/plans/M4.md excuses them. (1) pop_with_ring_tx reads serving_pool alone. Its WHERE clause is user_id, kp_id, and claimed_at IS NULL. It never joins content_store and never reads content_store.status. content_digest is selected and carried into PoolRow, and no caller uses it for a decision. A grep over crates/ confirms the only two writes to serving_pool outside tests are the claim and retire_row, both UPDATE ... SET claimed_at. There is no DELETE and no invalidation path anywhere in the store or the worker, so an operator who sets status = 'rejected' stops the refill from using the digest but leaves every unclaimed row it already wrote servable, one per serve, with the wrong expected_answer. (2) insert_batch ends ON CONFLICT (user_id, kp_id, instance_hash) DO NOTHING. The conflict target does 

### #8 [major] A pair whose source is exhausted returns Rows{inserted:0} forever, keeps the head of the depth-ordered target list, and re-creates the starvation finding #3 was ruled fixed

File: `crates/worker/src/refill.rs:405` — IDs: D-O4, A6, A1

**Claim.** The refill backoff starves only `Filled::NoSource`; a pair whose fill and insert both SUCCEED but write 0 rows (every instance digest is already in `serving_pool`) is never starved, stays permanently under `target_depth`, and sorts first on every tick, so it consumes the per-tick budget forever — which is exactly the failure mode of confirmed finding #3.

**Evidence.**

```
refill.rs:394-411 — `Ok(Filled::NoSource) => { ... state.starve(target.user_id, &target.kp_id, now); }` is the ONLY call to `starve`. The `Ok(Filled::Rows { source, inserted, refused, flagged })` arm at :405 increments counters and never starves, whatever `inserted` is. `insert_batch` (crates/store/src/pool.rs:260) is `ON CONFLICT (user_id, kp_id, instance_hash) DO NOTHING` and `ExemplarSource::fill` (crates/core/src/pool/source.rs:562, `let _ = seed;`) returns the SAME author-order list on every call, so once the exemplar rows exist the insert is 0 forever. `DEFAULT_TARGET_DEPTH = 24` (refill.rs:95) while `covers_ring()` already warns for fewer than `RING_CAPACITY = 20` exemplars, so every A6 pair is permanently below target.

Probe (scratchpad `tq/src/bin/starve.rs`, TestDb on the throwable cluster, target_depth 24, targets_per_tick 1, one exemplar pair `adding-two-digits/kp1` with 3 exemplars and one template pair `perfect-squares/kp1` with 12 instances):

  tick 1: targets=1 inserted=3 from_exemplar=3 without_source=0 skipped_starved=0 starved_len=0 | next list: ["perfect-squares/kp1@0", "adding-two-digits/kp1@3"]
  tick 2: targets=1 inserted=12 from_template=12 without_source=0 starved_len=0 | next list: ["adding-two-digits/kp1@3", "perfect-squares/kp1@12"]
  tick 3: targets=1 inserted=0 without_source=0 skipped_starved=0 starved_len=0 | next list: ["adding-two-digits/kp1@3", "perfect-squares/kp1@12"]
  tick 4..8: identical — inserted=0, starved_len=0, same list head.

This is NOT the accepted "repeated fill/insert failures" case: `report.failed` stays 0, no error is raised, and `Filled::Rows` is returned every tick.
```

**Failure scenario.** A deployment has 40 knowledge points on the A6 exemplar fallback (3 exemplars each) and 10 with approved templates. `targets_per_tick` is 32. Every exemplar pair sits at depth 3 < 24 permanently and can never gain a row; depth 3 sorts ahead of a template pair at depth 12 or 23. From the first tick after the exemplar rows land, all 32 slots of every tick go to the 40 exemplar pairs, each inserting 0. No template pair is ever reached again, `serving_pool` for those pairs drains to 0 as learners consume it, and the D-O1 pop returns nothing. Nothing in the report says so: `without_source` is 0, `failed` is 0, `skipped_starved` is 0, and `starved_len()` is 0.

**Refuter.** The claim holds. I read every link of the chain and each one is unconditional in the source; nothing in the worker, the store, or the M4 fix (FIXM4b) breaks it.

1. `starve` has exactly one call site. `grep -rn "starve(" crates/` gives the definition at /home/deploy/dev/cadus2.0/crates/worker/src/refill.rs:240 and one caller at :396, inside the `Ok(Filled::NoSource)` arm. The `Ok(Filled::Rows { source, inserted, refused, flagged })` arm (:398-421) adds to `report.inserted`, `report.refused_instances`, `report.flagged_refusals`, and the per-source counter. It reads `inserted` only to add it. A value of 0 takes the same path as a value of 24.

2. An exemplar pair returns `Filled::Rows` with `inserted = 0` forever. `ExemplarSource::fill` (/home/deploy/dev/cadus2.0/crates/core/src/pool/source.rs:562-566) writes `let _ = seed;` and then walks `self.exemplars` in author order. It has no draw a

## FIXM4f

### #6 [major] Benchmark A's two pinned literals pin neither the rendered text nor the hash, so a renderer or digest regression on 19 of the 20 fixtures passes the L1 gate untouched

File: `crates/core/tests/bench_l1.rs:117` — IDs: L1, C4, A1

**Claim.** RING_TAIL_AFTER_THE_RUN is the digest of iteration 1999 alone, whose template is fixture 20 of 20 (1999 % TEMPLATE_COUNT == 19), and BLOCKED_IN_THE_RUN counts equalities between digests, which are invariant under any injective rewrite of a statement; neither literal therefore detects a change in template::render or problem_text_hash on the other 19 fixtures, although bench_l1.rs:29-31, bench_l1.rs:115-116 and docs/reference/l1-budget.md:72-73 all state that they do.

**Evidence.**

```
bench_l1.rs:29-31 — "[`RING_TAIL_AFTER_THE_RUN`] and [`BLOCKED_IN_THE_RUN`] pin that sequence: a change to the draw, the renderer, the evaluator, or the hash moves one of the two literals."; bench_l1.rs:115-116 — "It pins the whole sequence: the draw order, the rendered text, and `problem_text_hash`."

The FIXM4a fix of review-1 finding 18 changed the renderer (render.rs:107-116 now brackets a non-atomic value). I ran benchmark A at the pre-fix commit and at HEAD, same box, same seed:

  $ git archive 5c61670 | tar -x -C <scratch>; cargo test --release -p cadus-core --test bench_l1 -- --test-threads=1 --nocapture benchmark_a_instantiation
  benchmark A instantiate (release): p50 3540 ns, ... 107581 allocations, 47 blocked, ring tail "c027d35bab27"

  $ (HEAD) cargo test --release -p cadus-core --test bench_l1 -- --test-threads=1 --nocapture benchmark_a_instantiation
  benchmark A instantiate (release): p50 3700 ns, ... 107680 allocations, 47 blocked, ring tail "c027d35bab27"

Both literals are byte-identical across the fix, and neither literal moved in the source: `git show 5c61670:crates/core/tests/bench_l1.rs` carries the same `RING_TAIL_AFTER_THE_RUN = "c027d35bab27"` and `BLOCKED_IN_THE_RUN = 47`.

A probe crate that replays the exact benchmark loop (same fixtures, same BENCH_SEED, same order) shows the text did change on HEAD:
  statements containing '(': 283 of 2000
    6  07-fraction-of-a-number.json "Compute $(1/4) \\times 11$. Give an exact answer."
    7  08-add-two-fractions.json    "Compute $(1/3) + (4/3)$. Give an exact answer in lowest terms."
  iter 1999 tmpl 20-two-digit-difference.json text "Compute $75 - 40$." hash c027d35bab27
Fixtures 07 and 08 rendered `1/4` and `1/3` before the fix and `(1/4)` and `(1/3)` after it, so 183 of the 2,000 measured statements and their instance hashes changed while both pinned literals held.
```

**Failure scenario.** Revert the bracket rule at crates/core/src/template/render.rs:110-115 — that is review-1 finding 18 verbatim, a C4 defect: the learner is printed `Compute $1/2 \times 12$` while the answer evaluator brackets the value, so the printed problem and the stored answer answer different questions. Run scripts/bench.sh. 183 of the 2,000 measured statements change and so do their digests, yet `blocked` stays 47, the ring tail stays "c027d35bab27", `answers` stays 2,000, the p95 stays about 8 µs and the allocation count drops back under the bound: benchmark A reports `test result: ok` on every assertion. The same holds for any change to `problem_text_hash` or to `render` that does not touch fixture 20's 100th draw. The file's stated determinism guard — the only thing in it that ties the timed loop to the production render and digest — cannot fail on 19 of the 20 fixtures.

**Refuter.** The claim is correct on every step, and the repository history alone demonstrates it. (1) The ring tail is the digest of iteration 1999 alone. crates/core/src/pool/ring.rs:80-83 and :134-136 append unconditionally and keep duplicates, and hashes() returns oldest first, so hashes().last() in bench_l1.rs:559 and :513 is the hash the last iteration pushed. 1999 % 20 == 19 selects the 20th fixture in sorted order, 20-two-digit-difference.json. That fixture declares two int params (a in 11..99, b in 10..98, a > b). Value::needs_brackets at crates/core/src/template/domain.rs:268-274 returns false for a positive whole number, so the FIXM4a bracket rule at crates/core/src/template/render.rs:110-115 cannot touch that fixture's text. (2) The blocked count is set membership on digest strings: Avoid::blocks at crates/core/src/pool/ring.rs:288-290 asks self.blocked.contains(hash). It reads equality b

### #7 [major] The recorded allocation measurement is stale by 99, so the L1 budget document reports a number benchmark A does not produce and the bound's margin is not the documented one

File: `crates/core/tests/bench_l1.rs:93` — IDs: L1

**Claim.** bench_l1.rs:93 and docs/reference/l1-budget.md:176-177 both record "107,581 times for 2,000 iterations" as the measured count and derive ALLOCATION_BOUND = 108,118 from it; HEAD deterministically measures 107,680, so the real headroom is 438 (0.407 percent, 0.219 allocations per iteration), not the "0.5 percent" and "0.26 allocations per iteration" that bench_l1.rs:100-102 and l1-budget.md:77-78 state, and the docstring's own rule at bench_l1.rs:109-110 — "A change that alters the count on purpose moves this literal in the same commit and records the new measured number in the paragraph above" — was broken by the fix wave that wrote it.

**Evidence.**

```
bench_l1.rs:93 — "allocates 107,581 times for 2,000 instantiations, which is 53.79 per"; bench_l1.rs:100-102 — "The bound is the measured count plus 0.5 percent, rounded down: 107,581 + 537 = 108,118. ... so the headroom is 0.26 allocations per iteration."; docs/reference/l1-budget.md:176-178 — "Benchmark A allocates 107,581 times for 2,000 iterations ... The bound is 108,118, the measured count plus 0.5 percent. The count is the same number in the debug profile and in the release profile."

Five consecutive release runs on this box, HEAD:
  benchmark A instantiate (release): ... 107680 allocations, 47 blocked, ring tail "c027d35bab27"   (x5, identical)
Debug run, HEAD:
  benchmark A instantiate (debug): ... 107680 allocations, 47 blocked, ring tail "c027d35bab27"
Same benchmark at the pre-fix commit 5c61670 (git archive into a scratch tree, same box, same toolchain rustc 1.98.0):
  benchmark A instantiate (release): ... 107581 allocations, 47 blocked, ring tail "c027d35bab27"

So 107,581 is the pre-fix count. The FIXM4a renderer change (`(1/4)` for `1/4` on fixtures 07 and 08) moved it to 107,680, and neither bench_l1.rs:93 nor l1-budget.md:176 was updated. 108,118 - 107,680 = 438, which is 0.407 percent and 0.219 per iteration.
```

**Failure scenario.** docs/reference/l1-budget.md section 8 is the M4 deliverable for L1/L2 and states "Build box, 2026-08-27, release profile, one test thread" for its numbers. Run scripts/bench.sh on that box at that commit: benchmark-a.json records `"allocations": {"total": 107680, "per_iteration": 53, "bound": 108118}` while the document says 107,581 and 53.79. The reviewer who compares the artifact to the document sees a 99-allocation drift with nothing in the repository explaining it, and cannot tell a real regression from the unrecorded baseline. The next author who follows the documented derivation re-computes the bound from the recorded base — 107,581 + 0.5 percent = 108,118 — and reproduces today's 0.407 percent margin instead of the intended 0.5 percent; on CI, where .github/workflows/ci.yml:63 installs `dtolnay/rust-toolchain@stable` unpinned and rust-toolchain.toml pins only `channel = "stable"`, a std allocation change of 0.22 per iteration turns this hard-gate assertion red with no code change and no recorded true baseline to compare it against.

**Refuter.** I tried to refute the claim and I failed. Every load-bearing part of it is true at HEAD (3209073, clean tree).

1. The recorded number is stale. `crates/core/tests/bench_l1.rs:93` and `docs/reference/l1-budget.md:176` both state 107,581. The benchmark artifact that HEAD's own code wrote, `/home/deploy/dev/cadus2.0/target/bench/benchmark-a.json`, records `"allocations": {"total": 107680, "per_iteration": 53, "bound": 108118}`. The file has a modification time of 2026-08-27 04:42:43, which is after the HEAD commit at 04:34:01 and after the modification time of `bench_l1.rs` itself (04:34:01), so the artifact is a run of the code at HEAD. The drift is 99 allocations.

2. The bound comes from the stale number. The docstring at `bench_l1.rs:100-101` derives it in the open: "107,581 + 537 = 108,118". 107,581 x 1.005 = 108,118.9, floor 108,118. A derivation from the true count of 107,680 gives 

### #11 [major] The L1 allocation bound is not the measured count plus 0.5 percent: HEAD allocates 107,680, while docs/reference/l1-budget.md and bench_l1.rs both record 107,581

File: `docs/reference/l1-budget.md:176` — IDs: L1 — duplicate of #7

**Claim.** The review-1 ruling on finding #21 set the allocation bound at "the measured count plus 0.5%", and FIXM4c wrote `ALLOCATION_BOUND = 108_118` from a measurement of 107,581 taken before FIXM4a was merged. The merged HEAD allocates 107,680 in the same loop, so the recorded measured count, the recorded per-iteration count, the recorded headroom, and the stated derivation of the bound are all wrong, and no run of the shipped code reproduces the number the budget document publishes.

**Evidence.**

```
docs/reference/l1-budget.md:176-178 — "Benchmark A allocates 107,581 times for 2,000 iterations, which is 53.79 per instance. The bound is 108,118, the measured count plus 0.5 percent. The count is the same number in the debug profile and in the release profile." The same numbers are in crates/core/tests/bench_l1.rs:93 and :100-103 ("the headroom is 0.26 allocations per iteration").

Measured on HEAD (3209073), CADUS_BENCH=1, three runs:
  benchmark A instantiate (release): p50 3580 ns, ..., 107680 allocations, 47 blocked, ring tail "c027d35bab27"
  benchmark A instantiate (release): p50 3630 ns, ..., 107680 allocations, 47 blocked, ring tail "c027d35bab27"
  benchmark A instantiate (debug):   p50 42239 ns, ..., 107680 allocations, 47 blocked, ring tail "c027d35bab27"

The pinned `BLOCKED_IN_THE_RUN` (47) and `RING_TAIL_AFTER_THE_RUN` ("c027d35bab27") assertions pass, so the drawn sequence is unchanged; only the allocation count moved, by the +1 String clone that `Scalar::value` now makes for every numeric-text choice value (`Value::Spelled { text: text.clone(), .. }`, domain.rs:120-127, from FIXM4a).
```

**Failure scenario.** An operator or reviewer opens docs/reference/l1-budget.md section 8, the document M4.md names as the record of the budget split, and runs `scripts/bench.sh`. The published measured count 107,581 and per-instance count 53.79 do not appear; the run prints 107,680 and 53.84 in both profiles. The stated bound derivation is false: 108,118 is measured + 0.407 percent, not measured + 0.5 percent, and the real headroom is 438 allocations over 2,000 iterations (0.219 per iteration), not the 537 and 0.26 both the document and crates/core/tests/bench_l1.rs:100-103 assert. The next reviewer who re-derives the bound from the ruling ("measured + 0.5%") gets 108,218 and changes a literal that did not need changing, and the trend line the artifacts carry starts from a baseline no build produces.

**Refuter.** I tried to refute the claim and I failed. Two runs of the shipped code at HEAD (3209073, clean worktree) give 107,680 allocations, not the 107,581 that docs/reference/l1-budget.md:176-178 and crates/core/tests/bench_l1.rs:93-104 publish. Every derived number in the record is wrong by the same cause. The measured count is 99 too low. The per-instance count is 107,680 / 2,000 = 53.84, not 53.79. The headroom is 108,118 - 107,680 = 438 allocations, which is 0.219 per iteration, not the 537 and 0.26 that both the document and the constant's docstring assert. The stated derivation is false: 108,118 is 107,680 plus 0.407 percent, not the "measured count plus 0.5 percent" that the review-1 ruling set (docs/reviews/M4-review-1.md:38). A reviewer who re-derives the bound from the ruling gets floor(107,680 * 1.005) = 108,218 and moves a literal for no reason. The cause is the one the claim names. 
