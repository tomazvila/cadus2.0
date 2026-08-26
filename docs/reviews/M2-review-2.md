# M2 adversarial review — round 2 (2026-08-27)

Run on commit e9231f7 (after FIXM2a–c). One find/refute round, major+ only: 20 raised, 17 confirmed. Assigned to FIXM2d (normalize/parse) and FIXM2e (canon + oracle harness); FIXM2f regenerates the fixtures after both merge.

## Orchestrator rulings (binding)

- Mixed numbers, complete rule: a whole number followed by a fraction in ANY of these
  spellings is the mixed number `whole + num/den`: `a b/c` (space), `a½` (glued glyph),
  `a ½` (one collapsed space or any Unicode space before the glyph), `a\frac{b}{c}` and
  `a \frac{b}{c}`. The same `0 < b < c` and plain-digit rules apply. A negative whole
  part carries the sign over the whole value (`-2 1/2` = -5/2).
- Juxtaposed function argument: the chain continues through an explicit `*` and through
  implicit multiplication; it stops at `/`, `+`, `-`, `,`, `)`, `=`, `<`, `>`. So `cos 2*x`
  = `cos(2*x)`; `sqrt 2/2` = `sqrt(2)/2` (pinned already).
- `\sqrt{a}` inserts `*` after a letter, a digit, or `)`, the same as `√`.
- A space-grouped number (`1 000`) is accepted only as the whole answer or as a full
  operand at the top level (the V4 full-match rule); after any factor it is Undecidable.
- The times-`x`/`X` reading accepts a negated numeric literal on the left (`-2.5 x 10^-4`).
- Canon: an `Atom::Exp` argument's whole-number part folds into `Atom::E` (`e^(x+2)` =
  `e^2 * e^x`); `Inverse(p)^n` is `Inverse(p^n)`; two `Inverse` atoms in one monomial
  merge into `Inverse(p*q)`; a monomial holds at most one `Inverse`.
- Oracle harness: no catch-all reason. Every class-4 pair must match a specific
  predicate with a cited 1.0 line; an unexplained divergence stays in class 3 and fails.
  Add the "internal space collapse" generator family.
- After the grammar and canon changes, regenerate the verdict fixture (live 1.0) and the
  corpus split; then one verification round; then M2 closes with any leftover recorded.

