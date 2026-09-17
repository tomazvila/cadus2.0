# Content Authoring Report — the remaining 10 courses

## The problem

The 10 courses after geometry contain ~1,822 KPs across ~718 topics. Every KP
needs template + teach documents authored via LLM calls (the content_store
pipeline). The deepseek-chat model (the only model that reliably passes the
gate for foundations/geometry content) **cannot produce gate-passing
templates for the harder topics** in these courses.

## Root cause

The template grammar requires:
- Valid JSON with exact schema (v, params, samples, statement, answer_expr, answer_kind)
- Decidable answer expressions (no multipart, no variable exponents, no prose)
- Properly escaped LaTeX (all literal braces doubled)
- Parameter names that don't collide with grammar keywords
- Sample coverage that spans the parameter space

The deepseek-chat model can produce valid templates for arithmetic and
elementary geometry but consistently fails on:
- Calculus expressions (improper derivative/integral notation, unescaped braces)
- Multipart answers (tuple/scalar sets not supported by the grammar)
- Proof/justification formats (not single-answer)
- Abstract algebra (parameter names colliding with grammar keywords)
- Physics formulas with π/LaTeX (undecidable answer forms)

## Gate refusal distribution (from the 10-course parallel run)

| Refusal class | Count | Example |
|---|---|---|
| Grammar: multipart/tuple not supported | ~35% | `multipart(x2-x1, y2-y1)` |
| Body: wrong JSON structure | ~25% | `invalid type: map, expected string` |
| LaTeX: unescaped braces | ~20% | `{x^2 - {a}^2` |
| Answer kind: multi-step not decidable | ~10% | `multi-step is not symbolically decidable` |
| Parameter name collision | ~5% | `order` reads as a label |
| Sample agreement failures | ~5% | expected ≠ computed |

## Per-course status

| Course | Topics | KPs | Templates needed | Authorable? |
|---|---|---|---|---|
| probability-statistics | 82 | ~246 | ~492 (template+teach) | **YES** — all numeric, simple stats |
| precalculus | 37 | ~111 | ~222 | **MAYBE** — polynomials OK, conics/trig harder |
| discrete-mathematics | 38 | ~114 | ~228 | **YES** — counting/graph theory is numeric |
| calculus-1 | 79 | ~237 | ~474 | **NO** — derivative/integral notation fails the grammar |
| calculus-2 | 67 | ~201 | ~402 | **NO** — series/integrals fail |
| linear-algebra | 75 | ~225 | ~450 | **NO** — vectors/matrices need multipart answers |
| multivariable-calculus | 65 | ~195 | ~390 | **NO** — 3D vectors need tuples |
| differential-equations | 59 | ~177 | ~354 | **NO** — DE notation fails the grammar |
| abstract-algebra | 54 | ~162 | ~324 | **NO** — group/ring notation fails |
| category-theory | 70 | ~210 | ~420 | **NO** — categorical concepts are not single-answer |

## The three viable courses

- **probability-statistics** (82 topics, 246 KPs) — all numeric, already 12 topics practiced
- **precalculus** (37 topics, 111 KPs) — polynomials, rational functions — numeric
- **discrete-mathematics** (38 topics, 114 KPs) — counting, graph theory — numeric

These 3 courses = 157 topics / 471 KPs / ~942 documents. At ~1-2 docs/min with
2-3 parallel passes: **~8-16 hours**.

## The 7 deferred courses

The 7 harder courses (calculus-1/2, linear-algebra, multivariable-calculus,
differential-equations, abstract-algebra, category-theory + proofs) = 561 topics
/ 1,351 KPs. These need either:
1. A stronger model (GPT-4o, Claude) that can produce valid template JSON
2. Grammar extensions to support multipart answers, variable exponents, and
   proof formats
3. A different content format (e.g. serve from exemplars only, skip templates)

The exemplar top-up pattern (subagent writes YAML exemplars) DOES work for
these courses — the exemplars live in YAML (not content_store) and can be
hand-written without the template grammar. But the LESSONS still need teach
pages (which CAN be LLM-authored — the teach-page gate is less strict than the
template gate) and templates for the practice pool (which CANNOT be LLM-authored
for these harder topics).

## Recommendation

1. Complete the 3 viable courses (probability-statistics, precalculus,
   discrete-mathematics) — ~8-16 hours
2. For the 7 deferred courses: top up exemplars in the YAML (the subagent
   pattern works) and author teach pages (the teach-page gate is less strict).
   The lessons will serve PRACTICE-ONLY (no templates) — the learner can
   study from the teach pages and practice from the exemplars. The topics
   won't have the full lesson pipeline (no template-based practice pool),
   but they'll have teach + practice + assessment, which is functionally
   equivalent for a self-motivated learner.
3. File a follow-up issue for the grammar extensions needed to support the
   harder topics (multipart answers, variable exponents, proof formats).
