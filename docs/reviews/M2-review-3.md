# M2 adversarial review — round 3 (2026-08-27)

Run on commit a2094a1 (after FIXM2d–f). One find/refute round, major+ only: 18 raised, 14 confirmed. Assigned to FIXM2g (lexer/parser restructure), FIXM2h (canon rational form), FIXM2i (generator family + regeneration; after g and h merge).

## Orchestrator rulings (binding)

- Structural change (FIXM2g): LaTeX and glyph constructs are LEXER TOKENS with parsed
  structure, not string rewrites. The lexer recognizes `\frac{A}{B}` (brace bodies lexed
  recursively, whitespace inside braces ignored), `\sqrt{A}` and `\sqrt A`, `^{A}`,
  `\cdot`, `\times`, `\left`/`\right` (dropped), `%` (postfix on the preceding primary),
  the vulgar glyphs (a fraction literal), `√`, superscript digits, and `°`. The parser
  builds `Frac`, `Sqrt`, `Percent(primary)`, `Mixed` nodes directly. The `normalize` step
  keeps only: outer `$…$`, trailing periods, whitespace collapse, the casefolded string
  key, the thousands-group full-match strip, and the one-character Unicode table for
  operators and constants. Every re-association class of findings #1–#4, #6, #8 must be
  impossible by construction: a `Percent` node wraps exactly its primary; `Frac` is one
  node whatever whitespace its braces hold.
- A juxtaposed function argument stops at a function name (`sec x tan x` = `sec(x)*tan(x)`),
  as 1.0 reads it; pin the three `derivatives-trig` corpus answers with their 1.0 source.
- A mixed number keeps the sign of a `-0` whole part (`-0 1/2` = -1/2); the parser reads
  the sign token, not the integer value.
- Canon (FIXM2h): `sqrt` of a rational reduces (`sqrt(4/9)` = 2/3, `sqrt(1/2)` = `sqrt(2)/2`);
  the `Atom::E` fold takes the integer part of a rational constant term (`e^(5/2)` =
  `e^2*e^(1/2)`); `Atom::Inverse` is REPLACED by a rational-function form: every canonical
  value is `Poly / Poly` with both expanded, the denominator content- and sign-normalized,
  monomial factors moved out of the denominator into negative exponents only when the
  denominator is a monomial, and NO polynomial GCD (so `(x^2-1)/(x-1)` vs `x+1` stays
  false — documented). Sums put terms over the common denominator (product). Pin the 16
  corpus answers finding #13 lists in both spellings.
- Oracle (FIXM2i): add a "rational rewrite" generator family that produces the
  common-denominator and the split spelling of every corpus answer with a denominator
  (use SymPy `together`/`apart` in the harness-side Python to generate, never to judge);
  regenerate; every class-3 disagreement is a Rust bug or a cited divergence.
- Then one verification round (round 4). If round 4 still confirms a C4 blocker, M2 stops
  and the owner decides on the grammar design; otherwise M2 closes.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `crates/core/src/answer/normalize.rs:326` | FIXM2g | A `\frac` with any space inside its braces falls back to the product spelling, so the round-2 mixed-number blocker is still open in that spelling |