| # | Sev | File | Unit | Title |
|---|---|---|---|---|
| 1 | blocker | `crates/core/src/answer/normalize.rs:236` | FIXM2d | A `\frac` after a digit run reads as a product, so a wrong mixed number is graded correct on the topic that writes mixed numbers in that notation |
| 2 | blocker | `crates/core/src/answer/normalize.rs:339` | FIXM2d | A vulgar-fraction glyph after a space reads as a product, so `2 ½` is 1 and a learner who wrote two and a half is graded correct |
| 3 | blocker | `crates/core/src/answer/normalize.rs:339` | FIXM2d (dup of #1) | A vulgar-fraction glyph or a \frac after a whole number keeps the product reading when a space or a brace separates them, so a mixed number is graded with the wrong value |
| 4 | blocker | `crates/core/src/answer/parse.rs:584` | FIXM2d | The bracket-free function argument stops at an explicit `*`, so `cos 2*x` is `x*cos(2)`: a wrong learner answer is correct and the right one is wrong |
| 5 | blocker | `crates/core/src/answer/normalize.rs:339` | FIXM2d (dup of #2) | A vulgar-fraction glyph after a space still reads as a product, so a wrong learner answer is correct |
| 6 | blocker | `crates/core/src/answer/normalize.rs:339` | FIXM2d (dup of #2) | A vulgar-fraction glyph after a space keeps the product reading, so `3 ½` is 3/2 while `3½` and `3 1/2` are both 7/2 |
| 7 | blocker | `crates/core/src/answer/normalize.rs:339` | FIXM2d (dup of #2) | One space in front of a vulgar-fraction glyph defeats the mixed-number reading, so `2/3` accepts `2 ⅓` |
| 8 | major | `crates/core/src/answer/canon.rs:757` | FIXM2e | The exponent law of Atom::Exp does not fold Atom::E, so e^(x+2) and e^2*e^x are two values |
| 9 | major | `crates/core/src/answer/normalize.rs:244` | FIXM2d | The new \sqrt{a} tolerance inserts no product sign, so a letter in front of it glues into one name and the answer gets no verdict |
| 10 | major | `crates/core/tests/answer_oracle.rs:1325` | FIXM2e | The oracle parity harness files the five `cos 2*x` parse divergences as a transcendental identity, so the class-3 assertion never sees them |
| 11 | major | `crates/core/src/answer/parse.rs:293` | FIXM2d | A space-grouped thousands number after a non-literal factor invents a value: `x/1 000` canonicalizes to 0 |
| 12 | major | `crates/core/src/answer/parse.rs:319` | FIXM2d | `eat_times_letter` does not accept a negated literal, so `-2.5 x 10^-4` is the polynomial `-0.00025*x` |
| 13 | major | `crates/core/src/answer/canon.rs:558` | FIXM2e | A reciprocal raised to a power and a power under a reciprocal are two canonical forms of one value, so `(1/(x+1))^2` is wrong against `1/(x+1)^2` |
| 14 | major | `crates/core/tests/answer_oracle.rs:1326` | FIXM2e (dup of #10) | The class-4 reason 'a transcendental identity is not simplified' excuses five pairs that contain no identity, hiding a real divergence |
| 15 | major | `crates/core/src/answer/parse.rs:576` | FIXM2d (dup of #4) | The bracket-free function argument stops at an explicit `*`, so the learner's explicit-multiplication spelling of an authored answer is marked wrong |
| 16 | major | `crates/core/tests/answer_oracle.rs:751` | FIXM2e | The generator set omits the spec section 9.3 'internal space collapse' family, so the 100% class-3 agreement is an artifact of generator choice |
| 17 | major | `crates/core/src/answer/canon.rs:686` | FIXM2e | Two reciprocals of sums are never merged, so `4/(x-2) * 1/(x+2)` is not `4/((x-2)(x+2))` |

## FIXM2d

### #1 [blocker] A `\frac` after a digit run reads as a product, so a wrong mixed number is graded correct on the topic that writes mixed numbers in that notation

File: `crates/core/src/answer/normalize.rs:236` — IDs: C4, V4, V3, V1

**Claim.** `rewrite_latex_braces` expands `\frac{a}{b}` into `((a)/(b))` with no operator and no look-back at the text in front of it, so a digit run before the `\frac` reads as an implicit product: `2\frac{1}{2}` canonicalizes to 1 instead of 5/2, and `check("1", "2\\frac{1}{2}")` returns correct=true.

**Evidence.**

```
normalize.rs:234-242 (`rewrite_latex_braces`):
        if let Some((numerator, denominator, next)) = read_frac(chars, i) {
            out.push_str("((");
            out.push_str(&rewrite_latex_braces(numerator, depth + 1));
            out.push_str(")/(");
            out.push_str(&rewrite_latex_braces(denominator, depth + 1));
            out.push_str("))");

Probe against `cadus_core::answer` (scratchpad binary, path dependency on crates/core):
  normalize("2\\frac{1}{2}").source == "2((1)/(2))"
  ast   = Mul([Integer(2), Fraction { numerator: 1, denominator: 2 }])
  canon = Rational(1/1)

  check(expected, learner, Expression):
    "1"     vs "2\\frac{1}{2}"  => correct=true   notation=false   <-- FALSE POSITIVE
    "1"     vs "3\\frac{1}{3}"  => correct=true   notation=false
    "2"     vs "4\\frac{1}{2}"  => correct=true   notation=false   <-- FALSE POSITIVE
    "5"     vs "10\\frac{1}{2}" => correct=true   notation=false
    "3/4"   vs "1\\frac{3}{4}"  => correct=true   notation=false
    "9/4"   vs "3\\frac{3}{4}"  => correct=true   notation=false
  The mirror error is present too: the CORRECT answer in the same notation is refused.
    "4 1/2" vs "4\\frac{1}{2}"  => correct=false
    "3 3/4" vs "3\\frac{3}{4}"  => correct=false
    "2 7/12" vs "2\\frac{7}{12}" => correct=false
  The same value in the two spellings the fix covers is read right, which shows the gap is 2.0's own:
    "5/2"   vs "2 1/2"          => correct=true
    "5/2"   vs "2½"            => correct=true

1.0 oracle (scripts/oracle/check_1_0.py with /home/deploy/dev/cadus/.venv/bin/python):
  {"expected":"1","learner":"2\\frac{1}{2}"}  -> {"equivalent": false}
  {"expected":"5/2","learner":"2\\frac{1}{2}"} -> {"equivalent": false}
  {"expected":"9/4","learner":"3\\frac{3}{4}"} -> {"equivalent": false}
1.0 deleted the backslash and could not parse the string, so it decided false. `\frac{a}{b}` is a 2.0 addition (docs/plans/M2.md, V4 row), so the wrong-correct verdict is a 2.0 regression, not carried behavior.

The curriculum authors this notation in the problem text of the very topic:
  curriculum/foundations/01-fractions-decimals.yaml:611-613
    - problem: 'Compute $1\frac{1}{2} \times \frac{2}{3}$.'
      answer: "1"
  curriculum/foundations/01-fractions-decimals.yaml:655-656 (dividing-mixed-numbers kp2)
    - problem: 'Compute $3\frac{1}{2} \div 1\frac{3}{4}$.'
      answer: "2"

No ruling covers it: crates/core/tests/answer_divergence.rs:182 pins only the bare `("1/2", "\\frac
```

**Failure scenario.** A learner works the item `mixed-numbers` kp3 exemplar 0, whose problem reads `Compute $1\frac{1}{2} \times \frac{2}{3}$` and whose authored answer is `1`. The learner multiplies wrongly, gets two and one half, and types the answer in the notation the problem itself uses: `2\frac{1}{2}`. `normalize` produces the source `2((1)/(2))`, the parser reads an implicit product, `canon` gives Rational(1), and `check` returns `Verdict { correct: true, notation: false }`. The learner is 1.5 out and the item is recorded as correct, with the XP and FIRe credit that follows. A3 then keeps the answer away from the model, so nothing corrects the record. On the same knowledge point, exemplar 1 (authored `4 1/2`) marks the RIGHT answer `4\frac{1}{2}` wrong. 1.0 decided both pairs false.

**Refuter.** The claim is demonstrable in full. I reproduced every listed result against the real crate, ran the 1.0 oracle, and read the rulings.

1. The mechanism is as stated. `rewrite_latex_braces` at /home/deploy/dev/cadus2.0/crates/core/src/answer/normalize.rs:236 emits `((num)/(den))` with no look-back at the character in front of `\frac`. The parser then reads the digit run before it as an implicit product. The same file does the opposite for the vulgar glyph: `unicode_math_to_ascii` (normalize.rs:337-343) tests `out.chars().last().is_ascii_digit()` and emits the mixed-number form. The `\frac` path has no such test.

2. The false positive is real, in both answer kinds. `check("1", "2\\frac{1}{2}", Expression)` and the same pair with `Numeric` both return `Decided(Verdict { correct: true, notation: false })`. All six false-positive pairs in the claim reproduce exactly.

3. The mirror error is 

### #2 [blocker] A vulgar-fraction glyph after a space reads as a product, so `2 ½` is 1 and a learner who wrote two and a half is graded correct

File: `crates/core/src/answer/normalize.rs:339` — IDs: C4, V4, V1

**Claim.** The mixed-number reading of a vulgar glyph fires only when the immediately preceding character is an ASCII digit, but `collapse_whitespace` leaves one space between the whole part and the glyph, so `2 ½`, `2\u{a0}½`, and `2\u{2009}½` all keep the product reading and canonicalize to 1 instead of 5/2; `check("1", "2 ½")` returns correct=true.

**Evidence.**

```
normalize.rs:336-344 (`unicode_math_to_ascii`):
        if let Some((_, alone, after_digits)) =
            VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
        {
            if matches!(out.chars().last(), Some(last) if last.is_ascii_digit()) {
                out.push_str(after_digits);
            } else {
                out.push_str(alone);
            }
The test reads the last character of `out`. `collapse_whitespace` (normalize.rs:191) turns every space run into one ASCII space and never deletes it, so a space between the whole part and the glyph sends the glyph down the `alone` branch.

Probe against `cadus_core::answer`:
  normalize("2 ½").source        == "2 (1/2)"   canon = Rational(1/1)
  normalize("2\u{a0}½").source   == "2 (1/2)"   canon = Rational(1/1)   (NBSP)
  normalize("2\u{2009}½").source == "2 (1/2)"   canon = Rational(1/1)   (thin space)
  normalize("2½").source         == "2 1/2"     canon = Rational(5/2)   (the covered spelling)

  check(expected, learner, Expression):
    "1"     vs "2 ½"  => correct=true  notation=false   <-- FALSE POSITIVE
    "2"     vs "4 ½"  => correct=true  notation=false   <-- FALSE POSITIVE
    "3/4"   vs "1 ¾"  => correct=true  notation=false   <-- FALSE POSITIVE
    "1/2"   vs "1 ½"  => correct=true  notation=false
    "9/4"   vs "3 ¾"  => correct=true  notation=false
  And the right answer in the same spelling is refused:
    "5/2"   vs "2 ½"  => correct=false
    "7/4"   vs "1 ¾"  => correct=false
    "4 1/2" vs "4 ½"  => correct=false

The three plain-space, NBSP, and thin-space forms are the typographic forms of a mixed number, and NBSP and thin space are already in the 1.0 `_SPACE_SEPARATORS` table (normalize.rs:35), so the module knows they group a number.

A generated sweep of 2,128 pairs that 2.0 calls equal, scored against the live 1.0 oracle, groups this defect with the `\frac` one: `2 ½`, `2\frac{1}{2}`, `3 ⅓`, `3\frac{1}{3}`, `1 x 1`, `2/2`, `sqrt(1)`, and `x/x` all land in the canonical class Rational(1).

No ruling covers it. crates/core/tests/answer_divergence.rs:291-297 pins the GLUED glyph only (`2/3` vs `2⅓` false, `7/3` vs `2⅓` true). docs/reviews/M2-review-1.md words the ruling as "a digit run followed by a vulgar-fraction glyph" and does not say the space kills the reading. The whole M2 suite passes with the defect present.
```

**Failure scenario.** A learner works the item `mixed-numbers` kp3 exemplar 0 (`curriculum/foundations/01-fractions-decimals.yaml:611`), authored answer `1`. The learner answers two and a half and types `2 ½`, which is the ordinary typographic form and the form a paste from a rendered document produces. `normalize` gives the source `2 (1/2)`, the parser builds `Mul([Integer(2), Fraction { 1, 2 }])`, `canon` gives Rational(1), and `check` returns `Verdict { correct: true, notation: false }`. A learner who is wrong by a factor of 2.5 is recorded as correct. The same item marks the correct `4 ½` wrong on exemplar 1 (authored `4 1/2`), and the glued form `4½` of the same string is graded correct, so one value in three spellings gets three verdicts.

**Refuter.** The claim reproduces exactly, and no ruling, parity argument, or guard excuses it. The mixed-number reading in `unicode_math_to_ascii` (crates/core/src/answer/normalize.rs:339) tests only `out.chars().last()` for an ASCII digit. `to_source` (normalize.rs:100) calls `collapse_whitespace` first, and that function maps every whitespace run to one ASCII space and never deletes it. Therefore the last character before the glyph is a space, the glyph takes the `alone` branch, and `2 ½` becomes the source `2 (1/2)`, which canonicalizes to Rational(1) instead of 5/2. The result is a C4 false positive: `check("1", "2 ½", Expression)` gives `correct = true` for a learner answer of two and a half.

The V3 parity defense fails on the same spelling. The live 1.0 oracle gives True for ("4 1/2", "4 ½"), and 2.0 gives false, so 2.0 already diverges from 1.0 on the spaced form, and the divergence is not r

### #3 [blocker] A vulgar-fraction glyph or a \frac after a whole number keeps the product reading when a space or a brace separates them, so a mixed number is graded with the wrong value

File: `crates/core/src/answer/normalize.rs:339` — IDs: C4, V4, V1, A3 — duplicate of #1

**Claim.** The round-1 mixed-number fix reads the glyph as the fractional part only when the glyph touches the digit run, so `2 ½` (one collapsed space) and `2\frac{1}{2}` still become the product `2*(1/2)` = 1 while `2½` and `2 1/2` become 5/2, and the checker decides the wrong value as correct.

**Evidence.**

```
normalize.rs:336-345 (`unicode_math_to_ascii`):
        if let Some((_, alone, after_digits)) =
            VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
        {
            if matches!(out.chars().last(), Some(last) if last.is_ascii_digit()) {
                out.push_str(after_digits);
            } else {
                out.push_str(alone);
            }

The guard reads the last character of `out`. `collapse_whitespace` leaves one ASCII space there, so the mixed-number branch never runs on a spaced glyph. The `\frac` branch of `rewrite_latex_braces` (normalize.rs:229-242) has no mixed-number reading at all.

Probe against `cadus_core::answer::check` and `canonical_form` (release build, kind = expression):
  canon "2 ½"           src="2 (1/2)"     => Rational(1/1)
  canon "2\\frac{1}{2}"  src="2((1)/(2))"  => Rational(1/1)
  canon "2½"            src="2 1/2"       => Rational(5/2)
  canon "2 1/2"         src="2 1/2"       => Rational(5/2)

  "3"     vs "6 ½"            => correct=true   notation=false
  "3"     vs "6\\frac{1}{2}"   => correct=true   notation=false
  "1"     vs "2 ½"            => correct=true   notation=false
  "1"     vs "2\\frac{1}{2}"   => correct=true   notation=false
  "2 1/2" vs "2 ½"            => correct=false  notation=false
  "3 1/2" vs "3 ½"            => correct=false  notation=false
  "3 3/4" vs "3 ¾"            => correct=false  notation=false

The 1.0 oracle (scripts/oracle/check_1_0.py) reads both spellings the same way, so the disagreement between the two spellings is 2.0's own:
  expected "2 1/2", learner "2 ½"  -> {"equivalent": true}
  expected "1",     learner "2 ½"  -> {"equivalent": true}

The corpus authors mixed numbers and plain values on the same two topics:
  corpus_1_0.jsonl: mixed-numbers kp2 ex1 '2 1/2', mixed-numbers kp3 ex0 '1',
  improper-fractions-mixed-numbers kp1 ex0 '3 1/2', kp3 ex1 '3'.

No test covers the spaced form. `crates/core/tests/answer_parse.rs:201-215` pins only the glued forms (`3½`, `2⅓`, `5¾`, `x½`, `(2)½`), and `answer_divergence.rs:296-297` pins only `2⅓`.
```

**Failure scenario.** Topic `mixed-numbers`, kp3 exemplar 0, authored answer `1`. A learner answers two and a half and types `2 ½` (or `2\frac{1}{2}`, the LaTeX mixed number). `normalize` builds the source `2 (1/2)`, `canon` gives 1, and `check` returns `Verdict { correct: true, notation: false }`. The learner is wrong by a factor of 2.5 and the checker records the answer as correct; A3 makes that verdict final, so the model never sees it. The mirror error runs on the same topic: `mixed-numbers` kp2 exemplar 1 authors `2 1/2`, and a learner who types the right answer as `2 ½` gets `correct = false`.

**Refuter.** The claim is demonstrable, and no ruling covers it. I tried to refute it on four routes and each route failed.

1. The code path. In `to_source` (`/home/deploy/dev/cadus2.0/crates/core/src/answer/normalize.rs:96-121`), `collapse_whitespace` runs before `unicode_math_to_ascii`, so one ASCII space stands between the digit run and the glyph. The mixed-number guard at line 339 tests `out.chars().last()` for an ASCII digit. A space fails that test, so the branch selects the `alone` reading `(1/2)`. The `\frac` branch of `rewrite_latex_braces` (lines 233-241) emits `((a)/(b))` and reads no whole part at all.

2. The measured behavior. A release probe against `cadus_core::answer` reproduced every line of the evidence, and two more: `3 ½` is 3/2, and `3 ¾` is 9/4. A non-breaking space and a thin space give the same wrong values, because `collapse_whitespace` maps them to the same ASCII space.

3

### #4 [blocker] The bracket-free function argument stops at an explicit `*`, so `cos 2*x` is `x*cos(2)`: a wrong learner answer is correct and the right one is wrong

File: `crates/core/src/answer/parse.rs:584` — IDs: C4, V3, A3, V1

**Claim.** `parse_juxtaposed_argument` breaks the chain at `Tok::Star`, so the FIXM2a fix of review-1 findings #3/#4/#6 covers `cos 2x` but not `cos 2*x`; the checker returns `correct = true` for the meaningless `x*cos(2)` and `correct = false` for the correct `cos(2*x)`, and it marks the natural learner spelling of five authored corpus answers wrong where 1.0 marked it right.

**Evidence.**

```
Release probe against `cadus_core::answer::check` (kind = Expression):
  "cos 2*x"  vs "x*cos(2)"  -> Decided(Verdict { correct: true,  notation: false })
  "cos 2*x"  vs "cos(2*x)" -> Decided(Verdict { correct: false, notation: false })
  "cos 2x"   vs "cos 2*x"  -> Decided(Verdict { correct: false, notation: false })
  canonical_form("cos 2*x") = Ok(Poly({{Var("x"): 1, Call("cos", [Rational(2/1)]): 1}: 1/1}))
  canonical_form("cos 2x")  = Ok(Func("cos", [Poly({{Var("x"): 1}: 2/1})]))

Live 1.0 oracle (/home/deploy/dev/cadus/.venv/bin/python scripts/oracle/check_1_0.py):
  {"equivalent": true,  "id": "4"}  expected "cos 2x",  learner "cos 2*x"
  {"equivalent": false, "id": "5"}  expected "cos 2*x", learner "x*cos(2)"

The committed 1.0 verdict records the same:
  crates/core/tests/fixtures/answers/oracle_verdicts_1_0.jsonl:5788
  {"equivalent": true, "expected": "cos 2x", "kind": "expression", "learner": "cos 2*x", ...}

All five authored corpus answers of the shape fail the same way (2.0 = false, 1.0 = true):
  "cos 2x" vs "cos 2*x"; "$\\cos 2t$" vs "cos 2*t"; "$(4/3)\\sin 3t$" vs "(4/3)*sin 3*t";
  "$2\\cos 2t + (5/2)\\sin 2t$" vs "2*cos 2*t + (5/2)*sin 2*t"; "$\\cos 3t + 2\\sin 3t$" vs "cos 3*t + 2*sin 3*t"

parse.rs:582-586
                if !parser.starts_operand() {
                    break;
                }
`starts_operand` is `Num | Ident | LParen`, so a `Tok::Star` ends the argument.

No test in crates/core/tests/answer_divergence.rs pins the pair, and docs/reference/checker-1.0-spec.md names no such divergence.
```

**Failure scenario.** A learner on `double-half-angle-identities` kp2 exemplar 1 (authored answer `cos 2x`) types the correct `cos 2*x`. `check("cos 2x", "cos 2*x", Expression)` returns `Decided{correct:false}`; 1.0 returned True on the same pair. Under A3 the false verdict is final, so the learner loses XP and FIRe for a correct answer and the model never sees it. The mirror is a C4 false positive: with the authored answer `cos 2*x`, `check("cos 2*x", "x*cos(2)", Expression)` returns `Decided{correct:true}` for an answer of a different value, while 1.0 returns False.

**Refuter.** The claim is demonstrable, and no ruling covers it. `parse_juxtaposed_argument` (/home/deploy/dev/cadus2.0/crates/core/src/answer/parse.rs:576-594) ends the bracket-free argument at `Tok::Star`, because `starts_operand` accepts only `Num | Ident | LParen`. The 1.0 checker does the opposite: SymPy implicit multiplication keeps the `*` inside the argument. Measured on the live 1.0 code, `_parse("cos 2*x")` gives `cos(2*x)`, and `_parse("cos 2x")` gives `cos(2*x)` as well. 2.0 gives `cos(2)*x` for the first form only.

Three consequences follow, and I reproduced all three.

1. V3 and A3 false negative on real content. Five authored corpus answers of the shape `f n t` reject the natural learner spelling `f n*t`. The live 1.0 oracle returns True on all five pairs; 2.0 returns `Decided{correct:false}`. The verdict is final under A3, so the learner loses XP and FIRe for a correct answer.
2. V1 

### #5 [blocker] A vulgar-fraction glyph after a space still reads as a product, so a wrong learner answer is correct

File: `crates/core/src/answer/normalize.rs:339` — IDs: C4, V4, V1 — duplicate of #2

**Claim.** The FIXM2a mixed-number rewrite looks only at the character immediately in front of the glyph, so `2 ⅓` (whole number, space, glyph) keeps the 1.0 product reading 2/3 while `2⅓` and the ASCII `2 1/3` both give 7/3, and `check("2/3", "2 ⅓")` returns correct=true.

**Evidence.**

```
normalize.rs:337-343
        if let Some((_, alone, after_digits)) =
            VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
        {
            if matches!(out.chars().last(), Some(last) if last.is_ascii_digit()) {
                out.push_str(after_digits);
            } else {
                out.push_str(alone);

Probe (release binary linking cadus_core by path):
SHOW "2 ⅓"   src="2 (1/3)" canon=Ok(Rational(Ratio { numer: 2, denom: 3 }))
SHOW "2⅓"    src="2 1/3"   canon=Ok(Rational(Ratio { numer: 7, denom: 3 }))
SHOW "2 1/3"  src="2 1/3"   canon=Ok(Rational(Ratio { numer: 7, denom: 3 }))
SHOW "2  1/3" src="2 1/3"   canon=Ok(Rational(Ratio { numer: 7, denom: 3 }))

"2/3"   vs "2 ⅓" => correct=true  notation=false   <-- C4 false positive
"7/3"   vs "2 ⅓" => correct=false notation=false   <-- the right answer is marked wrong
"3/2"   vs "3 ½" => correct=true  notation=false
"7/2"   vs "3 ½" => correct=false notation=false
"3 1/2" vs "3 ½" => correct=false notation=false   <-- two spellings of one mixed number disagree
"3 1/2" vs "3½"  => correct=true  notation=false

crates/core/tests/answer_divergence.rs:299-301 pins only the glued spelling:
    assert_eq!(check("2/3", "2⅓", N), decided(false, false));
    assert_eq!(check("7/3", "2⅓", N), decided(true, false));
```

**Failure scenario.** A fractions topic authors the answer `2/3`. A learner who believes the answer is two and one third types the whole number, a space, then the house glyph: `2 ⅓`. `unicode_math_to_ascii` sees a space in `out.chars().last()`, takes the `alone` reading, and emits the source `2 (1/3)`; the parser reads an implicit product and `canon` gives 2/3, so `check` returns `Verdict { correct: true, notation: false }`. A learner wrong by a factor of 3.5 is recorded as correct — the exact C4 false positive that confirmed round-1 blocker #1 was fixed for, left open for the spaced spelling. The mirror error is present too: the learner who correctly writes `2 ⅓` against an authored `7/3` is marked wrong.

**Refuter.** The claim is demonstrable and no ruling excuses it. I built a probe binary against `cadus_core` and got the reported results, character for character. `to_source` collapses whitespace but keeps it (normalize.rs:99-101), so `2 ⅓` reaches `unicode_math_to_ascii` with the space intact. The mixed-number test at normalize.rs:340 is `matches!(out.chars().last(), Some(last) if last.is_ascii_digit())`. The last character is a space, so the test fails, the `alone` reading `(1/3)` wins, and the source is `2 (1/3)`. The parser reads an implicit product and `canon` gives 2/3. `check("2/3", "2 ⅓", Numeric)` returns `Decided(Verdict { correct: true, notation: false })` — a C4 false positive, wrong by a factor of 3.5. The mirror error is present: `check("7/3", "2 ⅓", N)` is false, so the learner who writes the correct value in the house glyph style is marked wrong.

The behavior is not one of the fixed

### #6 [blocker] A vulgar-fraction glyph after a space keeps the product reading, so `3 ½` is 3/2 while `3½` and `3 1/2` are both 7/2

File: `crates/core/src/answer/normalize.rs:339` — IDs: C4, V4, V1, A3 — duplicate of #2

**Claim.** The FIXM2a mixed-number fix tests only `out.chars().last()` for an ASCII digit, so it fires on a glyph glued to the whole number and never on a glyph the learner separates with a space; the ASCII mixed number `a b/c` requires that same space, so the two spellings of one mixed number get opposite readings and a learner who writes three and a half is graded correct against 3/2.

**Evidence.**

```
crates/core/src/answer/normalize.rs:335-345 (`unicode_math_to_ascii`)
        if let Some((_, alone, after_digits)) =
            VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
        {
            if matches!(out.chars().last(), Some(last) if last.is_ascii_digit()) {
                out.push_str(after_digits);
            } else {
                out.push_str(alone);

Probe against `cadus_core::answer` (debug build, kind = numeric):
  --- "3 1/2"  source="3 1/2"    canon=Rational(7/2)
  --- "3½"     source="3 1/2"    canon=Rational(7/2)
  --- "3 ½"    source="3 (1/2)"  canon=Rational(3/2)   <-- the product reading

C4 false positives (all `correct=true`):
  "3/2" vs "3 ½"          => correct=true notation=false
  "3/2" vs "3\u{a0}½"     => correct=true notation=false   (no-break space)
  "3/2" vs "3\u{2009}½"   => correct=true notation=false   (thin space)
  "2/3" vs "2 ⅓"          => correct=true notation=false
  "1"   vs "2 ½"          => correct=true notation=false
The mirror false negative, from the same rule:
  "7/2" vs "3 ½"   => correct=false     while
  "7/2" vs "3½"    => correct=true  and "7/2" vs "3 1/2" => correct=true

The glued case is pinned and the spaced case is not: crates/core/tests/answer_parse.rs:200-207 pins `½`, `3½`, `2⅓`, `5¾`, `x½`, `(2)½` and no spaced form; crates/core/tests/answer_divergence.rs pins `2⅓` only.

The authored answer is real: crates/core/tests/fixtures/answers/corpus_1_0.jsonl:40 is {"answer": "3/2", "topic_id": "adding-subtracting-like-fractions", "kp_id": "kp3", "answer_kind": "expression"}, and the house mixed-number style writes the space (corpus_1_0.jsonl:1546 "3 1/2", :2012 "3 3/4", :2015 "2 1/2").
```

**Failure scenario.** A learner on `adding-subtracting-like-fractions` kp3 (authored answer `3/2`) believes the answer is three and a half and types `3 ½` — the house Unicode style, with the same space the house ASCII style `3 1/2` requires. `normalize` produces the source `3 (1/2)`, the parser reads an implicit product, `canon` gives 3/2, and `check` returns Verdict { correct: true, notation: false }. A learner who is wrong by a factor of 7/3 is recorded as correct, which is the C4 blocker of review-1 findings #1 and #9 left open in the spaced half of the same rule. The mirror case hurts too: the same learner on a topic that authors `7/2` types `3 ½` and is graded wrong, while `3½` and `3 1/2` are graded right.

**Refuter.** The defect is demonstrable on a real build and is not covered by any ruling. `unicode_math_to_ascii` (crates/core/src/answer/normalize.rs:339) selects the mixed-number reading only when `out.chars().last()` is an ASCII digit. `collapse_whitespace` runs earlier in `to_source`, so NBSP and thin space are already plain spaces; for `3 ½` the last char is a space, the branch takes the `alone` string `(1/2)`, and the source becomes `3 (1/2)`, which the parser reads as an implicit product with value 3/2. The glued `3½` and the ASCII `3 1/2` both give 7/2. crates/core/src/answer/lexer.rs:54 states the mixed-number production `a b/c` is the one place the grammar reads a space, so ASCII requires the space that the Unicode path forbids: one mixed number, two spellings, opposite values. Confirmed C4 false positives (correct=true): `3/2` vs `3 ½`, `3/2` vs `3\u{a0}½`, `3/2` vs `3\u{2009}½`, `2/3` vs 

### #7 [blocker] One space in front of a vulgar-fraction glyph defeats the mixed-number reading, so `2/3` accepts `2 ⅓`

File: `crates/core/src/answer/normalize.rs:339` — IDs: C4, V4 — duplicate of #2

**Claim.** The FIXM2a mixed-number rule fires only when the glyph touches a digit in the already-emitted output, and `collapse_whitespace` leaves one ASCII space between `2` and `⅓`, so `2 ⅓` reads as the product `2*(1/3)` = 2/3 while `2⅓` reads as 7/3 — the exact false verdict the pinned divergence test forbids.

**Evidence.**

```
normalize.rs:335-345:

    if let Some((_, alone, after_digits)) =
        VULGAR_FRACTIONS.iter().find(|(from, _, _)| *from == c)
    {
        if matches!(out.chars().last(), Some(last) if last.is_ascii_digit()) {
            out.push_str(after_digits);
        } else {
            out.push_str(alone);
        }

Measured:
  SRC "2⅓"  -> src="2 1/3"   parse: Ok(Mixed { whole: 2, numerator: 1, denominator: 3 })  canon: 7/3
  SRC "2 ⅓" -> src="2 (1/3)" parse: Ok(Mul([Integer(2), Fraction { numerator: 1, denominator: 3 }])) canon: 2/3

  "2/3"  vs "2 ⅓"          Decided(Verdict { correct: true,  notation: false })
  "2/3"  vs "2\u{a0}⅓"     Decided(Verdict { correct: true,  notation: false })
  "7/3"  vs "2 ⅓"          Decided(Verdict { correct: false, notation: false })
  "3/2"  vs "1 ½"          Decided(Verdict { correct: false, notation: false })
  "5⅓"   vs "5 ⅓"          Decided(Verdict { correct: false, notation: false })

crates/core/tests/answer_divergence.rs:296 pins the opposite for the unspaced form:
    assert_eq!(check("2/3", "2⅓", N), decided(false, false));
```

**Failure scenario.** A topic authors the fraction `2/3`. A learner pastes `2 ⅓` (or `2\u{00a0}⅓`, the non-breaking space a rendered page produces; `collapse_whitespace` turns both into one ASCII space). `check("2/3", "2 ⅓", Numeric)` returns `Decided{correct:true}` — two and one third is recorded as correct for two thirds. The mirror case is a mixed-number topic authoring `3/2`: the learner types `1 ½` and `check("3/2", "1 ½", Numeric)` returns `Decided{correct:false}`. The same learner answer flips its value on one space: `check("5⅓", "5 ⅓")` is `false`.

**Refuter.** The claim is demonstrable and it is a C4 false positive, not a ruling.

1. The mechanism is as stated. `to_source` (/home/deploy/dev/cadus2.0/crates/core/src/answer/normalize.rs:100) runs `collapse_whitespace` before `unicode_math_to_ascii` (line 119). `collapse_whitespace` (line 191) turns an ASCII space and U+00A0 alike into one ASCII space, because `char::is_whitespace` holds for U+00A0. The FIXM2a test at line 339 then reads `out.chars().last()`, which is that space, not a digit, so the glyph takes the `alone` string `(1/3)` and the mixed-number reading never fires.

2. The result is a wrong `correct=true` verdict. `check("2/3", "2 ⅓", Numeric)` and `check("2/3", "2\u{a0}⅓", Numeric)` both give `Decided{correct:true, notation:false}`. Two and one third is recorded as correct for two thirds. `check("1/2", "1 ½", Numeric)` gives the same wrong `true`. C4 makes a false positive a blocke

### #9 [major] The new \sqrt{a} tolerance inserts no product sign, so a letter in front of it glues into one name and the answer gets no verdict

File: `crates/core/src/answer/normalize.rs:244` — IDs: V4, V2

**Claim.** `rewrite_latex_braces` writes `sqrt(` straight into the output, while the `√` path at normalize.rs:369-371 first pushes a `*` when a letter, a digit, or a `)` touches the glyph. A letter in front of `\sqrt{…}` therefore glues into the identifier `xsqrt`, which no production reads, so `5x\sqrt{2}` is Undecidable while the same value written `5x√2` is decided correct.

**Evidence.**

```
normalize.rs:244-249 (the `\sqrt` branch of `rewrite_latex_braces`):
        if let Some((body, next)) = read_braced_after(chars, i, "\\sqrt") {
            out.push_str("sqrt(");
            out.push_str(&rewrite_latex_braces(body, depth + 1));
            out.push(')');
            i = next;
            continue;
        }
normalize.rs:369-371 (`rewrite_roots`, the `√` path) does the opposite:
        if matches!(out.chars().last(), Some(c) if c.is_alphanumeric() || c == ')') {
            out.push('*');
        }

Probe against `cadus_core::answer::normalize` and `check` (release build, kind = expression):
  normalize("5x√2").source        = "5x*sqrt(2)"
  normalize("5x\\sqrt{2}").source  = "5xsqrt(2)"
  normalize("x√3").source         = "x*sqrt(3)"
  normalize("x\\sqrt{3}").source   = "xsqrt(3)"

  "5x*sqrt(2)"  vs "5x√2"          => correct=true notation=false
  "5x*sqrt(2)"  vs "5x\\sqrt{2}"    => UNDEC(a name that is not a function or variable)
  "5x*sqrt(2)"  vs "5x\\sqrt 2"     => UNDEC(a name that is not a function or variable)
  "3x*sqrt(2x)" vs "3x√(2x)"       => correct=true notation=false
  "3x*sqrt(2x)" vs "3x\\sqrt{2x}"   => UNDEC(a name that is not a function or variable)
  "x*sqrt(3)"   vs "x\\sqrt{3}"     => UNDEC(a name that is not a function or variable)
A digit or a `)` in front stays decidable, because the lexer already separates them:
  "2*sqrt(3)" vs "2\\sqrt{3}" => correct=true notation=false

A corpus sweep over the 1,468 in-grammar authored answers, with every explicit `*` deleted, gives exactly three refusals, and all three are this shape:
  '3x*sqrt(2x)' -> '3xsqrt(2x)'   UNDEC
  '5x*sqrt(2)'  -> '5xsqrt(2)'    UNDEC
  '6x*sqrt(2x)' -> '6xsqrt(2x)'   UNDEC

`docs/plans/M2.md` lists `\sqrt{a}` as a V4 addition of 2.0, and normalize.rs:11-13 documents the product sign for a digit and a `)` only.
```

**Failure scenario.** A surds topic authors `5x*sqrt(2)` (the corpus carries `5x*sqrt(2)`, `3x*sqrt(2x)`, and `6x*sqrt(2x)`). A learner writes the same value in the LaTeX the V4 table exists to accept, `5x\sqrt{2}`. `normalize` builds the source `5xsqrt(2)`, the lexer reads one identifier `xsqrt`, and `check` returns `Outcome::Undecidable("a name that is not a function or variable")`. The correct answer gets no deterministic verdict and falls to the model, while the same learner writing `5x√2` is graded correct.

**Refuter.** The claim is demonstrable and no ruling covers it. `rewrite_latex_braces` (normalize.rs:244-250) writes `sqrt(` with no product sign. `rewrite_roots` (normalize.rs:369-371) pushes a `*` when the character before the glyph is alphanumeric or `)`. A letter in front of `\sqrt{...}` therefore joins the identifier `xsqrt`, and `parse` refuses it. The two paths give two different verdicts for one value. I built the crate and ran a probe. The output matches the reviewer's numbers exactly. A digit or a `)` in front stays decidable only because the lexer splits a digit run from a letter run, so the LaTeX path passes that case by accident, not by rule. The corpus holds the three authored answers the reviewer names, so the case is reachable from real content. I looked for a ruling that covers it and found none: docs/plans/M2.md:29 lists `\sqrt{a}` as a plain V4 addition of 2.0, docs/reference/check

### #11 [major] A space-grouped thousands number after a non-literal factor invents a value: `x/1 000` canonicalizes to 0

File: `crates/core/src/answer/parse.rs:293` — IDs: C4, V1, V4, A3

**Claim.** `check_implicit_number` refuses a spaced number only when the factor in front of it is `Integer`, `Decimal`, `Fraction`, or `Mixed`; when that factor is a `Div`, a `Var`, or a function call, the un-stripped second group of a space-grouped thousands number becomes a separate factor, so `x/1 000` reads as `x/1 * 0` and canonicalizes to 0, and `x/2 500` reads as `x/2 * 500`.

**Evidence.**

```
Release probe against `cadus_core::answer::check` and `parse`:
  parse("x/1 000")   = Ok(Mul([Div(Var("x"), Integer(1)), Integer(0)]))
  canonical_form("x/1 000") = Ok(Rational(0/1))
  check("0", "x/1 000",   Expression) -> Decided(Verdict { correct: true, notation: false })
  check("0", "2x/1 000",  Expression) -> Decided(Verdict { correct: true, notation: false })
  check("0", "pi/1 000",  Expression) -> Decided(Verdict { correct: true, notation: false })
  check("0", "sin x/1 000", Expression) -> Decided(Verdict { correct: true, notation: false })
  check("250*x", "x/2 500", Expression) -> Decided(Verdict { correct: true, notation: false })
  check("x/1000", "x/1 000", Expression) -> Decided(Verdict { correct: false, notation: false })
The guard fires only for a literal in front:
  parse("x*1 000") = Err(Undecidable { reason: "two numbers stand side by side" })
  parse("2/(1 000)") = Err(Undecidable { reason: "two numbers stand side by side" })

parse.rs:285-296
        if previous_is_literal {
            return Err(Undecidable::new("two numbers stand side by side"));
        }
        let spaced = self.tokens.get(self.at).is_some_and(|t| t.space_before);
        if spaced {
            Ok(())

`is_plain_digit_run` (parse.rs:663) already refuses a leading-zero run, but `read_mixed_number` is its only caller; `parse_number` reads "000" as `Integer(0)`.

docs/plans/M2.md, review round 1: a space-grouped numerator is "undecidable and never become[s] a value the checker invented (finding #7)". The rule holds for `1 000/3` and fails here.
```

**Failure scenario.** A topic authors the answer `0` (a limit, a derivative at a point, or a solved equation). A learner types `2x/1 000`, or types `x/1 000` on a rate topic. `parse` splits the space group, `canon` returns `Rational(0)`, and `check` returns `Verdict { correct: true, notation: false }` for an answer whose value is not 0. The mirror is the false negative: a learner who writes the correct `x/1 000` against the authored `x/1000` gets `correct = false`, and A3 makes that verdict final.

**Refuter.** The claim reproduces exactly, and no ruling covers it. I built a probe crate outside the repo against `cadus_core` (release) and got the reported results, character for character. The mechanism is the one the reviewer names. `Parser::check_implicit_number` (crates/core/src/answer/parse.rs:285-301) refuses a spaced number only when the factor in front is `Integer`, `Decimal`, `Fraction`, or `Mixed`. After a `Div`, a `Var`, a `Const`, or a `Func`, the `spaced` branch at parse.rs:293-295 returns `Ok(())`, so `parse_term` pushes the second group of a space-grouped thousands number as its own factor. `normalize::strip_thousands_groups` (crates/core/src/answer/normalize.rs:500-508) deletes a space group on a full match of the whole string only, so `x/1 000` never reaches the parser as `x/1000`. `is_plain_digit_run` (parse.rs:663) refuses a leading-zero run, but `read_mixed_number` is its only 

### #12 [major] `eat_times_letter` does not accept a negated literal, so `-2.5 x 10^-4` is the polynomial `-0.00025*x`

File: `crates/core/src/answer/parse.rs:319` — IDs: C4, V4, A3

**Claim.** The times-`x` reading of FIXM2a (review finding #18) matches `previous` against `Ast::Integer | Decimal | Fraction | Mixed` only, and `parse_unary` wraps a leading minus in `Ast::Neg`, so a negative scientific-notation answer keeps the letter as a variable: `3 x 10^5` is 300000 but `-3 x 10^5` is `-300000*x`.

**Evidence.**

```
Release probe against `cadus_core::answer::check` and `parse`:
  parse("-3 x 10^5") = Ok(Mul([Neg(Integer(3)), Var("x"), Pow(Integer(10), 5)]))
  canonical_form("-3 x 10^5") = Ok(Poly({{Var("x"): 1}: -300000/1}))
  check("300000",  "3 x 10^5",     Numeric) -> Decided(Verdict { correct: true,  notation: false })
  check("-300000", "-3 x 10^5",    Numeric) -> Decided(Verdict { correct: false, notation: false })
  check("-0.00025","-2.5 x 10^-4", Numeric) -> Decided(Verdict { correct: false, notation: false })
  check("-300000", "-3 × 10^5", Numeric) -> Decided(Verdict { correct: true, notation: false })
  check("-1617x",  "-3 x 539",     Expression) -> Decided(Verdict { correct: true, notation: false })

parse.rs:316-322
    fn eat_times_letter(&mut self, previous: Option<&Ast>) -> bool {
        if !matches!(
            previous,
            Some(Ast::Integer(_) | Ast::Decimal { .. } | Ast::Fraction { .. } | Ast::Mixed { .. })
        ) {
            return false;

`read_mixed_number` (parse.rs:~380) reads the same position through `whole_number`, which unwraps `Ast::Neg`, so `-2 1/2` is -5/2. The two productions disagree on the same shape.

22 of the 27 authored times-`x` corpus rows are scientific notation (`3 x 10^-2`, `2.5 x 10^-4`, `7.2 x 10^-4`, ...), so the reading exists for exactly this class of answer.
```

**Failure scenario.** A `scientific-notation` topic authors `-0.00025`. A learner writes the value in the house times-`x` style the ruling exists to accept, `-2.5 x 10^-4`. `parse` keeps the `x` as a variable, `canon` gives `-0.00025*x`, and `check` returns `Decided{correct:false}`. A3 makes the verdict final, so a correct answer is recorded as a failed attempt. The mirror is a C4 false positive: `check("-1617x", "-3 x 539", Expression)` returns `correct = true`, so the polynomial `-1617x` accepts a learner who meant the number -1617.

**Refuter.** The claim is demonstrable and I cannot refute it. Both halves reproduce exactly: `3 x 10^5` canonicalizes to 300000, and `-3 x 10^5` canonicalizes to -300000*x, because `eat_times_letter` matches `previous` against bare literal variants while `parse_unary` puts a leading minus in `Ast::Neg`. `check("-0.00025", "-2.5 x 10^-4", Numeric)` returns `Decided{correct:false}`, and A3 makes that verdict final, so a learner who writes a correct value in the house times-`x` style of the FIXM2a ruling records a failed attempt (V4). The defect is a true sign gap, not a ruling complaint: the M2.md times-`x` ruling states the reading for a spaced `x` between numeric literals and says nothing about sign, and the same tree already accepts the negative form through three other spellings of the identical value (`-2.5 × 10^-4`, `-(2.5 x 10^-4)`, `0 - 3 x 10^5` all give -1/4000 or -300000). The neighbor prod

### #15 [major] The bracket-free function argument stops at an explicit `*`, so the learner's explicit-multiplication spelling of an authored answer is marked wrong

File: `crates/core/src/answer/parse.rs:576` — IDs: V4, V3, A3, R5 — duplicate of #4

**Claim.** `parse_juxtaposed_argument` ends the argument chain at an explicit `*`, so the authored corpus answer `cos 2x` reads as `cos(2*x)` while the same answer written with the multiplication sign the learner is invited to use, `cos 2*x`, reads as `cos(2)*x`; 1.0 reads both as `cos(2*x)`, so a correct learner is marked wrong on five authored corpus answers.

**Evidence.**

```
parse.rs:569-576
    /// The argument is the juxtaposed chain of atoms with their powers, so
    /// `cos 2x` is `cos(2*x)` and `sin 3t^2` is `sin(3*t**2)`. The chain stops at
    /// `+`, `-`, `,`, `)`, `=`, `<`, `>`, and at an explicit `*` or `/`, because
    /// none of them starts a factor. 1.0 reads the five authored corpus answers
    /// of this shape the same way (review finding #3).

Probe:
"cos 2x" vs "cos 2*x"  => correct=false notation=false
"cos 2x" vs "cos(2*x)" => correct=true  notation=false
"cos 2x" vs "cos(2x)"  => correct=true  notation=false
SHOW "cos 2*x" canon=Ok(Poly({{Var("x"): 1, Call("cos", [Rational(2)]): 1}: 1}))

Live 1.0:
{"expected":"cos 2x","learner":"cos 2*x"} -> {"equivalent": true, "notation": false, "timeout": false}

The M2.md ruling covers only the authored form ("Match 1.0's reading on the five corpus answers and pin them"); the learner variant is not ruled, is not in crates/core/tests/answer_divergence.rs, and is silently absorbed by the class-4 catch-all (see the finding on answer_oracle.rs:1326).
```

**Failure scenario.** The corpus topic authoring `cos 2x` (and the four LaTeX siblings `$\cos 2t$`, `$(4/3)\sin 3t$`, `$2\cos 2t + (5/2)\sin 2t$`, `$\cos 3t + 2\sin 3t$`) asks for the same function. A learner types the identical answer with the multiplication sign made explicit — `cos 2*x` — which is the spelling the `explicit_multiplication` V4 tolerance exists to accept. 2.0 parses it as `x*cos(2)`, `check` returns `Verdict { correct: false }`, and the learner loses the item. 1.0 graded the same string `True`. This is an A3/V4 regression against the oracle on 5 of the 14,989 generated pairs, and it is the whole content of the class-4 'transcendental identity' bucket.

**Refuter.** The claim reproduces exactly, in 2.0 code, against the live 1.0 oracle, and in the committed fixtures. parse.rs:576 `parse_juxtaposed_argument` stops the chain at an explicit `*`, so `cos 2*x` canonicalizes to `x*cos(2)`. The live 1.0 checker reads the same string as `cos(2*x)`: it answers `true` for `cos 2*x` vs `cos(2*x)` and `false` for `cos 2*x` vs `cos(2)*x`. All five authored corpus answers of this shape are hit. 2.0 returns `Decided { correct: false }` on all five explicit-multiplication learner variants, and 1.0 returns `true` on all five. That is a correct learner marked wrong (V3 parity, V4 tolerance, A3). The `explicit_multiplication` generator carries `Intent::Same`, so the harness itself declares these pairs equal. The docs/plans/M2.md ruling of round 1 covers only the authored form ("the bracket-free function argument takes the whole juxtaposed chain of atoms with their pow

## FIXM2e

### #8 [major] The exponent law of Atom::Exp does not fold Atom::E, so e^(x+2) and e^2*e^x are two values

File: `crates/core/src/answer/canon.rs:757` — IDs: V4, V3, V1

**Claim.** `add_exp` merges a new exponential only with an `Atom::Exp` already in the monomial and never with `Atom::E`, so an exponent that is a sum of a variable part and a whole number splits into two atoms that never rejoin: `e^(x+2)` is `Exp(x+2)` and `e^2*e^x` is `{E:2, Exp(x):1}`, and the checker decides them unequal.

**Evidence.**

```
canon.rs:757-767 (`add_exp`):
        let present = monomial.iter().find_map(|(key, _)| match key {
            Atom::Exp(value) => Some(value.as_ref().clone()),
            _ => None,
        });
        let total = match present {
            Some(value) => {
                monomial.remove(&Atom::Exp(Box::new(value.clone())));
                self.add(&value, &scaled)?
            }
            None => scaled,
        };
`Atom::E` is not looked for, and canon.rs:768-769 sends a whole argument to `Atom::E`, so the two atoms coexist.

Probe against `cadus_core::answer::canonical_form` and `check` (release build, kind = expression):
  canon "e^(x+2)" => Poly({{Exp(Poly({{}: 2, {Var("x"):1}: 1})): 1}: 1})
  canon "e^2*e^x" => Poly({{E: 2, Exp(Poly({{Var("x"):1}: 1})): 1}: 1})
  canon "e^(x-1)" => Poly({{Exp(Poly({{}: -1, {Var("x"):1}: 1})): 1}: 1})
  canon "e^x/e"   => Poly({{E: -1, Exp(Poly({{Var("x"):1}: 1})): 1}: 1})

  "e^(x+2)"   vs "e^2*e^x"        => correct=false notation=false
  "e^2*e^x"   vs "e^(x+2)"        => correct=false notation=false
  "e^(x+1)"   vs "e*e^x"          => correct=false notation=false
  "e^(x-1)"   vs "e^x/e"          => correct=false notation=false
  "e^(2x+2)"  vs "e^2*e^(2x)"     => correct=false notation=false
  "exp(x+2)"  vs "exp(2)*exp(x)"  => correct=false notation=false
The law does hold when neither factor is whole, which shows the gap is the `E` atom alone:
  "e^(3x)" vs "e^x*e^(2x)" => correct=true, "e^(-x)" vs "1/e^x" => correct=true

The 1.0 oracle grades every one of those pairs equal:
  e^(x+2) vs e^2*e^x   -> {"equivalent": true}
  e^(x+1) vs e*e^x     -> {"equivalent": true}
  exp(x+2) vs exp(2)*exp(x) -> {"equivalent": true}
  e^(x-1) vs e^x/e     -> {"equivalent": true}
  e^(2x+2) vs e^2*e^(2x) -> {"equivalent": true}

The round-1 ruling (docs/plans/M2.md, "Revised after review") says "`exp(a)` is `e^(a)` for every argument, and the exponent law holds", and canon.rs:735-739 claims the law. No entry of crates/core/tests/answer_divergence.rs covers this pair shape.
```

**Failure scenario.** An exponential topic (the corpus carries 36 answers with `e^` or `exp(`, on `derivatives-natural-exp`, `u-substitution-basics`, `integration-by-parts`, `combining-differentiation-rules`) authors the answer `e^(x+1)`. The learner applies the exponent law and types the equally correct `e*e^x`. `check("e^(x+1)", "e*e^x", Expression)` returns `Verdict { correct: false, notation: false }`. 1.0 graded the same pair True. The learner loses XP and FIRe on a right answer, and A3 stops the answer from reaching the model.

**Refuter.** The defect is real and reproducible. `add_exp` (crates/core/src/answer/canon.rs:753-772) searches the monomial for `Atom::Exp` only. It never searches for `Atom::E`. `add_atom` (canon.rs:685-711) sends `Atom::E` to the generic branch, which only adds exponents for an identical key, so it never merges into an `Atom::Exp` either. The merge is one-directional: `add_exp` writes an `Atom::E` when the total argument is a whole number (canon.rs:769), but no path reads an existing `Atom::E` back into an exponential argument. One monomial can hold both atoms at the same time. `check` compares canonical forms by structural equality (check.rs:112-119, `same_answer`) with no numeric or sampling fallback, so the two forms decide unequal. I built the crate in release mode and ran the pairs. canon("e^(x+2)") is Poly({{Exp(Poly({{}: 2, {Var("x"):1}: 1})): 1}: 1}) and canon("e^2*e^x") is Poly({{E: 2, Exp

### #10 [major] The oracle parity harness files the five `cos 2*x` parse divergences as a transcendental identity, so the class-3 assertion never sees them

File: `crates/core/tests/answer_oracle.rs:1325` — IDs: V3, C4

**Claim.** `documented_reason` falls back to `names_a_transcendental`, a substring test for `sin`/`cos`/`log`/`exp` over both sides, for every pair where 1.0 says true and 2.0 says false; the five `explicit_multiplication` pairs of the previous finding are parse divergences and not identities, but the predicate moves them into class 4 with a false reason, so `the_two_checkers_agree_on_every_comparable_pair` reports 100.0000% agreement and passes.

**Evidence.**

```
answer_oracle.rs:1323-1327
            return Some("no float tolerance rung (D6)");
        }
        if names_a_transcendental(pair) {
            return Some("a transcendental identity is not simplified (V1)");
        }

`cargo test -p cadus-core --test answer_oracle -- --nocapture`:
  -- per class --
  class 1 outside_grammar: 931
  class 3 comparable: 14037
  class 4 documented_divergence: 21
  -- per documented reason --
  a transcendental identity is not simplified (V1): 5
  class 3 agreement: 14037/14037 = 100.0000%

A scan of the committed verdict file for every decided disagreement gives exactly 21 pairs, and the five under that reason are:
  2.0=false 1.0=true expected="cos 2x" learner="cos 2*x"
  2.0=false 1.0=true expected="$\\cos 2t$" learner="cos 2*t"
  2.0=false 1.0=true expected="$(4/3)\\sin 3t$" learner="(4/3)*sin 3*t"
  2.0=false 1.0=true expected="$2\\cos 2t + (5/2)\\sin 2t$" learner="2*cos 2*t + (5/2)*sin 2*t"
  2.0=false 1.0=true expected="$\\cos 3t + 2\\sin 3t$" learner="cos 3*t + 2*sin 3*t"
Each pair comes from the generator `explicit_multiplication` (answer_oracle.rs:848, `intent: Intent::Same`), and each canonical form shows a different argument, not an unsimplified identity:
  canon("cos 2x")  = Func("cos", [2*x])
  canon("cos 2*x") = Poly({{Var("x"):1, Call("cos",[2]):1}: 1})

docs/reviews/M2-review-1.md states the contract the predicate breaks: "A pair that leaves 1.0 for any other reason stays in class 3 and fails the parity assertion (R5)."
```

**Failure scenario.** Round 1 confirmed finding #3 because the harness could not see the `cos 2x` misreading. After FIXM2a the misreading moved to `cos 2*x`, the harness recorded it, and the catch-all reason hid it again: the suite is green while five recorded 1.0 verdicts disagree. The same catch-all masks any future regression in any answer whose text holds `sin`, `cos`, `tan`, `sec`, `csc`, `cot`, `sinh`, `cosh`, `tanh`, `exp`, `ln`, or `log`, because every 1.0-true / 2.0-false pair of that shape leaves class 3 with a reason that was never measured.

**Refuter.** The claim is demonstrable in every checkable part, so I cannot refute it. (1) `documented_reason` (crates/core/tests/answer_oracle.rs:1312-1330) puts `names_a_transcendental` last in the `oracle.equivalent && !rust_correct` branch, after `both_sides_are_numbers`. The predicate (answer_oracle.rs:1297-1310) is a plain substring test of 12 function names over both normalized sides, so every 1.0-true / 2.0-false pair whose text holds sin/cos/tan/sec/csc/cot/sinh/cosh/tanh/exp/ln/log goes to class 4 and never reaches `return None`. (2) The five pairs under that reason are exactly the five listed, all from generator `explicit_multiplication` with `intent: same`. I measured the canonical forms with a probe binary that links cadus-core: canon("cos 2x") = Func("cos", [2*x]) and canon("cos 2*x") = Poly({{Var("x"):1, Call("cos",[2]):1}: 1}). The two sides hold different arguments, so the divergence

### #13 [major] A reciprocal raised to a power and a power under a reciprocal are two canonical forms of one value, so `(1/(x+1))^2` is wrong against `1/(x+1)^2`

File: `crates/core/src/answer/canon.rs:558` — IDs: V3, A3, V1

**Claim.** `reciprocal` builds `Atom::Inverse` over the content-normalized sum, and `add_atom` raises that atom in place, so `(1/(x+1))^2` becomes `Inverse(x+1)^2` while `1/(x+1)^2` becomes `Inverse(x^2+2*x+1)^1`; the two forms are never equal, so one value has two canonical forms and the checker marks a correct learner answer wrong.

**Evidence.**

```
Release probe against `cadus_core::answer::check`:
  check("1/(x+1)^2", "(1/(x+1))^2", Expression) -> Decided(Verdict { correct: false, notation: false })
  check("1/(x+1)^2", "(x+1)^-2",    Expression) -> Decided(Verdict { correct: true,  notation: false })

Live 1.0 oracle (/home/deploy/dev/cadus/.venv/bin/python scripts/oracle/check_1_0.py):
  {"equivalent": true, "id": "1"}  expected "1/(x+1)^2", learner "(1/(x+1))^2"

canon.rs:545-560 (`reciprocal`)
        let (content, primitive) = self.content_normalize(&sum)?;
        ...
        let atom = Atom::Inverse(Box::new(from_sum(primitive)));
        self.add_atom(&mut monomial, &mut coefficient, &atom, 1)?;
canon.rs:686 (`add_atom`) keeps `Atom::Inverse` as a plain atom and adds the exponent, so `Inverse(x+1)` reaches exponent 2 and never expands.

The module header of canon.rs names one narrowing of this area — "Cancellation by a polynomial greatest common divisor is beyond this unit" — and this pair needs no cancellation, so no documented divergence covers it. No test in crates/core/tests/answer_divergence.rs pins it, and the `explicit_multiplication`, `star_power`, and `wrong_exponent` generators of answer_oracle.rs never build the shape.
```

**Failure scenario.** A `combining-differentiation-rules` or `derivatives-inverse-trig` topic authors the derivative `1/(x+1)^2`. A learner writes the same value as `(1/(x+1))^2`, which is the shape the quotient rule produces. `canon` gives `Inverse(x+1)^2` for the learner and `Inverse(x^2+2*x+1)` for the author, `check` returns `Decided{correct:false}`, and A3 makes the verdict final. 1.0 graded the same pair True.

**Refuter.** The claim is demonstrable and no ruling covers it. `reciprocal` (crates/core/src/answer/canon.rs:545-560) wraps the content-normalized divisor in `Atom::Inverse`, and `add_atom` (canon.rs:680-700) treats `Atom::Inverse` as an opaque atom whose exponents only accumulate. The exponent law `Inverse(p)**n == Inverse(p**n)` therefore never holds: `1/(x+1)**2` goes through `power` then `reciprocal` into `Inverse(x**2+2*x+1)**1`, and `(1/(x+1))**2` goes through `reciprocal` then `power_of_term` into `Inverse(x+1)**2`. One value, two canonical forms. I reproduced the divergence with a release binary linked against `cadus_core::answer::check`, and I confirmed the 1.0 verdict with the live oracle. The gap hits authored corpus answers, not invented shapes only. The module header narrowing (canon.rs:29-35) names polynomial greatest-common-divisor cancellation only, and this pair needs no cancellatio

### #14 [major] The class-4 reason 'a transcendental identity is not simplified' excuses five pairs that contain no identity, hiding a real divergence

File: `crates/core/tests/answer_oracle.rs:1326` — IDs: V3, R5, V1 — duplicate of #10

**Claim.** `names_a_transcendental` (answer_oracle.rs:1298) is a bare substring search for a function name and is the last catch-all for every 'oracle said equal, 2.0 said wrong' divergence, so all five pairs it labels are in fact the juxtaposed-function-argument divergence and none of them involves a simplified identity; the parity report therefore prints 100% agreement while five undocumented, unpinned divergences pass as documented.

**Evidence.**

```
answer_oracle.rs:1320-1328
    if oracle.equivalent && !rust_correct {
        if both_sides_are_numbers(pair) {
            return Some("no float tolerance rung (D6)");
        }
        if names_a_transcendental(pair) {
            return Some("a transcendental identity is not simplified (V1)");
        }
        return None;

answer_oracle.rs:1298-1306 — the predicate matches a substring of the normalized source, nothing more:
        normalize(&pair.expected).source.contains(name)
            || normalize(&pair.learner).source.contains(name)

$ cargo test -p cadus-core --release --test answer_oracle the_two_checkers -- --nocapture
class 4 documented_divergence: 21
a transcendental identity is not simplified (V1): 5
class 3 agreement: 14037/14037 = 100.0000%

The five pairs that carry that reason (re-derived by joining the dumped generated set with oracle_verdicts_1_0.jsonl and re-running check):
  "cos 2x"                      vs "cos 2*x"                    2.0 correct=false / 1.0 equiv=true
  "$\\cos 2t$"                  vs "cos 2*t"                    2.0 correct=false / 1.0 equiv=true
  "$(4/3)\\sin 3t$"             vs "(4/3)*sin 3*t"              2.0 correct=false / 1.0 equiv=true
  "$2\\cos 2t + (5/2)\\sin 2t$" vs "2*cos 2*t + (5/2)*sin 2*t"  2.0 correct=false / 1.0 equiv=true
  "$\\cos 3t + 2\\sin 3t$"      vs "cos 3*t + 2*sin 3*t"        2.0 correct=false / 1.0 equiv=true

The cause is a parse difference, not a simplification:
SHOW "cos 2x"  canon=Ok(Func("cos", [2*x]))
SHOW "cos 2*x" canon=Ok(Poly({{Var("x"):1, Call("cos",[2]):1}: 1}))   i.e. x*cos(2)

Live 1.0 (scripts/oracle/check_1_0.py, /home/deploy/dev/cadus/.venv/bin/python):
{"expected":"cos 2x","learner":"cos 2*x"} -> {"equivalent": true, "notation": false, "timeout": false}

No entry of crates/core/tests/answer_divergence.rs mentions `cos 2*x`; `a_transcendental_identity_is_not_decided` (line 235) pins only sin^2+cos^2 vs 1.
```

**Failure scenario.** A reviewer or the M2 gate reads `class 3 agreement: 14037/14037 = 100.0000%` and the pinned `REASON_COUNTS` entry `("a transcendental identity is not simplified (V1)", 5)` and concludes the checker matches 1.0 except for the four narrowings the plan names. It does not. All five pairs are the `cos 2*x` binding difference; SymPy runs no `simplify` on any of them (both sides are one product of one function application). Because the catch-all fires on any pair whose text merely contains `sin`, `cos`, `ln`, `log`, or `exp`, any future 2.0 defect that marks a correct trigonometric or logarithmic learner answer wrong is absorbed into class 4 the same way and never fails the R5 assertion.

**Refuter.** I cannot refute the claim. Every part of it reproduces.

1. The predicate is a bare substring test. `names_a_transcendental` (/home/deploy/dev/cadus2.0/crates/core/tests/answer_oracle.rs:1297-1306) tests `normalize(side).source.contains(name)` for 12 names. It is the last test in the `oracle.equivalent && !rust_correct` branch of `documented_reason` (line 1320-1328), after `both_sides_are_numbers`. A pair that it matches goes to class 4 and leaves `report.disagreements` empty.

2. All five labeled pairs are the juxtaposed-argument binding difference. I re-derived the set with a program outside the repo that reads the dumped pairs, joins `oracle_verdicts_1_0.jsonl`, and runs `check`. Seven pairs have 1.0 equivalent=true and 2.0 correct=false. Two are numeric (1/1000 vs 1/1001, 1/1920 vs 1/1921) and take the D6 rung. The other five are the five the claim lists, and all five come from the `

### #16 [major] The generator set omits the spec section 9.3 'internal space collapse' family, so the 100% class-3 agreement is an artifact of generator choice

File: `crates/core/tests/answer_oracle.rs:751` — IDs: V3, R5

**Claim.** Spec section 9.3 names 'internal space collapse' (`1 / 2`, `1 + 2 x`) as a True-verdict generator family, but `GENERATORS` implements only the narrow `comma_space_removed` and `plus_spaced` forms; running the named family over the same in-grammar corpus rows produces at least four pairs that land in class 3 with no documented reason, so the pinned `CLASS_COUNTS`/`REASON_COUNTS` literals and the 100% figure do not establish parity, and the documented-divergence list is incomplete.

**Evidence.**

```
answer_oracle.rs:751 `const GENERATORS: [Generator; 33]` — the only space generators are `comma_space_removed` (line 489, `", "` -> `","`) and `plus_spaced`. No generator removes or inserts an internal space.

I applied the spec-named family (drop every internal space) to the same deduplicated in-grammar corpus rows and asked the live 1.0 oracle. 33 pairs diverge; these four are covered by NO predicate of `documented_reason` (no `**`, no `2j`/`0x`, no nested `√(`, not a chained inequality, no `{`), so `classify` would put them in class 3 and `the_two_checkers_agree_on_every_comparable_pair` would fail:

  '$2\\sqrt 2 - 2$'        vs '$2\\sqrt2-2$'         expression  2.0 True / 1.0 False
  'cos 70°'                vs 'cos70°'               expression  2.0 True / 1.0 False
  '2x/((x^2 + 1) ln 3)'    vs '2x/((x^2+1)ln3)'      expression  2.0 True / 1.0 False
  'y = x'                  vs 'y=x'                  expression  2.0 True / 1.0 False

(1.0 verdicts read from scripts/oracle/check_1_0.py against /home/deploy/dev/cadus/.venv/bin/python, --timeout 3, no timeouts.)

The cause in each case is a 1.0 defect the class-4 list does not name: `to_sympy_source` deletes the backslash and leaves the bare name `sqrt2`/`ln3` (sympy_check.py:96), and `cos70` tokenizes as one symbol. `DOCUMENTED_REASONS` (line 1106) names the brace-group form of that defect only.

Also absent from `GENERATORS`, though spec 9.3 names them: case flip, decimal to >= 8 significant digits, commutative reorder of a product, algebraic refactor, word anagram.
```

**Failure scenario.** The M2 acceptance check is '100% agreement with the recorded 1.0 verdicts in divergence classes 3 and 4'. That gate passes today only because the harness never generates the space-collapse variant the spec names. Add the family the spec asks for and the same corpus rows produce four class-3 disagreements the reason table cannot explain, so the assertion at answer_oracle.rs:1668 fails and the pinned literals `("class 3 comparable", 14037)` and `("class 4 documented_divergence", 21)` all move. The measured parity number therefore reports the generators that were written, not the parity of the checker, and the class-4 list is not complete for the corpus.

**Refuter.** The claim is demonstrable, and the count of uncovered pairs is larger than the reviewer states. Spec section 9.3 lists 'internal space collapse' as a True-verdict generator family that applies to 'all' kinds. The array at crates/core/tests/answer_oracle.rs:751 carries the doc comment 'Every generator of spec section 9.3, in a fixed order', but that comment is false: the only space generators are `comma_space_removed` (line 489) and `plus_spaced` (line 493). Neither removes a general internal space. `case flip`, `decimal to >= 8 significant digits`, `commutative reorder of a product`, and `algebraic refactor` are also absent.

I applied the named family (drop every internal space) to the same deduplicated in-grammar corpus rows that `in_grammar_rows` builds, and I asked the live 1.0 oracle. 743 rows change under the family, 665 of them get a Decided 2.0 verdict, 0 time out, and 30 diverge

### #17 [major] Two reciprocals of sums are never merged, so `4/(x-2) * 1/(x+2)` is not `4/((x-2)(x+2))`

File: `crates/core/src/answer/canon.rs:686` — IDs: V3, A3, V1

**Claim.** `add_atom` merges a second `Atom::Sqrt` and a second `Atom::Exp` into the atom already in the monomial, but it drops `Atom::Inverse` into the generic branch, so two divisions by sums stay two independent factors; the same value written as one division becomes `Inverse(product)` and the two canonical forms differ, which marks a correct learner answer wrong on two authored corpus answers that 1.0 grades True.

**Evidence.**

```
crates/core/src/answer/canon.rs:686-711 (`add_atom`): the `Atom::Exp` branch merges (`return self.add_exp(...)`), the `Atom::Sqrt` branch merges (`extract_square(&(present * radicand))`), and `Atom::Inverse` falls to
            let previous = monomial.get(atom).copied().unwrap_or(0);
            ... monomial.insert(atom.clone(), total);
so `Inverse(A)` and `Inverse(B)` sit side by side.

Canonical forms (probe):
  "1/((x+1)(x+2))"   canon=Poly({{Inverse(Poly{x^2 + 3x + 2}): 1}: 1})
  "1/(x+1) * 1/(x+2)" canon=Poly({{Inverse(Poly{x + 1}): 1, Inverse(Poly{x + 2}): 1}: 1})

2.0 (debug build, kind = expression):
  "4/((x - 2)(x + 2))"   vs "4/(x - 2) * 1/(x + 2)"   => correct=false notation=false
  "$1/((s - 2)(s - 5))$" vs "1/(s - 2) * 1/(s - 5)"   => correct=false notation=false
  "$1/((s - 2)(s - 5))$" vs "(1/(s - 2))(1/(s - 5))"  => correct=false notation=false
  "1/(x+1)^2"            vs "1/(x+1) * 1/(x+1)"       => correct=false notation=false

1.0 oracle (/home/deploy/dev/cadus/.venv/bin/python scripts/oracle/check_1_0.py), same four pairs:
  {"equivalent": true, "id": "a"}  {"equivalent": true, "id": "b"}
  {"equivalent": true, "id": "c"}  {"equivalent": true, "id": "d"}

The authored answers are in the corpus:
  corpus_1_0.jsonl:56  {"answer": "4/((x - 2)(x + 2))", "topic_id": "adding-subtracting-rational-expressions"}
  corpus_1_0.jsonl:1718 {"answer": "$1/((s - 2)(s - 5))$", "topic_id": "laplace-of-derivatives"}

No ruling covers it. crates/core/tests/answer_divergence.rs:243-249 documents only the missing polynomial gcd (`(x**2-1)/(x-1)` against `x+1`) and pins the content rule (`1/(x+1)` = `2/(2*x+2)`); crates/core/src/answer/canon.rs:31-35 makes the same two claims and no more. The canonicalizer already merges the identical case when the learner writes one division, so this is an inconsistency inside 2.0 and not the documented narrowing.
```

**Failure scenario.** A learner on `adding-subtracting-rational-expressions` (authored answer `4/((x - 2)(x + 2))`) writes the same value as a product of two simple fractions, `4/(x - 2) * 1/(x + 2)`. `canon` builds one monomial with two `Atom::Inverse` factors and the authored side builds one `Atom::Inverse` over `x^2 - 4`, so `check` returns Verdict { correct: false, notation: false }. Under A3 the correct answer is recorded as a failed attempt with the XP and FIRe penalty and it is not handed to the model. 1.0 graded the same pair True, and the divergence is in no ruling and no pinned test, so V3 parity is broken silently.

**Refuter.** The claim is demonstrable, so I do not refute it.

1. The code reads as the reviewer describes. In /home/deploy/dev/cadus2.0/crates/core/src/answer/canon.rs, `add_atom` (line 686) sends `Atom::Exp` to `add_exp` (line 696-699) and merges `Atom::Sqrt` through `extract_square` (line 700, 712-731). `Atom::Inverse` falls into the generic `else` block at line 700-711, which only adds exponents for one identical key. Two different `Inverse` atoms therefore stay two independent factors in one monomial.

2. The divergence is real. I built the crate in an isolated cargo project and called `cadus_core::answer::check` with `AnswerKind::Expression`. All four pairs return `Verdict { correct: false, notation: false }`. The canonical forms differ exactly as claimed: the one-division side gives one `Inverse` over the expanded product, and the product-of-fractions side gives two `Inverse` atoms.

3. The 1
