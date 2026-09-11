# Structured answer contracts and finite solutions
This milestone extends `63abbf1` (D-F1, C4, D6). It changes the answer grammar and item policies. It leaves content approval, production, and learner history intact.

## Contract schema
Every policy is an `answer_contract` object. Unknown fields fail validation.

| Kind | Fields | Acceptance rule |
|---|---|---|
| `exact` | none | Exact canonical equivalence; the legacy label rule applies. |
| `approx` | `decimals: 0..18` | Half-to-even at the authored decimal precision. Extra fractional digits fail. |
| `approx` | `tolerance: "1/100"` | Inclusive absolute error, with an exact positive rational bound. |
| `unit` | `quantity: "volume", unit: "L"` | Exact measured value with a unit of the specified quantity. Equivalent unit conversions pass. |
| `quotient_remainder` | optional `divisor: 3` | Integer quotient, nonnegative integer remainder, and remainder less than the divisor when present. |
| `coordinates` | `arity: 2..4` | Ordered numeric tuple of the specified size. |
| `set` | none | Unordered set; duplicates collapse. |
| `label` | `options: [["yes", "true"], ["no", "false"]]` | One closed choice vocabulary. Each group holds explicit aliases for one option. |
| `multipart` | `parts: [{name: "x", contract: {kind: "exact"}}, ...]` | One policy per named part. Names bind values; field order has no effect. |
| `list` | `ordered: bool, member: {...}` | A bounded homogeneous list; unordered lists retain repeated members. |
| `inequality_union` | none | An exact union of rational intervals or explicit interval-union notation. |
| `reduced_ratio` | none | Two positive coprime integers separated by one colon. |
| `ascending_chain` | none | Two to 16 strictly increasing rational values joined by `<`. |
| `polynomial_relation` | none | One exact polynomial equality or inequality, normalized by a valid nonzero scalar. |
| `none` | none | Ungraded. |

An approximate contract specifies exactly one of `decimals` and `tolerance`. Tolerances support rational targets. Radical targets use authored decimal precision. All arithmetic is exact; no numeric samples prove equivalence.

A label comparison folds case and whitespace. It adds no implicit synonyms and removes no punctuation. The mathematical grammar still refuses arbitrary prose. An unknown choice is incorrect; an authored answer outside its vocabulary fails validation.

Multipart text has the form `x = 2; estimate = 0.33; feasible = yes`. Each authored name occurs exactly once. Unknown, duplicate, and missing names fail. There are at most 16 parts. Parts hold deterministic flat contracts. An unresolved component makes the aggregate ungraded.

## Grammar scope
Finite alternatives such as `x = 7 or x = -7` produce one exact solution set. Order and duplicate alternatives have no effect. A changed unknown, omitted root, or extra root fails. A branch with another unknown, an inequality union, a malformed relation, or a division by zero remains outside this production. The production has a 16-alternative cap and retains the existing parser and canonicalization bounds.

Assignment labels also admit the closed quantity names `area`, `perimeter`, `volume`, `length`, `width`, `height`, `radius`, `diameter`, `slope`, and `intercept`.

The diagnostic continuation adds three bounded forms. A reduced ratio requires positive coprime integer terms. An ascending chain requires strict rational order. A polynomial relation parses exactly one comparison, requires polynomial expressions on both sides, and compares the zero-side polynomials after exact scalar normalization. A negative factor reverses an inequality. Tautologies, multiple comparisons, non-polynomial functions, and unbounded prose remain undecidable. The inequality-union contract also reads up to 16 explicit intervals joined by `∪`, including open infinite endpoints.

## Content evidence
The Foundations loader now finds 1,393 grammar-decidable exemplar answers out of 1,695. The distribution of distinct decidable exemplars per knowledge point is `{0: 115, 1: 53, 2: 583, 3: 58}`. The prior distribution was `{0: 138, 1: 52, 2: 561, 3: 58}`. This is answer evidence, not whole-course readiness.

The original 3,492-answer oracle corpus stays at 3,257 parsed and 235 refused. Its historical fixtures require no change.

`foundations-contract-candidates.jsonl` is a review snapshot of all 1,695 exemplars at this milestone. It records the problem, answer, identity, existing policy, candidate policy, and review reason. Every row has `automatic_approval: false`. After the continuation in `answer-contract-forms-2026-09-06.md`, 216 items have explicit policies. Of the 1,479 items without an explicit policy:

| Review category | Items |
|---|---:|
| Exactness and required form | 1,103 |
| Vocabulary or grammar | 294 |
| Authored precision | 33 |
| Unit policy | 33 |
| Divisor | 12 |
| Set policy | 4 |

The exporter leaves approximation prompts for explicit precision review. Its keyword check uses word boundaries, so `ground distance` does not imply rounding. Regenerate with `CADUS_CONTRACT_INVENTORY_DUMP=<path> cargo test -p cadus-core --test answer_contract_inventory`.

## Verification
- The full core run passed 1,208 tests across 109 test binaries/doc-test groups. After the final metadata layout and multipart guard, all 28 targeted core/event tests passed, including one new wire-preservation test.
- The focused contract suites cover schema refusal, exact tolerance boundaries, unit dimensions, remainder bounds, coordinate order, set completeness, explicit aliases, named parts, persistence, finite alternatives, and hostile input.
- Workspace Clippy passed on all targets with `-D warnings`. The three web grade-contract tests and two worker authoring-contract tests passed against disposable test databases.
- Formatting, the source line limit, duplicate detection, unused public-item checks, and function complexity checks passed. The full coverage/release gate is separate.

## Remaining scope
- Review and annotate the remaining 1,479 Foundations exemplar policies. Inferred candidates require a content decision.
- The continuation adds numeric required forms, flat list contracts, and rational inequality unions. Algebraic required forms and broader interval bounds remain.
- Add template expression support for arbitrary choice and multipart text; the current symbolic template compiler still rejects these forms.
- Add per-step API/UI feedback for multipart/integrated tasks. This milestone returns one aggregate verdict.
- Unit conversions produce a mathematically correct result; this milestone adds no converted-unit notation message.
- Run integration release checks, browser checks, content approval, and deployment separately. This milestone does not close all handover rows.
