# Unit05 inequality source checkpoint

19 of the 27 assigned KPs are source-audit clean: 76 exemplars and 19 pending recipes, each with 12 exhaustive, material mathematical instances. Eight KPs remain blocked. Nothing was imported, approved, integrated, pushed, or deployed.

## Baseline and exact scope

Local baseline: `a325d867506af81cd88bb005e1d4d6e6da83af23`, recorded as remote integration `69e4f650`. All 27 keys were confirmed `absent_pending` before authoring; see [baseline-selected.json](baseline-selected.json). The remote mapping comes from the task and baseline commit message; no remote was contacted.

Closed KPs:

- `solutions-of-inequalities/kp2`
- `one-step-inequalities/kp1`, `kp2`, `kp3`
- `two-step-inequalities/kp1`, `kp2`, `kp3`
- `linear-inequalities/kp1`, `kp2`, `kp3`
- `and-or-inequalities/kp1`, `kp2`
- `compound-inequalities/kp2`
- `interval-notation/kp1`
- `inequality-word-problems/kp1`
- `basic-absolute-value-inequalities/kp1`
- `absolute-value-inequalities/kp1`, `kp3`
- `graphing-linear-inequalities/kp3`

Unchanged, blocked KPs:

| KP | Current evidence |
|---|---|
| `solutions-of-inequalities/kp3` | The inclusive-boundary probe cancels the varying input and always answers yes. |
| `graphing-inequalities-number-line/kp2` | Endpoint redraws leave the requested direction constant. |
| `writing-inequalities-from-statements/kp2` | The expression grammar rejects the unsolved polynomial relation. Solving would violate this KP's instruction. |
| `and-or-inequalities/kp3` | The expression grammar rejects the required inequality disjunction. |
| `compound-inequalities/kp3` | The expression grammar rejects the required inequality disjunction. |
| `basic-absolute-value-inequalities/kp2` | Positive-radius two-ray union is unsupported by the expression writer. |
| `absolute-value-inequalities/kp2` | Shifted/scaled positive-radius two-ray union is unsupported by the expression writer. |
| `basic-absolute-value-inequalities/kp3` | The negative-right-side probe has a constant empty solution set. |

[blockers.json](blockers.json) contains concrete rejected probes. Five fail the production grammar gate. Three pass that gate but fail the independent constant-family rule. Every probe has `status: blocked` and `kind: probe`; none enters the pending inventory. These are bounded candidate findings, not a proof that every possible future representation is impossible.

## Audit deltas

| Affected KPs | Foundations before → after | Unit05 before → after | Owned before → after |
|---|---:|---:|---:|
| Any issue | 129 → 110 | 49 → 30 | 27 → 8 |
| Absent pending recipe | 129 → 110 | 49 → 30 | 27 → 8 |
| Fewer than four exemplars | 90 → 71 | 49 → 30 | 27 → 8 |
| Missing sketch | 32 → 25 | 13 → 6 | 12 → 5 |
| Undecidable authored answer | 42 → 39 | 23 → 20 | 10 → 7 |
| Duplicate problem/answer family | 3 → 3 | 1 → 1 | 0 → 0 |
| Generic/tautological sketch | 1 → 1 | 1 → 1 | 0 → 0 |
| Singleton label contract | 0 → 0 | 0 → 0 | 0 → 0 |

[final-audit.json](final-audit.json) records exact accepted/blocked keys, per-KP results, source hashes and recipe metrics. The global audit remains failing because other residuals remain. No threshold, gate, evaluator, topic constraint, diagnostic, or unowned KP was changed.

## Contracts and materiality

