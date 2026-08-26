Source: survey of /home/deploy/dev/cadus on 2026-08-26 (read-only). Line citations point at 1.0 files.

# Cadus 1.0 answer-checking — the M2 source specification

Everything below is read from `/home/deploy/dev/cadus` (read-only) and the curriculum copy
`/home/deploy/dev/cadus2.0/curriculum`. Artifacts produced:

- `scripts/oracle/dump_answers_1_0.py`
- `crates/core/tests/fixtures/answers/corpus_1_0.jsonl` (3,492 lines)
- (the script's stderr run log is not kept)

---

## 1. Where the checker lives

| Concern | File:lines | Symbol |
|---|---|---|
| The whole equivalence engine | `/home/deploy/dev/cadus/cadus_web/sympy_check.py:1-400` | module |
| Verifiable answer kinds | `cadus_web/sympy_check.py:23` | `_VERIFIABLE = {AnswerKind.numeric, AnswerKind.expression}` |
| Text normalization | `cadus_web/sympy_check.py:35-44` | `_normalize` |
| Human/LaTeX → SymPy rewrite | `cadus_web/sympy_check.py:78-106` | `to_sympy_source` |
| Unicode-maths rewrite | `cadus_web/sympy_check.py:137-166` | `_UNICODE_SIMPLE`, `_unicode_math_to_ascii` |
| Plain-float compare | `cadus_web/sympy_check.py:169-176` | `_numeric_equal` |
| Builtins-free SymPy namespace | `cadus_web/sympy_check.py:179-197` | `_safe_sympy_globals`, `_SAFE_GLOBALS` |
| Evaluation bound (DoS guard) | `cadus_web/sympy_check.py:200-228` | `_MAX_EXPONENT`, `_MAX_SOURCE_CHARS`, `_POW_RE`, `_POW_TOWER_RE`, `_exponent_is_safe` |
| The one parse site | `cadus_web/sympy_check.py:231-264` | `_parse` |
| Symbolic compare | `cadus_web/sympy_check.py:317-364` | `_sympy_equivalent` |
| **The public entry point** | `cadus_web/sympy_check.py:367-400` | `answers_equivalent(expected, given, answer_kind) -> bool` |
| Dot-thousands tolerance | `cadus_web/sympy_check.py:109-130` | `dot_thousands_variant` |
| Template answer evaluation | `cadus_web/sympy_check.py:277-314` | `safe_eval_number` |
| "Does the checker have an opinion" | `cadus_web/prompts.py:1271-1283` | `symbolic_verdict -> bool \| None` |
| Model override | `cadus_web/prompts.py:1286-1299` | `apply_sympy_override`; applied at `cadus_web/prompts.py:1353` inside `materialize_grade` |
| **No-model grade path** | `cadus_web/deterministic_grade.py:109-152` | `deterministic_grade -> Grade \| None` |
| Blank-answer grade | `cadus_web/deterministic_grade.py:94-106` | `blank_answer_grade` |
| Correct tier constant | `cadus_web/deterministic_grade.py:72` | `CORRECT_TIER = WorkQuality.nearly_perfect` |
| Notation feedback text | `cadus_web/deterministic_grade.py:82-91` | `_notation_feedback` |
| Request-handler wiring | `cadus_web/api.py:1255-1273` | `_grade_answer` |
| Pre-grade `correct` prediction | `cadus_web/api.py:1106-1125` | `_known_correctness` |
| Input caps | `cadus_web/api.py:91-92, 1292` | `_MAX_ANSWER_CHARS = 4_000`, `_MAX_WORK_CHARS = 20_000` |
| Kill switch | `cadus_web/config.py:72-77` | `cadus_web_deterministic_grade: bool = True` (`CADUS_WEB_DETERMINISTIC_GRADE=0`) |
| Metrics | `cadus_web/metrics.py:103-114` | `cadus_deterministic_grade_total{result=…}` |
| FAKE engine grading | `cadus_web/engine.py:25, 463` | `answers_equivalent(problem.expected, answer, problem.answer_kind)` |
| Template authoring gate | `cadus_web/problem_templates.py:122-126, 345-350, 754, 773, 990` | `answers_equivalent`, `safe_eval_number`, `_to_sympy_source` |
| Docs | `docs/WEB_SERVICE.md:275-289` (number reading), `:340-391` (deterministic grading); `docs/DATA_MODEL.md:150-165, 196-242` (regrade) |

The dependency direction: `deterministic_grade` → `prompts.symbolic_verdict` → `sympy_check.answers_equivalent`.
There is exactly **one** function that decides `correct` without a model: `answers_equivalent`.

---

## 2. Normalization pipeline, step by step

### 2.1 `_normalize` (`sympy_check.py:35-44`) — the string-equality rung

```python
s = text.strip()
if s.startswith("$") and s.endswith("$") and len(s) > 1:  # :40-41
    s = s[1:-1].strip()
s = s.rstrip(".")            # :42  — rstrip, so "7329.." is stripped too
s = _WS_RE.sub(" ", s)       # :43  — _WS_RE = re.compile(r"\s+")  (:25)
return s.casefold().strip()  # :44
```

Order: strip → single outer `$…$` pair → strip **all** trailing periods → collapse every
whitespace run (Unicode `\s`, so NBSP/NNBSP collapse to one ASCII space) → `casefold`.
No NFC/NFKC pass — a decomposed accent or a fullwidth digit is a different string.

### 2.2 `to_sympy_source` (`sympy_check.py:78-106`) — the SymPy rung's rewrite

```python
s = text.strip()                                  # :85
if s.startswith("$") and s.endswith("$") ...: s = s[1:-1]   # :86-87
s = s.rstrip(".").strip()                         # :92
s = s.replace("^", "**")                          # :93
s = s.replace("\\cdot", "*").replace("\\times", "*")        # :94
s = s.replace("\\left", "").replace("\\right", "")          # :95
s = s.replace("\\", "")                           # :96  — every remaining backslash DELETED
s = s.replace("×", "*").replace("÷", "/")         # :97
s = _unicode_math_to_ascii(s)                     # :98
stripped = s.strip()
if _COMMA_GROUPS_RE.fullmatch(stripped):   s = stripped.replace(",", "")       # :102-103
elif _SPACE_GROUPS_RE.fullmatch(stripped): s = _WS_RE.sub("", stripped)        # :104-105
return s.strip()                                  # :106
```

No `casefold` here — the SymPy rung is case-sensitive (`I` vs `i`, `E` vs `e`).

**Thousands separators.**
- Commas: `_COMMA_GROUPS_RE = r"-?\d{1,3}(,\d{3})+"` (`:49`) — **fullmatch** only, so a
  comma is deleted only when the *whole* string is one grouped integer. `(4, 17)` is safe.
- Spaces: `_SPACE_SEPARATORS = " \u00a0\u202f\u2009\u2007"` (`:55`), `_SPACE_GROUPS_RE` (`:59`).
- Periods: `_DOT_GROUPS_RE = r"-?[1-9]\d{0,2}(\.\d{3})+"` (`:75`) — **not** applied here.
  A period stays a decimal point. The leading `[1-9]` is load-bearing; see §7.

**Unicode maths** (`_unicode_math_to_ascii`, `:146-166`), in this order:
1. `√(…)` → `sqrt(…)` and `√token` → `sqrt(token)`, applied to fixpoint (`:153-157`).
2. Superscript digits after `)` or a word char → `**n` (`:159-163`), via
   `_SUPERSCRIPTS = str.maketrans("⁰¹²³⁴⁵⁶⁷⁸⁹","0123456789")` (`:137`).
3. Plain substitutions (`_UNICODE_SIMPLE`, `:138-143`):
   `π→pi`, `τ→(2*pi)`, `∞→oo`, `·→*`, `−→-` (U+2212), `–→-` (en dash), `≤→<=`, `≥→>=`,
   `θ→theta`, `α→alpha`, `β→beta`, `λ→lamda`, `½→(1/2)`, `⅓→(1/3)`, `⅔→(2/3)`,
   `¼→(1/4)`, `¾→(3/4)`, `°→""` (degrees deleted — degree answers are bare numbers).

**What is NOT handled:** `\frac{a}{b}` (the `\` is deleted at `:96`, leaving `frac{1}{2}`),
brace groups after `^` (`x^{2}` → `x**{2}`), `\sqrt{…}`, decimal comma, `%`, unit suffixes,
`x=` prefixes, `\text{}`. Verified live:

```
eq('1/2', '\\frac{1}{2}', numeric) = False   | src = 'frac{1}{2}'
eq('x**2', 'x^{2}', expression) = False      | src = 'x**{2}'
eq('0.5',  '0,5',    numeric) = False        | src = '0,5'
eq('0.5',  '50%',    numeric) = False        | src = '50%'
eq('5',    '5 cm',   numeric) = False        | src = '5 cm'
eq('5',    'x=5',    numeric) = False        | src = 'x=5'
eq('2*x+1','y=2x+1', expression) = False     | src = 'y=2x+1'
```

### 2.3 Rung order in `answers_equivalent` (`sympy_check.py:367-400`)

```
0. given.strip() == ""                    -> False                      (:377-378)
1. _normalize(expected) == _normalize(given) -> True                    (:379-380)
      NOTE: this rung runs BEFORE the _VERIFIABLE gate.
2. answer_kind not in _VERIFIABLE         -> False                      (:381-382)
3. expected_src = to_sympy_source(expected); given_src = to_sympy_source(given)  (:383-384)
4. _numeric_equal(expected_src, given_src)                              (:385)
      True  -> True                                                     (:387-388)
      False -> return dot_thousands_variant(expected, given, kind)      (:389-391)
      None  -> fall through
5. _sympy_equivalent(expected_src, given_src) -> True                   (:396-397)
6. dot_thousands_variant(expected, given, kind)                         (:400)
```

Rung 1 running before the kind gate is why `test_curriculum_tree.py:83-88` says the old
self-equivalence gate "asserted a property it never measured".

### 2.4 `dot_thousands_variant` (`sympy_check.py:109-130`)

```python
if answer_kind not in _VERIFIABLE: return False        # :125-126
candidate = _normalize(given)                          # :127
if not _DOT_GROUPS_RE.fullmatch(candidate): return False   # :128-129
return _numeric_equal(to_sympy_source(expected), candidate.replace(".", "")) is True  # :130
```

Only the **learner** side may carry the dot grouping, and the value must match exactly
(through `_numeric_equal`'s 1e-9 rung, not the 1e-6 one).

---

## 3. SymPy usage and its exact semantics

### 3.1 Parsing (`sympy_check.py:231-264`)

```python
from sympy.parsing.sympy_parser import (
    implicit_multiplication_application, parse_expr, standard_transformations)   # :249-253
if not _exponent_is_safe(source): raise ValueError("expression exceeds the evaluation bound")  # :255-256
transformations = standard_transformations + (implicit_multiplication_application,)  # :258
return parse_expr(source, transformations=transformations,
                  global_dict=_SAFE_GLOBALS, local_dict={})                       # :259-264
```

- `_SAFE_GLOBALS` = `exec("from sympy import *")` with `__builtins__ = {}` (`:179-197`).
  **Every SymPy name is in scope** — `integrate`, `solve`, `factorint`, `prime`, `E`, `I`,
  `Interval`, `oo`, `zoo` — reachable from a learner's answer box.
- `implicit_multiplication_application` splits alphanumeric runs (`3xy^2 + 2y` → `3*x*y**2 + 2*y`)
  and applies bare function names (`15 sqrt 3` → `15*sqrt(3)`).
- Bounds (`:200-228`): `_MAX_SOURCE_CHARS = 4_000`; any literal exponent with
  `abs(int) > _MAX_EXPONENT = 1_000` is refused; `_POW_TOWER_RE = r"\*\*\s*[^*+\-/()\s]+\s*\*\*"`
  refuses `a**b**c`. There is **no wall-clock timeout anywhere.**

### 3.2 `_sympy_equivalent` (`sympy_check.py:317-364`) — what "equivalent" means

```python
try:
    lhs = _parse(expected); rhs = _parse(given)
except Exception: return False                                    # :337-343
try:                                                              # numeric rung
    if not (lhs.free_symbols or rhs.free_symbols):                # :345
        lv, rv = lhs.evalf(), rhs.evalf()                         # :346-347
        if bool(lv.is_real) and bool(rv.is_real):                 # :348
            lf, rf = float(lv), float(rv)                         # :349-350
            if math.isfinite(lf) and math.isfinite(rf):           # :351
                return abs(lf - rf) <= _NUMERIC_TOL * max(1.0, abs(lf))   # :352
except Exception: pass                                            # :353-354
try:                                                              # symbolic ladder
    if lhs == rhs: return True                                    # :356-357
    diff = simplify(lhs - rhs); return bool(diff == 0)             # :358-359
except (TypeError, ValueError, AttributeError):                   # :360
    try: return bool(lhs.equals(rhs))                             # :361-362
    except Exception: return False                                # :363-364
```

**The two tolerance numbers.**

| Rung | Where | Formula | Value |
|---|---|---|---|
| plain `float` | `_numeric_equal`, `sympy_check.py:172-174` | `abs(e-g) <= 1e-9 * max(1.0, abs(e))` | **1e-9** relative |
| SymPy `evalf` | `_sympy_equivalent`, `sympy_check.py:352`, constant at `:32` | `abs(lf-rf) <= _NUMERIC_TOL * max(1.0, abs(lf))` | **1e-6** relative |

Both are *relative to the expected side*, floored at 1.0 absolute — so for `|expected| < 1`
the bound is absolute 1e-9 / 1e-6. Measured boundary:

```
eq('1/3','0.333333',   numeric) = True     # |diff| = 3.3e-7 < 1e-6
eq('1/3','0.33333',    numeric) = False    # |diff| = 3.3e-6
eq('2/3','0.667',      numeric) = False
eq('6','6.0000000001', numeric) = True     # float rung, 1e-9 * max(1,6) = 6e-9
eq('6','6.000001',     numeric) = False
```

**Comparison order in words:** identical strings after normalization; else exact float
agreement to 1e-9; else, if both sides parse to free-symbol-free real finite numbers,
`evalf` agreement to 1e-6; else structural SymPy equality (`lhs == rhs`); else
`simplify(lhs - rhs) == 0`; else `lhs.equals(rhs)` (numeric random sampling); else False.
A plain Python container (`(4, 17)`, `{1, 2}`) has no `free_symbols`/`evalf`, so the
numeric rung raises, is swallowed at `:353-354`, and `lhs == rhs` decides it structurally.

**Latency.** `simplify` is unbounded. Measured on this box with
`/home/deploy/dev/cadus/.venv/bin/python`:

```
   276 ms  eq('(x+1)**200','x**200+1')                       = False
    74 ms  eq('integrate(exp(-x**2),(x,0,oo))','sqrt(pi)/2')  = True
    62 ms  eq('(x+1)**2','x**2+2*x+1')                        = True
    46 ms  eq('gamma(x)*gamma(1-x)','pi/sin(pi*x)')           = True
    16 ms  eq('sin(x)**2+cos(x)**2','1')                      = True
```

`(x+1)**200` is a legal 12-character learner answer and it alone consumes 276 ms of the
300 ms L2 budget. This is the concrete case for V1's "no heuristic simplification".

---

## 4. Eligibility rules — verifiable vs handed to the model

**Definition** (`sympy_check.py:21-23`):

```python
# Answer kinds whose equivalence we attempt to verify symbolically. ``multi-step``
# and ``proof`` are graded by the LLM/tutor, not by SymPy.
_VERIFIABLE = {AnswerKind.numeric, AnswerKind.expression}
```

`AnswerKind` (`cadus/model.py:35-40`): `numeric`, `expression`, `multi-step`, `proof`.
Per `docs/reference/curriculum-1.0-spec.md:148-157`: 257 numeric + 221 expression = **478
verifiable topics** of 1,090; 483 multi-step, 129 proof.

**The decision table** (`deterministic_grade.py:109-152`):

| Condition | Result | Line | Metric label |
|---|---|---|---|
| `not str(answer or "").strip()` | `blank_answer_grade()` — `correct=False`, `work_quality=poor`, `error_tags=["blank_answer"]`, `feedback="No answer given."` | `:120-122`, `:94-106` | `blank` |
| `answer_kind not in (numeric, expression)` | `None` → engine grades | `:123-125` | `unverifiable_kind` |
| `symbolic_verdict(problem, answer) is not True` | `None` → engine grades | `:129-131` | `incorrect_to_engine` |
| verified correct, `dot_thousands_variant` hit | `Grade(correct=True, work_quality=nearly_perfect, error_tags=["notation"], feedback=_notation_feedback(...), grader_note="deterministic")` | `:135-152` | `notation` |
| verified correct otherwise | same, `error_tags=[]`, feedback `"Correct."` or `_CORRECT_WITH_WORK_FEEDBACK` | `:138-152` | `correct` |

A **wrong** verifiable answer is deliberately *not* graded here — `deterministic_grade.py:26-31`
and `docs/WEB_SERVICE.md:371-376`: "A miss owes the learner an error-specific diagnosis and
the mandatory unaided re-solve (PEDAGOGY pp. 427, 431)". 2.0's A3 removes this restriction
("returns `correct` instantly for right AND wrong answers"), which is a *change* from 1.0
and must be spelled out in the M2 spec.

**Two callers must never diverge** (`prompts.py:1275-1279`, `api.py:1106-1125`):
`symbolic_verdict` is the single owner of "which kinds are decidable"; `_known_correctness`
predicts `correct` before the grader call and is sound only because `apply_sympy_override`
makes that verdict final.

### 4.1 What "verifiable" does NOT mean in 1.0

`answer_kind` is authored metadata on the topic (`cadus/model.py:132-146`). Nothing checks
that the *answer string* is machine-decidable. The only gate is
`tests/test_curriculum_tree.py:80-122`, which asserts `_sympy_equivalent(src, src)` — a test
that passes for `'yes'` (parsed as `e*s*y`) and for `'diverges'`. **167 of 3,492 answers
(4.8%) on numeric/expression topics are English words or prose** and claim a deterministic
verdict they cannot support. See §5 and §7.

---

## 5. Answer corpus statistics by shape

Produced by `dump_answers_1_0.py` over `/home/deploy/dev/cadus2.0/curriculum`:

```
topics: 478   answers: 3492
parsed: 3483   failed: 9
self_equivalent (reaches the SymPy rung): 3483
```

478 topics = 257 numeric + 221 expression, matching the curriculum spec. 3,492 answers =
1,833 numeric + 1,659 expression (one row per KP exemplar plus one diagnostic per topic).

| Shape | n | % | 5 examples |
|---|---:|---:|---|
| `integer` | 1622 | 46.4 | `6`, `9`, `7`, `6`, `4` |
| `expression_symbolic` | 686 | 19.6 | `2x cos(x^2)`, `3x^2 √(1 + x^3)`, `-x^2`, `-sin(x^2)`, `-2x cos(x^2)` |
| `fraction` a/b | 352 | 10.1 | `7/8`, `1/2`, `5/12`, `5/6`, `7/12` |
| `expression_numeric` (no free symbols) | 233 | 6.7 | `8*sqrt(2)`, `5*sqrt(5)`, `5*sqrt(2)`, `sqrt(3)`, `4*sqrt(3)` |
| `ordered_tuple` | 178 | 5.1 | `$(45, 12)$`, `(6, 4)`, `(4, 17)`, `(-6, 2)`, `(1, 3)` |
| **`prose_or_words`** | **167** | **4.8** | `18 degrees Celsius`, `vertices`, `sides`, `yes`, `no` |
| `decimal` | 128 | 3.7 | `0.7`, `2.4`, `1.5`, `0.7`, `2.5` |
| `comma_list` | 43 | 1.2 | `0.28, 0.3, 0.302`, `0.38, 0.4, 0.409`, `3/8, 1/2, 5/8`, `5/8, 2/3, 3/4`, `153, 315, 351` |
| `interval_ineq` | 34 | 1.0 | `-1 ≤ x ≤ 3`, `2 ≤ y ≤ 6`, `1 ≤ y ≤ 7`, `x <= -1`, `x > 4` |
| `quotient_remainder` | 16 | 0.5 | `9 R2`, `6 R2`, `5 R3`, `8 R2`, `23 R14` |
| `value_with_unit` | 12 | 0.3 | `5 m/s`, `7 L/min`, `6 m/s`, `4 m/s`, `16 m` |
| `mixed_number` | 8 | 0.2 | `3 1/2`, `3 2/5`, `3 3/4`, `4 1/6`, `1 2/3` |
| `other` (LaTeX-escaped set) | 7 | 0.2 | `$\{1, 3, 5, 7\}$`, `$\{2, 5\}$`, `$\{1, 2, 3, 4, 5, 6\}$`, `$\{2, 5\}$`, `$\{0, 3, 6, 9\}$` |
| `set_or_list` | 5 | 0.1 | `[-3, 3]`, `{1, 3, 5}`, `{2, 5}`, `{2, 4, 6}`, `{1, 3}` |
| `equation` | 1 | 0.0 | `y = x` |
| **total** | **3492** | **100** | |

Split by kind (kind → shape → n):

- **numeric (1833):** integer 1334, fraction 181, decimal 101, ordered_tuple 62,
  expression_symbolic 57, expression_numeric 44, prose_or_words 41, comma_list 8, other 4,
  interval_ineq 1.
- **expression (1659):** expression_symbolic 676(−reclass), integer 288,
  expression_numeric 190, fraction 171, ordered_tuple 116, prose_or_words 106,
  comma_list 35, interval_ineq 33, decimal 27, mixed_number 8, set_or_list 5, other 3,
  equation 1.

Surface features (share of all 3,492):

| feature | n | % |
|---|---:|---:|
| implicit multiplication (`3xy`, `2 x`, `)(`) | 433 | 12.4 |
| `^` power | 390 | 11.2 |
| `$…$` delimiters | 322 | 9.2 |
| comma present | 241 | 6.9 |
| backslash LaTeX | 137 | 3.9 |
| Unicode maths glyph | 106 | 3.0 |
| braces `{}` | 18 | 0.5 |

2,174 of 3,492 answers (62.3%) are already byte-identical to `str(parse_expr(...))`.

### 5.1 Parse results

**3,483 parse (99.74%); 9 fail (0.26%)** — and the 9 fall on exactly the 7 topics of
`tests/test_curriculum_tree.py:69-77`'s exemption list, no more and no fewer:

```
domain-range-of-relations.kp3[0]           '-1 ≤ x ≤ 3'            TypeError: cannot determine truth value of Relational: -1 <= x
domain-range-of-relations.kp3[1]           '2 ≤ y ≤ 6'             TypeError: cannot determine truth value of Relational: 2 <= y
domain-range-of-relations.<diagnostic>     '1 ≤ y ≤ 7'             TypeError: cannot determine truth value of Relational: 1 <= y
graphs-of-logarithmic-functions.kp3[0]     'y = x'                 SyntaxError: invalid syntax
integration-by-parts.<diagnostic>          '$(e^{\pi} + 1)/2$'     TypeError: unsupported operand type(s) for ** or pow(): 'Symbol' and 'set'
partial-derivatives.<diagnostic>           '$36x^2y^2$'            ValueError: expression exceeds the evaluation bound
riemann-sums-definite-integral.kp3[0]      '6 ≤ ∫ ≤ 15'            TypeError: cannot determine truth value of Relational: 6 <= ∫
sine-cosine-parent-graphs.kp3[0]           '-1 ≤ y ≤ 1'            TypeError: cannot determine truth value of Relational: -1 <= y
surface-area-revolution.<diagnostic>       '$\pi(13\sqrt{13} - 1)/6$'  SyntaxError: invalid syntax. Perhaps you forgot a comma?
```

`'$36x^2y^2$'` is notable: it is refused by the **DoS guard**, not by the grammar.
`to_sympy_source` yields `36x**2y**2`, and `_POW_TOWER_RE` (`sympy_check.py:215`) reads
`**2y**` as an exponent tower. A legitimate answer is rejected by a security bound.

### 5.2 Corpus file format

One JSON object per line, sorted keys, `ensure_ascii=False`:

```json
{"answer":"(4, 17)","answer_kind":"numeric","canonical":"(4, 17)",
 "canonical_srepr":"(Integer(4), Integer(17))","error":null,"exemplar_index":0,
 "kp_id":"kp2","normalized":"(4, 17)","parsed":true,"self_equivalent":true,
 "shape":"ordered_tuple","sympy_source":"(4, 17)",
 "topic_id":"arithmetic-sequences"}
```

---

## 6. Pinned behaviors — every literal input→verdict pair in the tests

### 6.1 `tests/test_sympy_check.py`

`test_numeric_equivalence` (`:23-43`) — `answers_equivalent(expected, given, kind) is want`:

| expected | given | kind | want |
|---|---|---|---|
| `1/3` | `0.3333333333` | numeric | True |
| `sqrt(2)` | `1.41421356` | numeric | True |
| `pi` | `3.14159` | numeric | True |
| `2/3` | `0.667` | numeric | **False** |
| `2*x+1` | `1 + 2*x` | expression | True |
| `x**2 - 1` | `(x-1)*(x+1)` | expression | True |
| `3` | `4` | numeric | False |
| `7` | `49/7` | numeric | True |
| `1/2` | `0.5` | numeric | True |

`test_unicode_maths_is_parseable` (`:58-87`):

| expected | given | kind | want |
|---|---|---|---|
| `15√3` | `15*sqrt(3)` | expression | True |
| `1/(2√x)` | `1/(2*sqrt(x))` | expression | True |
| `x/√(x^2 + 9)` | `x/sqrt(x**2+9)` | expression | True |
| `1/(4√x · √(1 + √x))` | `1/(4*sqrt(x)*sqrt(1+sqrt(x)))` | expression | True |
| `2√3/3` | `2*sqrt(3)/3` | expression | True |
| `π/6` | `pi/6` | numeric | True |
| `2π` | `2*pi` | numeric | True |
| `∞` | `oo` | expression | True |
| `-∞` | `-oo` | expression | True |
| `x²+1` | `x**2+1` | expression | True |
| `½` | `1/2` | numeric | True |
| `30°` | `30` | numeric | True |
| `15*sqrt(3)` | `15√3` | expression | True |
| `pi/6` | `π/6` | numeric | True |
| `15√3` | `15*sqrt(2)` | expression | False |
| `π/6` | `pi/3` | numeric | False |
| `x²+1` | `x**3+1` | expression | False |
| `√2` | `2` | numeric | False |

**The `½` row in 2.0.** 1.0 rewrites `½` to the text `(1/2)` (`sympy_check.py:141`). 2.0
rewrites it to the literal-fraction token `⟦1/2⟧` (marks U+27E6 and U+27E7), and
`\frac{1}{2}` — two plain digit runs — becomes the same token. Section 8.1 gives the rule
and the reason. The pinned verdict does not move: `½` against `1/2` is True in both
versions, and `2*½`, `2(1/2)`, `(2)½`, and `x½` keep the product reading in 2.0 as they do
in 1.0. What the token changes is the pair 1.0 never had: `2½` and `2\frac{1}{2}` are the
mixed number 5/2 in 2.0. 1.0 graded the first as the product 1, and 1.0 did not parse the
second. `crates/core/tests/answer_parse.rs` pins all five spellings.

`test_non_numeric_pairs_fall_through_to_the_symbolic_rung` (`:109-129`):

| expected | given | kind | want |
|---|---|---|---|
| `(4, 17)` | `(4,17)` | numeric | True |
| `(4, 17)` | `(4, 18)` | numeric | False |
| `(-6, 2)` | `(-6,2)` | numeric | True |
| `{1, 2}` | `{2,1}` | expression | True |
| `{1, 2}` | `{1,3}` | expression | False |
| `Interval(0, 1)` | `Interval(0,1)` | expression | True |
| `Interval(0, 1)` | `Interval(0,2)` | expression | False |
| `1 + I` | `1+I` | expression | True |
| `1 + I` | `1 - I` | expression | False |
| `zoo` | `-zoo` | expression | **True** |

`test_grouped_integers_are_accepted` (`:142-157`) — all `answers_equivalent("7329", given, numeric) is True`:
`"7,329"`, `"7329"`, `"7,329."`, `"7329."`, `"7 329"`, `"7\u00a0329"`, `"7\u202f329"`, `"$7,329$"`, `" 7,329 "`.

`test_a_wrong_value_stays_wrong_however_it_is_grouped` (`:160-162`) — all
`answers_equivalent("7329", given, numeric) is False`: `"7330"`, `"7,330"`, `"7.32"`, `"73,29"`, `"7329x"`.

`test_a_period_grouped_integer_is_accepted_as_a_notation_variant` (`:165-173`):
`answers_equivalent("7329","7.329",numeric) is True`; `dot_thousands_variant("7329","7.329",numeric) is True`.

`test_the_american_reading_always_wins_first` (`:176-182`):
`eq("0.5","0.5",N) is True`; `dtv("0.5","0.5",N) is False`;
`eq("1","1.000",N) is True`; `dtv("1","1.000",N) is False`.

`test_the_notation_reading_requires_an_exact_value_match` (`:185-187`):
`dtv("7239","7.329",N) is False`; `eq("7239","7.329",N) is False`.

`test_the_notation_reading_never_applies_to_an_unverifiable_kind` (`:190-192`):
`dtv("7329","7.329", multi_step) is False`; same for `proof`.

`test_a_trailing_period_does_not_break_an_expression` (`:195-196`):
`eq("2*x+1","1 + 2x.", expression) is True`.

`test_a_decimal_answer_survives_a_trailing_period` (`:199-202`):
`eq("0.5","0.5.",N) is True`; `eq("0.5","0.50",N) is True`.

### 6.2 `tests/test_deterministic_grade.py`

`test_verified_correct_answer_needs_no_engine` (`:43-61`) — each returns a `Grade` with
`correct is True` and `grader_note == "deterministic"`:

| kind | expected | answer |
|---|---|---|
| numeric | `12` | `12` |
| numeric | `12` | `12.0` |
| numeric | `12` | `sqrt(144)` |
| numeric | `7329` | `7,329` |
| numeric | `7400` | `7400` |
| expression | `2*x+1` | `1 + 2x` |

- `deterministic_grade(_problem(), "   ", None)` → `correct False`, `error_tags ["blank_answer"]` (`:64-68`).
- `deterministic_grade(_problem("anything", multi_step|proof), "some answer", None)` → `None` (`:71-74`).
- `deterministic_grade(_problem("12"), "14", None)` → `None` (`:77-84`).
- `CORRECT_TIER is WorkQuality.nearly_perfect`; `quality_q(CORRECT_TIER) < quality_q(perfect)`;
  `> quality_q(passable)`; `is_pass_quality(CORRECT_TIER)`; `>= PASS_QUALITY_THRESHOLD` (`:92-102`).
- `deterministic_grade(_problem(), "12", None).error_tags == []` (`:105-108`).
- shown work changes only `feedback`, never `(correct, work_quality)` (`:111-116`).
- feedback contains none of `smart`, `clever`, `genius`, `talented`, `natural` (`:119-126`).
- `deterministic_grade(_problem("7329"), "7.329", None)` → `correct True`,
  `error_tags == ["notation"]`, `"decimal point" in feedback` (`:145-150`).
- `deterministic_grade(_problem("2500"), "2.500", None)` → feedback contains `"2.500"` and
  `"2500"` and not `"7.329"` (`:153-162`).
- `deterministic_grade(_problem("0.5"), "0.5", None)` → `correct True`, `error_tags == []` (`:165-170`).
- `deterministic_grade(_problem("7239"), "7.329", None)` → `None` (`:173-175`).
- `blank_answer_grade()` → `correct False`, `work_quality poor`, `error_tags ["blank_answer"]`,
  `feedback == "No answer given."`, `grader_note is None` (`:183-188`).

### 6.3 `tests/test_sympy_security.py`

RCE payloads, all `answers_equivalent(..., numeric) is False`, and the marker file
`_rce_marker_should_not_exist` must not appear (`:24-58`):
`__import__('os').system('touch …')`, `exec("open('…','w').write('x')")`,
`eval("__import__('os').getenv('ANTHROPIC_API_KEY')")`, `print(open('.env.example').read())`
— as the learner answer (`:36-46`) and as `expected` (`:49-58`).

`test_legitimate_equivalence_still_works` (`:61-66`): `("2*x+1","1 + 2*x",E)→True`;
`("7","49/7",N)→True`; `("1/2","0.5",N)→True`; `("x**2 - 1","(x-1)*(x+1)",E)→True`;
`("3","4",N)→False`.

Exponent bombs, `False` in under `_DOS_TIMEOUT_S = 5.0` s (`:79-117`):
as the learner answer against expected `"4"` — `9**9**9`, `9^9^9`, `9**9**9**9`,
`2**10000000`, `(2)**(9999999)`; as `expected` against `"4"` — `9**9**9`, `2**10000000`.

`test_guard_does_not_reject_legitimate_powers` (`:120-132`), all expression, all True:
`("8","2**3")`, `("1024","2**10")`, `("x**2+1","x^2+1")`, `("x**2*y**3","x^2*y^3")`,
`("15√3","15*sqrt(3)")`.

### 6.4 `tests/test_curriculum_tree.py` — the SymPy-reachability gate

`test_every_numeric_expression_answer_reaches_the_sympy_rung` (`:80-122`): for every
numeric/expression topic, every KP exemplar answer and the diagnostic answer must satisfy
`_sympy_equivalent(to_sympy_source(text), to_sympy_source(text))`. Empty answers skipped
(`:107-108`); any exception counts as unreachable (`:112-113`).

**The 7-topic exemption list** (`_SYMPY_CANNOT_COMPARE`, `:69-77`), verbatim with its comments:

```python
_SYMPY_CANNOT_COMPARE = {
    "domain-range-of-relations",       # intervals: "-1 ≤ x ≤ 3", "2 ≤ y ≤ 6", "1 ≤ y ≤ 7"
    "graphs-of-logarithmic-functions",  # an equation: "y = x"
    "sine-cosine-parent-graphs",       # an interval: "-1 ≤ y ≤ 1"
    "riemann-sums-definite-integral",  # an interval over a symbol: "6 ≤ ∫ ≤ 15"
    "integration-by-parts",            # rewriter: "$(e^{\pi} + 1)/2$" → "e**{pi}"
    "surface-area-revolution",         # rewriter: braces + implicit multiplication
    "partial-derivatives",             # rewriter: implicit multiplication, "36x**2y**2"
}
```

Its stated two causes (`:60-68`): "``to_sympy_source`` gaps — it leaves LaTeX brace groups
after ``^`` (``^{2}`` becomes the invalid ``**{2}``) and inserts no implicit multiplication
(``36x**2y**2``)" and "Answers that are not a value — an interval (``-1 ≤ x ≤ 3``) or an
equation (``y = x``)".

`test_the_sympy_exemption_list_does_not_rot` (`:125-147`): every exempted topic must still
fail, or the entry must be removed. My corpus run confirms all 7 still fail — the list is
neither stale nor incomplete.

---

## 7. Known defects and traps

### 7.1 The dot-thousands note, verbatim

`cadus_web/deterministic_grade.py:82-91`:

```python
def _notation_feedback(answer: str, expected: str) -> str:
    """Feedback for a correct value written with periods as thousands separators.

    Cites the learner's OWN value rather than a fixed ``7.329`` example — the note is about
    what they just typed, and a stock example makes it read like boilerplate they can skip.
    """
    return (
        f"Correct value. One note on form: here a period is a decimal point, so"
        f" ${expected}$ is the way to write it — ${answer.strip()}$ reads as a decimal."
    )
```

For `expected="7329"`, `answer="7.329"` the rendered string is exactly:

> `Correct value. One note on form: here a period is a decimal point, so $7329$ is the way to write it — $7.329$ reads as a decimal.`

The design rule is `docs/WEB_SERVICE.md:284-289` and `sympy_check.py:109-124`: the notation
reading runs **last**, only on an exact value match, and is recorded as
`correct=True, error_tags=["notation"]` — "the value is recorded as right and the formatting
as wrong".

### 7.2 The 1000× hole (fixed) — `_DOT_GROUPS_RE`'s leading `[1-9]`

`sympy_check.py:67-74`, verbatim:

> The leading group must be NON-ZERO, and that is not cosmetic — it was a live hole.
> `-?\d{1,3}` also matched `0.008`, so `answers_equivalent("8", "0.008")` returned
> True: a learner's genuine 1000x error (a probability read as a count, a unit slip) was
> recorded as CORRECT, and 72% of the curriculum's numeric answers are plain 1-4 digit
> integers. It also opened the same 1000x hole in the template gate, whose sample check
> runs through this function — a model could claim `0.500` for an expression that
> computes `500` and pass.

Verified fixed: `eq('8','0.008',numeric) = False`.

### 7.3 The trailing-period regression (fixed)

`sympy_check.py:88-91` and `tests/test_sympy_check.py:136-139`: the live problem was
*"Which is larger, $7{,}239$ or $7{,}329$?"*. `_normalize` stripped the trailing period but
`to_sympy_source` did not, so `"7,329."` failed the comma-group fullmatch, kept its comma,
reached SymPy unparseable, and a correct learner was recorded WRONG. Both paths now
`rstrip(".")` at the same point in the sequence.

### 7.4 The Unicode-glyph regression (fixed)

`sympy_check.py:133-136`: "Without this the grader marks a correct learner WRONG:
`answers_equivalent("15√3", "15*sqrt(3)")` was False, and 99 graded answers in the tree use
these glyphs." My corpus counts 106 answers carrying a Unicode maths glyph.

### 7.5 The regrade incident — 18 correct attempts

`docs/DATA_MODEL.md:238-242`, verbatim:

> The ops tool is `scripts/regrade_attempts.py` (`list` / `plan` / `apply`); `plan` writes
> nothing. It was written for one incident: the pre-2026-08-17 model grader read an empty
> *optional* work field as a defect and tiered 18 correct, verified answers below
> `fire.PASS_QUALITY_THRESHOLD`, which cost the learner 29 XP and left 7 topics with punished
> review intervals.

`scripts/regrade_attempts.py:3-9`: the model "tiered correct, symbolically verified answers
``poor``, ``blowoff`` or ``nearly_passable`` — every tier below ``fire.PASS_QUALITY_THRESHOLD``.
A correct answer was therefore recorded as a failed attempt: XP multiplier 0.0 or −0.5,
FIRe ``q`` 0.15 or 0.0."

The repair rule (`scripts/regrade_attempts.py:20-33`) required **both** `correct is True`
(itself decided by the SymPy override) **and** a recorded tier below the pass line.
`correct` is deliberately not correctable by a `regraded` event
(`docs/DATA_MODEL.md:221-223`): "a correction restates how well the work was done, never
whether the answer was right." `docs/DATA_MODEL.md:159-162`: "It could repair those attempts
only because SymPy had independently decided `correct`. On a proof there would have been
nothing to repair from."

### 7.6 Live false positives in the current checker

All measured with `/home/deploy/dev/cadus/.venv/bin/python` importing
`cadus_web.sympy_check.answers_equivalent`:

```
eq('yes',    'sey',    numeric)    = True    # prose parses to e*s*y — multiplication commutes
eq('even',   'neve',   numeric)    = True    # any anagram of a word answer
eq('III',    '-I',     expression) = True    # 'III' -> I*I*I = -I (the imaginary unit)
eq('4, 17',  '(4, 17)',numeric)    = True    # parenthesized and bare tuples are one answer
eq('zoo',    '-zoo',   expression) = True    # pinned by a test (test_sympy_check.py:123)
```

Prose answers parse into meaningless symbol products, and the products are order-free.
Sample canonical forms from the corpus: `'yes' → e*s*y`, `'diverges' → d*e**2*g*i*r*s*v`,
`'DNE' → 2.71828182845905*D` (`E` is Euler's number), `'false' → False`, `'true' → True`,
`'prime' → <function prime at 0x…>`, `'binomial' → binomial`,
`'18 degrees Celsius' → 18*C*d*e**4*g*i*l*r*s**3*u`.

**167 answers (4.8% of the corpus, on 94 of the 478 topics) are in this class, and the 1.0
tree gate passes every one of them** because `_sympy_equivalent(src, src)` is trivially
true. This is the single biggest correctness gap and the strongest argument for V2.

### 7.7 Live false negatives

```
eq('3 1/2', '7/2',            numeric)    = False   # implicit mult reads "3 1/2" as 3*(1/2)
eq('3 1/2', '3.5',            numeric)    = False
eq('1/2',   '\frac{1}{2}',    numeric)    = False   # '\' deleted -> 'frac{1}{2}'
eq('x**2',  'x^{2}',          expression) = False   # -> 'x**{2}'
eq('0.5',   '0,5',            numeric)    = False   # no decimal-comma reading
eq('0.5',   '50%',            numeric)    = False   # no percent
eq('5',     '5 cm',           numeric)    = False   # no unit stripping
eq('5',     'x=5',            numeric)    = False   # no 'x=' prefix stripping
eq('-1 ≤ x ≤ 3', '-1 <= x <= 3', expression) = False # chained relational raises
eq('(4, 17)','(4, 17.0)',     numeric)    = False   # tuples compare structurally, not numerically
eq('(4, 17)','[4, 17]',       numeric)    = False
eq('∞',     'infinity',       expression) = False   # only 'oo' is the infinity spelling
```

Mixed numbers are therefore **string-match only** in 1.0: `eq('3 1/2','3 1/2') = True` via
rung 1, and every other spelling fails.

### 7.8 Silent semantic corruption

```
to_sympy_source('2y · dy/dx')  -> '2y * dy/dx'  -> parses to  2*x*y**2
to_sympy_source('log_b(x) + log_b(y)') -> parses to  log_b*x + log_b*y
to_sympy_source('$36x^2y^2$')  -> '36x**2y**2'  -> ValueError (DoS guard false positive)
```

`dy/dx` becomes `d*y/d*x = x*y` under left-associative division — the derivative notation is
silently multiplied out. Nothing detects it; `_sympy_equivalent(src, src)` is still True.

### 7.9 Structural traps for the Rust port

1. `_normalize` runs **before** the `_VERIFIABLE` gate, so an exact string match returns
   `True` for `proof` and `multi-step` too (`sympy_check.py:379-382`).
2. `_normalize` casefolds; `to_sympy_source` does not. `I` vs `i`, `E` vs `e` differ on the
   SymPy rung but not on the string rung.
3. `casefold()`, not `lower()` — `ß` → `ss`, Turkish dotted I, etc.
4. `rstrip(".")` removes *all* trailing periods; it never removes a significant digit
   because it operates only on the tail.
5. The comma/space thousands rewrite is a `fullmatch` on the whole string. Any surrounding
   structure disables it.
6. `°` is deleted, not converted (`sympy_check.py:142`).
7. `_MAX_EXPONENT = 1_000`, `_MAX_SOURCE_CHARS = 4_000`, `_POW_TOWER_RE` — these bound the
   *source string that `parse_expr` evaluates*, checked inside `_parse` on exactly that
   string (`sympy_check.py:241-244` explains why this is the one load-bearing invariant).
8. `answers_equivalent` never raises. Every failure degrades to `False`
   (`sympy_check.py:6-9`).
9. Answer body cap at the API boundary: 4,000 chars; `work` 20,000 (`api.py:91-92, 1292`).
10. There is no timeout on `simplify`. A 276 ms case is reachable from a 12-character answer.

---

## 8. A proposed decidable grammar for 2.0 (V1)

Design rules taken from D6 ("arbitrary-precision rationals plus a small expression AST.
No floats in any equality decision") and V1 ("parsing + canonical normalization + exact
arithmetic, not heuristic simplification").

### 8.1 The grammar

```
answer      := value | tuple | set | list
value       := rational | radical_expr | poly_expr
rational    := integer | decimal | fraction | mixed
integer     := SIGN? DIGIT+                       (arbitrary precision)
decimal     := SIGN? DIGIT* '.' DIGIT+            (exact: mantissa / 10^k)
fraction    := integer '/' integer                (b != 0; normalized by gcd, sign on numerator)
mixed       := integer WS integer '/' integer     (a b/c = sign(a)*(|a| + b/c))
radical_expr:= rational_lincomb over { 1, sqrt(n), pi, e }   (n a squarefree positive integer)
poly_expr   := sum of terms over variables in [a-zA-Z] (+ Greek), each term
               rational_coeff * PROD var^int_exp * PROD fn(poly_expr)
fn          := sqrt | sin | cos | tan | sec | csc | cot | asin | acos | atan
             | sinh | cosh | tanh | exp | ln | log | abs
tuple       := '(' value (',' value)+ ')'         (ordered; also accepted unparenthesized)
set         := '{' value (',' value)* '}'         (unordered, deduplicated)
list        := '[' value (',' value)* ']'         (ordered)
```

**Canonical form.** Rationals as reduced `BigRational`. Decimals converted exactly
(`0.7` → `7/10`), never to a float. Polynomial expressions as a sorted multivariate sparse
representation with rational coefficients; equality is coefficient-wise. Radicals as a
sorted map `{basis → rational}` after squarefree factoring of every `sqrt(n)`
(`sqrt(8)` → `2*sqrt(2)`). Function applications compared structurally on canonicalized
arguments (so `sin(x)^2 + cos(x)^2` is **not** equal to `1` — an intentional, decidable
narrowing of 1.0).

**Learner-notation tolerance to carry over (V4)** — normalize before parsing:
strip one outer `$…$`; `rstrip('.')`; collapse `\s+`; delete comma and space thousands
groups on a `fullmatch`; `^` → `**`; `\cdot`/`\times`/`·`/`×` → `*`; `÷` → `/`;
`\left`/`\right` deleted; the `_UNICODE_SIMPLE` table (`sympy_check.py:138-143`) plus
superscripts and `√`; **and, new in 2.0**: `\frac{a}{b}` → `(a)/(b)`, `\sqrt{a}` → `sqrt(a)`,
`^{n}` → `**(n)`, a trailing `%` → `/100`, a leading `x =` / `y =` prefix stripped, a
trailing unit token stripped and compared separately. The dot-thousands variant stays
last-resort with the `notation` tag, exactly as `dot_thousands_variant` does.

**The literal-fraction token (2.0, review round 2).** A vulgar-fraction glyph does **not**
become `(1/2)` in 2.0, and a `\frac{b}{c}` of two plain digit runs does not become
`((b)/(c))`. Both become one **literal-fraction token**, written `⟦b/c⟧` with the marks
U+27E6 and U+27E7, and the lexer reads that spelling as one `Tok::Frac`. So `½` is
`⟦1/2⟧` and `\frac{1}{2}` is `⟦1/2⟧`. The two marks are on no learner keyboard and no
other rewrite of `answer::normalize` produces them, so the token never comes from the
learner's own text.

The token exists because the mixed-number rule needs one place, and that place is the
parser. `parse::read_mixed_number` sees the one shape `Num [space] fraction` for all five
spellings — `2 1/2`, `2½`, `2 ½`, `2\frac{1}{2}`, and `2 \frac{1}{2}` — and it applies the
`0 < b < c` and plain-digit rules once. All five give 5/2, and none of them gives the
product 1. The learner's own product keeps its own reading: `2(1/2)` is 1, because the
learner wrote brackets and not the token. A token that no number precedes makes a product,
so `x½` is `x/2` and `(2)½` is 1. `\frac{a}{b}` with anything but two plain digit runs
keeps the `((a)/(b))` rewrite of the row above.

### 8.2 Coverage against the corpus (3,492 answers)

| Production | shape bucket | n | % | cumulative % |
|---|---|---:|---:|---:|
| `integer` | integer | 1622 | 46.4 | 46.4 |
| `poly_expr` | expression_symbolic | 686 | 19.6 | 66.1 |
| `fraction` | fraction | 352 | 10.1 | 76.1 |
| `radical_expr` | expression_numeric | 233 | 6.7 | 82.8 |
| `tuple` | ordered_tuple | 178 | 5.1 | 87.9 |
| `decimal` | decimal | 128 | 3.7 | 91.6 |
| `tuple` (unparenthesized) | comma_list | 43 | 1.2 | 92.8 |
| `mixed` | mixed_number | 8 | 0.2 | 93.0 |
| `set` (LaTeX-escaped) | other | 7 | 0.2 | 93.2 |
| `set` / `list` | set_or_list | 5 | 0.1 | **93.4** |

**93.4% of the corpus (3,262 of 3,492 answers) is inside the proposed grammar.**
384 of the 478 topics (80.3%) have every answer inside it.

> **Measured, 2026-08-27 (M2 U1 plus the review round 1 fixes).** The built grammar
> accepts **3,227 of the 3,492 answers (92.41%)** and refuses **265 (7.59%)**. 362 of the
> 478 topics have every answer inside it. The estimate above is 3,262; the built number is
> 35 lower. The estimate counted whole shape buckets, and the parser decides one answer at
> a time. The measured per-bucket split is the literal table `SHAPE_COUNTS` of
> `crates/core/tests/answer_parse.rs`:
>
> | shape bucket | parsed | refused |
> |---|---:|---:|
> | `integer` | 1622 | 0 |
> | `expression_symbolic` | 632 | 54 |
> | `fraction` | 350 | 2 |
> | `expression_numeric` | 228 | 5 |
> | `ordered_tuple` | 178 | 0 |
> | `decimal` | 128 | 0 |
> | `interval_ineq` | 29 | 5 |
> | `comma_list` | 28 | 15 |
> | `value_with_unit` | 11 | 1 |
> | `mixed_number` | 8 | 0 |
> | `other` | 7 | 0 |
> | `set_or_list` | 5 | 0 |
> | `equation` | 1 | 0 |
> | `prose_or_words` | 0 | 167 |
> | `quotient_remainder` | 0 | 16 |
>
> Three rules of `docs/plans/M2.md` and of review round 1 move the number away from the
> estimate. The **interval production** gives 29 `interval_ineq` rows to the grammar. The
> **multi-letter split** gives 11 of the 12 `value_with_unit` rows and 10
> `expression_symbolic` rows. The **three-digit numerator rule** of finding #7 narrows the
> mixed-number production — `1 000/3` is undecidable — and it costs no corpus row.
> `docs/reference/undecidable-answers.md` holds the 265 refusals, group by group.

Within the 919 expression rows, 882 (96.0%) use only the whitelisted function set. The 37
outliers are: `log_b(x)` / `log_2(x)` / `log_3(x)` pseudo-functions (a subscripted base — 14
rows), `dy/dx` derivative notation (2), `n!` (1), `3x^2 dx` (1), `sin^2 θ` / `sec θ` / `tan θ`
(3, all fine once `θ` maps to a variable), sample-space sets of labels
(`$\{HH, HT, TH, TT\}$`, 4), `50th`, `left`, `right` (3).

### 8.3 What is left out (V2 — these become non-deterministic)

| Excluded shape | n | % | Why | Recommended 2.0 action |
|---|---:|---:|---|---|
| `prose_or_words` | 167 | 4.8 | Not a mathematical value. `yes`, `no`, `diverges`, `all real numbers`, `DNE`, `undefined`, `perpendicular`, `18 degrees Celsius` | Re-kind the topic to `multi-step`, **or** add a fifth answer kind `choice` with an authored closed option set — decidable by set membership after casefold, and it would cover ~120 of the 167 |
| `interval_ineq` | 34 | 1.0 | `-1 ≤ x ≤ 3` is a predicate, not a value | Add an `interval` production (`[a,b]`, `(a,b]`, `x <= a`) with exact endpoint comparison — cheap, decidable, and would recover 4 of the 7 exempted topics |
| `quotient_remainder` | 16 | 0.5 | `9 R2`, `x + 2 remainder 3` — a pair with domain-specific spelling | Author as a `tuple` (quotient, remainder), or add a `q R r` production |
| `value_with_unit` | 12 | 0.3 | `5 m/s`, `7 L/min` — the unit is part of the answer | Add `value unit` with a unit token compared as an opaque casefolded string |
| `equation` | 1 | 0.0 | `y = x` | Re-kind, or add `lhs = rhs` compared as `canon(lhs - rhs) == 0` up to a nonzero rational scale |
| **total excluded** | **230** | **6.6** | | |

> **Measured, 2026-08-27.** The built grammar refuses **265** answers, not 230. The five
> buckets above give only **189** of them: `prose_or_words` 167, `quotient_remainder` 16,
> `interval_ineq` 5 of 34, `value_with_unit` 1 of 12, and `equation` 0 of 1. The interval
> production, the multi-letter split, and the value label recover the other 41 rows of
> these buckets. The remaining **76** refusals sit in buckets this table counted as
> covered: `expression_symbolic` 54 (a rational or symbolic exponent, a subscripted
> logarithm base, a subscripted variable, a factorial, an approximation marker, a label
> set), `comma_list` 15, `expression_numeric` 5, and `fraction` 2. 189 + 76 = 265.
>
> Two answers the multi-letter split recovered are a poor outcome, not a good one:
> `60 km/h` reads as `60*k*m/h` and `2π cm^2` reads as `2*pi*c*m**2`, so `2π cm^2` equals
> `2π m^2c`. `50th` reads as `50*t*h`, so the learner answer `50` is marked wrong.
> `docs/reference/undecidable-answers.md` section 3.18 lists the three and asks for a
> re-kind or a `value unit` production.

**Recommendation for the M2 spec:** implement the §8.1 grammar plus the `interval`
production and a `choice` answer kind. That lifts coverage from 93.4% to roughly 98%, and
A2/V2 then rejects the residue at authoring time rather than letting it claim a
deterministic verdict it cannot support — which is precisely the 1.0 defect in §7.6.

---

## 9. A proposed fuzz oracle (V3)

### 9.1 The exact call

```python
import sys
sys.path.insert(0, "/home/deploy/dev/cadus")
from cadus.model import AnswerKind
from cadus_web.sympy_check import answers_equivalent

verdict: bool = answers_equivalent(expected, learner, AnswerKind.numeric)
#                                                     or AnswerKind.expression
```

- Interpreter: `/home/deploy/dev/cadus/.venv/bin/python` (uv is not installed on this box).
- Return value: a plain Python `bool`. Never `None`, never an exception — every internal
  failure degrades to `False` (`sympy_check.py:6-9`, `:342-343`, `:363-364`).
- The third argument must be `AnswerKind.numeric` or `AnswerKind.expression`. Any other
  value makes rung 2 return `False` for everything except an exact normalized string match,
  which would make the oracle meaningless.
- The oracle is `answers_equivalent`, **not** `deterministic_grade`: `deterministic_grade`
  returns `None` for a wrong answer (§4), so it cannot express the `False` half of the
  contract that 2.0's A3 requires.
- For the notation half of V4, also call
  `cadus_web.sympy_check.dot_thousands_variant(expected, learner, kind) -> bool`; the Rust
  checker must produce the `notation` tag exactly where this returns `True`.

Recommended harness shape (a long-lived subprocess, one JSON line in / one out, so the
~1.4 s SymPy import is paid once):

```python
for line in sys.stdin:
    req = json.loads(line)
    kind = AnswerKind(req["kind"])
    print(json.dumps({
        "equivalent": answers_equivalent(req["expected"], req["learner"], kind),
        "notation":   dot_thousands_variant(req["expected"], req["learner"], kind),
    }), flush=True)
```

Wrap each call with a wall-clock guard: `simplify` is unbounded (§3.2), and a fuzzer will
find `(x+1)**200`-shaped inputs. Treat a timeout as "oracle has no opinion" and skip the
pair rather than recording a divergence.

### 9.2 The seed corpus

`answer_corpus.jsonl` — 3,492 rows, `answer` as the `expected` side and `answer_kind` as the
kind. Skip the 9 rows with `"parsed": false` (the oracle returns `False` for them against
everything, including themselves) or record them as a separate known-divergence class.

### 9.3 Learner-notation variant generators

For each corpus answer `A`, generate learner strings that a real learner would type. Each
generator carries its **expected oracle verdict**, so a divergence is attributable.

**Should be `True` (V4 tolerance):**

| Generator | Example (`A = 7329` / `1/2` / `2*x+1`) | Applies to |
|---|---|---|
| whitespace padding | `"  7329  "` | all |
| internal space collapse | `1 / 2`, `1 + 2 x` | all |
| trailing periods | `7329.`, `0.5.` | all |
| `$…$` wrapping | `$7329$` | all |
| case flip | `Yes`, `SQRT(2)` (string rung only) | all |
| comma thousands | `7,329` | integer, \|v\| ≥ 1000 |
| space thousands, each of ` `, `\u00a0`, `\u202f`, `\u2009`, `\u2007` | `7 329` | integer, \|v\| ≥ 1000 |
| equivalent fraction `k·a/k·b`, k ∈ 2..9 | `2/4`, `3/6` | fraction |
| fraction ↔ terminating decimal | `1/2` ↔ `0.5` | rational |
| trailing zeros | `0.50`, `12.0` | decimal, integer |
| decimal to ≥ 8 significant digits | `0.3333333333` for `1/3` | rational, irrational |
| `^` for `**` | `x^2` | poly |
| Unicode ↔ ASCII, per `_UNICODE_SIMPLE` | `15√3` ↔ `15*sqrt(3)`, `π` ↔ `pi`, `∞` ↔ `oo`, `−` ↔ `-`, `·` ↔ `*`, `½` ↔ `1/2`, `x²` ↔ `x**2`, `30°` ↔ `30` | all |
| explicit ↔ implicit multiplication | `2x` ↔ `2*x` | poly |
| commutative reorder of a sum / product | `1 + 2*x` for `2*x+1` | poly |
| algebraic refactor | `(x-1)*(x+1)` for `x**2-1` | poly |
| tuple space removal | `(4,17)` for `(4, 17)` | tuple |
| set reorder | `{2,1}` for `{1, 2}` | set |

**Should be `True` *and* `dot_thousands_variant == True` (the notation tag):**
`7.329` for `7329` — only when the expected value is an integer ≥ 1000 whose decimal
grouping has a non-zero leading group of 1..3 digits.

**Should be `False` (the false-positive hunt — this half matters more, per C4):**

| Generator | Example | Note |
|---|---|---|
| off-by-one / last-digit perturbation | `7330`, `7239` | |
| sign flip | `-7329` | |
| digit transposition | `73,29`, `7239` | pinned False in 1.0 |
| ×1000 and ÷1000 | `0.008` vs `8`, `7329000` | the §7.2 hole |
| coarse decimal, 3 significant digits | `0.667` for `2/3` | pinned False |
| tuple element swap | `(17, 4)` for `(4, 17)` | ordered |
| set element change | `{1,3}` for `{1,2}` | |
| wrong radicand | `15*sqrt(2)` for `15√3` | |
| wrong exponent | `x**3+1` for `x**2+1` | |
| **word anagram** | `sey` for `yes`, `neve` for `even` | **1.0 returns True — a known oracle defect; assert the divergence rather than parity** |
| appended junk | `7329x` | |

**Divergence policy.** Because the oracle has the defects catalogued in §7.6-§7.8, a
Rust-vs-SymPy mismatch is not automatically a Rust bug. Partition every divergence:

1. Learner string is outside the §8.1 grammar → 2.0 must refuse a deterministic verdict
   (V2). Not a parity failure; assert "Rust says undecidable".
2. Expected string is prose (`shape == "prose_or_words"`) → the topic is mis-kinded. Not a
   parity failure; feed the list to the V2 authoring gate.
3. Both sides inside the grammar and the verdicts differ → **a real parity bug**, and per
   R5 "Divergence is a bug in 2.0 until proven otherwise".
4. Rust says `False` where SymPy says `True` because SymPy used `simplify` on a
   transcendental identity (`sin(x)^2+cos(x)^2` vs `1`) → an intentional, documented
   narrowing. Pin it as a known-divergence fixture, not a bug.

The gate for M2: over the 3,262 in-grammar corpus answers × the True-generator set, the
Rust checker and `answers_equivalent` must agree on 100% of pairs in classes 3 and 4, and
the class-1/class-2 residue must equal the 230 answers enumerated in §8.3.

---

## 10. `work_quality` vs `correct` (C4)

The separation is structural, not conventional.

- `correct` is decided by `answers_equivalent` alone on a verifiable kind and is made final
  over the model by `apply_sympy_override` (`prompts.py:1286-1299`, `:1353`). No tier, no
  timing and no work text can move it.
- `work_quality` is a six-tier enum judged on "the method and completeness of the shown
  work" (`docs/DATA_MODEL.md:154-156`). Tier values (`cadus/fire.py:41-46`, `config.yaml:61-67`):

| tier | FIRe `q` | XP multiplier |
|---|---:|---:|
| `perfect` | 1.0 | 1.3 |
| `nearly_perfect` | 0.85 | 1.0 |
| `passable` | 0.7 | 0.85 |
| `nearly_passable` | 0.4 | 0.3 |
| `poor` | 0.15 | 0.0 |
| `blowoff` | 0.0 | −0.5 |

`PASS_QUALITY_THRESHOLD = 0.7` (`cadus/fire.py:51`).

- The deterministic path awards exactly `nearly_perfect` (`deterministic_grade.py:72`) and
  the reasoning is spelled out at `:35-52`: it is the **neutral** tier; `perfect` is a bonus
  for method this path never reads; every lower tier is a penalty for a flaw this path never
  observes. Rushing is excluded because `cadus.xp.is_rushing` already owns it and fires only
  on a wrong answer. Timing tags are excluded because `api._measure_secs` already produces
  them.
- A correct verdict carries **no** error tag, except `notation`
  (`deterministic_grade.py:146-149`): "not a claim about the mathematics but about how it was
  written, and which the symbolic checker observed directly".
- Partial credit therefore lives entirely in `work_quality`, and a `regraded` event may
  supersede `work_quality` / `error_tags` / `grader_note` but **never** `correct`
  (`docs/DATA_MODEL.md:221-223`).

For 2.0 this maps cleanly: the M2 checker returns `correct: bool` and, when the
dot-thousands reading fired, `notation: bool`. It must return **no** `work_quality` at all —
the caller supplies the neutral tier, and A4's async diagnosis owns `error_tags` and prose.