| 2 | blocker | `crates/core/src/answer/normalize.rs:326` | FIXM2g (dup of #1) | Whitespace inside \frac braces defeats the literal-fraction token, so a mixed number keeps the product reading and a wrong answer is graded correct |
| 3 | blocker | `crates/core/src/answer/normalize.rs:178` | FIXM2g | A percent used as a divisor is off by a factor of 100 squared, because bind_percent writes `(N)/100` and never parenthesizes the whole value |
| 4 | blocker | `crates/core/src/answer/normalize.rs:178` | FIXM2g (dup of #3) | The percent rewrite is unparenthesized, so `%` after `/` or `**` re-associates and grades a wrong answer correct |
| 5 | blocker | `crates/core/src/answer/parse.rs:706` | FIXM2g | The bracket-free function argument does not stop at a function name, so three authored `derivatives-trig` answers mean a nested function and the correct learner answer is graded wrong |
| 6 | major | `crates/core/src/answer/normalize.rs:161` | FIXM2g | The percent rewrite inserts a parenthesis that bypasses the space-grouped-number refusal, so `1 500%` means two different values |
| 7 | major | `crates/core/src/answer/parse.rs:952` | FIXM2g | A negative mixed number with a zero whole part loses its sign, so `-0 1/2` is graded as +1/2 |
| 8 | major | `crates/core/src/answer/normalize.rs:285` | FIXM2g | The new \sqrt product sign is inserted before \times and \cdot become `*`, so `2\times\sqrt{3}` normalizes to the power `2**sqrt(3)` and gets no verdict |
| 9 | major | `crates/core/src/answer/canon.rs:583` | FIXM2h | sqrt of a rational perfect square is never evaluated, so sqrt(1/2) and sqrt(4/9) are decided wrong against their own values |
| 10 | major | `crates/core/src/answer/canon.rs:961` | FIXM2h | An Inverse atom keeps a common monomial factor in its divisor, so `1/(x(x+h))` and `1/x * 1/(x+h)` are two canonical forms of one value |
| 11 | major | `crates/core/src/answer/canon.rs:583` | FIXM2h (dup of #9) | `sqrt` is reduced only for a whole-number radicand, so `√(1/2)` is not `√2/2` and `√(9/16)` is not `3/4` |
| 12 | major | `crates/core/src/answer/canon.rs:888` | FIXM2h | The `Atom::E` fold of FIXM2e fires only when the whole constant term is an integer, so the exponent law still fails for a fractional exponent and `e^(5/2)` is not `e^2*e^(1/2)` |
| 13 | major | `crates/core/src/answer/canon.rs:796` | FIXM2h | The round-2 reciprocal fix merges two Inverse atoms only, so a sum of reciprocals and a reciprocal of a product stay different values from their combined form on 16 authored corpus answers |
| 14 | major | `crates/core/tests/answer_oracle.rs:1402` | FIXM2i | No generator family rewrites a rational expression, so the 100.0000% class-3 agreement is still an artifact of generator choice — the defect round-2 finding #16 raised |

## FIXM2g

### #1 [blocker] A `\frac` with any space inside its braces falls back to the product spelling, so the round-2 mixed-number blocker is still open in that spelling

File: `crates/core/src/answer/normalize.rs:326` — IDs: C4, V4, V1, V3

**Claim.** `literal_fraction` writes the one-token `⟦b/c⟧` spelling only when both brace bodies are pure ASCII digit runs, so `\frac{ 1}{2}` (one space, which `collapse_whitespace` keeps inside the braces) drops to the round-1 `((a)/(b))` text, the parser reads a whole number in front of it as an implicit product, and `check("1", "2\frac{ 1}{2}")` returns correct=true.

**Evidence.**

```
normalize.rs:324-329 (`literal_fraction`):
    let digits = |run: &[char]| !run.is_empty() && run.iter().all(char::is_ascii_digit);
    if !digits(numerator) || !digits(denominator) {
        return None;
    }
normalize.rs:271-278 (`rewrite_latex_braces`), the `None` arm that still emits the round-1 text:
            match literal_fraction(numerator, denominator) {
                Some(token) => out.push_str(&token),
                None => {
                    out.push_str("((");

Probe against `cadus_core::answer` (scratchpad crate, path dependency on crates/core), kind Expression:
  "1"      vs "2\frac{ 1}{2}"      => correct=true  notation=false   <-- FALSE POSITIVE
  "1"      vs "2\frac{1}{ 2}"      => correct=true  notation=false
  "1"      vs "2\frac{ 1 }{ 2 }"   => correct=true  notation=false
  "1"      vs "2 \frac{ 1}{2}"     => correct=true  notation=false
  "1"      vs "2\frac{+1}{2}"      => correct=true  notation=false
  "2"      vs "4\frac{ 1}{2}"      => correct=true  notation=false
  "9/4"    vs "3\frac{ 3 }{ 4 }"   => correct=true  notation=false
    source  = "2(( 1)/(2))"   ast = Mul([Integer(2), Fraction { 1, 2 }])   canon = Rational(1/1)
The fix works only for the space-free spelling, which shows the gap is this gate alone:
  "5/2"    vs "2\frac{1}{2}"       => correct=true   (source "2⟦1/2⟧", ast Mixed { 2, 1, 2 })
  "5/2"    vs "2\frac{ 1}{2}"      => correct=false
The mirror error of review-2 finding #1 is back too — the RIGHT answer in the same notation is refused:
  "4 1/2"  vs "4\frac{ 1}{2}"      => correct=false
  "2 7/12" vs "2\frac{ 7 }{ 12 }"  => correct=false

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  {"expected":"1","learner":"2\frac{ 1}{2}"}    -> {"equivalent": false}
  {"expected":"1","learner":"2\frac{ 1 }{ 2 }"} -> {"equivalent": false}
  {"expected":"9/4","learner":"3\frac{ 3 }{ 4 }"} -> {"equivalent": false}
1.0 deleted the backslash and decided false, so the wrong-correct verdict is a 2.0 regression, not carried behavior. `\frac` is a 2.0 addition (docs/plans/M2.md, V4 row), and the round-2 ruling says the mixed-number rule has one place and covers `a\frac{b}{c}` and `a \frac{b}{c}`.
```

**Failure scenario.** A learner works `mixed-numbers` kp3 exemplar 0 (curriculum/foundations/01-fractions-decimals.yaml:611-613), whose problem reads `Compute $1\frac{1}{2} \times \frac{2}{3}$` and whose authored answer is `1`. The learner multiplies wrongly, gets two and one half, and types it in the LaTeX the problem itself uses but with one space inside the brace: `2\frac{ 1}{2}`. `normalize` produces `2(( 1)/(2))`, the parser reads the implicit product, `canon` gives Rational(1), and `check` returns Verdict { correct: true, notation: false }. The learner is 1.5 out and the item is recorded correct; A3 keeps the answer away from the model, so nothing corrects the record. On the same knowledge point, exemplar 1 (authored `4 1/2`) marks the RIGHT answer `4\frac{ 1}{2}` wrong. 1.0 decides both pairs false.

**Refuter.** The claim is demonstrable, and I reproduced every line of its evidence against the real crate. `literal_fraction` (crates/core/src/answer/normalize.rs:322-336) accepts a brace body only when it is a non-empty run of pure ASCII digits. `to_source` runs `collapse_whitespace` before `rewrite_latex_braces`, and `collapse_whitespace` folds a whitespace run to one ASCII space; it does not delete the space. The numerator of `\frac{ 1}{2}` therefore reaches `literal_fraction` as [' ', '1'], the digit gate refuses it, and the `None` arm of `rewrite_latex_braces` (normalize.rs:271-278) emits the round-1 text `((a)/(b))`. That text erases the `\frac` origin, so the parser never sees the `Num fraction` token shape that `read_mixed_number` needs. It reads an implicit product instead. `check("1", "2\frac{ 1}{2}", Expression)` returns Verdict { correct: true, notation: false }: a C4 false positive on a

### #2 [blocker] Whitespace inside \frac braces defeats the literal-fraction token, so a mixed number keeps the product reading and a wrong answer is graded correct

File: `crates/core/src/answer/normalize.rs:326` — IDs: C4, V4, V1, A3 — duplicate of #1

**Claim.** `literal_fraction` writes the `⟦b/c⟧` token only when both brace bodies are pure ASCII digit runs, so one space inside a brace sends `\frac` back to the `((a)/(b))` spelling of round 1, `2\frac{ 1 }{ 2 }` reads as the product 1, and `check("1", "2\frac{ 1 }{ 2 }")` returns correct=true — the exact C4 false positive that review round 2 finding #1 was fixed to close.

**Evidence.**

```
normalize.rs:324-334 (`literal_fraction`):
    let digits = |run: &[char]| !run.is_empty() && run.iter().all(char::is_ascii_digit);
    if !digits(numerator) || !digits(denominator) {
        return None;
    }
`to_source` runs `collapse_whitespace` first (normalize.rs:127), so a space inside a brace survives as one ASCII space and fails `digits`. `rewrite_latex_braces` then takes the `None` arm (normalize.rs:271-279) and emits `((…)/(…))`, which the parser reads as an implicit product.

Probe against `cadus_core::answer` (release build, path dependency on crates/core):
  normalize("2\\frac{ 1 }{ 2 }").source == "2(( 1 )/( 2 ))"
    ast   = Mul([Integer(2), Fraction { numerator: 1, denominator: 2 }])
    canon = Rational(1/1)
  normalize("2\\frac{1 }{2}").source  == "2((1 )/(2))"    canon = Rational(1/1)
  normalize("2\\frac{ 1}{ 2}").source == "2(( 1)/( 2))"    canon = Rational(1/1)
  normalize("2\\frac{1}{2}").source   == "2⟦1/2⟧"          canon = Rational(5/2)   (the covered spelling)

  check(expected, learner, Expression):
    "1"   vs "2\\frac{ 1 }{ 2 }"  => correct=true  notation=false   <-- FALSE POSITIVE
    "1"   vs "2\\frac{1 }{2}"     => correct=true  notation=false   <-- FALSE POSITIVE
    "1"   vs "2\\frac{ 1}{ 2}"    => correct=true  notation=false   <-- FALSE POSITIVE
    "5/2" vs "2\\frac{ 1 }{ 2 }"  => correct=false notation=false   <-- the right answer is refused

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  {"expected":"1","learner":"2\\frac{ 1 }{ 2 }"}   -> {"equivalent": false}
  {"expected":"5/2","learner":"2\\frac{ 1 }{ 2 }"} -> {"equivalent": false}
1.0 decides both pairs false, so the wrong-correct verdict is 2.0's own.

The pre-fix build (git archive 4763e2d, same probe) gives the same false positive, so FIXM2d closed the no-space spelling only.
```

**Failure scenario.** A learner works `mixed-numbers` kp3 exemplar 0 (curriculum/foundations/01-fractions-decimals.yaml:611), whose problem reads `Compute $1\frac{1}{2} \times \frac{2}{3}$` and whose authored answer is `1`. The learner multiplies wrongly, gets two and a half, and types the mixed number in the notation the problem uses, with a space inside the brace: `2\frac{ 1 }{ 2 }`. `normalize` produces `2(( 1 )/( 2 ))`, the parser builds `Mul([Integer(2), Fraction { 1, 2 }])`, `canon` gives Rational(1), and `check` returns `Verdict { correct: true, notation: false }`. The learner is out by a factor of 2.5 and the item is recorded correct; A3 keeps the answer away from the model, so nothing corrects the record. On the same knowledge point the mirror error runs too: the RIGHT answer `2\frac{ 1 }{ 2 }` against an authored `5/2` is graded false.

**Refuter.** The claim is demonstrable, and no ruling, parity argument, or test excuses it.

1. The mechanism is as stated. `to_source` (/home/deploy/dev/cadus2.0/crates/core/src/answer/normalize.rs:127) runs `collapse_whitespace` (normalize.rs:220-235), which maps every whitespace run to one ASCII space and never deletes it. `read_frac` (normalize.rs:360-368) returns the brace bodies verbatim, with no trim. `literal_fraction` (normalize.rs:324-334) accepts a body only when every character is an ASCII digit. One space inside a brace therefore fails the test, `rewrite_latex_braces` takes the `None` arm (normalize.rs:271-279), and the output is `((…)/(…))`, which the parser reads as an implicit product.

2. I measured the behavior. I built a release probe crate with a path dependency on crates/core and got every reported line, character for character: `normalize("2\\frac{ 1 }{ 2 }").source == "2(( 1 )/

### #3 [blocker] A percent used as a divisor is off by a factor of 100 squared, because bind_percent writes `(N)/100` and never parenthesizes the whole value

File: `crates/core/src/answer/normalize.rs:178` — IDs: C4, V4, A3

**Claim.** `bind_percent` replaces `N%` with the unparenthesized text `(N)/100`, so the percent is one operand only under `+`, `-` and `*`; under `/` the trailing `/100` re-associates and `15/30%` becomes `(15/30)/100` = 1/200 instead of 15/0.30 = 50, which both refuses the right answer on the topic that teaches that division and accepts a wrong one.

**Evidence.**

```
normalize.rs:172-180 (`bind_percent`):
            Some(start) if guard != Some(start) => {
                let number = out.get(start..trimmed).unwrap_or("").to_string();
                out.truncate(start);
                out.push('(');
                out.push_str(&number);
                out.push_str(")/100");
Only the number is bracketed; the `/100` is left in the surrounding term, so a `/` in front of the percent binds to the numerator and not to the whole percent value.

Probe against `cadus_core::answer`:
  normalize("15/30%").source  == "15/(30)/100"   ast = Div(Fraction{15,30}, Integer(100))   canon = Rational(1/200)
  normalize("36/45%").source  == "36/(45)/100"   canon = Rational(1/125)
  normalize("100/25%").source == "100/(25)/100"  canon = Rational(1/25)
  normalize("1/(50%)").source == "1/((50)/100)"  canon = Rational(2/1)     (the bracketed form is right)
  normalize("2*50%").source   == "2*(50)/100"    canon = Rational(1/1)     (`*` is unaffected)

  check(expected, learner, Numeric):
    "50"    vs "15/30%"   => correct=false notation=false   <-- a correct answer graded wrong
    "80"    vs "36/45%"   => correct=false notation=false   <-- a correct answer graded wrong
    "160"   vs "8/5%"     => correct=false notation=false   <-- a correct answer graded wrong
    "1/200" vs "15/30%"   => correct=true  notation=false   <-- FALSE POSITIVE (learner value is 50)
    "0.04"  vs "100/25%"  => correct=true  notation=false   <-- FALSE POSITIVE (learner value is 400)

The review round 1 ruling #17 is "a percent binds to the number it follows" (docs/plans/M2.md, revised section round 1). Under `/` it does not: it binds to the numerator in front of it.

A 29,808-expression random fuzz over the grammar (numbers, decimals, \frac, \sqrt, vulgar glyphs, mixed numbers, all V4 product signs, exact rational reference values) reports 0 value mismatches once every percent atom is bracketed, and the unbracketed percent atom was the only arithmetic mismatch the first fuzz round found.
```

**Failure scenario.** Topic `percent-finding-the-whole` kp1 exemplar 0 (curriculum/foundations/01-fractions-decimals.yaml:1310) reads `$15$ is $30\%$ of what number?`, authored answer `50`, and its own solution sketch is `$15 \div 0.30 = 50$`. A learner writes the division exactly as the sketch does but keeps the percent sign: `15/30%`. `normalize` produces the source `15/(30)/100`, `canon` gives 1/200, and `check("50", "15/30%", Numeric)` returns `Verdict { correct: false }` — a right answer recorded wrong, final under A3. The same rewrite runs the other way: `check("1/200", "15/30%", Numeric)` and `check("0.04", "100/25%", Numeric)` both return `correct = true` for a learner whose value is 50 and 400, which is a C4 false positive on any topic whose authored answer is the hundred-fold-small number the misreading invents.

**Refuter.** I cannot refute the claim. It is demonstrable, and it is a defect, not a ruling.

`bind_percent` brackets the number but leaves `/100` in the surrounding term. Under `+`, `-` and `*` the value stays right by accident, because `*` and `/` associate freely. Under `/` the leading operator captures the `/100`, so `15/30%` becomes `(15/30)/100` = 1/200 instead of 15/(30/100) = 50. I found one more operator with the same break that the claim does not list: `^`. `2^50%` becomes `2**(50)/100`.

The two verdicts are real. `check("50", "15/30%", Numeric)` returns `correct = false` on the exact division the topic `percent-finding-the-whole` teaches, and A3 makes that verdict final. `check("1/200", "15/30%", Numeric)` returns `correct = true` for a learner whose value is 50, which is a C4 false positive.

The only percent ruling, docs/plans/M2.md:75, states that a percent binds to the number it foll

### #4 [blocker] The percent rewrite is unparenthesized, so `%` after `/` or `**` re-associates and grades a wrong answer correct

File: `crates/core/src/answer/normalize.rs:178` — IDs: C4, V4, A3 — duplicate of #3

**Claim.** `bind_percent` splices the bare text `(n)/100` into the source instead of `((n)/100)`, so when the percent number stands after a `/` or under a `**` the `/100` binds to the surrounding operator rather than to its own number, and the checker returns `correct = true` for a learner answer whose documented value is different.

**Evidence.**

```
crates/core/src/answer/normalize.rs:174-179 —
```
let number = out.get(start..trimmed).unwrap_or("").to_string();
out.truncate(start);
out.push('(');
out.push_str(&number);
out.push_str(")/100");
```
Probe against `cadus_core::answer::check` (debug build of the current HEAD):
```
norm("1/4%")   = "1/(4)/100"
norm("3 / 4%") = "3/(4)/100"
norm("4%^2")   = "(4)/100**2"
[numeric] "0.0025" vs "1/4%"  => correct=true notation=false
[numeric] "1/400"  vs "1/4%"  => correct=true notation=false
[numeric] "0.0004" vs "4%^2"  => correct=true notation=false
[numeric] "3/400"  vs "3 / 4%" => correct=true notation=false
[numeric] "25"     vs "1/4%"  => correct=false notation=false
[numeric] "75"     vs "3 / 4%" => correct=false notation=false
[numeric] "0.0016" vs "4%^2"  => correct=false notation=false
```
The ruling this violates is `docs/plans/M2.md:75-76` ("A percent binds to the number it follows, so `3 + 4%` is 3.04") and the module's own header, `crates/core/src/answer/normalize.rs:21` ("A `%` divides the number it follows, never the whole body"). `crates/core/tests/answer_parse.rs:192-205` pins the percent rewrite only after `+`, `-`, `*` and at the start of a string (`"50%"`, `"3 + 4%"`, `"1 + 49%"`, `"2*4%"`, `"1,500%"`, `"0.5%"`); no test puts a `%` after a `/` or under a `**`.
```

**Failure scenario.** A topic authors the numeric answer `0.0025`. A learner types `1/4%`, which under the M2 ruling is 1 / (4/100) = 25 — a wrong answer. `check("0.0025", "1/4%", AnswerKind::Numeric)` returns `Verdict { correct: true, notation: false }`, because `normalize` produces `1/(4)/100` = 1/400 = 0.0025. The same bug fires the other way: `check("75", "3 / 4%", Numeric)` returns `correct = false` for the learner answer 3 / 0.04 = 75, which is right.

**Refuter.** The claim is demonstrable, and I did not refute it.

1. The code does what the reviewer says. `bind_percent` (crates/core/src/answer/normalize.rs:161-185) truncates the number, then pushes four pieces: `(`, the number, `)`, and `/100`. The parenthesis pair covers the number alone. The quotient `<number>/100` stays bare in the source. A `/` or a `**` that touches the rewrite then binds to the `100` or to the number, and not to the percent value.

2. I reproduced every result. I built a scratch crate against crates/core (no repo file changed). The output matches the reviewer's probe character for character, and it adds two more cases.

3. The result breaks a binding ruling. docs/plans/M2.md:75-76 says a percent binds to the number it follows. crates/core/src/answer/normalize.rs:21 says the same: a `%` divides the number it follows, never the whole body. Under that rule `4%` is 4/100, so `1

### #5 [blocker] The bracket-free function argument does not stop at a function name, so three authored `derivatives-trig` answers mean a nested function and the correct learner answer is graded wrong

File: `crates/core/src/answer/parse.rs:706` — IDs: C4, V1, V4, A3

**Claim.** `parse_juxtaposed_argument` continues the bracket-free argument chain over any token that `starts_operand` accepts, and `starts_operand` accepts every `Tok::Ident`, so a second function name is swallowed into the first function's argument: the authored answer `sec x tan x` parses as `sec(x*tan(x))`, which marks the correct learner answer `sec(x)tan(x)` wrong and the wrong answer `sec(x tan x)` correct.

**Evidence.**

```
crates/core/src/answer/parse.rs:705-712 (`parse_juxtaposed_argument`):
                if !parser.starts_operand() {
                    break;
                }
                if matches!(parser.peek(), Some(Tok::Num(_))) {
                    parser.check_implicit_number(factors.last())?;
                }
                factors.push(parser.parse_power()?);

crates/core/src/answer/parse.rs:353-358 (`starts_operand`) accepts `Tok::Ident(_)`, and `parse_power` -> `parse_atom` -> `parse_name` turns a function name into another `parse_call`. Nothing in the stop set of the review round 2 ruling (`/`, `+`, `-`, `,`, `)`, `=`, `<`, `>`) names a function.

Probe against the real crate (scratchpad binary, path dependency on crates/core):
  normalize("sec x tan x").source == "sec x tan x"
  ast   = Func("sec", [Mul([Var("x"), Func("tan", [Var("x")])])])
  canon = Func("sec", [Poly({{Var("x"): 1, Call("tan", [ ... ]): 1}: 1/1}])

  check(expected, learner, Expression):
    "sec x tan x"   vs "sec(x tan x)"      => correct=true  notation=false   <-- FALSE POSITIVE (C4)
    "sec x tan x"   vs "sec(x)tan(x)"      => correct=false notation=false   <-- the right answer
    "sec x tan x"   vs "sec(x)*tan(x)"     => correct=false notation=false
    "sec x tan x"   vs "tan(x)/cos(x)"     => correct=false notation=false
    "-csc x cot x"  vs "-csc(x)cot(x)"     => correct=false notation=false
    "-csc x cot x"  vs "-csc(x)*cot(x)"    => correct=false notation=false
    "2 sin x cos x" vs "2 sin(x) cos(x)"   => correct=false notation=false
    "2 sin x cos x" vs "2*sin(x)*cos(x)"   => correct=false notation=false
    "2 sin x cos x" vs "2 sin(x cos x)"    => correct=true  notation=false   <-- FALSE POSITIVE (C4)
  The round 2 fix that runs the chain through `*` widens the same hole:
    "sec x tan x"   vs "sec x * tan x"     => correct=true  notation=false

The three answers are authored, not synthetic:
  curriculum/calculus-1/02-differentiation-rules.yaml:224-227
    - problem: 'Differentiate $f(x) = \sec x$.'
      answer: "sec x tan x"
    - problem: 'Differentiate $f(x) = \csc x$.'
      answer: "-csc x cot x"
  curriculum/calculus-1/02-differentiation-rules.yaml:238-240
    - problem: 'Differentiate $f(x) = \sin^2 x$.'
      answer: "2 sin x cos x"
      solution_sketch: 'General power rule: $2\sin x \cdot \cos x$ ...'
  Corpus rows (crates/core/tests/fixtures/answers/corpus_1_0.jsonl), all three `parsed: true`, kind `expression`:
    derivatives-trig kp1 1 'sec x t
```

**Failure scenario.** A learner works `derivatives-trig` kp1 exemplar 1, whose problem reads `Differentiate $f(x) = \sec x$` and whose authored answer is `sec x tan x`. The learner types the memorized, correct derivative `sec(x)tan(x)`. `parse` reads the authored side as `sec(x*tan(x))` and the learner side as `sec(x)*tan(x)`; the two canonical forms differ, so `check` returns `Verdict { correct: false, notation: false }`. Under A3 that is the final grade: the learner loses the item, the XP, and the FIRe credit for a correct answer, and no model call revisits it. On the same knowledge point a learner who types the meaningless composite `sec(x tan x)` gets `Verdict { correct: true }`. Exemplar 2 of the same knowledge point (`-csc x cot x`) and `kp2` exemplar 2 (`2 sin x cos x`) fail the same way; for `2 sin x cos x` even the spelling of its own solution sketch, `2 sin(x) cos(x)`, is graded wrong.

**Refuter.** The claim is demonstrable, and I reproduced it in full. I found no defense that holds.

What I confirmed:

1. The code path is as the reviewer describes. `parse_call` (parse.rs:669-673) hands a bracket-free function name to `parse_juxtaposed_argument`. That loop (parse.rs:705-712) continues on any token that `starts_operand` accepts, and `starts_operand` (parse.rs:353-358) accepts every `Tok::Ident`. `parse_name` (parse.rs:630-633) sends an ident in `FUNCTIONS` to `parse_call`. So a second function name enters the first function's argument. The stop set named in the round 2 ruling of docs/plans/M2.md ("It stops at `/`, `+`, `-`, `,`, `)`, `=`, `<`, and `>`") names no function.

2. The parse is wrong for the authored intent, and it compounds. `sec x tan x` gives `Func("sec", [Mul([Var("x"), Func("tan", [Var("x")])])])`. `sin x cos x tan x` nests three deep.

3. The C4 false positive is re

### #6 [major] The percent rewrite inserts a parenthesis that bypasses the space-grouped-number refusal, so `1 500%` means two different values

File: `crates/core/src/answer/normalize.rs:161` — IDs: C4, V4, V1

**Claim.** `bind_percent` runs after `strip_thousands_groups` and rewrites the trailing digit run into `(N)/100`, so the second group of a space-grouped number reaches the parser behind a `(`; `check_implicit_number` fires only on a `Tok::Num`, so the round-2 finding #11 refusal never runs and `3 + 1 500%` canonicalizes to 8 instead of 18.

**Evidence.**

```
normalize.rs:161-186 (`bind_percent`) rewrites the number the `%` follows, and normalize.rs:151-155 shows the order — `strip_thousands_groups` (a full-string match only) runs first, then the `%` is put back and bound:
    let body = strip_thousands_groups(body.trim());
    let body = if is_percent { body + "%" } else { body };
    let body = bind_percent(&body);
parse.rs:302-330 (`check_implicit_number`) and parse.rs:338-350 (`continues_a_space_group`) are the round-2 fix #11 guard, and parse.rs:283-286 only calls it when the next token is a number:
                if parser.starts_operand() {
                    if matches!(parser.peek(), Some(Tok::Num(_))) {
                        parser.check_implicit_number(factors.last())?;

Probe against `cadus_core::answer`, kind Expression:
  "8"  vs "3 + 1 500%"  => correct=true  notation=false   <-- FALSE POSITIVE
      source = "3 + 1 (500)/100"
      ast    = Add([Integer(3), Div(Mul([Integer(1), Integer(500)]), Integer(100))])
      canon  = Rational(8/1)          (the value the learner wrote is 3 + 1500% = 18)
  "18" vs "3 + 1 500%"  => correct=false
  "5x" vs "x/1 500%"    => correct=true  notation=false
      source = "x/1 (500)/100"        canon = Poly({{Var("x"): 1}: 5})   (learner wrote x/150000)
The same digits without the `%` are refused, which is the guard the percent path walks around:
  canonical_form("3 + 1 500")  -> Err("two numbers stand side by side")
  canonical_form("x/1 500")    -> Err("a space-grouped number stands after a factor")
And as a whole answer the grouping is read, so one string has two readings:
  canonical_form("1 500%")     -> Rational(15/1)      (group kept)
  canonical_form("3 + 1 500%") -> Rational(8/1)       (group split)
1.0 oracle: {"expected":"8","learner":"3 + 1 500%"} -> {"equivalent": false}; {"expected":"18",...} -> {"equivalent": false}. The trailing `%` is a 2.0 addition, so both readings are 2.0's own.
```

**Failure scenario.** An item asks for a total and the authored answer is `8`. A learner adds a base of 3 to a fifteen-hundred-percent increase and types `3 + 1 500%`, writing the thousand with the space separator the V4 table accepts. `bind_percent` turns the last group into `(500)/100`, the inserted `(` keeps the token after `1` from being a `Tok::Num`, the space-group refusal of review round 2 (finding #11) never runs, and the parser reads `3 + 1*(500)/100`. `canon` gives Rational(8) and `check` returns correct=true for an answer worth 18. The same hole gives `x/1 500%` the value 5x instead of x/150000.

**Refuter.** I cannot refute the claim. The defect reproduces on the current tree, and every step of the stated mechanism holds.

`to_source` runs `strip_thousands_groups` before it puts the `%` back, and `strip_thousands_groups` matches the whole string only. In `3 + 1 500%` the space group is not a full match, so the digits stay apart. `bind_percent` then rewrites the last digit run and produces `3 + 1 (500)/100`. The `(` makes the token after `1` a `Tok::LParen`, and parse.rs:283-286 calls `check_implicit_number` only for a `Tok::Num`. The finding #11 refusal never runs. The parser reads `3 + 1*(500)/100`, `canon` gives Rational(8), and `check("8", "3 + 1 500%", Expression)` returns correct=true. The learner wrote 3 + 1500% = 18. This is a false positive, which C4 calls a blocker.

The same hole gives `x/1 500%` the value 5x and `2 + 1 000%` the value 2, and it leaves one string with two readings:

### #7 [major] A negative mixed number with a zero whole part loses its sign, so `-0 1/2` is graded as +1/2

File: `crates/core/src/answer/parse.rs:952` — IDs: C4, V4, V1

**Claim.** `whole_number` folds the leading minus into the whole part as `-0`, which is the integer 0, and `canon::mixed` then reads the sign from `whole.is_negative()`, so the minus of `-0 1/2` is dropped and `check("1/2", "-0 1/2")` returns correct=true for the value -1/2.

**Evidence.**

```
parse.rs:949-955 (`whole_number`) — the sign is folded into the integer and `-0` is `0`:
    fn whole_number(node: &Ast) -> Option<BigInt> {
        match node {
            Ast::Integer(value) => Some(value.clone()),
            Ast::Neg(inner) => whole_number(inner).map(|value| -value),
canon.rs:469-475 (`mixed`) — the only place the sign is read back:
        let value = if whole.is_negative() { -magnitude } else { magnitude };

Probe against `cadus_core::answer`, kind Expression:
  "1/2"  vs "-0 1/2"  => correct=true  notation=false   <-- FALSE POSITIVE
      source = "-0 1/2"   ast = Mixed { whole: 0, numerator: 1, denominator: 2 }
      canon  = Rational(1/2)          (the value the learner wrote is -1/2)
  "-1/2" vs "-0 1/2"  => correct=false
  "1/2"  vs "-0½"     => correct=true  notation=false
The non-zero whole part carries its sign correctly, which shows the defect is the zero case alone (round-2 ruling `-2 1/2` = -5/2):
  canonical_form("-2 1/2") -> Rational(-5/2)
  canonical_form("-1 1/3") -> Rational(-4/3)
1.0 oracle: {"expected":"1/2","learner":"-0 1/2"} -> {"equivalent": false}; {"expected":"-1/2","learner":"-0 1/2"} -> {"equivalent": false}. The mixed-number production is a 2.0 addition, so the sign loss is 2.0's own.
```

**Failure scenario.** An item on subtracting mixed numbers has the authored answer `1/2`. A learner who subtracts in the wrong order gets minus one half and writes it in the mixed-number form the topic teaches: `-0 1/2`. The parser builds `Mixed { whole: 0, numerator: 1, denominator: 2 }` because `-0` is the integer 0, `canon::mixed` finds `whole.is_negative()` false, and the value becomes +1/2. `check` returns correct=true for an answer with the wrong sign, and the same string against the true value `-1/2` is marked wrong. `-0½` and `-0 3/4` behave the same way.

**Refuter.** The defect is real and reproducible. `parse::whole_number` (crates/core/src/answer/parse.rs:949-955) folds the unary minus into the integer with `-value`. For the source `-0`, `-BigInt::zero()` is `BigInt::zero()`, so the written minus disappears at that point. `parse::read_mixed_number` (parse.rs:419-453) then stores that value as `Ast::Mixed { whole: 0, .. }`, and `canon::mixed` (crates/core/src/answer/canon.rs:461-476) reads the sign back from `whole.is_negative()` alone, which is false for 0. No other node carries the sign, because `parse_term` puts the `Neg` inside `factors[0]` and `read_mixed_number` replaces the whole factor list with the `Mixed` node. The result is a FALSE POSITIVE under C4: `check("1/2", "-0 1/2", Expression)` returns correct=true for a learner value of minus one half.

Three points make the case that this is a defect and not a ruling:

1. The M2 plan states the

### #8 [major] The new \sqrt product sign is inserted before \times and \cdot become `*`, so `2\times\sqrt{3}` normalizes to the power `2**sqrt(3)` and gets no verdict

File: `crates/core/src/answer/normalize.rs:285` — IDs: V4, V2

**Claim.** `rewrite_latex_braces` calls `push_product_sign` in front of `\sqrt` while `\cdot` and `\times` are still literal backslash words, so the `*` lands on the `s` or `t` that ends `\cdot`/`\times`; the later `.replace("\\cdot", "*")` then turns the pair into `**`, and `2\times\sqrt{3}` becomes the exponent form `2**sqrt(3)` — Undecidable — where the same pair was decided correct before FIXM2d.

**Evidence.**

```
normalize.rs:139-145 (`to_source`), the two passes in this order:
    let body = rewrite_latex_braces(&body.chars().collect::<Vec<char>>(), 0);
    let body = body.replace('^', "**");
    let body = body
        .replace("\\cdot", "*")
        .replace("\\times", "*")
normalize.rs:284-285 (inside `rewrite_latex_braces`):
        if let Some(after_keyword) = match_literal(chars, i, "\\sqrt") {
            push_product_sign(&mut out);
normalize.rs:337-340 (`push_product_sign`) tests `c.is_alphanumeric() || c == ')'`, and the last character of `\cdot` and `\times` is a letter.

Probe against `cadus_core::answer` (current HEAD):
  normalize("2\\times\\sqrt{3}").source == "2**sqrt(3)"   ast = Err("an exponent that is not a whole number")
  normalize("2\\cdot\\sqrt{3}").source  == "2**sqrt(3)"   ast = Err("an exponent that is not a whole number")
  normalize("x\\cdot\\sqrt{2}").source  == "x**sqrt(2)"   ast = Err("an exponent that is not a whole number")
  normalize("2 \\times \\sqrt{3}").source == "2 * sqrt(3)"  canon = 2*sqrt(3)      (the spaced spelling still works)
  normalize("\\pi\\sqrt{2}").source     == "pi*sqrt(2)"    canon = pi*sqrt(2)     (other backslash words are harmless)

  check("2*sqrt(3)", "2\\times\\sqrt{3}", Expression) => Undecidable("an exponent that is not a whole number")
  check("5*sqrt(2)", "5\\cdot\\sqrt{2}", Expression)  => Undecidable("an exponent that is not a whole number")

The same probe against a build of the pre-FIXM2d tree (git archive 4763e2d into the scratchpad, path dependency on its crates/core):
  normalize("2\\times\\sqrt{3}").source == "2*sqrt(3)"
  check("2*sqrt(3)", "2\\times\\sqrt{3}", Expression) => correct=true  notation=false
  check("2*sqrt(3)", "2\\cdot\\sqrt{3}", Expression)  => correct=true  notation=false
So FIXM2d finding #9 turned two decided-correct pairs into refusals.

Footprint: in a 29,808-expression random fuzz over the grammar, 326 expressions (every one that writes `\cdot` or `\times` with no space in front of a `\sqrt`) are refused with "an exponent that is not a whole number", e.g. `27\times\sqrt{64} / 46-0.83`.
```

**Failure scenario.** A learner answers a radicals item — for example `adding-subtracting-radicals`, authored `5*sqrt(2)` — in the LaTeX the curriculum itself uses, writing the product sign with no space: `5\cdot\sqrt{2}`. `to_source` inserts the new product sign while `\cdot` is still spelled `\cdot`, giving `5\cdot*sqrt(2)`; the `\cdot` rewrite then produces `5**sqrt(2)`, the parser refuses it with "an exponent that is not a whole number", and `check` returns `Outcome::Undecidable`. The learner wrote the right answer in two tolerances the V4 table names (`\cdot` carried from 1.0, `\sqrt{a}` added by 2.0) and gets no deterministic verdict, where the same build one commit earlier returned correct=true.

**Refuter.** I tried to refute the claim and failed. The defect is real, and I reproduced every step.

The pass order in `to_source` is the cause. `rewrite_latex_braces` runs first, while `\cdot` and `\times` are still literal backslash words. `push_product_sign` tests the last character of the output so far, and that character is the `t` of `\cdot` or the `s` of `\times`. The test passes, so a `*` joins the keyword. The later `.replace("\\cdot", "*")` turns `\cdot*` into `**`. The parser then reads an exponent whose right side is `sqrt(...)` and refuses it with "an exponent that is not a whole number".

The regression is real. At HEAD, `check("5*sqrt(2)", "5\\cdot\\sqrt{2}", Expression)` returns `Undecidable`. At commit 4763e2d, one commit before FIXM2d, the same call returns `correct=true, notation=false`. FIXM2d finding #9 fixed `\pi\sqrt{2}` and broke the unspaced `\cdot`/`\times` product.

No ru

## FIXM2h

### #9 [major] sqrt of a rational perfect square is never evaluated, so sqrt(1/2) and sqrt(4/9) are decided wrong against their own values

File: `crates/core/src/answer/canon.rs:583` — IDs: V4, V3, A3

**Claim.** `Work::call` reduces `sqrt` only when `integer_value` returns a whole number, so a rational radicand keeps the opaque `Atom::Call("sqrt", …)` and never meets the radical arithmetic that already reduces `sqrt(4)`, `sqrt(8)` and `1/sqrt(2)`; both sides parse, so `check("sqrt(2)/2", "sqrt(1/2)")` is a decided `correct = false` on an answer the 1.0 oracle calls equivalent.

**Evidence.**

```
canon.rs:582-587 (`Work::call`):
        if arguments.len() == 1 {
            if name == "sqrt"
                && let Some(value) = arguments.first().and_then(integer_value)
            {
                return self.root_of_integer(&value, arguments);
            }
A non-integer argument falls through to `Ok(atom_value(Atom::Call(name.to_string(), arguments)))`.

Probe against `cadus_core::answer`:
  canon("sqrt(1/2)")            = Func("sqrt", [Rational(1/2)])
  canon("sqrt(4/9)")            = Func("sqrt", [Rational(4/9)])
  canon("sqrt(0.25)")           = Func("sqrt", [Rational(1/4)])
  canon("\\sqrt{\\frac{4}{9}}") = Func("sqrt", [Rational(4/9)])
  canon("sqrt(2)/2")            = Radical({Basis { radicand: 2, pi: 0, e: 0 }: 1/2})
  canon("1/sqrt(2)")            = Radical({Basis { radicand: 2, pi: 0, e: 0 }: 1/2})   (the same value IS reduced here)
  canon("sqrt(4)")              = Rational(2/1)                                        (an integer radicand IS reduced)

  check(expected, learner, Numeric):
    "sqrt(2)/2" vs "sqrt(1/2)"            => correct=false notation=false
    "sqrt(2)/2" vs "\\sqrt{1/2}"           => correct=false notation=false
    "sqrt(2)/2" vs "\\sqrt{\\frac{1}{2}}"  => correct=false notation=false
    "2/3"       vs "sqrt(4/9)"            => correct=false notation=false
    "1/2"       vs "sqrt(0.25)"           => correct=false notation=false

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  {"expected":"sqrt(2)/2","learner":"sqrt(1/2)"} -> {"equivalent": true}
  {"expected":"2/3","learner":"sqrt(4/9)"}       -> {"equivalent": true}
  {"expected":"3/2","learner":"sqrt(9/4)"}       -> {"equivalent": true}
  {"expected":"1/2","learner":"sqrt(0.25)"}      -> {"equivalent": true}

No ruling covers the verdict. `crates/core/tests/answer_divergence.rs` pins no sqrt pair of this shape, and no generator family of `answer_oracle.rs` writes a rational radicand, so the 15,940-pair class-3 parity claim never reaches it. The only statement of the behavior is a canon.rs module comment and the FORM test `a_radicand_that_is_not_a_whole_number_stays_a_function` (crates/core/tests/answer_check.rs:443), which asserts the canonical shape and no verdict.
```

**Failure scenario.** A learner works `variance-random-variables` kp3 exemplar 1 (curriculum/probability-statistics/03-random-variables.yaml:290, answer_kind numeric, authored answer `sqrt(2)/2`). The problem states `Var(X) = 1/2` and asks for SD(X) exactly, and the authored solution sketch is `SD(X) = \sqrt{1/2} = 1/\sqrt{2} = \sqrt{2}/2`. The learner types the first form of that very sketch: `sqrt(1/2)`. Both sides parse, `canon` gives `Func("sqrt", [Rational(1/2)])` for the learner and `Radical({sqrt(2)}: 1/2)` for the authored answer, and `check` returns `Verdict { correct: false, notation: false }`. A fully correct exact answer is recorded wrong and A3 makes that verdict final, while the 1.0 checker answers True for the same pair.

**Refuter.** I could not refute it. Every step reproduces. `Work::call` (crates/core/src/answer/canon.rs:582-587) gates the radical path on `integer_value`, so a rational radicand falls through to `Atom::Call("sqrt", …)` and never reaches `root_of_integer` or the `Atom::Sqrt` merge in `add_atom`. The two sides of the same value therefore land in two different `Canon` variants — `Func("sqrt", [Rational(1/2)])` against `Radical({sqrt(2)}: 1/2)` — and `check` compares them structurally and returns a DECIDED `correct=false`, not `Undecidable`. I ran the shipped `cadus_core::answer::check` and the 1.0 oracle side by side: the checker says false on four pairs the oracle calls equivalent. The pair is reachable from authored content: curriculum/probability-statistics/03-random-variables.yaml:290 (kp3 exemplar 2, answer `sqrt(2)/2`) prints the solution sketch `SD(X) = \sqrt{1/2} = 1/\sqrt{2} = \sqrt{2}/2`, so

### #10 [major] An Inverse atom keeps a common monomial factor in its divisor, so `1/(x(x+h))` and `1/x * 1/(x+h)` are two canonical forms of one value

File: `crates/core/src/answer/canon.rs:961` — IDs: V3, A3, V1

**Claim.** `content_normalize` extracts only a rational content from the divisor sum, so a divisor that carries a common variable factor stays whole inside `Atom::Inverse`; a monomial power (`x**-1`) never merges with that atom, and the round-2 fix for finding #17 therefore covers Inverse×Inverse but not monomial×Inverse.

**Evidence.**

```
canon.rs:961-970 (`content_normalize`) folds a gcd over the numerators and an lcm over the denominators only. No monomial gcd is taken:
        let mut numerator_gcd = BigInt::zero();
        let mut denominator_lcm = BigInt::one();
        for coefficient in sum.values() { ... numerator_gcd = numerator_gcd.gcd(coefficient.numer()); ... }
canon.rs:812-815 (`add_inverse`) merges only when the monomial already holds an `Atom::Inverse`:
        let present = monomial.iter().find_map(|(key, power)| match key { Atom::Inverse(value) => Some(...), _ => None });

Probe against cadus_core::answer (release, path dependency on crates/core):
  canonical_form("1/(x*(x+1))") = Poly({{Inverse(Poly({{x:1}:1, {x:2}:1})):1}:1})
  canonical_form("1/x/(x+1)")   = Poly({{x:-1, Inverse(Poly({{}:1,{x:1}:1})):1}:1})

  check(expected, learner, Expression):
    "-1/(x(x + h))"      vs "-1/x * 1/(x+h)"          => correct=false   <-- corpus answer
    "-2/(x(x + h))"      vs "-2/x * 1/(x+h)"          => correct=false   <-- corpus answer
    "-1/(x(x + h))"      vs "-(1/x)(1/(x+h))"         => correct=false
    "1/(2√x (1 + x))"    vs "1/(2√x) * 1/(1 + x)"     => correct=false   <-- corpus answer
    "1/(x(x+1))"         vs "1/x/(x+1)"               => correct=false
    "1/(2x(x+1))"        vs "1/(2x) * 1/(x+1)"        => correct=false
  The fixed half of #17 still works, which shows the gap is the unfixed half:
    "4/((x-2)*(x+2))"    vs "4/(x-2) * 1/(x+2)"       => correct=true
    "-1/(x(x + h))"      vs "-1/(x^2 + xh)"           => correct=true

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  {"expected":"-1/(x(x + h))","learner":"-1/x * 1/(x+h)"}      -> {"equivalent": true}
  {"expected":"-2/(x(x + h))","learner":"-2/x * 1/(x+h)"}      -> {"equivalent": true}
  {"expected":"-1/(x(x + h))","learner":"-(1/x)(1/(x+h))"}     -> {"equivalent": true}
  {"expected":"1/(2√x (1 + x))","learner":"1/(2√x) * 1/(1 + x)"} -> {"equivalent": true}

The authored answers are in the corpus and in the tree:
  curriculum/calculus-1/01-derivative.yaml:121  answer: "-1/(x(x + h))"   (topic difference-quotients, kp3 exemplar 0)
  curriculum/calculus-1/01-derivative.yaml:123  answer: "-2/(x(x + h))"   (kp3 exemplar 1)
  curriculum/calculus-1/02-differentiation-rules.yaml:887  answer: "1/(2√x (1 + x))"
The same three strings appear in crates/core/tests/fixtures/answers/corpus_1_0.jsonl (difference-quotients kp3 0/1 and the diagnostic, derivatives-inverse-trig k
```

**Failure scenario.** A learner works difference-quotients kp3 exemplar 0: for f(x) = 1/x, simplify (f(x+h) - f(x))/h. The authored answer is `-1/(x(x + h))`. The learner reaches the same value but stops one step earlier and writes `-1/x * 1/(x+h)` — the product of the two reciprocals the solution sketch itself builds. `canon` gives the monomial {Var x: -1, Inverse(x+1... i.e. x+h): 1} for the learner and {Inverse(x^2+xh): 1} for the author, `same_answer` compares two unequal Poly values, and `check` returns Verdict { correct: false, notation: false }. A correct answer is graded wrong, A3 keeps it away from the model, and the learner loses the item. 1.0 grades the same pair equivalent, so this is a 2.0 regression, not carried behavior. The same failure hits the two sibling exemplars of the same knowledge point and derivatives-inverse-trig kp2 exemplar 2.

**Refuter.** The claim is demonstrable, and no ruling covers it. Three checks confirm it.

1. The code reads as the reviewer describes. `/home/deploy/dev/cadus2.0/crates/core/src/answer/canon.rs:961` (`content_normalize`) folds a `gcd` over the coefficient numerators and an `lcm` over the coefficient denominators only. It takes no monomial content, so a divisor with a common variable factor keeps that factor inside `Atom::Inverse`. `/home/deploy/dev/cadus2.0/crates/core/src/answer/canon.rs:812-815` (`add_inverse`) finds a merge partner only through `Atom::Inverse(value)`; a negative variable power such as `Var("x"): -1` matches no arm of that `find_map`, so the merge never starts.

2. The divergence is real, and it hits authored corpus answers. I built a release binary that links `cadus_core::answer` by path and ran `check` with `AnswerKind::Expression`. All six product-of-reciprocals pairs return `V

### #11 [major] `sqrt` is reduced only for a whole-number radicand, so `√(1/2)` is not `√2/2` and `√(9/16)` is not `3/4`

File: `crates/core/src/answer/canon.rs:583` — IDs: V3, A3, V1 — duplicate of #9

**Claim.** `Work::call` gates the radical reduction on `integer_value`, so a rational radicand — including a perfect square such as 9/16 — never reaches `root_of_integer` and stays an opaque `Atom::Call("sqrt", …)`; the checker therefore reduces `1/√2` to `√2/2` but leaves `√(1/2)` unreduced.

**Evidence.**

```
canon.rs:583-587 (`Work::call`):
            if name == "sqrt"
                && let Some(value) = arguments.first().and_then(integer_value)
            {
                return self.root_of_integer(&value, arguments);
            }
canon.rs:1087-1092 (`integer_value`) returns Some only for `Canon::Rational(r)` with `r.is_integer()`.

Probe against cadus_core::answer (release):
  canonical_form("√(1/2)")   = Func("sqrt", [Rational(1/2)])          <-- unreduced
  canonical_form("√2/2")     = Radical({Basis{radicand:2,pi:0,e:0}: 1/2})
  canonical_form("1/√2")     = Radical({Basis{radicand:2,pi:0,e:0}: 1/2})   <-- reduced
  canonical_form("sqrt(9/16)") = Func("sqrt", [Rational(9/16)])
  canonical_form("sqrt(8/2)")  = Rational(2/1)                        <-- reduced, because 8/2 is a whole number

  check(expected, learner, kind):
    "√2/2"  vs "1/√2"      => correct=true       <-- the reduction works one way
    "√2/2"  vs "√(1/2)"    => correct=false      <-- and not the other
    "√3/3"  vs "1/√3"      => correct=true
    "√3/3"  vs "√(1/3)"    => correct=false
    "3/4"   vs "√(9/16)"   => correct=false
    "2/3"   vs "sqrt(4/9)" => correct=false
    "1/2"   vs "√(1/4)"    => correct=false
    "0.5"   vs "sqrt(0.25)"=> correct=false
    "2"     vs "sqrt(8/2)" => correct=true

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  {"expected":"√2/2","learner":"√(1/2)"}   -> {"equivalent": true}
  {"expected":"2/3","learner":"sqrt(4/9)"} -> {"equivalent": true}
  {"expected":"1/2","learner":"√(1/4)"}    -> {"equivalent": true}
  {"expected":"3/4","learner":"√(9/16)"}   -> {"equivalent": true}
  {"expected":"0.5","learner":"sqrt(0.25)"}-> {"equivalent": true}

The authored answers are in the tree and in the corpus fixture:
  curriculum/foundations/09-rational-trig.yaml:1374  answer: "√2/2"   (unit-circle special angles, kp2: "Find $\sin(\pi/4)$")
  curriculum/foundations/09-rational-trig.yaml:1387  answer: "√3/3"   (kp3: "Find $\tan(\pi/6)$")
  crates/core/tests/fixtures/answers/corpus_1_0.jsonl carries "√2/2", "-√2/2", "√3/3", "-√3/3", "2√3/3".

No ruling covers it. docs/plans/M2.md says nothing about a rational radicand; crates/core/tests/answer_divergence.rs pins only the decimal-approximation class (lines 175-199) and the `\sqrt` product sign (lines 432-444); docs/reference/undecidable-answers.md pins refusals, not wrong verdicts. The canon header sentence "A radicand that is not a whole number stays a function applicat
```

**Failure scenario.** A learner answers unit-circle-values kp2 exemplar 1, "Find sin(pi/4)", whose authored answer is `√2/2`. The learner reads the value off the 45-45-90 triangle as the square root of one half and types `√(1/2)` — the same exact real number. `normalize` gives the source `sqrt(1/2)`, `parse` gives `Func("sqrt", [Fraction 1/2])`, and `canon` leaves it as `Func("sqrt", [Rational(1/2)])` because `integer_value` refuses 1/2, so `check` returns Verdict { correct: false, notation: false }. The neighbouring spelling `1/√2` of the same value is graded correct, so one knowledge point grades two exact spellings two ways. 1.0 grades `√(1/2)` equivalent to `√2/2`, so this is a 2.0 regression. The same failure hits `√3/3` against `√(1/3)` on kp3, and every "simplify the radical of a fraction" item where the authored answer is the reduced rational (`3/4` against `√(9/16)`).

**Refuter.** The claim is demonstrable and no ruling covers it. I reproduced every probe line against a release build of `cadus_core::answer`, and I reproduced every 1.0 oracle line with the live oracle.

The code path is as stated. `Work::call` (crates/core/src/answer/canon.rs:582-587) gates the `sqrt` reduction on `integer_value`, and `integer_value` (canon.rs:1087-1092) returns `Some` only for `Canon::Rational(r)` when `r.is_integer()`. A rational radicand therefore never reaches `root_of_integer` and stays `Atom::Call("sqrt", ...)`.

The inconsistency inside the same module is real, and it is not an accident of one input. `Work::add_atom` (canon.rs:721-776) holds a deliberate invariant: "A monomial holds at most one `Atom::Sqrt`, and that atom always has exponent 1." That invariant is what rationalizes `1/√2` into `√2/2`. The canonical form therefore does rationalize a radical denominator by desi

### #12 [major] The `Atom::E` fold of FIXM2e fires only when the whole constant term is an integer, so the exponent law still fails for a fractional exponent and `e^(5/2)` is not `e^2*e^(1/2)`

File: `crates/core/src/answer/canon.rs:888` — IDs: V1, V3, A3, C4

**Claim.** `add_exp` folds the constant term of an `Atom::Exp` argument into `Atom::E` only when that whole constant `is_integer()`, not when it merely has a non-zero whole part, so a learner who applies the exponent law to an exponent with a fractional part gets `correct = false`: `e^(5/2)` and `e^2*e^(1/2)` are two canonical values in 2.0 and one value in 1.0.

**Evidence.**

```
crates/core/src/answer/canon.rs:886-895 (`add_exp`):
        let whole = argument
            .get(&constant)
            .filter(|value| value.is_integer())
            .map(BigRational::to_integer)
            .as_ref()
            .and_then(BigInt::to_i64);
        if let Some(whole) = whole {
            argument.remove(&constant);
            insert_atom(monomial, &Atom::E, whole)?;
        }
The `filter` refuses 5/2, so `e^(5/2)` keeps the monomial `{Exp(5/2): 1}` while `e^2*e^(1/2)` keeps `{E: 2, Exp(1/2): 1}`. The docs/plans/M2.md round 2 entry says "The whole-number part of an `Atom::Exp` argument folds into `Atom::E`"; 5/2 has the whole-number part 2, and it is not folded.

Probe against the real crate:
  check(expected, learner, Expression):
    "e^(5/2)"     vs "e^2*e^(1/2)"     => correct=false notation=false
    "e^(3/2)"     vs "e*e^(1/2)"       => correct=false notation=false
    "e^(x+5/2)"   vs "e^2*e^(x+1/2)"   => correct=false notation=false
    "e^(x+3/2)"   vs "e*e^(x+1/2)"     => correct=false notation=false
  The integer case that FIXM2e added does work, which shows the gap is the filter and not the rule:
    "e^(x+2)"     vs "e^2*e^x"         => correct=true  notation=false
    "e^(x+1/2)"   vs "e^(1/2)*e^x"     => correct=true  notation=false

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python), measured:
  {"expected":"e^(5/2)","learner":"e^2*e^(1/2)"}       -> {"equivalent": true}
  {"expected":"e^(3/2)","learner":"e*e^(1/2)"}         -> {"equivalent": true}
  {"expected":"e^(x+5/2)","learner":"e^2*e^(x+1/2)"}   -> {"equivalent": true}
  {"expected":"e^(x+3/2)","learner":"e*e^(x+1/2)"}     -> {"equivalent": true}
The divergence carries no reason in `DOCUMENTED_REASONS` (answer_oracle.rs:1816-1832) and is not the D6 float class: both sides are exact. `documented_reason` would return `None` for such a pair and it would fail the class 3 assertion; no generator builds one, so `the_two_checkers_agree_on_every_comparable_pair` never sees it.

Scale, measured: I enumerated 480 spellings of `e^a`/`exp(a)`/`e^a*e^b`/`e^a/e^b` over the arguments {x, 2x, x+1, x+2, x+1/2, x+3/2, x+5/2, 1/2, 3/2, 5/2, 2, 3, -x, x-1/2, 2x+3/2}, took the pairs 2.0 calls different, and asked SymPy. Of a 4,000-pair sample restricted to the fractional arguments, 28 pairs are the same value and 2.0 grades them different; every one has a non-integer constant part, and no other exponent-law shape failed. `crates/core/tests/answer_ch
```

**Failure scenario.** A learner on a laws-of-exponents or continuous-growth item is asked for a value the author writes as `e^(5/2)`. The learner writes the same value with the exponent law applied, `e^2*e^(1/2)`. `canon` gives the authored side the monomial `{Exp(5/2): 1}` and the learner side `{E: 2, Exp(1/2): 1}`; `same_answer` compares two different `Canon::Poly` values and `check` returns `Verdict { correct: false, notation: false }`. Under A3 that verdict is final, so a correct answer is recorded wrong. 1.0 returned `equivalent: true` for the same pair, so the fix that review round 2 finding #8 ordered closes the integer half of the exponent law and leaves the fractional half open.

**Refuter.** The claim is demonstrable, and no ruling covers it. I reproduced both halves against the real code and the live 1.0 oracle.

1. The code. `add_exp` at crates/core/src/answer/canon.rs:886-895 reads the constant term of the exponential argument and applies `.filter(|value| value.is_integer())`. The filter drops every non-integer constant, so no part of it folds into `Atom::E`. The canonical forms I printed show the split directly: `e^(5/2)` gives `Poly({{Exp(Rational(5/2)): 1}: 1})`, and `e^2*e^(1/2)` gives `Poly({{E: 2, Exp(Rational(1/2)): 1}: 1})`. `same_answer` compares two different `Canon::Poly` values, so `check` decides `correct = false`.

2. The verdicts. A probe test in the crate gave `Decided(Verdict { correct: false, notation: false })` for `e^(5/2)` vs `e^2*e^(1/2)`, `e^(3/2)` vs `e*e^(1/2)`, `e^(x+5/2)` vs `e^2*e^(x+1/2)`, and `e^(x+3/2)` vs `e*e^(x+1/2)`. The integer control 

### #13 [major] The round-2 reciprocal fix merges two Inverse atoms only, so a sum of reciprocals and a reciprocal of a product stay different values from their combined form on 16 authored corpus answers

File: `crates/core/src/answer/canon.rs:796` — IDs: V3, C4, A3, V1

**Claim.** `add_inverse` merges `Atom::Inverse` with `Atom::Inverse` inside one monomial, but two terms that carry different `Atom::Inverse` atoms are never put over a common denominator and an `Atom::Inverse` never merges with a monomial reciprocal, so 22 learner spellings of 16 authored corpus answers that 1.0 grades True are graded correct=false.

**Evidence.**

```
crates/core/src/answer/canon.rs:796-838 (`add_inverse`): the merge runs only when the SAME monomial already holds an `Atom::Inverse` (`monomial.iter().find_map(|(key, power)| match key { Atom::Inverse(value) => ... })`). Addition (`insert_term`, canon.rs:634) never combines two terms over a common divisor, and `1/x` produces `Var("x"): -1` and no `Atom::Inverse` at all, so it never reaches the merge.

Sweep: 401 learner variants built with SymPy `together`/`apart`/`cancel`/`factor`/`expand`/`radsimp` over the deduplicated in-grammar corpus, then scored with 2.0 `check` and with the live 1.0 oracle. 22 pairs are 1.0 True / 2.0 correct=false, over 16 distinct authored answers and 8 topics (adding-subtracting-rational-expressions, complex-fractions, derivatives-natural-log, dividing-rational-expressions, multiplying-dividing-rational-expressions, multiplying-rational-expressions, rational-expressions, rational-expressions-common-denominators). Zero pairs go the other way.

Anchors, reproduced one by one:
  "2/x + 1/(x + 1)"      vs "(3x + 2)/(x(x + 1))"   2.0 false, 1.0 {"equivalent": true}
  "2/x - 1/(x + 3)"      vs "(x + 6)/(x(x + 3))"    2.0 false, 1.0 true
  "4/((x - 2)(x + 2))"   vs "1/(x - 2) - 1/(x + 2)" 2.0 false, 1.0 true
  "(5x - 1)/((x + 1)(x - 1))" vs "3/(x + 1) + 2/(x - 1)" 2.0 false, 1.0 true
  "(x + 2)/(x - 2)"      vs "1 + 4/(x - 2)"         2.0 false, 1.0 true
  "-1/(x(x + h))"        vs "-1/x * 1/(x + h)"      2.0 false, 1.0 true
  "-1/(x(x + h))"        vs "(-1/x)/(x + h)"        2.0 false, 1.0 true
The fix DOES hold on the shapes it was written for, which shows the gap is the rule's reach and not its arithmetic:
  "4/((x - 2)(x + 2))" vs "4/(x^2 - 4)"      => correct=true
  "1/(x+1)^2"          vs "(1/(x+1))^2"      => correct=true
  "1/(x*(x+1))"        vs "1/(x^2+x)"        => correct=true

No ruling covers it. docs/plans/M2.md 'Revised after review (round 2)' rules `Inverse(p)^n` and the two-atom merge and states only that the rule 'cancels no common factor' — combining `2/x + 1/(x+1)` over a common denominator cancels nothing. crates/core/tests/answer_divergence.rs pins only `(x^2-1)/(x-1)` vs `x+1`.
```

**Failure scenario.** A learner works `derivatives-natural-log` kp3, whose authored answer is `2/x + 1/(x + 1)`. The learner differentiates correctly and writes the derivative over one denominator, `(3x + 2)/(x(x + 1))`, which is the same function. `canon` gives the authored side `2*Inverse(x) as Var(x):-1 plus Inverse(x+1)` and the learner side one monomial `(3x+2)*Inverse(x^2+x)`; the two Poly maps differ, `same_answer` is false, and `check` returns Verdict { correct: false, notation: false }. 1.0 returned True for the same pair. A3 makes the wrong verdict final: the answer never reaches the model, and the learner loses the knowledge point for a correct derivative. The same happens on 15 further authored answers across 8 topics.

**Refuter.** I cannot refute the claim. Every fact in it is demonstrable, and my own sweep gives a larger count than the reviewer reports.

What holds, exactly:
1. The code path is as described. `add_inverse` (canon.rs:796-829) merges a divisor only into an `Atom::Inverse` that the same monomial already holds. `insert_term` (canon.rs:634) adds coefficients of equal monomials and never puts two terms over a common divisor.
2. All seven anchor pairs give `correct: false` in 2.0 and `true` in the live 1.0 oracle. I ran both sides.
3. The scale is real. My sweep found 43 such pairs over 19 authored corpus answers and 12 topics, with zero pairs in the opposite direction.
4. The authored answer `2/x + 1/(x + 1)` is in the corpus at derivatives-natural-log kp3, so the failure scenario is a real knowledge point.

Two corrections to the claim, which change its scope but not its truth:

First, the claim bundle

## FIXM2i

### #14 [major] No generator family rewrites a rational expression, so the 100.0000% class-3 agreement is still an artifact of generator choice — the defect round-2 finding #16 raised

File: `crates/core/tests/answer_oracle.rs:1402` — IDs: V3, C4

**Claim.** The 38 families of `GENERATORS` change spelling, spacing, digits, and two hand-written polynomial refactors, and none of them puts a rational expression over a common denominator or splits one, so the harness cannot reach the shape where 2.0 and 1.0 actually disagree; a 401-pair SymPy rewrite sweep over the same corpus finds 22 class-3 divergences the harness reports as 0.

**Evidence.**

```
crates/core/tests/answer_oracle.rs:1402 `const GENERATORS: [Generator; 38]`. The only algebraic family is `generate_algebraic_refactor` (answer_oracle.rs:1164), whose doc-comment says it runs exactly two rules: 'A difference of two squares takes its factored form' and 'A product whose last factor is a parenthesized sum multiplies out'. `factor_a_difference_of_squares` requires `terms.len() == 2` and a perfect square on each side, so no rational expression enters it. `generate_product_reorder` (answer_oracle.rs:1129) refuses anything that is not one multiplicative term.

answer_oracle.rs:2294-2301 `CLASS_COUNTS` asserts ("class 3 comparable", 15940) with 100% agreement, and docs/plans/M2.md repeats 'Class 3 agreement with the live 1.0 checker is 15,940 of 15,940 (100.0000%), and no pair of the set carries an undocumented divergence.'

I ran the family the harness lacks: for each deduplicated corpus answer, parse it with the 1.0 parser (`cadus_web.sympy_check._parse`) and emit `together`, `apart`, `cancel`, `factor`, `expand`, `radsimp` of it — every one of these is a spelling a learner writes and 1.0 grades True by construction. 401 pairs, 353 of them comparable on both sides. 22 are 1.0 True / 2.0 correct=false, e.g.
  ("2/x + 1/(x + 1)", "(3*x + 2)/(x*(x + 1))")   [together]
  ("4/((x - 2)(x + 2))", "-1/(x + 2) + 1/(x - 2)") [apart]
  ("(x + 2)/(x - 2)", "1 + 4/(x - 2)")            [apart]
All 22 fall in class 3 by the harness's own partition (both sides in the grammar, expected not prose), and `documented_reason` names none of them, so they would fail `the_two_checkers_agree_on_every_comparable_pair` if the generator existed.

Round 2 finding #16 raised this exact shape ('the 100% class-3 agreement is an artifact of generator choice'); the fix added internal_space_collapse, case_flip, significant_decimal, product_reorder, and algebraic_refactor, and none of the five reaches a rational expression.
```

**Failure scenario.** The M2 exit gate reads 'class 3 agreement 15,940 of 15,940 (100.0000%)' from `the_two_checkers_agree_on_every_comparable_pair` and closes the milestone on it. Add one generator that calls the rational-normal-form rewrite a learner performs by hand — combine two fractions over a common denominator, or split one — and the same corpus, the same oracle, and the same partition produce 22 class-3 pairs with no documented reason, on 16 authored answers across 8 topics. The parity number therefore measures which generators were written, not the parity of the checker, and the milestone closes with the V3 gap of finding #2 above unmeasured.

**Refuter.** The claim is demonstrable, and the gap is larger than the reviewer measured. I reproduced the sweep live. The 38 GENERATORS families reach no rational normal form: generate_algebraic_refactor runs only difference-of-squares factoring (terms.len()==2, perfect square on each side) and multiply-out of a trailing parenthesized sum, and generate_product_reorder refuses anything that is not one multiplicative term. Spec section 9.3 scopes "algebraic refactor" to "poly", so the generator set is complete against the spec and still never builds the shape where the two checkers disagree. I ran together/apart/cancel/factor/expand/radsimp over the 1,733 deduplicated non-prose corpus answers through the 1.0 parser: 614 distinct pairs, 565 decided on both sides, 64 of them 1.0 True / 2.0 correct=false, 0 false positives. Every one of the 64 carries shape "expression_symbolic", so classify() puts it in
