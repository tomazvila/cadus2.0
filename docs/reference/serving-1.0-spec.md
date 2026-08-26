Source: survey of /home/deploy/dev/cadus on 2026-08-27 (read-only). Line citations point at 1.0 files.

# Cadus 2.0 — M4 port specification (surveyed from 1.0)

Source: read-only survey of `/home/deploy/dev/cadus` on 2026-08-26. Nothing was edited. No 1.0 test suite ran. Two read-only snippets imported the 1.0 code; their output is quoted verbatim below. No database was contacted.

Requirement IDs: A1, A5, A6, A7, D5, D-S4, D-S5, D-S6, D-O1, D-O4, L1, L2, C6, T1.

---

## 1. Where things live (1.0, file:line)

| Subject | Location |
|---|---|
| Template module (1,545 lines) | `cadus_web/problem_templates.py` |
| Module docstring: the whole design argument, the six gate checks, the limits | `problem_templates.py:1-96` |
| **The "200 problems in 86 ms" claim** | `problem_templates.py:69` and `cadus_web/config.py:85` |
| Constants (`TEMPLATE_OP`…`TEMPLATABLE_KINDS`) | `problem_templates.py:137-221` |
| Placeholder regex, stray-brace scanner | `problem_templates.py:230-259` |
| `IntDomain`, `ChoiceDomain`, `ProblemTemplate`, `Instantiated` | `problem_templates.py:271-325` |
| `_render`, `_sample`, `solve` | `problem_templates.py:328-350` |
| `_candidate_bindings`, `instantiate`, `_build` | `problem_templates.py:353-474` |
| `problem_hash = problem_text_hash` (the aliased core function) | `problem_templates.py:469-476` |
| Gate: `_domain_from_payload` … `_check_samples` | `problem_templates.py:484-1018` |
| `gate_template` (the entry point) | `problem_templates.py:839-940` |
| Stored form: `template_digest`, `is_approved`, `template_payload`, `template_from_payload`, `template_key` | `problem_templates.py:1021-1162` |
| Store seam (`tutor_cache`, `op='template'`) | `problem_templates.py:1165-1253` |
| `TemplatedTutorEngine` (serve path, bank, authoring) | `problem_templates.py:1262-1470` |
| FAKE engine exemplar rotation | `cadus_web/engine.py:433-442` |
| `ProblemSpec` (the serve input) | `cadus_web/engine.py:202-231` |
| Serve route `POST /api/task/{task_id}/serve` | `cadus_web/api.py:1014-1060` |
| `_problem_spec` (builds the spec) | `cadus_web/api.py:350-396` |
| `_serve_one` (generate + write state) | `cadus_web/api.py:423-498` |
| `_serve_payload` (client-safe shape) | `cadus_web/api.py:501-527` |
| `_serve_live` (reuse the live problem) | `cadus_web/api.py:530-565` |
| `Pregenerated` / `_take_pregenerated` / `_pregenerate` | `cadus_web/api.py:308-347` |
| `WebState`, `ServedProblem`, `SERVED_TEXT_MEMORY = 12` | `cadus_web/state.py:129-175` |
| Web-state advisory lock + `web_states` upsert | `cadus_web/webstate.py:317-426` |
| `problem_text_hash` (the one definition) | `cadus/projector.py:108-118` |
| `LAST_PROBLEMS_WINDOW = 20` | `cadus/projector.py:92` |
| `_on_attempt` (the window fold) | `cadus/projector.py:213-224` |
| `TopicState.last_problems` | `cadus/model.py:578` |
| `recent_problem_hashes` fill | `cadus/selector.py:961`, `1088`, `1138` |
| `safe_eval_number` | `cadus_web/sympy_check.py:277-314` |
| Exponent/length bound (`_MAX_EXPONENT = 1000`, `_MAX_SOURCE_CHARS = 4000`) | `cadus_web/sympy_check.py:204-228` |
| `TEMPLATE_SCHEMA` / `TEMPLATE_CONTRACT` (`emit_template`) | `cadus_web/prompts.py:314-392` |
| `TEMPLATE_SYSTEM` (authoring prompt) | `cadus_web/prompts.py:441-494` |
| `template_user` (user message) + `template_prompt_digest` | `cadus_web/prompts.py:735-798` |
| Review CLI (`list` / `show` / `approve` / `reject`) | `scripts/review_templates.py:1-237` |
| Prometheus counter `cadus_problem_template_total` | `cadus_web/metrics.py:143-148` |
| Docs: "Problem templates" section, gate table, Hard Rule 4 | `docs/WEB_SERVICE.md:393-525` |
| Docs: the five Hard Rules | `docs/WEB_SERVICE.md:22-32` |
| Docs: randomized-parameter deviation | `docs/PEDAGOGY.md:329-332` |
| Tests (1,553 lines) | `tests/test_problem_templates.py` |
| Serve avoid-list test | `tests/test_web_api.py:665-721` |

