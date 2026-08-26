# M2 adversarial review — round 4 (2026-08-27)

Run on commit aae4201 (after FIXM2g–i). One find/refute round: 9 raised, 4 confirmed, 0 blockers. Per the round-3 ruling M2 closes after fix unit FIXM2j (no further review round; the trend over four rounds was 21, 17, 14, 4 confirmed).

## Orchestrator rulings (binding)

- #1: a `Num` token that a `/` or `^` already consumed is not a whole part; the `b/c` spelling after such a factor is Undecidable (`a fraction stands after a number that is no whole part`), the same as the glyph and `\frac` spellings. No product reading anywhere.
- #2: the work bound charges the O(terms) rebuild of a sum (one step per term touched); the worst in-grammar corpus case and the reviewer's 4,000-char case must each decide or refuse in under 50 ms (release).
- #3, #4: add the missing tests with literal pairs and mutation-check them.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | major | `crates/core/src/answer/parse.rs:545` | FIXM2j | The b/c mixed-number spelling falls back to the product reading when the whole-part token was consumed as a divisor or an exponent, and invents a value |
| 2 | major | `crates/core/src/answer/canon.rs:625` | FIXM2j | One in-grammar check costs 378 ms in a release build, over the 300 ms L2 bound |
| 3 | major | `crates/core/tests/answer_check.rs:612` | FIXM2j | The descending chained inequality keeps no test of its upper-end closedness, so a C4 mutant lives |
| 4 | major | `crates/core/tests/answer_check.rs:981` | FIXM2j | Rule 5 of the canonical rational form keeps no test whose numerator and denominator hold different monomials |

## FIXM2j

### #1 [major] The b/c mixed-number spelling falls back to the product reading when the whole-part token was consumed as a divisor or an exponent, and invents a value

File: `crates/core/src/answer/parse.rs:545` — IDs: C4, V1, V2

**Claim.** In `read_mixed_number` the `part.digit_run` escape at line 545 hands the `a b/c` spelling back to `check_implicit_number`, which refuses only when the previous FACTOR is a numeric literal, so a `Num` token that a `/` or a `^` already consumed lets the parser silently take the product reading that the four other mixed-number spellings refuse.

**Evidence.**

```
parse.rs:521-526 states the invariant: "A number token in front of a fraction is a mixed number or it is nothing. ... a checker that picks one of the two readings grades a wrong answer correct (C4)."

parse.rs:536-551:
        let previous = self.at.checked_sub(1).and_then(|at| self.tokens.get(at));
        if !matches!(previous.map(|token| &token.kind), Some(Tok::Num(_))) {
            return Ok(None);
        }
        let whole = match factors {
            [only] => signed_whole(only),
            _ => None,
        };
        let Some((negative, whole)) = whole else {
            if part.digit_run {
                return Ok(None);
            }
            return Err(Undecidable::new(
                "a fraction stands after a number that is no whole part",
            ));
        };

`previous` is token-based; `whole` is factor-based. For `t/4 3/4` the token before the fraction IS `Num("4")`, but `factors == [Div(t, 4)]`, so `signed_whole` gives None and line 545 returns Ok(None). parse.rs:400-403 then passes the answer, because `Ast::Div` is not `is_numeric_literal`.

Probe output (release build against crates/core at HEAD):
  't/4 3/4'      AST Div(Mul([Div(Var("t"), Integer(4)), Integer(3)]), Integer(4))  canon 3t/16
  't/4 ¾'        ERR:a fraction stands after a number that is no whole part
  't/4 \frac{3}{4}'  ERR:a fraction stands after a number that is no whole part
  check("3*t/16", "t/4 3/4", Expression)   -> correct=true
  check("4*t/19", "t/4 3/4", Expression)   -> correct=false
  check("x/4",    "x/2 1/2", Expression)   -> correct=true
  check("x/4",    "x/2 ½",   Expression)   -> UNDECIDABLE
  check("x^2/2",  "x^2 1/2", Expression)   -> correct=true
  check("x^2/2",  "x^2 ½",   Expression)   -> UNDECIDABLE
  check("cos(x)/4", "cos(x)/2 1/2", Expression) -> correct=true
  check("pi/4",   "pi/2 1/2", Expression)  -> correct=true
```

