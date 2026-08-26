# M2 adversarial review — round 1 (2026-08-26)

Run on the tree at commit fb320ee (M2 U1–U3 integrated). Two find/refute rounds, major+ only: 39 raised, 21 confirmed. Assigned to FIXM2a (normalize/lexer/parse), FIXM2b (canon/check/tests), FIXM2c (oracle harness, regeneration, docs; runs after a and b merge).

## Orchestrator rulings (binding)

- Mixed numbers: a digit run followed by a vulgar-fraction glyph (`2⅓`, `3½`) is the
  mixed number, the same value as `2 1/3` and `3 1/2`. A mixed number `a b/c` requires
  `0 < b < c`, both written as plain digit runs without grouping; `1 000/3` is Undecidable.
- Value labels: a leading `<var> =` on either side becomes `Assign(var, value)`. If the
  expected answer carries no label, the learner's label is ignored (tolerance). If both
  carry a label, the variable names must be equal (casefolded) or the verdict is false.
  `y = x` vs `x = y` stays false (equations are outside the grammar; 1 corpus row).
- Bracket-free function argument: the argument is the following implicit-product chain
  of atoms with their powers (`cos 2x` = `cos(2*x)`, `sin 3t^2` = `sin(3*t^2)`), which
  stops at `+ - , ) = < >` or an explicit `*` or `/`. Match 1.0's reading on the five
  corpus answers and pin them.
- Percent binds to the number it follows (`4%` → 4/100), never to the whole body.
- A spaced single `x` between two numeric literals is multiplication (`3 x 4`); every
  other `x` is the variable. Pin the 27 corpus rows.
- Multi-letter runs: split an unknown letter run into single-letter variables (`3xy^2`
  → `3*x*y^2`) EXCEPT when the run is a function name, a Greek constant name, or starts
  with `d` followed by one letter (a differential: Undecidable); a run that contains a
  digit stays Undecidable. `e` and `E` both stay Euler's number (pinned divergence).
- `ln` and `log` are one function (natural logarithm). `exp(a)` is `e^(a)` for every
  argument; `1/e^x` equals `e^(-x)`.
- Work bound: every rational coefficient and every intermediate (LCM, content) is
  checked against `MAX_BITS` after each operation; exceeding it is Undecidable. Keep
  `MAX_STEPS`. Measure the worst in-grammar corpus case in release and pin it under 50 ms.
- L2 tests: debug-build bounds are 50 ms per check and 5 s for the corpus; a release
  assertion (5 ms per check, 1 s corpus) runs only when `CADUS_RELEASE_BENCH` is set.
- Oracle harness: run each 1.0 call in a child process that is killed on timeout, so
  a timeout is recorded as `timeout: true`, never as a decided `false`.
- After the grammar changes, regenerate the corpus split literals, the undecidable
  fixture, the oracle verdict fixture (live 1.0), and `docs/reference/undecidable-answers.md`;
  annotate spec §8.2/§8.3 with the measured split.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `crates/core/src/answer/normalize.rs:285` | FIXM2a | A vulgar-fraction glyph after a whole number reads as a product, not a mixed number |
