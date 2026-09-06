# Required forms, lists, and rational interval unions
This continuation extends the structured-contract milestone. It adds three explicit policies and reviews 108 further Foundations exemplar policies.

## Acceptance policies
| Contract | Fields | Rule |
|---|---|---|
| `required_form` | `form: integer`, `decimal`, or `reduced_fraction` | The numeric value matches exactly and the answer has the required literal form. A reduced fraction has a positive denominator and coprime numerator and denominator. |
| `list` | `ordered: true` or `false`, plus `member: {kind: exact}` or another supported flat policy | Up to 32 homogeneous members. Ordered lists grade corresponding members. Unordered lists compare canonical multisets, so repeated members count. |
| `inequality_union` | none | Up to 16 rational intervals over one unknown. Exact normalization merges overlaps and preserves holes and open endpoints. |

Lists accept `13 and 14`, `13, 14`, and `[13,14]`. Nested coordinate or function commas stay inside their member. Empty, unfinished, and oversized lists fail. Unordered approximate and required-form member policies fail validation; exact matching requires no search.

Examples of interval equivalence:
- `0 < x < 2 or 1 < x < 3` equals `0 < x < 3`.
- `x < 0 or x >= 0` equals `x < 100 or x >= 100`.
- `x < 0 or x > 0` differs from `x < 0 or x >= 0`.

The explicit interval policy leaves the original parser and its corpus unchanged. Radical endpoints, different unknowns within one union, and more than 16 branches are ungraded. Bracket intervals without a named unknown and infinity glyphs remain outside this policy.

## Reviewed metadata
The change adds 108 explicit contracts across 22 further topics:
- 98 plain arithmetic, GCF, and LCM targets use `exact`. Independent Python `Fraction` and integer arithmetic recomputed all 98 answers from their problem expressions.
- Four prompts explicitly require lowest-term fractions and use `required_form: reduced_fraction`.
- Six prompts explicitly require terminating decimal conversion and use `required_form: decimal`.

The review excludes requests for power notation, prime factorization, partial-product work, estimation or rounded output, currency/unit answers, and ambiguous square-root prompts. Existing problem and answer text stays byte-identical. A separate canonical JSON check removed precisely the 108 new policy fields and recovered the prior curriculum digest.

Foundations now has 216 explicitly contracted exemplars and 1,479 without an explicit contract. The candidate snapshot is regenerated. The canonical curriculum dump has 4,060,727 bytes and SHA-256 `8930faf21b5f7ca38a6591a0c927fb09573b08829c9ebfd62c8e55165c015e3a`.

## Verification and boundaries
The 49 targeted core tests passed across contract, curriculum, original-corpus, and parity suites. The final form/list/union suite passed again after the list complexity cleanup and case-sensitive interval-variable guard. Workspace all-targets Clippy passed; the final private helper edits also passed targeted Clippy. Formatting, source line limits, duplicate detection, unused-public checks, and complexity checks passed.

The targeted regressions cover form/value separation, list order and multiplicity, malformed answers, exact union endpoints, gaps, overlap, variable identity, and schema round trips. Curriculum lint and the 19-topic prior milestone checks pass with the new policies.

Required algebraic forms such as expanded, factored, exponent, or prime-factor form still need dedicated policies. Numeric decimal form specifies a literal representation; authored rounding stays under `approx`. General nested lists, unordered approximate matching, radical interval bounds, and integrated per-step UI remain outside this slice. Content-store approval and deployment remain separate.
