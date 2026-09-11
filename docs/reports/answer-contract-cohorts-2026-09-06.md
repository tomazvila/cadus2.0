# Reviewed choice and unit policies

## Scope

98 additional Foundations exemplars have explicit policies: 71 closed labels and 27 exact quantities, across ten files. The answer grammar and original corpus are unchanged. All problem and answer text is preserved. Foundations now has 314 explicit policies: 206 exact, 71 label, 27 unit, and 10 required numeric forms.

The closed vocabularies follow the question: yes/no, true/false, prime/composite, or parallel/perpendicular/neither. Whitespace and case normalization preserve the selected option. Additional prose, combined options, and an opposite option fail the closed policy.

The quantity cohort covers prompts with named physical units, explicit degrees, or euro-denominated balances. Equivalent supported units retain exact values. The authored money computations terminate at exact cents; this cohort contains no rounding request. Bare numbers lack the required quantity. The current bounded unit table includes ml, L, metric length, currency glyphs, and degrees; mL, EUR, and radian suffixes remain outside that table.

## Independent review evidence

Run `python3 scripts/review/foundations_choice_units.py` from the repository root. Its output reproduces `docs/reports/foundations-reviewed-choice-units.jsonl`.

The verifier uses integer divisibility and primality checks, exact fractions, complete polynomial coefficient comparisons, point determinants, direct equation/inequality substitutions, and finite relation checks. Affine-function cases use the nonzero-slope injectivity criterion. Counterexamples establish failures of the vertical/horizontal line tests. Quantity computations use exact fractions, vertex evaluation, degree identities, and the cosine rule with exact squared positive lengths. The Rust cohort test validates every reviewed answer and policy in this manifest and exercises competing options and adversarial submissions.

This manifest is immutable evidence for the reviewed 2026-09-06 snapshot. Later audited content slices retained all 98 exemplar identities while replacing 57 problems or answer representations. The cohort regression validates the historical policies independently, resolves all 98 identities fail-closed, and verifies that each installed replacement has an explicit self-decidable contract. The remaining 41 entries still match the reviewed snapshot byte for byte.

Removing precisely the 98 new contract fields from the canonical dump recovers the previous 4,060,727 bytes and SHA-256 `8930faf21b5f7ca38a6591a0c927fb09573b08829c9ebfd62c8e55165c015e3a`. The new dump is 4,066,943 bytes with SHA-256 `19c761f67e182ad80d10a5d381f4995e0694f3c1b68d0e66b309dddf260d3ee0`. Lengths exclude the CLI trailing newline.

## Residual annotation inventory

1,381 Foundations exemplars still have no explicit policy. Candidate reasons are review queues, not claims that every remaining item is semantically ambiguous.

| Review reason | Count |
|---|---:|
| Exactness and required form | 1,103 |
| Vocabulary or unsupported grammar | 223 |
| Authored precision | 33 |
| Quotient divisor | 12 |
| Unit policy | 6 |
| Set policy | 4 |

The six unit candidates left in the queue are:

- `linear-word-problems/kp1/1`: the question depends on an earlier taxi model; its local prompt lacks the model.
- `right-triangle-trig/kp1/0`, `/1`, `/2`: angle questions leave the output unit implicit.
- `trig-applications/kp3/0`: the ladder-angle question leaves the output unit implicit.
- `law-of-sines-cosines/kp2/1`: the largest-angle question leaves the output unit implicit.

The 223 vocabulary/grammar rows include further choice and structured-response families outside the four reviewed vocabularies. The precision queue needs authored rounding or tolerance, and the exact/form queue needs review of each requested representation. The six unit cases need an explicit convention before a degree-only or context-derived policy is attached. Existing content-store approvals and deployment state are unchanged.

## Verification

16 targeted tests passed: `answer_contract_cohorts` (2), `answer_contract_content` (3), `answer_contract_inventory` (2), and `parity` (9). The content test includes full curriculum lint. The independent Python verifier reproduced all 98 manifest rows byte for byte. Workspace/all-target Clippy passed with warnings denied. Formatting, changed-file LOC, whole-repository Rust dead-code scan, clone scan, and new-test Rust complexity checks passed. No full suite, live deployment, or content-store approval was run for this metadata-only slice.
