# Per-item answer contract milestone

## Scope

Branch `framework/codex-answer-contract`, based on framework integration commit
`699fa89`. This implements the exact/approximate portion of f3 and exercises the
f4 grade path. The full framework assignment and f3 remain open.

## Implemented behavior

- An exemplar or template can declare `answer_contract: {kind: exact}`,
  `{kind: approx, decimals: 2}`, or `{kind: none}`. Missing metadata retains the
  historical grading policy. Templates require a deterministic policy.
  Approximation uses half-to-even rounding and a
  bounded, authored decimal count (0–18).
- Exact grading accepts mathematical equivalence within the existing grammar.
  It rejects learner-selected approximations such as `0.3` for `1/3`.
- Contracts survive template instantiation, pool persistence, the served
  problem snapshot, and attempt events. New exemplar serves capture the current
  authored policy only when both problem text and expected answer match.
  Previously served problems retain their captured policy.
- Template authoring derives a uniform deterministic policy from its exemplars;
  model output cannot replace that policy. Contract-bearing multi-step
  templates can pass validation and enter the pending approval lifecycle.
- Curriculum loading, linting, and readiness validate explicit policies.
  The semantic curriculum fingerprint includes contract metadata.

## Curriculum changes

108 existing exemplars in 19 Foundations multi-step topics now explicitly use
exact grading. Problem text, expected answers, and solutions are unchanged.
The topic inventory comes from `foundations-answer-inventory.md` section 3.2.
These annotations establish deterministic final-answer assessment within the
supported grammar; they do not assess a learner's intermediate reasoning.

## Verification evidence

Focused regressions are in:

- `crates/core/tests/answer_contract.rs`: exact versus approximate thirds,
  equivalent radicals and units, wrong values, half-even ties, malformed input,
  input limits, and legacy pool serialization.
- `crates/core/tests/answer_contract_content.rs`: all 108 annotated items,
  curriculum validation, and full multi-step template validation.
- `crates/core/tests/parity.rs`: independently derived fingerprint and dump size;
  the unannotated parity fixtures retain their original representation.
- `crates/web/src/serve/draw.rs`: legacy pool rows receive only a matching
  current exemplar policy; template policies remain tied to their own content.
- `crates/web/tests/grade_route_contract.rs`: HTTP grading and captured contract
  evidence in attempt events, using disposable test databases.
- `crates/worker/tests/authoring_contract.rs`: fake-model authoring stores a
  contract-bearing template as pending and preserves the server-owned policy.

Execution results (2026-09-06):

- Full workspace run: 2,441 passed, one CLI snapshot failed, two pre-existing
  documentation examples ignored. The snapshot still named the original
  curriculum fingerprint and length. After updating its independently derived
  constants, all nine `parity_oracle` tests passed. Thus all 2,442 active tests
  have passing evidence; the whole workspace was not repeated for this
  test-constant-only fix.
- All 13 new regressions passed in the full run.
- `cargo clippy --workspace --all-targets --offline -- -D warnings`, formatting,
  and `git diff --check` passed.
- File-size check: 769 files, zero at or over 500 lines. Complexity: 8,296
  functions, zero over a limit. Clone check: zero. Unused public items: zero.
- The earlier rational-exponent HTTP expectation failed identically on untouched
  framework `699fa89`. Its corrected expectation passed in the full run.

The full log is `/home/deploy/.cache/cadus2_contract_scratch/final-workspace-tests.log`;
the corrected snapshot run is `final-parity-oracle.log` in that directory.

## Remaining work

- Explicit contracts for tolerance, units policy, required expression form,
  labelled coordinates, multipart answers, and unsupported answer grammar.
- Placement diagnostics' third-outcome handling and diagnostic content coverage.
- Full content authoring, human approvals, item diversity, integrated tasks,
  readiness closure, browser acceptance, deployment and rollback verification.
- Release-wide quality/coverage validation. This milestone does not establish
  the repository's 100% coverage gate or whole-course readiness.

No live learner data or service was changed, and no paid model API was called.