| 2 | blocker | `crates/core/src/answer/normalize.rs:164` | FIXM2a | The `x =` / `y =` label strip drops the variable, so an answer for the wrong unknown is correct |
| 3 | blocker | `crates/core/src/answer/parse.rs:436` | FIXM2a | A juxtaposed function argument binds one atom only, so five authored corpus answers (`cos 2x`, `\cos 2t`, `\sin 3t`, …) canonicalize to `x*cos(2)` instead of `cos(2*x)`: the correct learner answer is marked wrong and a meaningless one is marked correct |
| 4 | blocker | `crates/core/src/answer/parse.rs:436` | FIXM2a (dup of #3) | Bracket-free function application binds one atom, so the authored answer `cos 2x` reads as `x*cos(2)` |
| 5 | blocker | `crates/core/src/answer/canon.rs:731` | FIXM2b | `content_normalize` accumulates an unbounded LCM outside the work budget, so one check costs 8.4 s |
| 6 | blocker | `crates/core/src/answer/parse.rs:436` | FIXM2a (dup of #3) | A juxtaposed function argument binds only the first atom, so the corpus answer `cos 2x` canonicalizes to `x*cos(2)` |
| 7 | blocker | `crates/core/src/answer/parse.rs:268` | FIXM2a | The mixed-number production swallows a space-grouped thousands numerator, so `1 000/3` canonicalizes to 1 |
| 8 | major | `crates/core/src/answer/canon.rs:477` | FIXM2b | The `ln` and `log` spellings are two different function atoms, so the change-of-base answer written with the other logarithm is marked wrong on a topic that authors both spellings |
| 9 | major | `crates/core/src/answer/normalize.rs:41` | FIXM2a (dup of #1) | A vulgar fraction glued to a whole number becomes a product, not a mixed number: `3½` canonicalizes to 3/2, so a learner who wrote three and a half is marked correct against `1.5` and wrong against `7/2` |
| 10 | major | `crates/core/src/answer/normalize.rs:164` | FIXM2a (dup of #2) | The `x =` / `y =` prefix strip does not check which variable the answer names, so `x = 3` and `y = 3` are one answer — a false positive 1.0 did not have |
| 11 | major | `crates/core/src/answer/canon.rs:63` | FIXM2b | `MAX_STEPS` charges term operations, not bit operations, so 2,000 steps cost 155 ms in a release build |
| 12 | major | `crates/core/tests/answer_oracle.rs:18` | FIXM2c | The documented live-oracle command runs zero tests and prints ok |
| 13 | major | `crates/core/tests/answer_check.rs:393` | FIXM2b | No test distinguishes an open interval end from a closed one, so three closedness mutants survive the whole M2 suite |
| 14 | major | `crates/core/tests/answer_check.rs:409` | FIXM2b | No test separates a set from a list, so the set-canonicalization mutant survives and makes `{1, 3, 5}` equal `[1, 3, 5]` |
| 15 | major | `crates/core/tests/answer_check.rs:350` | FIXM2b | The only radical-product assertion is the case a broken squarefree merge also passes, so the merge mutant survives |
| 16 | major | `crates/core/src/answer/normalize.rs:158` | FIXM2a (dup of #2) | `strip_value_label` deletes an `x =` / `y =` label from both sides without comparing it, so two different assignments compare equal |
| 17 | major | `crates/core/src/answer/normalize.rs:105` | FIXM2a | A trailing `%` divides the whole normalized body, not the number it attaches to, so `3 + 4%` is graded equal to 7/100 |
| 18 | major | `crates/core/src/answer/parse.rs:233` | FIXM2a | An authored `x` that means times is read as the variable x, so every multiplication spelling is decided wrong |
| 19 | major | `crates/core/src/answer/canon.rs:467` | FIXM2b | A reciprocal of an exponential is not the negative exponent, so 1.0 says equal and 2.0 says wrong |
| 20 | major | `crates/core/tests/answer_check.rs:539` | FIXM2b | The two L2 budget tests assert a 5 ms wall-clock per-answer bound and fail on ordinary load |
| 21 | major | `scripts/oracle/check_1_0.py:66` | FIXM2c | The oracle wall-clock guard is swallowed by 1.0, so a timed-out pair is recorded as a decided 1.0 `false` |

## FIXM2a

### #1 [blocker] A vulgar-fraction glyph after a whole number reads as a product, not a mixed number

File: `crates/core/src/answer/normalize.rs:285` — IDs: C4, V4, V1

**Claim.** `UNICODE_SIMPLE` rewrites `½` into `(1/2)` with no operator between the glyph and the digit in front of it, so `2⅓` becomes `2*(1/3)` = 2/3 instead of the mixed number 7/3, and a wrong learner answer gets `correct = true`.

**Evidence.**

```
Probe against `cadus_core::answer::check` (kind = numeric):
  "2/3" vs "2⅓"  => D correct=true  notation=false
  "1"   vs "2½"  => D correct=true  notation=false
  "3/4" vs "3¼"  => D correct=true  notation=false
  "1/2" vs "2¼"  => D correct=true  notation=false
The same value in ASCII is read correctly, which shows the inconsistency is inside 2.0:
  "9/2"   vs "4 1/2" => D correct=true   (parse.rs:260 `read_mixed_number` -> Ast::Mixed)
  "3 1/2" vs "3½"    => D correct=false  (the two spellings of one mixed number disagree)
normalize.rs:283-289:
    for c in with_powers.chars() {
        match UNICODE_SIMPLE.iter().find(|(from, _)| *from == c) {
            Some((_, to)) => out.push_str(to),
The 1.0 oracle returns False for ("9/2", "4 1/2"), so the mixed-number production is a 2.0 addition and this gap is 2.0's own.
```

**Failure scenario.** A fractions topic authors the answer `2/3`. A learner who believes the answer is two and one third types `2⅓` (the house Unicode style the V4 table exists to accept). `normalize` produces the source `2(1/3)`, the parser reads an implicit product, `canon` gives 2/3, and `check` returns `Verdict { correct: true, notation: false }`. A learner who is wrong by a factor of 3.5 is recorded as correct.

**Refuter.** The claim reproduces exactly and I could not find a ruling, a parity argument, or a guard that excuses it. normalize.rs:283-289 walks UNICODE_SIMPLE one character at a time with no look-back, so ('⅓', "(1/3)") at normalize.rs:42 turns `2⅓` into the source `2(1/3)`. parse.rs:260 `read_mixed_number` only fires on a `Tok::Num` with `space_before: true`, so it never sees the parenthesized fraction, and the `check_implicit_number` guard at parse.rs:236 (which refuses two adjacent numbers) is bypassed because the rewrite wrapped the fraction in parentheses. The parser reads an implicit product and canon gives 2/3. Verified: check("2/3","2⅓") = Decided(correct: true, notation: false), check("1","2½") = true, check("1/2","2¼") = true — a C4 false positive. The mirror error is also present: check("7/3","2⅓") = false, so a learner who types the RIGHT answer in the house Unicode style is marked wro

### #2 [blocker] The `x =` / `y =` label strip drops the variable, so an answer for the wrong unknown is correct

File: `crates/core/src/answer/normalize.rs:164` — IDs: C4, V4, V3, A3

**Claim.** `strip_value_label` removes a leading `x =`, `y =`, `X =`, or `Y =` from both the authored answer and the learner answer without recording which variable it removed, so `x = 4` and `y = 4` produce the same source `4` and compare equal.

**Evidence.**

```
Probe against `check`:
  "x = 3" vs "y = 3" [Expression] => D correct=true  notation=false
  "x = 4" vs "y = 4" [Numeric]    => D correct=true  notation=false
  "x = 3" vs "Y = 3" [Expression] => D correct=true  notation=false
  "y = 2*x + 3" vs "x = 2*x + 3"  => D correct=true  notation=false
`canonical_form("x = 3")` and `canonical_form("y = 3")` both return `Ok(Rational(3/1))`.
normalize.rs:164:
    if !matches!(first, 'x' | 'y' | 'X' | 'Y') {
The 1.0 oracle disagrees, so this is a 2.0 regression and not a carried behavior:
  {"expected":"x = 4","learner":"y = 4","kind":"numeric"} -> {"equivalent": false}
The strip is a 2.0 addition (the doc comment at normalize.rs:157 says so). `crates/core/tests/answer_parse.rs:112` pins only the rewrite `("x = 5", "5")`; no test and no entry in `crates/core/tests/answer_divergence.rs` covers the cross-variable case, so the divergence is undocumented.
```

**Failure scenario.** A simultaneous-equations topic authors the answer `x = 4` (the corpus already carries a labeled answer, `y = x` in `graphs-of-logarithmic-functions`). A learner solves for the other unknown, gets 4, and types `y = 4`. Both sides lose their label, both canonicalize to `Rational(4)`, and `check` returns `Verdict { correct: true, notation: false }`. 1.0 graded the same pair False.

**Refuter.** The claim is demonstrable and no ruling covers it. `strip_value_label` (crates/core/src/answer/normalize.rs:157-172) is called unconditionally from `to_source` (normalize.rs:92), so it runs on the authored answer and on the learner answer alike, and it discards the variable name. `check` (crates/core/src/answer/check.rs:84) then compares the two label-free sources, so a cross-variable pair compares equal.

I reproduced the probe with a scratchpad binary that links `cadus-core` by path. `check("x = 4", "y = 4", Numeric)` returns `Decided(Verdict { correct: true, notation: false })`, and `normalize("x = 3").source` and `normalize("y = 3").source` are both `"3"`. The 1.0 oracle returns `equivalent: false` for the same pair, so this is a 2.0 change, not a carried behavior.

The M2.md V4 row rules that 2.0 adds a "leading `x =`/`y =` prefix" tolerance, and crates/core/tests/answer_divergence.

### #3 [blocker] A juxtaposed function argument binds one atom only, so five authored corpus answers (`cos 2x`, `\cos 2t`, `\sin 3t`, …) canonicalize to `x*cos(2)` instead of `cos(2*x)`: the correct learner answer is marked wrong and a meaningless one is marked correct

File: `crates/core/src/answer/parse.rs:436` — IDs: V4, V3, V1, C4

**Claim.** `parse_call` reads a bracket-free argument with `parse_power()`, which consumes exactly one atom plus one power, so `cos 2x` parses as `Mul(cos(2), x)` while 1.0 reads `cos(2*x)`; five corpus answers on `double-half-angle-identities` and `inverse-laplace-table` are inside the grammar and are silently corrupted, and the checker still claims a deterministic verdict on them.

**Evidence.**

```
parse.rs:433-441
        if !self.starts_operand() {
            return Err(Undecidable::new("a function name with no argument"));
        }
        let argument = self.parse_power()?;
        let call = Ast::Func(name.to_string(), vec![argument]);

Probe (cadus_core::answer::check / canonical_form):
  authored cos 2x                    -> canon Poly({{Var("x"):1, Call("cos",[Rational(2)]):1}: 1})
  authored $2\cos 2t + (5/2)\sin 2t$ -> canon Poly({{Var("t"):1, Call("cos",[Rational(2)]):1}: 2,
                                                    {Var("t"):1, Call("sin",[Rational(2)]):1}: 5/2})
  cos 2x                    vs cos(2x)                   -> correct=false notation=false
  cos 2x                    vs cos(2*x)                  -> correct=false notation=false
  $\cos 2t$                 vs cos(2t)                   -> correct=false notation=false
  $(4/3)\sin 3t$            vs (4/3)*sin(3*t)            -> correct=false notation=false
  $2\cos 2t + (5/2)\sin 2t$ vs 2*cos(2*t) + 2.5*sin(2*t) -> correct=false notation=false
  cos 2x                    vs x*cos(2)                  -> correct=TRUE  notation=false

1.0 oracle (/home/deploy/dev/cadus/.venv/bin/python, cadus_web.sympy_check.answers_equivalent):
  'cos 2x'                    vs 'cos(2x)'                   -> True
  'cos 2x'                    vs 'x*cos(2)'                  -> False
  '$\cos 2t$'                 vs 'cos(2t)'                   -> True
  '$(4/3)\sin 3t$'            vs '(4/3)*sin(3*t)'            -> True
  '$2\cos 2t + (5/2)\sin 2t$' vs '2*cos(2*t) + 2.5*sin(2*t)' -> True

corpus_1_0.jsonl:1016 records the 1.0 canonical form of the authored answer as `cos(2*x)`;
:1668 `cos(2*t)`; :1669 `4*sin(3*t)/3`; :1670 `5*sin(2*t)/2 + 2*cos(2*t)`; :1672 `2*sin(3*t) + cos(3*t)`.
None of the five is in crates/core/tests/fixtures/answers/undecidable_1_0.jsonl, so 2.0 decides all five.
The oracle harness misses this because every generator keeps the juxtaposition on both sides:
oracle_verdicts_1_0.jsonl:5740 pins `cos 2x` vs `cos 2*x` = true, and 2.0 corrupts both sides alike.
```

**Failure scenario.** A learner on `double-half-angle-identities` kp2 exemplar 1 (authored answer `cos 2x`) types the correct `cos(2x)`. `check("cos 2x", "cos(2x)", Expression)` returns `Decided{correct:false}` — the correct answer is recorded as a failed attempt, with XP and FIRe penalties, and A3 means it is no longer handed to the model. The same learner typing the meaningless `x*cos(2)` gets `Decided{correct:true}`. On `inverse-laplace-table` all four authored answers are affected the same way.

**Refuter.** The claim reproduces exactly, in code, against the 1.0 oracle, and in the shipped fixtures. parse.rs:436 reads a bracket-free function argument with parse_power(), which is one atom plus at most one integer power; parse_term then collects the rest of the juxtaposed product as siblings of the call. So `cos 2x` canonicalizes to x*cos(2), while 1.0 parses the same string to cos(2*x). check() returns Decided{correct:false} for the correct learner spellings `cos(2x)` / `cos(2*x)` and Decided{correct:true} for the meaningless `x*cos(2)` — a wrong-correct verdict, which C4 calls a blocker. Five authored corpus answers (corpus_1_0.jsonl:1016, 1668, 1669, 1670, 1672 — double-half-angle-identities and inverse-laplace-table, all kind `expression`) hit this, and I verified none of the five appears in undecidable_1_0.jsonl, so 2.0 claims a deterministic verdict on all five (V2/V3/V4/V1). The parity h

### #4 [blocker] Bracket-free function application binds one atom, so the authored answer `cos 2x` reads as `x*cos(2)`

File: `crates/core/src/answer/parse.rs:436` — IDs: C4, V3, A3, V1 — duplicate of #3

**Claim.** `Parser::parse_call` reads its bracket-free argument with `self.parse_power()`, which takes one atom only, so `cos 2x` parses as `Mul([Func("cos",[2]), Var("x")])` instead of `cos(2*x)`; the checker then returns `correct = true` for a learner answer of a different value and `correct = false` for the right value.

**Evidence.**

```
parse.rs:433-437 —
        if !self.starts_operand() {
            return Err(Undecidable::new("a function name with no argument"));
        }
        let argument = self.parse_power()?;
        let call = Ast::Func(name.to_string(), vec![argument]);

Release-build run against the checker:
  "cos 2x"  <-  "cos(2*x)"   2.0 = Decided(Verdict { correct: false, notation: false })
  "cos 2x"  <-  "x*cos(2)"   2.0 = Decided(Verdict { correct: true,  notation: false })
  canon("cos 2x") = Ok(Poly({{Var("x"): 1, Call("cos", [Rational(2/1)]): 1}: 1/1}))

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  {"equivalent": true,  "id": "a"}   expected "cos 2x", learner "cos(2*x)"
  {"equivalent": false, "id": "b"}   expected "cos 2x", learner "x*cos(2)"

The corpus fixture records the 1.0 reading of the same authored answer:
  crates/core/tests/fixtures/answers/corpus_1_0.jsonl:1016
  {"answer": "cos 2x", ..., "canonical": "cos(2*x)", "topic_id": "double-half-angle-identities", "kp_id": "kp2", "exemplar_index": 1}

A 1,091-pair differential over every corpus answer and its bracketed variant, scored against the 1.0 oracle, gives 5 disagreements, and all 5 are this shape:
  2.0=false 1.0=TRUE  expected="cos 2x" learner="cos(2*x)"
  2.0=false 1.0=TRUE  expected="$\\cos 2t$" learner="$cos(2*t)$"
  2.0=false 1.0=TRUE  expected="$(4/3)\\sin 3t$" learner="$(4/3)sin(3*t)$"
  2.0=false 1.0=TRUE  expected="$2\\cos 2t + (5/2)\\sin 2t$" learner="$2cos(2*t) + (5/2)sin(2*t)$"
  2.0=false 1.0=TRUE  expected="$\\cos 3t + 2\\sin 3t$" learner="$cos(3*t) + 2sin(3*t)$"
(agree=992, 2.0-true/1.0-false=8 — all 8 are 2.0 tolerance gains, not misreadings.)
```

**Failure scenario.** The topic `double-half-angle-identities`, kp2, exemplar 1 authors the answer `cos 2x`. A learner types `cos(2*x)`, which is the same value the corpus records as the 1.0 canonical form. `check("cos 2x", "cos(2*x)", Expression)` returns `Decided(correct: false)`, so a right answer is graded wrong. A second learner types `x*cos(2)`, which is a different function (at x = 1.2, cos(2.4) = -0.737 against 1.2*cos(2) = -0.499). `check("cos 2x", "x*cos(2)", Expression)` returns `Decided(correct: true)` — a false positive (C4). The 1.0 oracle returns the opposite verdict in both cases (V3). At least 21 corpus rows carry this shape (`sin 3t` x6, `cos 2t` x6, `sin 2t` x3, `cos 3t` x3, `cos 2x` x3), and none of them is in `undecidable_1_0.jsonl`. The committed oracle-variant set never catches it, because every generated variant repeats the same misreading on both sides: `{"equivalent": true, "expected": "cos 2x", "learner": "cos 2*x"}` passes because 2.0 reads both sides as `cos(2)*x`.

**Refuter.** The claim is correct and I reproduced every part of it. `Parser::parse_call` (/home/deploy/dev/cadus2.0/crates/core/src/answer/parse.rs:436) reads the bracket-free argument with `self.parse_power()`, which takes one atom plus at most one integer power. For `cos 2x` the atom is `2`, so the call closes at `cos(2)`, and `parse_term` then multiplies the trailing `x` onto it. The AST is `Mul([Func("cos", [Integer(2)]), Var("x")])`, which is `x*cos(2)`, not `cos(2*x)`. That is a different function (at x = 1.2: cos(2.4) = -0.737 against 1.2*cos(2) = -0.499), so the checker grades a right learner answer wrong and a wrong learner answer right. No ruling covers it: docs/plans/M2.md has no fixed decision on bracket-free function application, crates/core/tests/answer_divergence.rs records no such divergence, and none of the affected topics is in crates/core/tests/fixtures/answers/undecidable_1_0.jso

### #6 [blocker] A juxtaposed function argument binds only the first atom, so the corpus answer `cos 2x` canonicalizes to `x*cos(2)`

File: `crates/core/src/answer/parse.rs:436` — IDs: V1, V3, C4, A3 — duplicate of #3

**Claim.** `Parser::parse_call` reads a bracket-less function argument with `parse_power`, which consumes one atom only, so `cos 2x` parses as `cos(2)*x` instead of `cos(2*x)`; five authored corpus answers are read with the wrong meaning, a correct learner is graded wrong, and a wrong learner answer is graded correct.

**Evidence.**

```
parse.rs:433-437
        if !self.starts_operand() {
            return Err(Undecidable::new("a function name with no argument"));
        }
        let argument = self.parse_power()?;
        let call = Ast::Func(name.to_string(), vec![argument]);

Measured with `check`/`canonical_form` linked against crates/core:
  "cos 2x" canon=Ok(Poly({{Var("x"): 1, Call("cos", [Rational(2)]): 1}: 1}))
  "cos 2x" vs "cos(2x)"   -> Decided(Verdict { correct: false, notation: false })
  "cos 2x" vs "cos(2*x)" -> Decided(Verdict { correct: false, notation: false })
  "cos 2x" vs "x*cos(2)" -> Decided(Verdict { correct: true,  notation: false })

1.0 oracle (scripts/oracle/check_1_0.py, /home/deploy/dev/cadus/.venv/bin/python):
  'cos 2x' vs 'cos(2x)'   -> eq=True
  'cos 2x' vs 'cos(2*x)'  -> eq=True
  'cos 2x' vs 'x*cos(2)'  -> eq=False

The corpus fixture itself records the intended meaning:
corpus_1_0.jsonl:1016 {"answer": "cos 2x", ..., "canonical": "cos(2*x)", "canonical_srepr": "cos(Mul(Integer(2), Symbol('x')))", ...}

Affected corpus rows (answer, topic_id, kp_id, exemplar_index):
  'cos 2x' double-half-angle-identities kp2 1
  '$\\cos 2t$' inverse-laplace-table kp1 2
  '$(4/3)\\sin 3t$' inverse-laplace-table kp2 0
  '$2\\cos 2t + (5/2)\\sin 2t$' inverse-laplace-table kp2 1
  '$\\cos 3t + 2\\sin 3t$' inverse-laplace-table <diagnostic> -1

The oracle harness cannot see this: no generator in answer_oracle.rs GENERATORS/MORE_GENERATORS brackets a function argument. The one generator that touches this shape, `generate_explicit_multiplication`, produces "cos 2*x", which both checkers call equal to "cos 2x", so the pair lands in class 3 as agreed.
```

**Failure scenario.** Topic `double-half-angle-identities`, kp2, exemplar 1 has the authored answer `cos 2x`. A learner answers `cos(2x)` — the mathematically correct answer, and the answer 1.0 accepts. `check("cos 2x", "cos(2x)", AnswerKind::Expression)` returns `Decided(Verdict { correct: false })`, and under A3 that wrong verdict goes straight to the learner with no model in the loop. The same call on `x*cos(2)` returns `correct: true`, which is a C4 false positive: 1.0 answers False for that pair.

**Refuter.** The claim is correct and reproducible. `Parser::parse_call` (crates/core/src/answer/parse.rs:436) reads a bracket-less function argument with `parse_power`, and `parse_power` (parse.rs:314) reads one atom plus at most one integer exponent. It never continues into the implicit-multiplication loop of `parse_term` (parse.rs:195-223). So `cos 2x` binds the argument to `2` only, and the trailing `x` becomes a separate factor of the enclosing product: the AST is `cos(2)*x`, not `cos(2*x)`.

I built a probe binary against crates/core and measured the result. The measurements in the claim reproduce exactly, and the 1.0 oracle disagrees with 2.0 on two of the three pairs. That is a V3 parity break, and the `x*cos(2)` pair is a C4 false positive: 2.0 says `correct: true` where 1.0 says False.

The defect is live, not fixture-only. The authored answer is in the curriculum, not only in the corpus fi

### #7 [blocker] The mixed-number production swallows a space-grouped thousands numerator, so `1 000/3` canonicalizes to 1

File: `crates/core/src/answer/parse.rs:268` — IDs: V4, C4, V3

**Claim.** `read_mixed_number` accepts any space-preceded `Tok::Num` as the numerator of a mixed number, with no guard on leading zeros or on the three-digit thousands group that V4 explicitly invites, so a learner who groups thousands with a space inside a fraction gets a value the checker invented.

**Evidence.**

```
parse.rs:267-283 accepts the numerator with no shape guard:
        let Some(Token {
            kind: Tok::Num(numerator),
            space_before: true,
        }) = self.tokens.get(self.at)
        else {
            return Ok(None);
        };
        ...
        if numerator.contains('.') || denominator.contains('.') {
            return Ok(None);
        }

(`strip_thousands_groups` in normalize.rs:440 only fires on a FULL match, so `1 000/3` keeps its space, and `read_mixed_number` runs before `check_implicit_number`, which is the guard that otherwise refuses two numbers side by side.)

Measured:
  canonical_form("1 000/3")      = Ok(Rational(1/1))
  canonical_form("2 000/500")    = Ok(Rational(2/1))
  canonical_form("1 200/300")    = Ok(Rational(5/3))
  check("1",      "1 000/3",   Numeric) -> Decided(Verdict { correct: true })
  check("1",      "1\u{a0}000/3", Numeric) -> Decided(Verdict { correct: true })
  check("2",      "2 000/500", Numeric) -> Decided(Verdict { correct: true })
  check("5/3",    "1 200/300", Numeric) -> Decided(Verdict { correct: true })
  check("1000/3", "1 000/3",   Numeric) -> Decided(Verdict { correct: false })

1.0 oracle on the same pairs:
  '1' vs '1 000/3'      -> eq=False
  '2' vs '2 000/500'    -> eq=False
  '1000/3' vs '1 000/3' -> eq=False

No test and no oracle generator reaches the shape: `thousands_grouped` in answer_oracle.rs only rewrites a row whose whole source is a plain integer, so `1 000/3` is never generated.
```

**Failure scenario.** The authored answer is `1`. A learner in a space-grouping locale — the locale the V4 table is built for, with ' ', U+00A0, U+202F all listed in `SPACE_SEPARATORS` — writes `1 000/3`, meaning 333.33. `check("1", "1 000/3", AnswerKind::Numeric)` returns `Decided(Verdict { correct: true, notation: false })`. 1.0 returns False. The learner is told a wrong answer is right, with no notation flag to hint at the reading.

**Refuter.** The claim is demonstrable and I could not refute it. All three mechanical steps are true in the source, all measured values reproduce exactly, and the 1.0 oracle disagrees on every pair.

1. Order of the guards. `parse_product` calls `read_mixed_number` at parse.rs:209 and reaches `check_implicit_number` only at parse.rs:213-218. `check_implicit_number` is the rule that refuses two numbers side by side, so it never sees this shape.

2. No shape guard on the numerator. `read_mixed_number` (parse.rs:267-283) matches any `Tok::Num` with `space_before: true`. The only rejection is `numerator.contains('.') || denominator.contains('.')`. Nothing rejects a leading zero, and nothing requires the three-digit group that a thousands separator makes.

3. The space survives normalization. `strip_thousands_groups` (normalize.rs:440-448) deletes the separators only when `is_grouped_integer` matches the

### #9 [major] A vulgar fraction glued to a whole number becomes a product, not a mixed number: `3½` canonicalizes to 3/2, so a learner who wrote three and a half is marked correct against `1.5` and wrong against `7/2`

File: `crates/core/src/answer/normalize.rs:41` — IDs: V4, C4 — duplicate of #1

**Claim.** The V4 table rewrites `½` to the parenthesized `(1/2)`, and `Parser::read_mixed_number` only recognizes the `Num WS Num '/' Num` token shape, so `3½` reaches the parser as `3(1/2)` and multiplies out to 3/2; the M2 plan carries over the vulgar-fraction table AND pins the mixed-number fix (`3 1/2` = `7/2`), but the two never compose, and the result is a silently wrong value rather than a refusal.

**Evidence.**

```
normalize.rs:41-45
    ('½', "(1/2)"),
    ('⅓', "(1/3)"),
    ('⅔', "(2/3)"),
    ('¼', "(1/4)"),
    ('¾', "(3/4)"),
parse.rs:260-294 (`read_mixed_number`) matches only `Tok::Num` `Tok::Slash` `Tok::Num` with `space_before: true`.

Probe:
  source of 3½ = "3(1/2)"  canon = Ok(Rational(3/2))
  1.5 vs 3½    -> correct=TRUE  notation=false      (the learner wrote 3.5)
  7/2 vs 3½    -> correct=false notation=false      (the learner wrote 3.5, which IS 7/2)
  2.5 vs 2½    -> correct=false notation=false
  5/2 vs 2 1/2 -> correct=true  notation=false      (the ASCII spelling is fixed, as pinned in answer_divergence.rs:181)
M2.md's V4 row lists "vulgar fractions" as carried over and spec section 8.1 lists the `mixed` production; neither M2.md nor answer_divergence.rs records this reading.
```

**Failure scenario.** On any `numeric` topic whose authored answer is `7/2`, a learner types `3½` (the glyph reached by a mobile keyboard or by an editor that auto-replaces `1/2`). `check("7/2", "3½", Numeric)` returns `Decided{correct:false}` — the correct answer is recorded as a failed attempt. In the other direction, on a topic whose authored answer is `1.5`, the same learner typing `3½` gets `Decided{correct:true}` for an answer that is wrong by 2.

**Refuter.** The claim is demonstrable exactly as written, and it produces a C4 false positive, so I cannot refute it.

Mechanism, confirmed by reading and by execution:
1. `normalize.rs:41-45` (`UNICODE_SIMPLE`) rewrites `½` to the parenthesized `(1/2)`, with no look at the preceding character. So `3½` becomes the source `3(1/2)`.
2. `parse.rs:260-294` (`read_mixed_number`) matches only the token shape `Tok::Num`, `Tok::Slash`, `Tok::Num` with `space_before: true`. `3(1/2)` never reaches it, because the token after `3` is `Tok::LParen`.
3. `parse_term` (`parse.rs:213-218`) accepts an `LParen` as an implicit-multiplication factor. `check_implicit_number` runs only when the next token is `Tok::Num`, so nothing refuses the shape. The result is `Mul([Integer(3), Fraction 1/2])`, and `canon` gives `Rational(3/2)` instead of `7/2`. The value is silently wrong; there is no `Undecidable`, so V2 does not cat

### #10 [major] The `x =` / `y =` prefix strip does not check which variable the answer names, so `x = 3` and `y = 3` are one answer — a false positive 1.0 did not have

File: `crates/core/src/answer/normalize.rs:164` — IDs: V4, C4 — duplicate of #2

**Claim.** `strip_value_label` deletes any leading `x`/`y`/`X`/`Y` followed by `=`, on the expected side as well as the learner side, and it keeps no record of which name it deleted; two answers that name different variables therefore collapse to one canonical value, and a learner who solved for the wrong unknown is recorded correct.

**Evidence.**

```
normalize.rs:158-173
    fn strip_value_label(s: &str) -> &str {
        …
        if !matches!(first, 'x' | 'y' | 'X' | 'Y') {
            return s;
        }
        let rest = chars.as_str().trim_start();
        let Some(after_eq) = rest.strip_prefix('=') else {
            return s;
        };
        let value = after_eq.trim_start();
        if value.is_empty() { s } else { value }

Probe:
  source of 'x = 3' = "3", of 'y = 3' = "3"
  x = 3  vs y = 3  -> correct=TRUE notation=false
  x = -2 vs y = -2 -> correct=TRUE notation=false
1.0 oracle:
  'x = 3'  vs 'y = 3'  -> False
  'x = -2' vs 'y = -2' -> False
M2.md's V4 row says only "leading `x =`/`y =` prefix"; it does not say the two names are interchangeable, and answer_divergence.rs records no such divergence.
```

**Failure scenario.** A topic authors the answer `x = 3` (an intersection abscissa, a solved unknown, an asymptote). A learner solves the system and reports the other unknown, `y = 3`. `check("x = 3", "y = 3", Expression)` returns `Decided{correct:true}` and the wrong answer is recorded as correct with full XP; 1.0 returned False for the same pair. The hole is new in 2.0 and it opens on every authored answer that carries the prefix the M2 plan added the strip for.

**Refuter.** The claim is demonstrable and is not covered by any recorded ruling. `strip_value_label` is applied by `to_source`, and `check` calls `normalize` on BOTH the expected and the learner string (normalize.rs:92, check.rs:97-99). The function returns only the text after the `=` and keeps no record of which of `x`/`X`/`y`/`Y` it removed. The `string_key` rung keeps the name, but that rung only decides the exact-string case; a name mismatch falls through to the source rung, where both names are already gone. I built a probe against the real crate and reproduced every pair: `check("x = 3", "y = 3", Expression)` returns Decided{correct:true, notation:false}, as do `x = -2` vs `y = -2`, `y = x` vs `x = x`, `y = 2*x` vs `x = 2*x`, and `x = 1/2` vs `Y = 0.5`. The 1.0 oracle returns False for all of them, so V3 parity breaks and each pair is a FALSE POSITIVE under C4. The case is not hypothetical: th

### #16 [major] `strip_value_label` deletes an `x =` / `y =` label from both sides without comparing it, so two different assignments compare equal

File: `crates/core/src/answer/normalize.rs:158` — IDs: C4, V4, V1 — duplicate of #2

**Claim.** The 2.0 V4 addition strips a leading `x =` / `y =` from the parser source of both the authored and the learner answer and never compares the two labels, so `x = 5` and `y = 5` become the same value, while the equation `y = x` becomes the bare value `x` and no longer equals its own transposition `x = y`.

**Evidence.**

```
normalize("x = 5").source == "5"   normalize("y = 5").source == "5"
normalize("y = x").source == "x"   normalize("x = y").source == "y"

check("x = 5", "y = 5",     Expression) -> correct=true   <-- FALSE POSITIVE
check("x = -5", "y=-5",    Expression) -> correct=true
check("(2, 3)", "x = (2, 3)", Expression) -> correct=true
check("y = x", "x",        Expression) -> correct=true
check("y = x", "x = y",    Expression) -> correct=false  <-- same line, marked wrong

Live 1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  ("x = 5", "y = 5", expression) -> {"equivalent": false}
So the true verdict is 2.0-only; it is not in crates/core/tests/answer_divergence.rs.

`y = x` is a real corpus answer: topic `graphs-of-logarithmic-functions`
(the one `equation` row, pinned as parsed by crates/core/tests/answer_parse.rs:501
`("equation", 1, 0)`). docs/reference/checker-1.0-spec.md:8.3 recommends for this row
"compare as `canon(lhs - rhs) == 0` up to a nonzero rational scale"; the shipped code
deletes the label instead.
```

**Failure scenario.** Topic `graphs-of-logarithmic-functions` serves the exemplar whose authored answer is `y = x`. A learner who answers `x = y` — the identical line — gets `Decided { correct: false }` and a permanent miss event, while a learner who answers the bare `x`, which names no line at all, gets `correct = true`. On any future A1 template that authors `x = 5` (M2.md adds the prefix rule precisely so authors may write it), a learner who solves for the wrong variable and writes `y = 5` gets `correct = true`; 1.0 returns False for that pair.

**Refuter.** The claim is demonstrable and it is a C4 false positive. `strip_value_label` (crates/core/src/answer/normalize.rs:158) runs inside `to_source`, and `to_source` runs on both sides in `check` (crates/core/src/answer/check.rs:96-97), so the label leaves the authored answer and the learner answer, and nothing compares the two labels. I ran the shipped code and reproduced all eight evidence lines exactly. The live 1.0 oracle returns false for ("x = 5","y = 5",expression) and for ("x = -5","y=-5",expression), so 2.0 marks a wrong answer correct against the parity target (V3, C4). The pair is in no divergence row: crates/core/tests/answer_divergence.rs:187 pins only ("5","x=5") -> true, the one-sided tolerance that docs/plans/M2.md rules in. Three points defeat the "it is a ruling, not a finding" defense. First, M2.md names the addition a V4 learner-notation tolerance ("leading `x =`/`y =` pref

### #17 [major] A trailing `%` divides the whole normalized body, not the number it attaches to, so `3 + 4%` is graded equal to 7/100

File: `crates/core/src/answer/normalize.rs:105` — IDs: C4, V4

**Claim.** `to_source` strips one trailing `%` from the collapsed string and then wraps the ENTIRE remaining body in `({body})/100`, so the percent silently rescales every term of a sum instead of only the number it follows, and `check` returns `correct = true` for a learner answer whose value is a hundredfold different.

**Evidence.**

```
normalize.rs:87-90 `let (body, is_percent) = match collapsed.strip_suffix('%') { Some(rest) => (rest.trim_end(), true), None => (collapsed, false) };` and normalize.rs:105-109 `let body = if is_percent { format!("({body})/100") } else { body };`

Run against the built crate:
  canonical_form("3 + 4%")  = Ok(Rational(7/100))
  canonical_form("7/100")   = Ok(Rational(7/100))
  check("7/100", "3 + 4%",  Numeric) = Decided(Verdict { correct: true, notation: false })
  canonical_form("1 + 49%") = Ok(Rational(1/2))
  check("1/2",   "1 + 49%", Numeric) = Decided(Verdict { correct: true, notation: false })
  check("3/50",  "2 + 4%",  Numeric) = Decided(Verdict { correct: true, notation: false })

The trailing `%` is a 2.0-only addition (docs/plans/M2.md V4 row: "New in 2.0: ... trailing `%`"), so no 1.0 parity covers the scope error.
```

**Failure scenario.** A numeric-kind item whose authored answer is `7/100`. A learner submits `3 + 4%`, which reads as 3 + 0.04 = 3.04. `to_source` produces `(3 + 4)/100`, so the canonical form is 7/100 and `check` returns `correct = true` for an answer that is 43 times the authored value. Likewise `check("1/2", "1 + 49%") = correct: true` although `1 + 49%` is 1.49, not 0.5.

**Refuter.** The claim is demonstrable and I could not refute it. crates/core/src/answer/normalize.rs:87-90 strips one trailing '%' from the entire collapsed string, and :105-109 wraps the WHOLE remaining body in "({body})/100". The percent therefore rescales every term of a sum rather than only the literal it follows. I built a scratch crate against crates/core and reproduced every result the reviewer listed, verbatim. No ruling covers this: docs/plans/M2.md:29-30 lists "trailing `%`" as a 2.0 V4 addition without fixing a scope; crates/core/tests/answer_divergence.rs:185-186 pins only ("0.5","50%",N); crates/core/tests/answer_parse.rs:111 pins only ("50%","(50)/100"). Every pinned case is a bare number, so the compound-body scope is untested and undecided. V3 oracle parity is no defense either — the divergence comment at answer_divergence.rs:185 and docs/reference/checker-1.0-spec.md:647 both record

### #18 [major] An authored `x` that means times is read as the variable x, so every multiplication spelling is decided wrong

File: `crates/core/src/answer/parse.rs:233` — IDs: V4, A3, C4

**Claim.** The parser reads a spaced `x` between numbers as the variable `x`, so the 27 corpus answers that write multiplication as `x` canonicalize to a polynomial, and every learner spelling of the same product (`×`, `\times`, `·`, `*`, or the evaluated value) is decided wrong.

**Evidence.**

```
parse.rs:233 states the intent — "A space makes it a product, which is how `6 x 10**3` reads" — but parse.rs:407 (`if name.chars().count() == 1 { return Ok(Ast::Var(name.to_string())) }`) makes `x` a variable, so the product carries it.
Probe against `check`:
  FALSE  expected=6 x 10^3   learner=6 × 10^3   exp_canon=Poly({{Var("x"):1}: 6000})
  FALSE  expected=6 x 10^3   learner=6*10^3
  FALSE  expected=2 x 2 x 3  learner=2 × 2 × 3  exp_canon=Poly({{Var("x"):2}: 12})
`crates/core/tests/answer_parse.rs:246` pins this parse (`ast("6 x 10^3") == Mul([6, var("x"), 10^3])`), but no rule in docs/plans/M2.md or crates/core/tests/answer_divergence.rs rules on the verdict, and none of the 27 answers is in the excluded residue (`crates/core/tests/fixtures/answers/undecidable_1_0.jsonl` holds 0 of them).
```

**Failure scenario.** 27 authored corpus answers of 6 topics (prime-factorization, scientific-notation, scientific-notation-addition-subtraction, scientific-notation-conversion, whole-number-exponents), all `answer_kind: expression`, write the times sign as `x`: `6 x 10^3`, `2 x 2 x 3`, `2.5 x 10^-4`, `5 x 5 x 5`. A learner answers `6 × 10^3` — the exact glyph the M2.md V4 tolerance row promises. `check("6 x 10^3", "6 × 10^3", Expression)` returns `Decided { correct: false }`, because the authored side is 6000*x and the learner side is 6000. `6*10^3`, `6 · 10^3`, `6 \times 10^3`, and `6000` all fail the same way. The mirror hole is a false positive: the meaningless `12x^2` is decided correct against the authored `2 x 2 x 3`.

**Refuter.** The claim is demonstrable, and I found no ruling that covers it. I reproduced every probe against the built `check`, and I confirmed the count, the kind, and the live authoring.

What I tried, to refute the claim:

1. Oracle parity (V3). The 1.0 oracle gives the same verdicts. `answers_equivalent('6 x 10^3', '6 × 10^3', 'expression')` is `False`, and `answers_equivalent('2 x 2 x 3', '12x^2', 'expression')` is `True`. The corpus fixture records the 1.0 canonical forms `6000*x` and `12*x**2`. So `parse.rs` is in parity with 1.0, and V3 is not broken. But parity does not clear the claim. M2.md fixes the A3 change: 1.0 handed a wrong verifiable answer to the model, and 2.0 decides it. The 1.0 `False` was a hand-off; the 2.0 `Decided { correct: false }` is a final wrong verdict for the learner. The harm is new in 2.0, and C4 ranks a false positive above every budget.

2. An existing ruling. T

## FIXM2b

### #5 [blocker] `content_normalize` accumulates an unbounded LCM outside the work budget, so one check costs 8.4 s

File: `crates/core/src/answer/canon.rs:731` — IDs: L2, V1

**Claim.** `content_normalize` folds an LCM over every coefficient denominator with no size check inside the loop and no charge against `MAX_STEPS`, so the intermediate grows past `MAX_BITS` without limit and one `check` of a 2,072-character in-grammar answer burns 8.4 s of CPU in a release build, which is 28 times the 300 ms of L2.

**Evidence.**

```
canon.rs:726-745 —
fn content_normalize(sum: &Poly) -> Result<(BigRational, Poly), Undecidable> {
    let mut numerator_gcd = BigInt::zero();
    let mut denominator_lcm = BigInt::one();
    for coefficient in sum.values() {
        numerator_gcd = numerator_gcd.gcd(coefficient.numer());
        denominator_lcm = denominator_lcm.lcm(coefficient.denom());   // <- no bound, no spend
    }
    ...
    let magnitude = BigRational::new(numerator_gcd, denominator_lcm);
    let content = bounded(...)?;   // the size bound runs only AFTER the work is spent

The caller charges the term count only (canon.rs:449-450):
        self.spend(sum.len())?;
        let (content, primitive) = content_normalize(&sum)?;

Release build (`cargo build --release`, rustc 1.98.0), best of three runs of `check("1", bomb, Expression)`:
    308.0 ms  learner_len=416   -> Undecidable("a number past the size bound")
   8377.8 ms  learner_len=2072  -> Undecidable("a number past the size bound")

The LCM growth is the cause. Same shape, same term count, one prime instead of distinct primes:
  A distinct primes 128 terms exp250: len=1624 canon=4.2s   -> Undec(a number past the size bound)
  D one prime       128 terms exp250: len=1395 canon=5.6ms  -> Ok
That is a 750x difference from the same production at the same size.

The 416-character input, verbatim:
1/(a/3**500+b/5**500+c/7**500+d/11**500+f/13**500+g/17**500+h/19**500+i/23**500+j/29**500+k/31**500+l/37**500+m/41**500+n/43**500+o/47**500+p/53**500+q/59**500+r/61**500+s/67**500+u/71**500+v/73**500+w/79**500+x/83**500+y/89**500+z/97**500+A/101**500+B/103**500+C/107**500+D/109**500+F/113**500+G/127**500+H/131**500+I/137**500+J/139**500+K/149**500+L/151**500+M/157**500+N/163**500+O/167**500+P/173**500+Q/179**500)
```

**Failure scenario.** A learner pastes a 2,072-character answer of the form `1/(a/3**250 + b**2/5**250 + ... )` into the answer box. Every character is inside the §8.1 grammar and the input is under the 4,000-character cap of `MAX_ANSWER_CHARS`, so `check` reaches rung 3 and calls `canon`. `reciprocal` charges 160 steps of the 2,000-step budget, then `content_normalize` folds 160 coprime denominators of about 580 bits each into one LCM of about 93,000 bits. The gcd and multiplication cost of that fold is charged nothing. `check` returns `Undecidable("a number past the size bound")` after 8.4 s. The module header at canon.rs:36-41 states "the cost of a check depends on the input only (L2)"; the cost here grows with the LCM, not with the input length, and it has no upper bound at all. One request from one unauthenticated answer box holds one core for 8.4 s.

**Refuter.** The claim is correct, and I reproduced it in a release build. `content_normalize` at /home/deploy/dev/cadus2.0/crates/core/src/answer/canon.rs:726-745 folds an LCM over every coefficient denominator. The loop has no size check and no `spend` call. The only size check, `bounded(magnitude)?`, runs after the fold completes, so the work is already spent when the refusal is returned. The caller at canon.rs:449-450 charges `sum.len()` steps (160 of the 2,000 of MAX_STEPS) and charges nothing for the fold itself. Each individual denominator stays under MAX_BITS, so no per-coefficient bound catches the input; the intermediate LCM is the only value that goes past MAX_BITS, and nothing measures it until the fold ends. Measured cost: 332.7 ms for the verbatim 416-character input, and 8.53 s for a 2,214-character input of the same shape. Both are in-grammar and under the 4,000-character MAX_ANSWER_C

### #8 [major] The `ln` and `log` spellings are two different function atoms, so the change-of-base answer written with the other logarithm is marked wrong on a topic that authors both spellings

File: `crates/core/src/answer/canon.rs:477` — IDs: V4, V3

**Claim.** `Work::call` keys `Atom::Call` by the function spelling, and `ln` and `log` are two separate entries of `parse::FUNCTIONS`, so `log(12)/log(5)` and `ln(12)/ln(5)` are different values; 1.0 (SymPy, where `ln` is an alias of `log`) calls them equal, and the corpus authors both spellings on the same topic, `change-of-base-formula`, where the base of the ratio is mathematically irrelevant.

**Evidence.**

```
canon.rs:459-478 (`fn call`)
        Ok(atom_value(Atom::Call(name.to_string(), arguments)))
parse.rs:17-20
    const FUNCTIONS: [&str; 17] = [ …, "exp", "ln", "log", "abs" ];
normalize.rs has no `ln`→`log` rewrite entry.

Probe:
  log(12)/log(5) vs ln(12)/ln(5) -> correct=false notation=false
  ln(7)/ln(3)    vs log(7)/log(3) -> correct=false notation=false
1.0 oracle:
  'log(12)/log(5)' vs 'ln(12)/ln(5)' -> True
  'ln(7)/ln(3)'    vs 'log(7)/log(3)' -> True

Corpus rows on the SAME topic `change-of-base-formula`:
  kp1 exemplar 0 | log(12)/log(5)
  kp1 exemplar 1 | ln(7)/ln(3)
Neither M2.md nor crates/core/tests/answer_divergence.rs records this divergence.
```

**Failure scenario.** A learner on `change-of-base-formula` kp1 exemplar 0 (authored `log(12)/log(5)`) applies the change-of-base formula with the natural logarithm and types `ln(12)/ln(5)` — the same number, and the spelling the sibling exemplar of the same knowledge point authors. `check("log(12)/log(5)", "ln(12)/ln(5)", Expression)` returns `Decided{correct:false}`; 1.0 returned True.

**Refuter.** The claim holds. I tried to refute it on four fronts and each attempt failed.

1. The mechanism is real. `canon.rs:477` returns `Atom::Call(name.to_string(), arguments)`, so the function spelling is part of the value key. `parse.rs:17-20` lists `"ln"` and `"log"` as two separate entries of `FUNCTIONS`. `normalize.rs` contains no match for `ln` or `log` at all, so no rewrite folds one spelling into the other. The lexer contains no alias either.

2. The probe reproduces. I built a separate crate in the scratchpad against `crates/core` (the repo stayed read-only, and no file entered `crates/core/tests`). Results: `log(12)/log(5)` against `ln(12)/ln(5)` gives `Decided{correct:false, notation:false}`, and `ln(7)/ln(3)` against `log(7)/log(3)` gives the same. The verdict is Decided, not Undecidable, so V2 gives no fallback to the model. The learner sees a wrong verdict.

3. The 1.0 oracle disa

### #11 [major] `MAX_STEPS` charges term operations, not bit operations, so 2,000 steps cost 155 ms in a release build

File: `crates/core/src/answer/canon.rs:63` — IDs: L2

**Claim.** `Work::spend` charges one unit per term pair in `multiply` and one unit per term in `add`, and it never charges for the operand width, so a step that the doc comment prices at 70 microseconds costs up to 75 microseconds in a RELEASE build on 4,096-bit rational coefficients; a 20-character in-grammar answer therefore spends 155 ms, which is 52% of the whole 300 ms of L2.

**Evidence.**

```
canon.rs:57-63 —
/// The number is a latency bound, not a taste. A debug build runs one operation
/// in about 70 microseconds on the build box, so the budget holds one check well
/// inside the 300 ms of L2.
const MAX_STEPS: usize = 2_000;

canon.rs:381-383 — the charge counts terms, never bits:
        let left = to_sum(left)?;
        let right = to_sum(right)?;
        self.spend(left.len().saturating_mul(right.len()))?;

Release build, best of three runs:
    154.5 ms  expected_len=1 learner_len=20  check("1", "((7/3)**23*x+y)**616")
            -> Undecidable(Undecidable { reason: "a number past the size bound" })
    146.8 ms  check("(x+(7/3)**23)**511", "(x+(7/3)**23)**512")
            -> Undecidable(Undecidable { reason: "the answer goes past the work bound" })

A 60 s structured fuzz (333,023 cases, no panic) found the same class on its own:
  worst=112.150793ms worst_len=105 worst_in="([2, (((e+(7/3)**17))**616/[(log(7/3, 0.5)*(y*sqrt(2))), 0.5])]*-[-1, ((-2)**377/log(sqrt(2), (0.5/3)))])"

The committed L2 tests never reach this path. `crates/core/tests/answer_check.rs:511` and `:539` assert `worst < Duration::from_millis(5)` over the benign corpus only, and `answer_oracle.rs:1915` (`ten_seconds_of_corpus_token_soup_never_panics`) asserts no panic and measures no time.
```

**Failure scenario.** A learner types the 20-character answer `((7/3)**23*x+y)**616`. Every token is inside the §8.1 grammar. `canon` raises a two-term polynomial by binary exponentiation; the term count stays under `MAX_TERMS` for the first six squarings, so the 2,000-step budget is spent almost in full, and every one of those steps multiplies `BigRational` values whose numerator and denominator sit just under the 4,096-bit `MAX_BITS`. Each step therefore costs about 75 microseconds of gcd and big-integer multiplication in a release build, not the 70 microseconds the comment claims for a DEBUG build. One `check` call returns `Undecidable` after 155 ms and leaves 145 ms for the HTTP handler, the database write, and everything else in the 300 ms of L2. The budget constant is calibrated against the wrong quantity: it bounds the count of coefficient operations while the cost of one operation varies by a factor of about 40 with the operand width.

**Refuter.** The claim is demonstrable, and my measurements are worse than the reviewer's, not better. The mechanism is exactly as stated: `Work::spend` (crates/core/src/answer/canon.rs:192) subtracts a count of coefficient operations only. `multiply` charges `left.len() * right.len()` (canon.rs:381-383) and `add` charges 1 per right-hand term (canon.rs:370-374). Neither charge reads `BigRational` width, while `bounded` (canon.rs:553) admits numerators and denominators up to `MAX_BITS = 4_096` bits. The unit price of one step therefore varies by about two orders of magnitude, and `MAX_STEPS = 2_000` bounds the wrong quantity.

I built a release binary against the crate and drove the public `check`/`canonical_form` API. `check("1", "((7/3)**23*x+y)**616", AnswerKind::Expression)` — a 20-character, fully in-grammar learner answer — takes 173 ms (best of 5) and returns `Undecidable("a number past the si

### #13 [major] No test distinguishes an open interval end from a closed one, so three closedness mutants survive the whole M2 suite

File: `crates/core/tests/answer_check.rs:393` — IDs: V1, C4

**Claim.** Every range assertion in the M2 suite pins a closed end, so canon.rs can be made to force `lo_closed`/`hi_closed` to `true` on chains, on simple inequalities, and on bracket intervals without any test failing — and each of those mutations turns a strictly different answer into `correct = true`.

**Evidence.**

```
answer_check.rs:385-405 pins only closed ends:
    assert_eq!(form("-1 \u{2264} x \u{2264} 3"), expected);   // lo_closed: true, hi_closed: true
    assert_eq!(form("-1 <= x <= 3"), expected);
    assert_eq!(form("3 >= x >= -1"), expected);
    let open_below = Canon::Interval { ..., hi_closed: true };
    assert_eq!(form("x <= -1"), open_below);
(answer_parse.rs:289-346 pins open ends at the Ast level only; canon.rs is what the mutants touch.)

Mutation run on a sandbox copy of crates/core, against answer_check + answer_parse + answer_divergence + answer_oracle (the 14,875-pair parity test included):
  chain_closed   (canon.rs:308-324 Chain -> lo_closed: true, hi_closed: true): killed_by=0
  ineq_closed    (canon.rs:292-298 Lt|Le -> hi_closed: true):                  killed_by=0
  bracket_closed (canon.rs:273-288 Interval -> both true):                     killed_by=0
For comparison, the mutants the suite does kill: radical_extract killed_by=2, poly_merge killed_by=1, decimal_scale killed_by=6.

Verdicts under each surviving mutant (probe linked against the mutated crate):
  chain_closed:   check("-1 <= x <= 3", "-1 < x < 3", E) -> correct: true   (baseline false)
  ineq_closed:    check("x <= 3",       "x < 3",       E) -> correct: true   (baseline false)
  bracket_closed: check("(0, 1]",       "[0, 1)",     E) -> correct: true   (baseline false)

1.0 answers False for `x > 4` vs `x >= 4` and for `(0, 1]` vs `[0, 1)`, so the mutants also break V3 without the parity test noticing.

The corpus holds 29 parsing interval_ineq answers, of which `x > 4`, `x > -3`, `x > 0`, `x < 5`, `x < 10`, `n < 8`, `x > 5`, `x > 3` are strict — exactly the rows the mutants would mis-grade.
```

**Failure scenario.** Insert `lo_closed: true, hi_closed: true` into the `Ast::Chain` arm of `canon.rs:317-323` and run `cargo test -p cadus-core --test answer_check --test answer_parse --test answer_divergence --test answer_oracle`: every test passes. The shipped checker now answers `Decided(Verdict { correct: true })` for `check("-1 <= x <= 3", "-1 < x < 3", Expression)`, and for the corpus row `x > 4` (topic with a strict inequality answer) it accepts the learner answer `x >= 4`. No regression test in M2 reports it.

**Refuter.** I could not refute the claim; the mutation experiment reproduces it exactly, and the gap is wider than claimed. On a sandbox copy of the repo (originals untouched), the baseline suite passes 71 tests across answer_check + answer_parse + answer_divergence + answer_oracle. Each of the three named canon.rs mutations passes all 71 tests (killed_by=0) and each produces a false positive: chain_closed makes check("-1 <= x <= 3", "-1 < x < 3", E) correct=true; ineq_closed makes check("x <= 3", "x < 3", E) correct=true; bracket_closed makes check("(0, 1]", "[0, 1)", E) correct=true. Baseline returns correct=false for all three. A fourth sibling mutation (IneqOp::Gt|Ge -> lo_closed: true) survives the ENTIRE cadus-core suite (all targets, 166 tests) and accepts learner "x >= 4" for expected "x > 4" - the corpus row the claim names. Cause of the coverage gap: answer_check.rs:385-405 pins only close

### #14 [major] No test separates a set from a list, so the set-canonicalization mutant survives and makes `{1, 3, 5}` equal `[1, 3, 5]`

File: `crates/core/tests/answer_check.rs:409` — IDs: V1, C4

**Claim.** `a_set_is_unordered_and_a_list_and_a_tuple_are_ordered` only checks reordering inside each collection kind; nothing asserts that a set and a list of the same members are different answers, nor that a repeated set member collapses, so replacing `Canon::Set` with a sorted `Canon::List` passes the entire M2 suite while creating a C4 false positive.

**Evidence.**

```
answer_check.rs:408-414 — the whole set/list test:
    assert_eq!(check("{1, 3, 5}", "{5, 3, 1}", E), decided(true, false));
    assert_eq!(check("[-3, 3]", "[3, -3]", E), decided(false, false));
    assert_eq!(check("(4, 17)", "(17, 4)", N), decided(false, false));
    assert_eq!(check("(4, 17)", "(4, 17.0)", N), decided(true, false));

canon.rs:146 documents a rule no test pins: "An unordered set. Repeated members collapse into one member."

Mutation (canon.rs:272, `Ast::Set(items)` -> sort the items and return `Canon::List`) run against answer_check + answer_parse + answer_divergence + answer_oracle: killed_by=0.

Verdicts under the surviving mutant:
  check("{1, 2}", "[1, 2]",    E) -> correct: true   (baseline false)
  check("{1, 2}", "{1, 2, 2}", E) -> correct: false  (baseline true)
  canonical_form("{1, 2}") = Ok(List([Rational(1), Rational(2)]))   // identical to [1, 2]

1.0 oracle: '{1, 3, 5}' vs '[1, 3, 5]' -> eq=False; '{1, 3, 5}' vs '{1, 3, 5, 5}' -> eq=True. Both baseline rules are right and both are unguarded.

The corpus holds all five set_or_list answers — `[-3, 3]`, `{1, 3, 5}`, `{2, 5}`, `{2, 4, 6}`, `{1, 3}` — so a set and a list of the same numbers are live authored answers.
```

**Failure scenario.** Change `canon.rs:272` to sort the members and return `Canon::List` instead of `Canon::Set` and run the four answer test binaries: all pass, the 14,875-pair oracle parity test included. The shipped checker then answers `Decided(Verdict { correct: true })` for `check("{1, 3, 5}", "[1, 3, 5]", Expression)` — the corpus answer `{1, 3, 5}` accepting the ordered list `[1, 3, 5]`, which 1.0 rejects — and answers `correct: false` for the repeated-member form `{1, 3, 5, 5}`, which 1.0 accepts.

**Refuter.** The claim is demonstrable exactly as written. I copied the repo to a scratchpad, confirmed a clean baseline, then applied the stated mutation at crates/core/src/answer/canon.rs:272 (sort the members and return Canon::List instead of Canon::Set). With the mutant live, the entire cadus-core test suite passes: answer_check 30, answer_divergence 13, answer_oracle 8 (the recorded 14,875-pair parity test included), answer_parse 20, plus arena 27, lint 25, loader 28, parity 12, purity 2. killed_by=0. A probe test inside the mutated tree prints check("{1, 3, 5}", "[1, 3, 5]", Expression) -> Decided(Verdict { correct: true }) where the baseline gives false, and check("{1, 3, 5}", "{1, 3, 5, 5}", Expression) -> correct: false where the baseline gives true; canonical_form("{1, 3, 5}") becomes byte-identical to canonical_form("[1, 3, 5]"). The 1.0 oracle gives eq=False for set vs list and eq=True fo

### #15 [major] The only radical-product assertion is the case a broken squarefree merge also passes, so the merge mutant survives

File: `crates/core/tests/answer_check.rs:350` — IDs: V1, D6, C4

**Claim.** `a_radical_is_reduced_to_a_squarefree_radicand` pins `sqrt(2)*sqrt(3) == sqrt(6)`, which is exactly the product whose merged radicand is already squarefree, so removing the `extract_square` call from the two-root merge in `add_atom` changes no assertion in the M2 suite while breaking every product whose merge does carry a square.

**Evidence.**

```
answer_check.rs:341-350 — the whole radical-product coverage:
    // sqrt(2)*sqrt(3) is sqrt(6), and 1/sqrt(2) is sqrt(2)/2.
    let root_six = Canon::Radical(BTreeMap::from([(Basis { radicand: BigInt::from(6), pi: 0, e: 0 }, whole(1))]));
    assert_eq!(form("sqrt(2)*sqrt(3)"), root_six);

canon.rs:670-675 is the line the mutation removes:
    monomial.remove(&Atom::Sqrt(present.clone()));
    let (outside, merged) = extract_square(&(present * radicand))?;
    *coefficient = bounded(&*coefficient * BigRational::from_integer(outside))?;

Mutation `radical_merge` (replace that with `let (outside, merged) = (BigInt::one(), present * radicand);`) against answer_check + answer_parse + answer_divergence + answer_oracle: killed_by=0. The neighbouring mutant `radical_extract`, which disables the extraction in `root_of_integer` instead, is killed by 2 tests — so the suite guards `sqrt(8) -> 2*sqrt(2)` but not the merge path that reaches the same rule.

Under the surviving mutant:
  canonical_form("sqrt(2)*sqrt(6)") = Ok(Radical({Basis { radicand: 12, ... }: 1}))   // baseline: 2*sqrt(3)
  canonical_form("sqrt(2)*sqrt(2)") = Ok(Radical({Basis { radicand: 4, ... }: 1}))    // baseline: Rational(2)
  check("2*sqrt(3)", "sqrt(2)*sqrt(6)", E) -> correct: false   (baseline true)
  check("2",         "sqrt(2)*sqrt(2)", N) -> correct: false   (baseline true)
  check("6",         "sqrt(12)*sqrt(3)", N) -> correct: false  (baseline true)

1.0 oracle: '2*sqrt(3)' vs 'sqrt(2)*sqrt(6)' -> eq=True, so the mutant also breaks V3 silently.
```

**Failure scenario.** Drop the `extract_square` call from the two-root merge in `canon.rs:671` and run `cargo test -p cadus-core --test answer_check --test answer_parse --test answer_divergence --test answer_oracle`: 49 tests pass, nothing reports. The shipped checker then answers `Decided(Verdict { correct: false })` for `check("2", "sqrt(2)*sqrt(2)", Numeric)` and for `check("2*sqrt(3)", "sqrt(2)*sqrt(6)", Expression)`, marking a correct learner wrong under A3 on the whole class of radical products whose merged radicand carries a square.

**Refuter.** I could not refute it; the claim reproduces exactly as stated. The `radical_merge` mutant survives all 71 tests of the four M2 answer targets on a warm build, twice, while changing the shipped verdict from correct:true to correct:false for every product of two integer radicals whose merged radicand carries a square (sqrt(2)*sqrt(2), sqrt(2)*sqrt(6), sqrt(12)*sqrt(3)). The single pinned product, `sqrt(2)*sqrt(3) == sqrt(6)` at crates/core/tests/answer_check.rs:350, has an already-squarefree merged radicand of 6, so it is insensitive to the extraction; `sqrt(8)` covers only the other `extract_square` call site in `root_of_integer` (canon.rs:495). No fixture row in crates/core/tests/fixtures/answers/*.jsonl is a product of two integer radicals, so the oracle and parity tests never reach the merge either. The 1.0 oracle answers True for all three pairs, so the gap hides a V3 divergence as we

### #19 [major] A reciprocal of an exponential is not the negative exponent, so 1.0 says equal and 2.0 says wrong

File: `crates/core/src/answer/canon.rs:467` — IDs: V3, V4, A3

**Claim.** `Work::call` folds `exp` into the atom `e` only when the argument is a whole number, so `exp(t)` with a symbolic argument stays an opaque `Atom::Call`, and `e^(-x)` and `1/e^x` become two different canonical values.

**Evidence.**

```
canon.rs:462-475: `let only = arguments.first().and_then(integer_value); if let Some(value) = only { ... if name == "exp" && let Some(exponent) = value.to_i64() { ... add_atom(..., &Atom::E, exponent) ... } }`. With a `Poly` argument the code falls through to `Ok(atom_value(Atom::Call(name, arguments)))`, so `Call("exp",[-x])` and `Call("exp",[x])^-1` never meet.
Probe against `check`:
  FALSE  expected=e^(-x)                    learner=1/e^x
  FALSE  expected=e^(-x)(2x - x^2)          learner=(2x - x^2)/e^x
  FALSE  expected=-2x e^(-x^2)              learner=-2x/e^(x^2)
  FALSE  expected=$-(x^2 + 2x + 2)/e^x + C$ learner=-(x^2 + 2x + 2)e^(-x) + C
1.0 oracle (`/home/deploy/dev/cadus/.venv/bin/python scripts/oracle/check_1_0.py`) on the same four pairs: `{"equivalent": true}` for every one. `crates/core/tests/answer_divergence.rs` documents no such divergence.
```

**Failure scenario.** `crates/core/tests/fixtures/answers/corpus_1_0.jsonl` authors `$-(x^2 + 2x + 2)/e^x + C$` (tabular-integration-by-parts, `answer_kind: expression`). A learner writes the same antiderivative with the negative exponent, `-(x^2 + 2x + 2)e^(-x) + C`. `check` returns `Decided { correct: false }`, and A3 makes that final. The 1.0 checker returns True on the identical pair, so this is a V3 parity break that no divergence row covers. The same break marks `-2x/e^(x^2)` wrong against the authored `-2x e^(-x^2)`, and `(2x - x^2)/e^x` wrong against the authored `e^(-x)(2x - x^2)`.

**Refuter.** The claim is demonstrable, and I reproduced every part of it. `crates/core/src/answer/parse.rs:311-321` turns `e**t` into the whitelisted call `exp(t)`, so `e` is the one base with a free exponent. `Work::call` (`crates/core/src/answer/canon.rs:461-477`) folds `exp` into `Atom::E` only when `integer_value(argument)` gives a whole number. A `Poly` argument falls through to `Ok(atom_value(Atom::Call(name, arguments)))`. The result: `e^(-x)` canonicalizes to the atom `Call("exp",[-x])` with exponent +1, and `1/e^x` canonicalizes to the atom `Call("exp",[x])` with exponent -1. The two monomial keys differ, so `check` rung 3 fails, rung 4 (dot grouping) does not apply, and rung 5 returns `Decided { correct: false }`. Every other atom in the canonical form obeys the exponent law, because `add_atom` adds exponents; base `e` with a symbolic exponent is the one place where it does not. The four p

### #20 [major] The two L2 budget tests assert a 5 ms wall-clock per-answer bound and fail on ordinary load

File: `crates/core/tests/answer_check.rs:539` — IDs: L2

**Claim.** `the_corpus_canonicalization_holds_the_l2_budget` and `the_corpus_self_check_holds_the_l2_budget` assert `worst < Duration::from_millis(5)` on a wall-clock measurement of a single debug-build call, so scheduler noise — not the work under test — decides the verdict, and the only automated gate on L2 goes red at random.

**Evidence.**

```
Run under CPU contention (16 busy loops on a 16-core box), the compiled test binary failed 6 of 6 runs, and one failure blamed the canonicalization of the literal answer `2`:

    $ for i in $(seq 1 $(nproc)); do (while :; do :; done) & done
    $ target/debug/deps/answer_check-1aeab293bf52a6f6 l2_budget
    test the_corpus_canonicalization_holds_the_l2_budget ... FAILED
    panicked at crates/core/tests/answer_check.rs:538:5:
    the longest single check took 8.029769ms on "2", and the budget is 5 ms
    ...
    failures under load: 6 / 6

It also fires without any load contrivance. During a mutation sweep, three mutants were reported "killed by the_corpus_canonicalization_holds_the_l2_budget" — including `dot-groups-four-lead-digits`, which edits `is_dot_grouped` in `crates/core/src/answer/check.rs`, a function `canonical_form` never calls. Re-running that same mutant twice on a quiet box gave `*** SURVIVED ***` both times, so the kill was a timing flake in a full-suite run, not a real failure. Run alone on an idle box the test passed 25/25.
```

**Failure scenario.** CI runs `cargo test -p cadus-core` on a shared runner. `the_corpus_canonicalization_holds_the_l2_budget` panics at answer_check.rs:539 with `the longest single check took 8.0ms on "2", and the budget is 5 ms` even though nothing changed. Because a flake is indistinguishable from a real regression, the fix is to raise the bound or mute the test — and L2 then has no gate at all. That matters here specifically: the two real latency defects already found in this module (one check costing 8.4 s through an unbounded LCM in `content_normalize`, and 2,000 `MAX_STEPS` costing 155 ms in a release build) are exactly the class this assertion exists to catch, and both would be waved through by a bound that has been raised to stop the noise. The `total < Duration::from_secs(1)` assertion beside it (lines 507 and 535) is the plan's actual U3 acceptance row and is 3,492x less sensitive to a single scheduling hiccup; the per-answer bound is an added invariant that only contributes noise.

**Refuter.** The claim is demonstrable and I reproduced it. The two tests assert a 5 ms wall-clock bound on a single debug-build call, and the real work has only 4.9x headroom (canonicalization) at the worst corpus answer, so a scheduler preemption alone decides the verdict. Under 16 spin loops on a 16-core box the test failed 2 of 4 runs with 5.60 ms and 16.28 ms readings, while the `total < 1 s` assertions beside it held at 133 ms and 156 ms in the same load — confirming the per-answer bound is the sensitive one. The reviewer's non-contrived evidence also holds structurally: `is_dot_grouped` (crates/core/src/answer/check.rs:156) has one caller, `dot_thousands_variant` (check.rs:144), which is only in the `check` path; `canonical_form` (check.rs:129) calls `normalize` then `canonical` -> `parse` -> `canon` and never reaches it, so a mutant of `is_dot_grouped` cannot change any canonicalization resul

## FIXM2c

### #12 [major] The documented live-oracle command runs zero tests and prints ok

File: `crates/core/tests/answer_oracle.rs:18` — IDs: V3, R5

**Claim.** The module doc tells the reader to prove the committed verdict file against live 1.0 with `-- --ignored`, but `the_live_oracle_reproduces_the_committed_verdicts` (line 1781) is a plain `#[test]` with an environment-variable guard and carries no `#[ignore]`, so the documented command filters it out and reports success without running any comparison.

**Evidence.**

```
answer_oracle.rs:16-18 documents: `CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python \ cargo test -p cadus-core --test answer_oracle -- --ignored --nocapture`. Run verbatim: `running 0 tests` / `test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out`. Without `--ignored` the test does run and passes: `the live 1.0 checker reproduced 14875 verdicts` in 19.34 s.
```

**Failure scenario.** A reviewer or the M2 close-out follows the documented procedure to satisfy the U3 acceptance item "live oracle run when CADUS_ORACLE_PYTHON is set", sees `ok`, and records the 14,875-line fixture as verified against 1.0. Nothing compared anything. The same green appears if the fixture is later edited by hand, because the one test that reads live 1.0 never executes under that command.

**Refuter.** The claim is demonstrable and I reproduced it exactly. `/home/deploy/dev/cadus2.0/crates/core/tests/answer_oracle.rs` lines 16-18 document the live-1.0 proof procedure with `-- --ignored --nocapture`, but the file contains zero `#[ignore]` attributes: `grep -n "ignore"` over the whole file matches only line 18 — the doc comment itself. All 8 `#[test]` functions, `the_live_oracle_reproduces_the_committed_verdicts` at line 1781 included, are plain `#[test]`. Its skip path is an environment-variable guard (`let Ok(python) = std::env::var("CADUS_ORACLE_PYTHON") else { ... return; }`), not the libtest ignore mechanism, so `--ignored` — which selects only ignored tests — filters every test out. The documented command therefore prints `ok` while comparing nothing, and it prints the same `ok` whether or not `CADUS_ORACLE_PYTHON` is set and whether or not `oracle_verdicts_1_0.jsonl` was edited by

### #21 [major] The oracle wall-clock guard is swallowed by 1.0, so a timed-out pair is recorded as a decided 1.0 `false`

File: `scripts/oracle/check_1_0.py:66` — IDs: V3, C4

**Claim.** `verdict()` catches `Timeout` in its own frame, but 1.0 `_sympy_equivalent` wraps its parse and numeric rungs in bare `except Exception` handlers that catch the guard first and return `False`, so the harness writes `{"equivalent": false, "timeout": false}` for a pair the guard actually stopped.

**Evidence.**

```
scripts/oracle/check_1_0.py:62-70
    signal.setitimer(signal.ITIMER_REAL, timeout_s)
    try:
        equivalent = bool(answers_equivalent(expected, learner, kind))
        notation = bool(dot_thousands_variant(expected, learner, kind))
    except Timeout:
        return {"equivalent": None, "notation": None, "timeout": True}

/home/deploy/dev/cadus/cadus_web/sympy_check.py:340-365 (the swallow sites)
    try:
        from sympy import simplify
        lhs = _parse(expected)
        rhs = _parse(given)
    except Exception:
        return False
    ...
    except Exception:
        pass  # not a clean numeric pair -- the symbolic ladder still decides it
    ...
        except Exception:
            return False

Measured, one request per run, `expected="4"`, `learner="factorial(1000000)"`, kind numeric:
  --timeout 1.0:  elapsed   1.00s   response {"equivalent": false, "notation": false, "timeout": false}
  --timeout 3.0:  elapsed   3.00s   response {"equivalent": false, "notation": false, "timeout": false}
  --timeout 5.0:  elapsed   5.00s   response {"equivalent": false, "notation": false, "timeout": false}
The elapsed time tracks `--timeout` exactly, so the alarm fires every time, and the response still claims a decided verdict.

crates/core/tests/answer_oracle.rs:1041-1043 keys the whole `Class::OracleSilent` path on that flag:
        let verdict = if row.timeout { None } else { Some(OracleVerdict { ... }) };
and `grep -c '"timeout": true' crates/core/tests/fixtures/answers/oracle_verdicts_1_0.jsonl` returns 0.
```

**Failure scenario.** An operator re-records the oracle set (`CADUS_ORACLE_PYTHON=... cargo test --test answer_oracle`) after a corpus or generator change. One generated pair reaches 1.0 `_parse` or the `evalf` rung and runs past the 2.0 s guard. The alarm fires inside 1.0, `_sympy_equivalent` returns `False`, `answers_equivalent` returns `False`, and `check_1_0.py` writes `equivalent: false, timeout: false`. `committed_verdicts()` reads that row as a real 1.0 verdict instead of `None`, so the pair enters class 3 and `the_two_checkers_agree_on_every_comparable_pair` asserts 2.0 against a verdict 1.0 never produced. If 2.0 decides `true` there, the gate fails on a phantom divergence; if 2.0 also says `false`, V3 records 100% parity on a pair the oracle never judged. The `except Exception: pass` at sympy_check.py:353 makes it worse: the one-shot timer is spent there and is never re-armed, so the following `simplify` call runs with no guard at all. I demonstrated the swallow above; the committed file is clean today (I replayed all 14,875 rows against the live oracle: 0 mismatches, 0 rows over 1.5 s), so the defect is latent, not yet realized.

**Refuter.** The claim is correct, and I demonstrated it directly. `Timeout` in scripts/oracle/check_1_0.py is a subclass of `Exception`, so the bare `except Exception` handlers in 1.0 `_sympy_equivalent` (/home/deploy/dev/cadus/cadus_web/sympy_check.py:340-365) catch the guard before it reaches the `except Timeout` at check_1_0.py:66. `answers_equivalent` returns `False`, and `verdict()` writes `{"equivalent": false, "notation": false, "timeout": false}` for a pair that the guard stopped. The harness has no independent wall-clock cross-check: `live_verdicts` (crates/core/tests/answer_oracle.rs:1703-1770) trusts the `timeout` flag alone, and `committed_verdicts` (line 1041-1043) keys `Class::OracleSilent` on the same flag. One severity correction: this is a V3 evidence-integrity defect in the parity gate, not a C4 false positive in the 2.0 checker; the 2.0 verdict path is untouched. The defect is lat