**The quote (A1's evidence), verbatim.** `problem_templates.py:68-69`:

> "The saving when it lands is real and large (200 problems in 86 ms, no API calls), and every failure path is the pre-existing behaviour…"

and `config.py:84-85`:

> "When it lands, a knowledge point becomes free forever: measured, 200 problems in 86 ms with zero API calls."

I re-measured the same path on this box (`instantiate` on the `a**2`, `a` in 1..12 template, 200 calls after one warm-up): **total 27.6 ms, mean 0.138 ms, p95 0.171 ms, max 0.434 ms**. The 86 ms figure is the honest upper end; the path is far under L1 in Python already.

---

## 2. The template document shape

### 2.1 What 1.0 stores (`tutor_cache.payload`, `op='template'`)

`template_payload` (`problem_templates.py:1071-1095`) writes exactly these keys:

| Field | Type | Example | Notes |
|---|---|---|---|
| `v` | int | `2` | `TEMPLATE_VERSION` (`:151`). A bump retires every old row, because `template_key` hashes it (`:1133`). |
| `approved` | bool | `false` | C6. Default false (`:1071`). |
| `approved_digest` | str (16 hex) | `498eee5fb77c1db4` | Written only when approved (`:1093`). |
| `text` | str | `"Compute ${a}^{{2}}$."` | `{name}` placeholders; every literal brace doubled. |
| `answer_expr` | str | `"a**2"` | SymPy source over parameter names. |
| `solution_expr` | str \| null | `"${a} \\times {a}$ gives the answer."` | Prose; rendered, never evaluated (`:906`). |
| `params` | object | `{"a": {"kind":"int","low":1,"high":12}}` | One domain per parameter. |
| `space_size` | int | `12` | Filled by the gate, not by the model (`:309`). |

Domain forms (`_domain_from_payload`, `:484-511`): `{"kind":"int","low":i,"high":j}` (inclusive, `j >= i`, `j-i+1 <= 10000`) and `{"kind":"choice","values":[...]}` (non-empty, `<= 24` entries, strings or ints, never bools; coerced to strings).

A decline marker is a different document: `{"v": 2, "declined": "<reason truncated to 500 chars>"}` (`:1030-1033`).

The verified round trip, printed from the live code:

```
{"v": 2, "approved": false, "text": "Compute ${a}^{{2}}$.", "answer_expr": "a**2",
 "solution_expr": "${a} \\times {a}$ gives the answer.",
 "params": {"a": {"kind": "int", "low": 1, "high": 12}}, "space_size": 12}
digest: 498eee5fb77c1db4
```

`template_digest` (`:1040-1054`) hashes only `("v","text","answer_expr","solution_expr","params")` with `json.dumps(..., sort_keys=True, ensure_ascii=False)`, SHA-256, first 16 hex characters. It excludes `approved`, `approved_digest`, and `space_size`.

`is_approved` (`:1056-1069`) fails closed three ways: no `approved` key; `approved` truthy but not exactly `True`; `approved_digest` absent or not equal to the recomputed digest.

### 2.2 The authoring tool schema (what the model returns)

`prompts.TEMPLATE_SCHEMA` is a plain JSON-Schema dict, not a pydantic model (`prompts.py:314-383`). Dumped live:

- `type: object`, `additionalProperties: false`
- `required: ["text", "params", "answer_expr", "solution_expr", "samples"]`
- `samples`: array of `{params: object, expected: string}`, both required, `additionalProperties: false`.

`samples` never reaches storage. It is the verification oracle only (`:1063-1071` of the gate, `problem_templates.py:929-931`).

`template_prompt_digest()` = SHA-256 over `{"system": TEMPLATE_SYSTEM, "tool": "emit_template", "schema": TEMPLATE_SCHEMA}`, first 16 hex. Current value on this checkout: **`f322b85a40b9ac50`**.

### 2.3 Proposed 2.0 `content_store.body` for `kind = 'template'`

`content_store` is keyed by `digest` and carries `kp_id`, `kind`, `status` (`migrations/0005_content.sql:11-23`). The body holds the template document only — never the KP id, never approval state, because those live in columns.

```jsonc
{
  "v": 1,                                  // 2.0 template document version
  "topic_id": "perfect-squares",           // the topic whose exemplars it mirrors
  "answer_kind": "numeric",                // "numeric" | "expression" only (V2)
  "statement": "Compute ${a}^{{2}}$.",     // {name} holes, doubled literal braces
  "params": {                              // A1: per-parameter domains
    "a": { "kind": "int",  "low": 1, "high": 12 },
    "op": { "kind": "choice", "values": ["+", "-"] },
    "r":  { "kind": "rational", "num": {"low": 1, "high": 9},
            "den": {"low": 2, "high": 12} }        // NEW in 2.0 (see below)
  },
  "constraints": [                         // A1: inter-parameter constraints — NEW
    { "op": "gt",       "left": "a",  "right": "b" },
    { "op": "ne",       "left": "a",  "right": "b" },
    { "op": "divides",  "left": "b",  "right": "a" },
    { "op": "gt",  "left": {"add": ["a", "b"]}, "right": {"lit": 100} }
  ],
  "answer_expr": "a - b",                  // must stay inside the M2 grammar
  "solution_sketch": "Subtract ${b}$ from ${a}$, borrowing from the tens.",
  "hints": [                               // A1 hint ladder; L5, T1
    "Which column do you subtract first?",
    "The ones digit of ${a}$ is smaller than the ones digit of ${b}$. What does that force?",
    "Borrow one ten, then subtract the ones column."
  ],
  "distractors": [                         // A4: pre-authored diagnosis, optional
    { "answer": "b - a", "error_tag": "operand-swap",
      "note": "You subtracted the larger number from the smaller one." }
  ],
  "samples": [                             // authoring-time oracle; kept for audit
    { "params": {"a": 50, "b": 10}, "expected": "40" }
  ],
  "space_size": 1225                       // computed by the gate, not by the model
}
```

**The constraint language (A1 — the feature whose absence blocked templates in 1.0).** Keep it small and total. Every predicate decides in constant time on a bound tuple, so the gate and the instantiator run the same code.

```
constraint := { "op": CMP, "left": term, "right": term }
CMP        := "eq" | "ne" | "lt" | "le" | "gt" | "ge" | "divides" | "coprime" | "carries"
term       := "<param>"                     // the bound value, an exact rational
            | { "lit": <integer|decimal> }
            | { "add": [term, term, ...] }
            | { "sub": [term, term] }
            | { "mul": [term, term, ...] }
            | { "abs": term }
            | { "mod": [term, term] }        // right != 0
            | { "digit_sum": term }
```

`carries` is the one domain predicate 1.0 named and could not express (`problem_templates.py:59-66`, `docs/WEB_SERVICE.md:452`): "`a + b` must carry" and its partner "`a - b` must borrow". Define both on the decimal digit runs of two integers. Reject a template whose constraints reference a non-integer parameter under `divides`, `coprime`, `carries`, `mod`, or `digit_sum`.

**The rational domain** is new. 1.0 had `IntDomain` and `ChoiceDomain` only (`:271-292`). A KP about fractions needs a rational parameter, and a float never enters (D6): store numerator and denominator ranges, draw both, reduce by gcd.

**Decisions the orchestrator must make here:**
1. `space_size` under constraints is the count of *satisfying* tuples, not the product of the domain sizes. Compute it exactly when the product is at or under the exhaustive limit; estimate it by rejection sampling above that, and store both the estimate and the sample count.
2. Keep `samples` in the body or move them to an authoring-side table. Keeping them makes the digest cover the oracle, which is what a reviewer read. I recommend keeping them.

---

## 3. Instantiation

### 3.1 How parameters are drawn

`_sample` (`problem_templates.py:338-340`) is one `rng.choice(domain.values())` per parameter, over the fully materialized value list. `IntDomain.values()` builds `list(range(low, high+1))` (`:278-280`).

`_candidate_bindings` (`:353-375`) picks one of two strategies:

- **Space at or under `EXHAUSTIVE_SPACE_LIMIT = 4096`** (`:217`): build the full cartesian product over `sorted(params)`, `rng.shuffle` it, and yield every tuple once. Avoidance is then exact — if an unblocked instance exists, the walk reaches it.
- **Above 4096**: yield `RESAMPLE_ATTEMPTS = 24` independent draws (`:209`).

The reason for the split is stated at `:211-216`: with 12 values and 11 already served, 24 random draws miss the free value about 13% of the time.

### 3.2 The RNG and its seeding

1.0 uses `random.Random` (Mersenne Twister). The serve path constructs one per engine instance and reuses it: `TemplatedTutorEngine.__init__` takes `rng` and defaults to `random.Random()` — unseeded, so it takes OS entropy (`:1296`). `instantiate` accepts an `rng` and defaults the same way (`:394`). The gate uses `random.Random(0)` when the caller passes nothing (`:932`).

**2.0 decision.** Do not carry Mersenne Twister. Use one explicit, documented PRNG in `cadus-core` (`rand`'s `ChaCha8Rng` or a hand-rolled PCG-64) and seed it per pool-refill job from a recorded `u64`. Record the seed and the drawn bindings on the pool row. Two properties follow: a reviewer reproduces any served instance, and the L1 benchmark is deterministic (§10). Draw an integer with a rejection-free bounded method, never `value % range`.

### 3.3 How the statement is rendered

`_render` (`:328-336`) is `text.format_map(dict(bindings))`. Consequences that carry over:

- A missing key raises `KeyError`, so a hole never reaches a learner.
- Every literal brace must be doubled, because `format_map` reads a single brace as markup. LaTeX is full of braces: `$7^{{2}}$`, `$7{{,}}329$`.
- The placeholder grammar is deliberately narrow (`_PLACEHOLDER_RE`, `:230`): `\{([A-Za-z_][A-Za-z0-9_]*)\}`. No format spec, no conversion, no attribute access, no indexing.

`_build` re-checks the rendered text for a surviving placeholder (`:449`).

**2.0 decision.** Do not use a general template engine. Write a 40-line renderer over the same grammar: scan for `{{`/`}}` (emit one brace) and `{name}` (emit the bound value's canonical string); any other brace is an error. A value renders through its exact canonical form, never through a float formatter (§8).

### 3.4 How the answer is computed

1.0: `solve` (`:342-350`) calls `sympy_check.safe_eval_number(template.answer_expr, bindings)` (`sympy_check.py:277-314`). That function:

1. substitutes each binding **textually**, longest name first, each value wrapped in parentheses — `a**2` with `a = -3` would otherwise parse as `-3**2` (`sympy_check.py:297-305`);
2. uses a callable replacement in `re.sub`, because a `ChoiceDomain` value such as `\times` broke a string replacement with `re.PatternError` (`:299-303`);
3. parses in the builtins-free SymPy namespace with the exponent bound already applied (`_MAX_EXPONENT = 1000`, `_MAX_SOURCE_CHARS = 4000`, `sympy_check.py:204-228`);
4. calls `.simplify()` and returns `str(...)`;
5. raises `ValueError` when the result stringifies to nothing. It does **not** check finiteness — `sqrt(-4)` returns `"2*I"` and `1/0` returns `"zoo"` (`sympy_check.py:291-295`).

Live output forms from `solve`, measured:

```
a/b   {a:1,  b:3} -> '1/3'
a/b   {a:10, b:4} -> '5/2'
sqrt(a) {a:8}     -> '2*sqrt(2)'
Rational(a,b) {a:3,b:6} -> '1/2'
a*1.5 {a:2}       -> '3.00000000000000'      <-- the float trap, §8
```

**2.0 requirement.** No textual substitution, and no SymPy. Parse `answer_expr` **once at authoring time** into the M2 `Ast` (`crates/core/src/answer/parse.rs`), with parameter names as free atoms. Evaluate the AST against a binding map of exact rationals. Two gates follow, both at authoring time (V2):

1. `answer_expr` parses inside the M2 grammar (`docs/reference/checker-1.0-spec.md:726-751`). Anything else rejects the template.
2. Every instance's computed answer, rendered to its canonical string, round-trips through `answer::canonical_form` (`crates/core/src/answer/check.rs:168`) without an `Undecidable`. That is the property that makes the M5 grade path deterministic (L2, A3).

The 1.0 allowed-name list (`_ALLOWED_NAMES`, `:533-548`) is wider than the M2 grammar: it admits `Piecewise`, `Eq`/`Ne`/`Lt`/`Gt`/`Le`/`Ge`, `And`/`Or`/`Not`, `ITE`, `simplify`, `expand`, `factor`, `together`, `cancel`, `nsimplify`, `Max`, `Min`, `sign`, `binomial`, `factorial`, `gcd`, `lcm`, `floor`, `ceiling`. **These are not in the M2 grammar.** Two options, and the orchestrator must pick one:

- **(a)** Narrow the 2.0 answer expression to the M2 grammar plus a short arithmetic-function set (`abs`, `sqrt`, `gcd`, `lcm`, `floor`, `ceiling`, `min`, `max`, `factorial`, `binomial`), evaluated exactly on rationals and **erased before the answer string is produced**. The answer string then always lies in the grammar. A conditional answer becomes an instantiation-time branch expressed through the constraint language, not a `Piecewise` in the answer.
- **(b)** Keep a separate, wider *evaluation* grammar and require only that the *result* lies in the M2 grammar.

I recommend (a): one grammar, one parser, one canonicalizer. It costs the "which is larger" KP shape that 1.0 needed `Piecewise` for (`:539-545`); that shape is expressible with a constraint that forces `a > b`.

---

## 4. The verification gate — every check and its literal text

`gate_template(payload, spec, *, rng=None)` (`problem_templates.py:839-940`). Order is exact; the first failure raises `TemplateRejected` with the message quoted.

| # | Check | Literal threshold | Rejection message (verbatim) |
|---|---|---|---|
| 0 | Answer kind is `numeric` or `expression` | `TEMPLATABLE_KINDS` (`:221`) | `answer kind {kind} is not symbolically decidable` |
| 1 | `text` present and non-empty | — | `template text is missing or empty` |
| 2 | `answer_expr` present and non-empty | — | `answer_expr is missing or empty` |
| 3 | At least one parameter | — | `a template needs at least one parameter` |
| 4 | Parameter name is an identifier | — | `parameter name {name!r} is not an identifier` |
| 5 | Name does not shadow an allowed function | `_ALLOWED_NAMES` (`:533`) | `parameter name {name!r} collides with a SymPy function the answer expression may call` |
| 6 | For `expression` kind, name is not an unknown | `_FREE_SYMBOLS` = `{x,y,z,t,n,k,u,v,w,r,theta}` (`:553`) | `parameter name {name!r} collides with the unknown an expression answer is written in` |
| 7 | Int domain has integer ends, `high >= low` | — | `an int domain needs integer 'low' and 'high'` / `int domain {low}..{high} is empty` |
| 8 | Int domain size | `MAX_DOMAIN_SIZE = 10_000` (`:160`) | `int domain {low}..{high} exceeds MAX_DOMAIN_SIZE` |
| 9 | Choice domain non-empty, bounded, scalar | `MAX_CHOICES = 24` (`:188`) | `a choice domain needs a non-empty 'values' list` / `a choice domain of {n} exceeds MAX_CHOICES (24); every choice must appear in a worked sample, so use an int domain or split the template` / `choice values must be strings or integers` |
| 10 | Every `{placeholder}` is declared (text and sketch) | — | `text uses undeclared parameters {sorted}` |
| 11 | Every literal brace is doubled | — | `text has an unescaped brace at index {i} ({snippet!r}) — literal LaTeX braces must be doubled` |
| 12 | `solution_expr` is a string when present | — | `solution_expr must be a string when present` |
| 13 | No dead parameter (text ∪ sketch ∪ answer names) | — | `parameters {sorted} are declared but never used` |
| 14 | `answer_expr` names nothing outside params + allowed + (unknowns for `expression`) | — | `answer_expr references unknown names {sorted}` |
| 15 | **Distinct-problem floor** | `MIN_SPACE_SIZE = SERVED_TEXT_MEMORY = 12` (`:174`, `state.py:133`) | `the declared domains produce only {n} distinct problem(s); at least 12 are needed for randomized values and for avoidance of a recently-served problem to mean anything (Hard Rule 4)` |
| 16 | Samples present and non-empty | — | `a template needs worked samples to verify it` |
| 17 | Each sample is an object with `params` + scalar `expected` | — | `a sample was not an object` / `a sample needs 'params' and a scalar 'expected'` |
| 18 | Sample binds exactly the declared names | — | `sample binds {sorted}, template declares {sorted}` |
| 19 | **Sample agreement** — the check that matters | one disagreement rejects all | `answer_expr gives {computed!r} for {bindings} but the sample claims {claimed!r} — the expression does not compute the stated answer` |
| 19b | Evaluation failure on a sample | `_MAX_EXPONENT = 1000` | `answer_expr failed on sample {bindings}: {exc}` (the exponent case reads `exceeds the evaluation bound`) |
| 20 | Sample lies inside its own domain | — | `sample {i} binds {name}={value!r}, which its own domain cannot produce — a sample outside the domain verifies nothing` |
| 21 | **Every choice is exercised** | all choices | `no worked sample uses {name}={sorted!r} — every choice must appear in a sample, or the expression is unverified for it` |
| 22 | **Both ends of every int domain** | low and high | `no worked sample uses the {low\|high} end of {name} ({edge}) — the edges are where an expression stops being right` |
| 23 | **One crossed corner per int pair** | skipped when either axis is single-valued | `no worked sample crosses {l} and {r} — one of them at its low end WITH the other at its high end ({l}={low_l} with {r}={high_r}, or {l}={high_l} with {r}={low_r}). Matching corners are exactly where a swapped-operand expression looks right` |
| 24 | Every instance renders and solves | exhaustive at or under `EXHAUSTIVE_SPACE_LIMIT = 4096`, else `GATE_SAMPLES = 200` draws (`:183`, `:217`) | `instantiation failed for {bindings}: {exc}` / `a rendered problem still contains a placeholder` / `instantiation for {bindings} produced no answer` |
| 25 | The answer is an answer | `_NON_ANSWERS = {I, zoo, oo, -oo, nan, AccumBounds, True, False, true, false}` (`:203`) | `instance {bindings} answers {answer!r}, which is not a number ({sorted})` |
| 26 | A `numeric` answer keeps no free symbol | — | `numeric answer {answer!r} for {bindings} still contains {sorted} — a parameter is undeclared` |
| 27 | **Exemplar envelope — sign** | derived from `spec.exemplars` | `instance {bindings} answers {answer!r}, but every authored answer for this knowledge point is non-negative — narrow the domains so no instance goes below zero` |
| 28 | **Exemplar envelope — integrality** | derived from `spec.exemplars` | `instance {bindings} answers {answer!r}, but every authored answer for this knowledge point is a whole number` |

**Four rejections, run live against the 1.0 code on this box:**

```
inter-parameter constraint (a>b) unexpressible:
  instance {'a': 59, 'b': 63} answers '-4', but every authored answer for this
  knowledge point is non-negative — narrow the domains so no instance goes below zero

no crossed corner:
  no worked sample crosses a and b — one of them at its low end WITH the other at its
  high end (a=2 with b=4, or a=5 with b=2). Matching corners are exactly where a
  swapped-operand expression looks right

missing int edge:
  no worked sample uses the low end of a (1) — the edges are where an expression stops
  being right

space too small:
  the declared domains produce only 5 distinct problem(s); at least 12 are needed for
  randomized values and for avoidance of a recently-served problem to mean anything
  (Hard Rule 4)
```

The envelope itself (`_exemplar_envelope`, `:726-764`): parse every exemplar answer through `to_sympy_source` then `float()`. If one fails, return `{}` — no envelope, no rule. Otherwise return `{"non_negative": all(v >= 0), "integral": all(v == int(v))}`.

**`_build` re-runs the identical check set at serve time** (`:427-467`). The comment at `:432-437` records why: the envelope once ran only in the gate, and a 10,000-instance subtraction template was accepted on 22 of 60 seeds with 55 negative instances.

**What the gate cannot do, stated as a proof (`:606-630`).** Every point the coverage rules force lies on a domain boundary, so `a*b + (a-1)(12-a)(b-1)(12-b)` agrees with `a*b` on all of them and is wrong on 69% of the interior. For **any** finite forced set S, `truth + (boundary factors)·Π_{s∈S}((a−s_a)² + (b−s_b)²)` agrees on S and is wrong nearly everywhere else. Seven forced points were honored while 97 of 144 instances stayed wrong. This is why C6 exists, and why the rejection-message quality is the yield lever (A2).

**2.0 additions to the gate.**
- Reject an `answer_expr` outside the M2 grammar (V2).
- Reject an instance whose canonical answer is `Undecidable` under `answer::canonical_form`.
- Check every constraint on every sample: a sample that violates a declared constraint is a rejection, and so is a constraint set with no satisfying tuple.
- Recompute `space_size` as the satisfying count.
- Require a hint ladder of at least one rung, and reject a rung that names the final answer (Hard Rule 1 / Hard Rule 3, `docs/WEB_SERVICE.md:22-32`).

---

## 5. Anti-repeat (A5, D5)

### 5.1 The hash

`problem_text_hash(text) = sha1(text.encode("utf-8")).hexdigest()[:12]` — `cadus/projector.py:108-118`. Twelve hex characters. **No whitespace normalization, no strip, no casefold.**

The docstring at `:111-117` is a warning, and 1.0 learned it the hard way: `problem_templates.py` briefly carried its own `sha256(text.strip())[:16]`. The two digests never compared equal, the avoidance was a silent no-op, and the covering test passed because it built its blocked list with the same wrong function. The fix is the alias at `problem_templates.py:476`: `problem_hash = problem_text_hash`. `tests/test_problem_templates.py:594-608` pins the equality against the core function on three inputs, including `" padded "`.

**2.0 decision.** Pick one spelling and put it in `cadus-core`, exported once. SHA-1 is a poor default in 2026; SHA-256 truncated to 16 hex is the better choice, and 2.0 has no historical rows to match unless O1 resolves to migrate. **If O1 migrates 1.0 events, the 2.0 hash must stay `sha1(utf8(text))[:12]`**, because `last_problems` carries 1.0 digests that a new spelling never matches. Flag this to the owner: it is an O1 consequence, not a free choice.

### 5.2 The window

`LAST_PROBLEMS_WINDOW = 20` (`projector.py:92`). The fold, `_on_attempt` (`projector.py:213-224`):

```
last_problems ← (last_problems ++ [problem_text_hash(attempt.problem.text)])[-20:]
```

It fires on `attempt`, per topic, and only when FIRe applies. `ServedProblem.text_hash` on the `task_served` event is **inert** — nothing computes or reads it (`cadus/model.py:228-235`).

### 5.3 The two avoid-lists, and what "recently served" means

1.0 keeps two, from different sources, both on `ProblemSpec` (`engine.py:212-231`):

| Field | Source | Scope | Carries |
|---|---|---|---|
| `recent_problem_hashes` | `TopicState.last_problems`, the 20-entry projector window (`selector.py:961`, `1138`; pooled across components at `1088`) | across sessions, per topic | 12-hex digests only |
| `recent_problem_texts` | `WebState.served_texts[task_id]`, capped at `SERVED_TEXT_MEMORY = 12`, oldest dropped first (`api.py:494-496`, `state.py:129-133`) | one task | verbatim statements |

`recent_problem_hashes` is fixed at plan time and reaches an LLM prompt as a bare count. `recent_problem_texts` exists because without it the user message for problem 2 of a task was byte-identical to problem 1's, and the model re-emitted the same problem — 5 generations from one spec returned 2 distinct problems, and the live log holds three duplicate `Compute $8 - 5$.` pairs (`tests/test_web_api.py:665-678`).

### 5.4 The redraw loop and its bound

`instantiate` (`problem_templates.py:377-425`):

1. Build the exemplar envelope once.
2. `avoid_texts = {t.strip() for t in spec.recent_problem_texts}` — **note the strip on the avoid side only**; the candidate is compared as `text.strip()`.
3. `avoid_hashes = set(spec.recent_problem_hashes)`.
4. For each candidate binding from `_candidate_bindings`: `_build` it (skip on `TemplateRejected`), remember it as `last`, skip if `text.strip()` is in `avoid_texts`, skip if `problem_hash(text)` is in `avoid_hashes`, else return it.
5. If nothing was ever built: count `no_valid_instance` and raise `TemplateRejected("no instance of this template produced a usable answer — it should not have passed the gate, and it must not be served")`.
6. If every candidate collided: count `resample_exhausted` and **serve the last one anyway**. `:388-392`: "A repeat is a far smaller failure than no problem, and the alternative is the model call this module exists to avoid."

The bound is therefore the candidate stream: the whole shuffled space at or under 4096, otherwise 24 draws.

### 5.5 The 2.0 shape (D5, D-S5)

Three layers, and they are not the same thing:

1. **`serving_pool.instance_hash`** with `UNIQUE (user_id, kp_id, instance_hash)` (`migrations/0005_content.sql:38-41`). This stops the same instance entering the pool twice. It is a *pool* invariant, enforced by the database.
2. **The D5 ring**, per `(user, topic)`, inside the D-S6 state row: a fixed-size ring of recent instance hashes plus a `HashSet` view. Size **20**, to match `LAST_PROBLEMS_WINDOW`.
3. **The per-task served-text memory**, size **12**, to match `SERVED_TEXT_MEMORY`. In 2.0 no model reads it, so keep hashes, not verbatim statements — that shrinks the D-S6 row and removes the `.strip()` asymmetry above.

`instance_hash` must be the hash of the **rendered statement**, not of the binding tuple. Two different templates for one KP produce the same statement often enough to matter, and the learner notices the statement.

**The redraw loop in 2.0 (D-O1 + D-O4).** Serve must not redraw. Serve pops, checks the ring, and pops again — bounded at a small constant (I propose **8** pops, then serve the last one and count `pool_exhausted`, mirroring 1.0's "a repeat beats no problem"). The *redraw* belongs in the worker (D-O4), where the bound is the candidate stream of §3.1 and a miss costs nothing on the request path.

---

## 6. Fallback (A6)

### 6.1 The FAKE-engine rotation rule

`FakeTutorEngine.generate` (`engine.py:436-442`), in full:

```python
pool = spec.exemplars or [_FALLBACK]
exemplar = pool[spec.index % len(pool)]
return Generated(text=exemplar.problem, expected=exemplar.answer,
                 solution_sketch=exemplar.solution_sketch)
```

`_FALLBACK = Exemplar(problem="Compute 1 + 1.", answer="2")` (`engine.py:422`).

`spec.index` is the 0-based position of the problem inside its task, derived from server-side web state, never from engine memory (`engine.py:206-209`). `_next_serve_index` (`api.py:398-407`) computes it: quiz and multi-step index by **answered** count, everything else by **served** count.

So the rotation is a modular walk over the KP's authored exemplars, restart-safe because the index lives in the state row. It has **no** anti-repeat of its own: with one exemplar it serves the same problem forever. `teach` uses `pool[0]` — the first, simplest exemplar (`engine.py:481-490`).

**2.0 (A6).** The exemplar source feeds the same pool with `source = 'exemplar'` (`0005_content.sql:34`). Two changes from 1.0:
- Instantiate the whole exemplar list into the pool at once, and let the D5 ring pick, instead of a bare `index % len`. That gives a KP with three exemplars a real 3-cycle rather than a per-task restart.
- A KP whose exemplar count is below the ring size cannot satisfy anti-repeat. Record that fact rather than hide it (§6.2).

### 6.2 The A6 operator dashboard — 1.0 has none

**1.0 has only a Prometheus counter.** `PROBLEM_TEMPLATE = Counter("cadus_problem_template_total", …, ["result"])` (`metrics.py:143-148`), scraped at `/metrics`. The label values, from every call site (`problem_templates.py:418, 423, 1300, 1326, 1348, 1371, 1380, 1401, 1411, 1430, 1432, 1442`):

`hit`, `hit_partial_bank`, `authored`, `authored_served`, `rejected`, `author_failed`, `fallback_generate`, `kind_not_templatable`, `awaiting_review`, `store_error`, `no_valid_instance`, `resample_exhausted`.

`docs/WEB_SERVICE.md:524-525` names the counter and says a rising `rejected` is a drifting authoring prompt. There is **no operator dashboard, no Grafana config, and no per-KP fallback view** anywhere in the 1.0 tree; the only other audit surface named is `SELECT payload FROM tutor_cache WHERE op = 'template'` (`docs/WEB_SERVICE.md:481`).

**2.0 must build the flag, not port it.** A6 asks for "flagged in an operator dashboard". The minimum that satisfies it: one read-only endpoint and one page listing, per KP, the source that served it, the approved-template count, the pool depth, and the last time the KP fell back to exemplars. The data is already in `content_store` and `serving_pool`; the query is a group-by. Keep the counter too.

---

## 7. The serve path end to end (D-O1)

### 7.1 What 1.0 does

`POST /api/task/{task_id}/serve` (`api.py:1014-1060`), in order:

1. Build the service context, open the session, find the task, get the graph, get the engine, get the per-user web-state store.
2. **Transaction 1** (holds the tenant's transaction-scoped advisory lock, `webstate.py:353-390`):
   - `store.load(session)` — one `SELECT doc FROM web_states WHERE user_id = %s` (`webstate.py:398-401`).
   - `_progress_for` creates the `TaskProgress` if absent.
   - 409 `task_complete` if `progress.done`.
   - If a problem is already live for this task: `_serve_live` re-stamps `started_at`, `store.save(state)`, return. **One transaction only.**
   - Otherwise compute `spec = _spec_for_next_serve(...)` and commit.
3. **Outside the lock:** `_pregenerate(engine, spec)` — the 7-12 s model call. Every failure is swallowed (`api.py:336-347`).
4. **Transaction 2:**
   - reload state, re-derive progress, re-check `done`;
   - `_serve_live` → `_serve_one`, which rebuilds the spec and takes the pregenerated problem **only if the two specs are equal** (`_take_pregenerated`, `api.py:328-333`);
   - write `state.served[task_id] = ServedProblem(problem_id=uuid4().hex, task_id, topic, kp, answer_kind, text, expected, solution_sketch, started_at, index)` (`api.py:473-491`) — **keyed by task, so a re-serve overwrites**;
   - append the statement to `state.served_texts[task_id]` and truncate to the newest 12 (`api.py:492-496`);
   - `progress.served += 1` (`api.py:497`);
   - `store.save(state)` — one upsert into `web_states` (`webstate.py:417-421`).
5. Return `_serve_payload`: `problem_id`, `index+1`, `total`, `text`, `kp`, `time_budget_secs`, `countdown`. **Never `expected`, never `solution_sketch`** (Hard Rule 1, `api.py:501-527`).

The two-transaction split exists because holding the tenant's advisory lock across a 7-12 s model call froze that user's second tab, their dashboard, and the very teach request the serve runs alongside (`api.py:1024-1031`).

### 7.2 What 2.0 must do

There is no model call, so **one transaction is correct**, and it must be one:

```
BEGIN;
  SELECT ... FROM learner_models WHERE user_id = $1;          -- D-S3, 1 read
  -- scheduler traverses the arena in memory (D1); no I/O
  SELECT id, problem, expected_answer, instance_hash, source, content_digest
    FROM serving_pool
   WHERE user_id = $1 AND kp_id = $2 AND claimed_at IS NULL
   ORDER BY created_at
   FOR UPDATE SKIP LOCKED
   LIMIT 8;                                                   -- D-O1 pop, D7
  -- reject instance_hash values in the D5 ring; take the first survivor
  UPDATE serving_pool SET claimed_at = now() WHERE id = $3;
  UPDATE web_state SET doc = $4 WHERE user_id = $1;            -- served problem + ring
COMMIT;
```

Reads: the learner model row, up to 8 pool rows. Writes: one pool claim, one state row. Zero content-store reads on the hot path — the template body is needed by the **worker** (D-O4), not by serve. The hint ladder is read on the hint path (D-O3, L5), not here.

**A pool miss must not generate.** A6 forbids a synchronous fallback. On an empty pool the handler serves an exemplar instantiated in-process (pure CPU, no model, no I/O beyond the arena) and enqueues a refill. Record that serve as `source = 'exemplar'` and raise the A6 flag.

`claimed_at` is the served marker (`0005_content.sql:38`). Decide whether a claimed row is deleted, or kept as the audit trail with a retention job. I recommend keeping it: it is the served-instance log A5 names, and the D5 ring is then a cache of it, not the only copy.

---

## 8. Parity traps and 2.0 decisions to make

1. **Hash spelling.** `sha1(utf8(text))[:12]`, no normalization (§5.1). Two spellings once made the whole guard a no-op *and the test still passed*. Export one function; forbid a second. Tie the decision to O1.
2. **The `.strip()` asymmetry.** `instantiate` strips both sides for the text compare (`:397`, `:411`) but the **hash** is computed on the unstripped text (`:413`), and `problem_text_hash` never strips. So a statement that differs only by leading whitespace is blocked by text and not by hash. Pick one rule in 2.0 and apply it to both.
3. **Float formatting.** `a*1.5` with `a = 2` yields the string `'3.00000000000000'` — SymPy's `Float` repr. A rendered statement or an expected answer carrying that string is a C4 hazard and an ugly problem. 2.0 forbids floats entirely (D6): a decimal literal in `answer_expr` is an exact rational, and the canonical output form is the M2 canonical string. Reject a template whose answer renders with a trailing zero run.
4. **RNG.** Mersenne Twister with OS entropy is not reproducible. 2.0 records a seed per refill batch and stores it on the pool row (§3.2).
5. **LaTeX in statements.** Every statement is KaTeX-subset LaTeX inside `$…$` (`prompts.py:317-325`, `docs/WEB_SERVICE.md` LaTeX-canonical rule). Two consequences: literal braces are doubled in the template and single in the output; and `repair_latex_escapes` is applied to a model's template fields because an under-escaped `$\times$` mangles every problem the structure ever produces (`docs/WEB_SERVICE.md:496-500`). 2.0 renders JSON, not a JSON-escaped LLM reply, so the escape repair belongs in the authoring pipeline (M6) only. Keep the doubled-brace rule; it is the renderer's grammar.
6. **Choice values that contain a backslash.** `\times` as a choice value broke `re.sub`'s replacement side in 1.0 (`sympy_check.py:299-303`; pinned by `tests/test_problem_templates.py:513`). 2.0 substitutes into an AST and renders through a scanner, so the class disappears — but pin a test with a backslash choice value anyway.
7. **Non-ASCII and locale.** 85 of 88 curriculum unit files hold non-ASCII, mostly `U+2014` (`docs/reference/curriculum-1.0-spec.md` §6). Hash the UTF-8 bytes of the NFC-unnormalized string, exactly as 1.0 does. Never call a locale-aware number formatter: the M2 grammar reads `7,329` and `7 329` as thousands groups and `7.329` as a decimal (American English), and `dot_thousands_variant` is the last rung with the `notation` tag. A rendered statement must not introduce a thousands separator the checker then has to undo.
8. **Digest scope.** 1.0's `template_digest` covers `("v","text","answer_expr","solution_expr","params")` and excludes `space_size`. 2.0's `content_store.digest` covers the **whole body**, so `space_size` and `samples` are inside it. That is stricter and correct: a reviewer read them. State it, because it changes when a re-approval is needed.
9. **`space_size` is not `MAX_DOMAIN_SIZE`-capped truthfully.** `_space_size` returns `MAX_DOMAIN_SIZE` as soon as the running product exceeds it (`:952-959`), so a stored `space_size` of 10,000 means "at least 10,000". `_candidate_bindings` compares it against 4,096, so the saturation never changes the branch — but do not read the stored number as a count.
10. **The bank is cross-tenant in 1.0, per-user in 2.0.** `tutor_cache` rows are content-addressed and shared by every tenant (`docs/WEB_SERVICE.md:518-520`); `serving_pool` is keyed by `user_id`. `content_store` stays shared. Anti-repeat is therefore per user in 2.0 and was per user in 1.0 too (the window lives on `TopicState`). No change in behavior, but the sharing boundary moved.
11. **`_check_off_diagonal` skips a single-valued axis** (`:695-696`). Under 2.0 constraints an axis is effectively single-valued more often. Re-derive the crossed-corner requirement from the *satisfying* tuples, not from the declared ends, or the check silently weakens.
12. **The envelope reads exemplar answers through `float()`** (`:751-757`) — a float in a correctness decision. 2.0 must read them through `answer::canonical_form` and decide sign and integrality on the exact rational.

---

## 9. Pinned literals from the 1.0 tests

From `tests/test_problem_templates.py` (1,553 lines) and `tests/test_web_api.py`. The header at `tests/test_problem_templates.py:1136-1140` records why these are literals: an evaluator mutated `BANK_TARGET` to 1 and `GATE_SAMPLES` to 1 and the file stayed green, because three tests derived their expectations from the constant.

| Literal | Value | Test |
|---|---|---|
| `BANK_TARGET` | `3` | `:1144-1147` |
| `GATE_SAMPLES` | `200` | `:1172-1177` |
| `EXHAUSTIVE_SPACE_LIMIT` | `4096` | `:1177` |
| `MIN_SPACE_SIZE` | `12` | `:1293` |
| `TEMPLATE_VERSION` | `2` | `:1442-1449` |
| `SERVED_TEXT_MEMORY` | `12` | `tests/test_web_api.py:717-721` |
| `LAST_PROBLEMS_WINDOW` | `20` | `cadus/projector.py:92` (no direct test; the spec pins it) |
| `space_size` of `a` in 1..12 | `12` | `:148` |
| `space_size` of `6*b`, `b` in 1..12 | `12` | `:1315` |
| Templating default | `False` | `:1036-1047` |
| Review default | on | `:1408-1413` |
| Sampled-branch space | `40_000` (200×200) | `:1180-1200` |
| Domain-too-large payload | `low 1, high 10**6` | `:169` |
| Exponent bomb | `a**99999` → `exceeds the evaluation bound` | `:174` |
| Blocked-set behavior | 11 of 12 blocked → always serves `Compute $12^{2}$.` over 10 seeds (texts) and 6 seeds (hashes) | `:583-618` |
| Fully blocked | still serves a problem | `:622-630` |
| Zero-model-call claim | 50 serves after a full approved bank, `author_calls` and `generate_calls` unchanged | `:721-745` |
| Instance range | `1 <= base <= 12` and `expected == str(base**2)` over 20 serves | `:747-760` |
| `problem_hash` identity | equals `cadus.projector.problem_text_hash` on `"Compute $7^{2}$."`, `"Which is larger, $7{,}239$ or $7{,}329$?"`, `" padded "` | `:594-608` |
| Unreadable stored rows | `{}`, `v: 999`, empty `params`, missing `answer_expr`, `kind: "??"` → all `None` | `:660-676` |
| Key invariance | `index`, `recent_problem_hashes`, `recent_problem_texts` do not change the key | `:678-696` |
| Key sensitivity | `topic_id`, `kp_id`, `constraints`, `difficulty_target`, `exemplars`, `answer_kind` each change it | `:684-701` |
| Approval truthiness | `1`, `"true"`, `[1]` all fail | `:1435-1440` |
| Serve avoid-list order | `[[], ["problem #1"], ["problem #1","problem #2"]]` | `tests/test_web_api.py:703-706` |
| Rejection substrings asserted | `does not compute the stated answer`, `undeclared parameters`, `unescaped brace`, `worked samples`, `at least one parameter`, `MAX_DOMAIN_SIZE`, `is empty`, `unknown names`, `template declares`, `exceeds the evaluation bound`, `non-negative`, `whole number`, `distinct problem` | `:150-196`, `:1049-1130`, `:1270-1292` |

**Mutation-check requirement (HANDOVER §3).** Every one of these must be a literal in the 2.0 test, never a value re-read from the constant under test.

---

## 10. Proposed L1 benchmark design for CI

**Target.** L1: serve a problem, p95 < 150 ms, server-side, single-tenant interactive load. L2: grade, p95 < 300 ms.

**The problem.** A GitHub `ubuntu-latest` runner is shared and noisy. A wall-clock p95 on it is a flaky gate, and HANDOVER §3 says a broken budget does not merge — so a flaky gate blocks good work. The fix is two benchmarks with a **documented budget split**, not one wall-clock number.

### 10.1 Benchmark A — core-only micro-benchmark (deterministic, hard gate)

- **What it measures.** Pure CPU, no I/O: render a statement from a template, evaluate the answer AST on the drawn bindings, canonicalize the answer, hash the statement, test the D5 ring. Plus the M2 check path for L2.
- **Where.** `crates/core/benches/` with `criterion`, and a **plain `#[test]` assertion harness** that CI runs — criterion's own statistics are for humans, the gate needs one number.
- **Determinism.** Seed the PRNG with a fixed `u64`. Fix the input set: a committed fixture of 20 templates spanning the shapes (1 param, 3 params, a choice domain, a constrained pair, a rational parameter, an `expression` kind). Run 2,000 iterations. Assert on **p95 of per-iteration nanoseconds**.
- **Budget.** 5 ms of the 150 ms. This is generous by two orders of magnitude — the 1.0 Python path already measures 0.171 ms p95 — so it never flakes, and it fails loudly if someone puts a search, an allocation storm, or a regex compile in the loop.
- **Also assert allocation-free-ish behavior** with a counting allocator: the render plus evaluate path must stay under a fixed allocation count. That catches the real regression (a `format!` in a hot loop) that a timing bound on a noisy runner never catches.

### 10.2 Benchmark B — store round-trip benchmark (soft gate, recorded)

- **What it measures.** The full D-O1 transaction against the CI Postgres service (`.github/workflows/ci.yml:35-53` already provides `postgres:16` on `127.0.0.1:5432` with `CADUS_TEST_DATABASE_URL`): load the learner model row, pop up to 8 pool rows with `FOR UPDATE SKIP LOCKED`, claim one, write the state row, commit.
- **Fixture.** One seeded user, one KP, 200 pool rows, a D5 ring at full 20 entries with a deliberate 3-hash overlap so the redraw loop runs. Warm the connection pool with 50 untimed transactions first.
- **Sample.** 500 timed transactions, single connection, no concurrency. Report p50, p95, p99.
- **Budget.** 100 ms of the 150 ms for the DB round trip, leaving 45 ms of headroom for framework overhead and the arena traversal.
- **Gate policy.** Fail the build at **p95 > 100 ms**. Do **not** fail on p50 or on a single slow sample. Write the numbers to a job artifact every run so a trend is visible; the review cycle reads the trend, the gate reads only p95.

### 10.3 The budget split, written down

| Segment | Budget | Measured by |
|---|---|---|
| Arena traversal + scheduler decision (in-memory, D1/D3) | 5 ms | Benchmark A |
| Template render + answer evaluation + canonicalization + hash + ring | 5 ms | Benchmark A |
| Postgres: model read, pool pop, claim, state write, commit | 100 ms | Benchmark B |
| axum/tokio/serde overhead, session lookup, RLS `SET LOCAL` | 40 ms | unmeasured headroom |
| **Total** | **150 ms (L1)** | |

Put this table in `docs/reference/l1-budget.md` and cite it from both benchmarks' comments. A future change that needs more than its segment must move the split explicitly, in a reviewed commit — that is the mechanism that stops "fix it later".

### 10.4 L2 in the same harness

L2 (grade, < 300 ms) splits the same way: `answer::check` on the 3,227 parsed corpus answers is already asserted under 1 s for the whole corpus in a debug build (`docs/plans/M2.md`, U3 acceptance), which is roughly 0.3 ms per check. Budget 5 ms for the check, 150 ms for the append-event-plus-fold transaction (one insert, two updates), 145 ms headroom. Benchmark B covers the transaction; Benchmark A covers the check.

### 10.5 What not to do

- Do not benchmark through HTTP in CI. It adds the runner's loopback stack and the tokio scheduler to the noise, and it measures nothing the two benchmarks above miss.
- Do not run the benchmark concurrently with the test suite. Project memory: parallel full suites contend on this box, and the same holds on a 2-core runner. Serialize the benchmark step after `cargo test`.
- Do not gate on a mean. A mean hides the tail the budget is about.