- Single-ray expressions use the current `exact` path. A raw inequality expression under `inequality_union` produced `canonical-mismatch`; no runtime code was changed to bypass it.
- Bounded-set questions explicitly request interval notation and use `exact`. This avoids the multi-step gate's free-variable restriction while preserving both endpoints and their inclusion.
- Four yes/no recipes use `equalitylabel(signcase(...),1)`. Direct substitution independently reconstructs each decision. Each has 12 distinct predicate/test-point instances, both outcomes, at least 0.8 bits of answer entropy, and a demonstrated truth change along every numeric axis.
- Solving and bounded-set recipes have 12 distinct answers and at least 12 mathematical-instance signatures. Every varying numeric axis changes the answer with other axes held fixed. Graph-solving uses `boundary` and `graph` fields; the singleton graph-style string is fixed metadata and contributes no instance count.
- “Family” in these checks retains actual mathematical data and operations while ignoring presentation. It establishes distinct material instances, not 12 different pedagogical strategies per recipe. Four authored exemplars per KP have distinct number-normalized prompt structures.
- Collision checks cover all authored and generated items in this slice, plus 45 parseable compatible signatures outside the slice. Arbitrary unsupported prose and every other pending template were not semantically reconstructed.
- The owned origin-test visual was corrected: the origin fails `y ≤ x − 1`; the shaded side contains `(0, −2)`. Both boundary points and the shaded-side test are independently checked.

## Verification

| Check | Result |
|---|---|
| `unit05_inequalities` worker tests | 2 passed; 228 exhaustive instances, real worker gate and canonical instantiation; constants, cancellations and false samples rejected |
| `unit05_inequalities_evidence` | 1 passed; all 27 source/probe outcomes matched |
| Existing `unit05_residual` worker tests | 2 passed; existing 96-instance checkpoint preserved |
| `test_semantic.py` | 4 passed; source generation, semantics, negative controls, fresh Rust facts and unowned-KP equality |
| `test_review.py` | 5 passed; residual partition, cross-curriculum collisions, visual reconstruction, adversarial controls and every displayed recipe transformation |
| Core `answer_contract`, `answer_contract_forms`, `answer_contract_shapes`, `structured_list_writers` | 20 passed |
| `cargo fmt --all --check`, `git diff --check` | Passed |
| Source limits | 15 new/edited code files; largest 148 lines; longest function 43 lines; limits remain `<500` and `≤70` |

[production-gates.json](production-gates.json) contains actual production-worker results, serialized gated bodies and SHA-256 digests. [authored-facts.json](authored-facts.json) records current Rust contract decisions. Test logs and [code-limits.json](code-limits.json) are adjacent.

Checks used Nix-provided Cargo and Rust 1.97.1, with `SQLX_OFFLINE=true` for worker tests. The initial available Rust 1.92 was below the declared minimum; final gate evidence uses 1.97.1. Dependencies were not installed globally. Test processes exited; none was left running.

Not verified: full workspace/release gate, database import, serving readiness, browser rendering, human review, or deployment. “Production gate” here means the current production worker's source verification function run locally, not an action against a production service.

## Reproduction and checkpoints

From the repository root, with a compatible Rust toolchain:

```sh
python3 scripts/authoring/unit05_inequalities/build.py --write
python3 scripts/authoring/unit05_inequalities/blockers.py
SQLX_OFFLINE=true CADUS_U05_GATE_EVIDENCE=/tmp/u05-gates.json cargo test -p cadus-worker --test unit05_inequalities --test unit05_inequalities_evidence --test unit05_residual
cargo run --quiet -p cadus-core --bin content_audit_facts -- curriculum > /tmp/u05-facts.json
python3 scripts/authoring/unit05_inequalities/test_semantic.py /tmp/u05-facts.json /path/to/baseline-facts.json
python3 scripts/authoring/unit05_inequalities/test_review.py /tmp/u05-facts.json
python3 scripts/authoring/unit05_inequalities/report.py /tmp/u05-facts.json /tmp/u05-gates.json
```

The baseline facts must be generated from `a325d86`. The generator changes owned source and pending artifacts only; it has no approval or import operation.

First checkpoint: `d0cc555`, after eight accepted KPs, 32 exemplars and 96 gate-verified instances. The second checkpoint adds the remaining eleven accepted KPs, explicit residuals and this report. Exact commit hashes, patch paths and patch-series verification are recorded in `patches/manifest.json` and the delivery reply.