**Failure scenario.** Authored answer `3*t/16`. The learner writes `t/4 3/4`, which under the mixed-number rule of docs/plans/M2.md (round 2, findings #1, #2, #3, #5, #6, #7; round 3, #1, #2) is `t / (4 + 3/4)` = 4t/19. The parser instead reads `((t/4) * 3) / 4` = 3t/16, and `check` returns `Decided{correct: true, notation: false}`: a wrong answer graded correct (C4). The same learner answer spelled `t/4 ¾` or `t/4 \frac{3}{4}` returns Undecidable, so two of the five spellings the plan declares to be one rule give two different verdicts. The shapes that reach the hole are `<non-literal>/<int> <b>/<c>` (`x/2 1/2`, `cos(x)/2 1/2`, `pi/2 1/2`) and `<base>^<int> <b>/<c>` (`x^2 1/2`); the `<literal> <b>/<c>` shapes (`9/2 1/2`, `x 2 1/2`) stay refused, which is why the corpus tests do not reach it.

**Refuter.** The claim is demonstrated, so I cannot refute it. `read_mixed_number` tests the previous TOKEN (`Tok::Num`) but derives the whole part from the previous FACTOR. When a `/` or `^` has already folded that `Num` token into an `Ast::Div` or `Ast::Pow`, the token test passes, `signed_whole` returns None, and the `part.digit_run` escape at parse.rs:545 returns `Ok(None)`. The caller's backstop at parse.rs:401 refuses only when the previous factor `is_numeric_literal`, and `Div`/`Pow` are not, so the parser silently takes the product reading. The four other spellings of the same shape (`½`, `¾`, `\frac{b}{c}`, spaced or glued) reach one `Tok::Frac`, `digit_run` is false, and they refuse with "a fraction stands after a number that is no whole part". I reproduced every row of the reviewer's probe against a release build of cadus-core at HEAD: `check("3*t/16", "t/4 3/4", Expression)` returns `Deci

### #2 [major] One in-grammar check costs 378 ms in a release build, over the 300 ms L2 bound

File: `crates/core/src/answer/canon.rs:625` — IDs: L2, V1

**Claim.** The canonicalizer charges one work step for an arithmetic operation but charges nothing for the O(terms) rebuild of the whole sum that every operation performs, so an in-grammar answer inside the 4,000-character cap spends 188 ms in one canonicalization and one check of two such answers spends 378 ms in a release build, which is 1.26x the 300 ms of L2.

**Evidence.**

```
canon.rs:621-631 (poly_mul) returns a full clone of a T-term sum for zero budget:

    fn poly_mul(&mut self, left: &Poly, right: &Poly) -> Result<Poly, Undecidable> {
        // A factor of 1 is the common case of the form: ...
        if is_one(left) {
            return Ok(right.clone());
        }
        if is_one(right) {
            return Ok(left.clone());
        }
        self.spend(left.len().saturating_mul(right.len()))?;

canon.rs:1161-1181 (frac_of) rebuilds every term of the value on each operation, and canon.rs:758-775 (insert_term) never calls `spend` (its only charge is `bounded`, whose `spend_width` is 0 for a coefficient under 64 bits):

            Canon::Radical(parts) => {
                let mut sum = Poly::new();
                for (basis, coefficient) in parts { ... self.insert_term(&mut sum, monomial, coefficient.clone())?; }
                sum
            }
            Canon::Poly(parts) => parts.clone(),

canon.rs:1433/1455 (from_sum -> as_radical) rebuilds the same T entries again after every operation. The only charge on the whole `*1` chain is the one `spend(1)` of `Work::node` per `Ast::Mul` item, so N operations on a T-term sum cost N x T term rebuilds for N steps of MAX_STEPS.

Measured, release build (AMD Ryzen 7 3700X), harness /tmp/claude-1000/-home-deploy-dev-cadus2-0/423a634f-40c8-4ad8-9fdd-67df2281434b/scratchpad/rvlens4/src/bin/final.rs:

    MAX_ANSWER_CHARS=4000 A.chars=3267 B.chars=3269
    check(A,B) -> Decided(Verdict { correct: true, notation: false })  in 377.870934ms
    check(A,B) -> Decided(Verdict { correct: true, notation: false })  in 375.768162ms
    check(A,B) -> Decided(Verdict { correct: true, notation: false })  in 378.559228ms
    canonical_form(A) alone: 188.151751ms
    learner-only bomb chars=3999
    check("42", bomb) -> Undecidable(Undecidable { reason: "the answer goes past the work bound" }) in 195.043659ms

The machine is not slow: `CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test answer_check -- l2` runs both corpus L2 tests in 0.01 s.
```

**Failure scenario.** Build A = "(" + "e^2+e^3+...+e^281" + ")" + "*1" repeated 846 times (3,267 characters, inside MAX_ANSWER_CHARS = 4,000, inside the section 8.1 grammar, no LaTeX and no glyph). Build B = A with its first "(" replaced by "(0+" (3,269 characters). Call check(A, B, AnswerKind::Expression). The call returns Decided { correct: true, notation: false } after 376-379 ms in a release build, so one deterministic grade of a verifiable answer takes 26% longer than the whole 300 ms of L2, and the D-O2 grade operation has no time left for its insert and its two updates. The bound also fails on the learner side alone: with a short authored expected such as "42", a 3,999-character learner answer of the same shape costs 195 ms, which is 65% of the L2 budget for a string the learner controls. The cause is that the sum holds about 280 terms and the 846 `*1` factors each cost one work step and one full rebuild of those 280 terms, so MAX_STEPS bounds the count of operations but not the cost of one operation.

**Refuter.** I could not refute the claim. I rebuilt the reviewer's harness from source myself (cargo build --release, default release profile — the workspace Cargo.toml declares no [profile.release], so the harness codegen matches a production build) and ran it. The numbers reproduce, and my run is worse than the reviewer's: check(A,B) took 390-421 ms across three runs and one canonical_form(A) took 197 ms. The machine is not slow: CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test answer_check -- l2 passes both corpus tests (3,492 pairs) in 0.03 s, so one crafted 3,267-character pair costs about 13x the whole corpus.

The stated cause is accurate in the source. In /home/deploy/dev/cadus2.0/crates/core/src/answer/canon.rs:
- poly_mul (lines 621-631) returns `right.clone()` / `left.clone()` on the `is_one` short cut BEFORE `self.spend(...)`, so a `*1` factor copies a T-term BTreeMap for 

### #3 [major] The descending chained inequality keeps no test of its upper-end closedness, so a C4 mutant lives

File: `crates/core/tests/answer_check.rs:612` — IDs: C4, V1, V2

**Claim.** The round-1 fix for finding #13 added the strict ASCENDING chain only, so the `hi_closed: op == IneqOp::Ge` rule of the descending arm (crates/core/src/answer/parse.rs:325) is exercised by no test, and the oracle harness excuses the whole class, so a wrong closedness there grades a wrong learner answer correct and no test fails.

**Evidence.**

```
parse.rs:321-326 holds the descending arm:
    (IneqOp::Gt | IneqOp::Ge, IneqOp::Gt | IneqOp::Ge) => Ok(Ast::Chain {
        lo_closed: second == IneqOp::Ge,
        var,
        hi_closed: op == IneqOp::Ge,
The only descending chain in any test is answer_check.rs:540 `assert_eq!(form("3 >= x >= -1"), expected)`, whose `op` is `Ge`, so `op == IneqOp::Ge` is never evaluated as false. answer_check.rs:612 pins `-1 < x < 3` and never its twin `3 > x > -1`.
I copied the tree to a scratch dir, replaced `hi_closed: op == IneqOp::Ge,` with `hi_closed: true,` and ran the whole suite:
  test result: ok. 45 passed (answer_check)
  test result: ok. 23 passed (answer_parse)
  test result: ok. 23 passed (answer_divergence)
  test result: ok. 42 passed (answer_oracle)
The oracle harness cannot catch it either. answer_oracle.rs:2926 excuses every 2.0-correct / 1.0-false pair whose expected side is a chained inequality, and answer_oracle.rs:2528 reads the expected side alone. I added a probe test to the scratch copy:
  learner "2 < x <= 5" -> Some("a chained inequality raises inside 1.0 (spec 7.7)")
  learner "anything at all" -> Some("a chained inequality raises inside 1.0 (spec 7.7)")
```

**Failure scenario.** The authored corpus answer `-1 ≤ x ≤ 3` (topic `domain-range-of-relations`) against the learner answer `3 > x >= -1`, which excludes the endpoint 3 and is therefore a different set. HEAD decides `correct: false`. With `hi_closed: true` the same pair decides `correct: true` (measured: `Interval { var: Some("x"), lo: -1, lo_closed: true, hi: 3, hi_closed: true }` on both sides), and the reverse pair `-1 ≤ x < 3` against `3 > x >= -1` flips from `correct: true` to `correct: false`. Both moves pass every test in the four answer suites.

**Refuter.** The claim is correct, and I reproduced every part of it. The descending arm at crates/core/src/answer/parse.rs:321-326 sets `hi_closed: op == IneqOp::Ge`. Two tests reach that arm: crates/core/tests/answer_check.rs:540 `form("3 >= x >= -1")` and crates/core/tests/answer_parse.rs:1419 `ast("3 ≥ x > -1")`. In both, `op` is `Ge`, so `op == IneqOp::Ge` gives true only. No test in the repository parses a descending chain whose FIRST operator is strict (`3 > x >= -1`, `3 > x > -1`); a grep over crates/core/tests/*.rs and over all four fixtures in crates/core/tests/fixtures/answers/ returns no such string. The companion field `lo_closed: second == IneqOp::Ge` IS covered in both directions by answer_parse.rs:1419, which shows the round-1 fix pinned the descending arm in part only. I copied the tree, replaced `hi_closed: op == IneqOp::Ge,` with `hi_closed: true,`, and the complete cadus-core suit

### #4 [major] Rule 5 of the canonical rational form keeps no test whose numerator and denominator hold different monomials

File: `crates/core/tests/answer_check.rs:981` — IDs: C4, V1, D6

**Claim.** Every test of the scalar-ratio rule uses a numerator and a denominator over the SAME monomials, so the key guard `!num.keys().eq(den.keys())` of crates/core/src/answer/canon.rs:1118 is exercised by no test, and its loss collapses two unrelated rational functions into the number 1.

**Evidence.**

```
The rule-5 assertions are answer_check.rs:977-981:
    assert_eq!(check("(x+1)/(x+1)", "1", E), decided(true, false));
    assert_eq!(check("(2x+2)/(x+1)", "2", E), decided(true, false));
    assert_ne!(form("(x+2)/(x+1)"), form("1"));
The C4 negative on line 981 holds the monomial set {{}, {x:1}} on both sides, so `num.len() != den.len()` alone already rejects nothing and the key test is never the deciding half. The 17-row C4 table at answer_check.rs:946-963 has the same property.
I replaced canon.rs:1118 `if num.len() != den.len() || !num.keys().eq(den.keys()) {` with `if num.len() != den.len() {` in the scratch tree and ran the whole suite:
  test result: ok. 45 / 23 / 23 / 42 passed, 0 failed.
```

**Failure scenario.** Expected `(x+1)/(y+1)` against the learner answer `1`. HEAD decides `correct: false`. With the key guard removed, `canonical_form("(x+1)/(y+1)")` returns `Rational(Ratio { numer: 1, denom: 1 })` and `check` decides `correct: true` in both directions; `(x+2)/(y+2)` against `1` decides `correct: true` as well. The two sums hold two terms each, the BTreeMap order puts the constant term first on both sides, and the coefficient ratio is 1, so rule 5 returns the scalar 1 for a quotient of two unrelated polynomials. No test in the four answer suites fails.

**Refuter.** The claim is demonstrable, and I reproduced it end to end. I made a copy of the tree outside the repo, replaced the guard at /home/deploy/dev/cadus2.0/crates/core/src/answer/canon.rs:1118 with `if num.len() != den.len() {`, and ran the WHOLE cadus-core suite, not only the four answer suites. Every test passed: answer_check 45, answer_divergence 23, answer_oracle 23, answer_parse 42, arena 27, lint 27, loader 36, parity 18, purity 2. 0 failed. The mutant survives.

The failure the mutant creates is a C4 blocker. With the guard gone, `canonical_form` returns the number 1 for a quotient of two unrelated polynomials, and `check` decides `correct: true` against the learner answer `1`. I measured five such pairs, and the 1.0 oracle answers False for all of them, so each one is a FALSE POSITIVE and a V3 parity break, not a documented divergence.

One part of the reviewer's wording is too strong
