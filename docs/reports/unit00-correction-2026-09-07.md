# Unit00 pending-template correction

## Result

64 of 81 arithmetic-core KPs are clean across all seven authoritative audit codes. Sixteen KPs have explicit schema/domain blockers, and one family is withheld because its prime-only domain does not exercise the factor-listing objective.

The bundle contains 64 production-gated template drafts: 60 newly covered KPs and four replacements for earlier unit00 templates. All stored-review rows remain `pending`. No database import, approval, production operation, or deployment was performed.

## Owned before/after counts

The before snapshot is the supplied `audit.json`, including the interrupted semantic repair. Counts are affected KPs, not individual examples.

| Authoritative code | Before | After |
|---|---:|---:|
| `fewer_than_four_exemplars` | 0 | 0 |
| `missing_solution_sketch` | 0 | 0 |
| `undecidable_authored_answer` | 0 | 0 |
| `singleton_label_contract` | 0 | 0 |
| `duplicate_problem_answer_family` | 0 | 0 |
| `absent_pending_template_recipe` | 77 | 17 |
| `generic_or_tautological_sketch` | 0 | 0 |
| Clean owned KPs | 4 | 64 |

All 728 non-owned KP audit rows are unchanged. The full-course audit exits 1 because it includes these 17 owned blockers and the existing non-owned findings.

## Production-gate and lifecycle evidence

- `crates/worker/examples/unit00_template_gate.rs` calls the actual worker `verify_kind(Kind::Template, ...)`, which repairs arguments, assembles server-owned fields and the shared answer contract, runs the template gate, and fills `space_size`.
- Every satisfying tuple was exhaustively instantiated and checked with the production per-instance gate: **1,414 distinct rendered instances**, with **at least 13 distinct operand–canonical-answer combinations per template**. The required floor is 12. Numeric substitutions are instances of one parameterized family.
- Collision checks cover all authored KPs in the same topic plus its diagnostic exemplar. They compare normalized text and operand multisets paired with production-canonical answers, catching reworded and reordered repeats. Both collision counts are zero.
- Samples have independently computed Python answers. Domain tests cover every supported KP and check carrying, borrowing, operand bounds, exact division, square recognition, prime bounds, estimation boundaries, and the required coprime/divisor special cases.
- `document_digest` computes each digest from the exact worker-returned body. Tests re-gate every draft and compare its entire stored body, digest, and exhaustive instance evidence. Raw draft rows use the importer's exact `kp_id`, `kind`, `arguments` shape; pending status belongs to the stored row.
- Four earlier unit00 template rows were retired from each zero-api artifact and replaced by freshly gated bodies. Their previous digests are recorded in `unit00-retired-pending-digests.json`. All 802 retained rows in each zero-api artifact match HEAD exactly. Existing approvals were untouched.

The previous subtraction-facts, multiplication-tables, and evaluating-powers recipes exceeded their stated KP bounds. Their replacements use the correct bounds. The parentheses recipe was also replaced and freshly digested as part of the coherent unit00 bundle.

## Exact blockers and semantic exclusion

`unit00-schema-blockers.json` contains the 16 schema/domain KP constraints, candidate probe arguments, literal worker rejection, and additional evaluator evidence where applicable. Blocked candidates live in the report area and are excluded from pending-template evidence.

| KP key | Required capability or limiting domain |
|---|---|
| `divisibility-rules/kp1` | Emit a yes/no label for the 2/5/10 rules. |
| `divisibility-rules/kp2` | Emit a yes/no label for the 3/9 rules. |
| `divisibility-rules/kp3` | Emit a yes/no label for the 4/6 rules. |
| `factors-and-multiples/kp3` | Emit a yes/no factor/multiple label. |
| `prime-composite-numbers/kp1` | Emit prime/composite labels. |
| `prime-composite-numbers/kp3` | Emit the applicable edge-case classification; the existing numerical “only even prime” question has one fixed answer and does not provide a fresh parameterized classification family. |
| `division-with-remainders/kp1` | Emit `q Rr` and retain a divisor-specific quotient/remainder contract. |
| `division-with-remainders/kp2` | Emit `q Rr` with a remainder smaller than the divisor. |
| `long-division-one-digit/kp3` | Emit `q Rr` for three-digit dividends and one-digit divisors. |
| `long-division/kp3` | Emit `q Rr` for two-digit divisors. |
| `whole-number-exponents/kp1` | Preserve exponent notation or repeated multiplication as the emitted answer. |
| `prime-factorization/kp1` | Preserve a product of primes for numbers 4–30. |
| `prime-factorization/kp2` | Preserve a product of primes for numbers 30–100. |
| `prime-factorization/kp3` | Preserve repeated prime factors for numbers up to 150. |
| `perfect-squares/kp1` | Bases are restricted to 1–12; authored calculations already use 1, 7, 9, and 12. Eight fresh numerical instances remain, below the gate's 12-instance floor. |
| `factors-and-multiples/kp2` | Bases 2–12 give at most 11 instances for a fixed list length. The allowed lengths are 3–5; the template AST has no parameter-dependent collection length. |
| `factors-and-multiples/kp1` | The candidate used only prime inputs, making every factor list `(1, a)` and failing to exercise composite factor pairs. It remains absent pending a varied list-output family. |

Label probes fail in the mathematical grammar. Quotient/remainder probes cannot emit the `R` representation; the four KPs also lack one shared divisor-specific template contract. Author-supplied contracts are stripped by worker assembly.

For notation/product objectives, the production evaluator demonstrably folds the requested representation: `3**4` becomes `81`, and `2*2*3` becomes `12`. A text-choice binding cannot be evaluated as an answer. The alternate one-choice worker probes also encounter the space floor; that rejection alone is not evidence of the representation limitation. The direct evaluator checks supply that evidence.

`perfect-squares/kp2` is supported by a numerical selection question that asks which of two candidates is a perfect square. Both answer positions occur, both numbers remain below 150, and exactly one candidate is a square. This matches its recognition objective and its heterogeneous numerical/label exemplar contract situation.

## Verification

| Check | Result |
|---|---|
| Worker pending-body/digest/instance and blocker regression tests | 2 passed |
| Existing closed-family completion tests | 2 passed |
| Independent semantic-domain, boundary, and import-manifest tests | 3 passed |
| Matching authoritative audit's negative controls | 9 passed |
| Production curriculum lint/load and authored-answer fact adapter | Passed |
| Authoritative owned audit | 64 clean; exact 17 blockers above |
| Added source files and functions | Every file below 500 lines; every function at most 70 lines |
| Retained non-owned artifact rows and audit findings | Unchanged |
| `git diff --check` | Passed |

The broader existing `authoring_foundations_drafts` suite has two passing tests and one failing test. Its failing test reports **13 existing instruction rejections: 11 teach rows and two hint ladders** against the supplied curriculum repair. This suite reads the unchanged arithmetic-core instruction artifacts; `CADUS_DRAFT_TEMPLATES` was unset, so the new template bundle was not an input.

| Instruction kind | Affected KPs |
|---|---|
| Teach | `division-facts/kp1`, `long-division/kp2`, `multiplication-division-word-problems/kp3`, `factors-and-multiples/kp2`, `prime-factorization/kp1`, `greatest-common-factor/kp2`, `least-common-multiple/kp1`, `least-common-multiple/kp2`, `least-common-multiple/kp3`, `gcf-lcm/kp2`, `rounding-estimation/kp3` |
| Hint ladder | `prime-composite-numbers/kp3`, `rounding-estimation/kp3` |

The teach rejections expose authored answers in their final steps; the two hint rejections name classification answers. These instruction artifacts were left unchanged within this template-only correction. The full workspace and database-backed import suites were not run.

## Reproduction and provenance

- Exact checkout: `/Users/lilvilla/Programming/workofo/.codex-harness/tmp/cadus2-u00`.
- Base commit: `5ff75177a01d9ef8c4ee98d75bc8cc1e472bc58b`; the supplied dirty curriculum repair is included in this correction.
- Authoritative audit: adjacent `cadus2-filtered-checkpoints/foundations_content_audit.py`, including the reflexive-equality marker present in the supplied audit. The older `cadus2-final-content-audit` Python copy lacks that marker and was superseded before the final comparison. Audit semantics were not edited.
- The fact adapter at `crates/core/examples/content_audit_facts.rs` is byte-identical to the supplied authoritative adapter (SHA-256 `32a7402ae7c0f828e3c0e93a6b68f908f309921b07d7e711d0c9408002ecf74c`).
- Rust 1.95 was run from a Nix toolchain. Cargo cache and target outputs are project-local, `CARGO_BUILD_JOBS=1`, `-j 1`, and `SQLX_OFFLINE=true`; no global pins or dependencies were installed or changed. PyYAML was supplied by Nix.

From a Nix shell with Rust 1.94+ and PyYAML, use:

```sh
export CARGO_HOME="$PWD/.tooling/cargo"
export CARGO_BUILD_JOBS=1
export SQLX_OFFLINE=true
python3 scripts/authoring/unit00/build.py --output .tooling/unit00-candidates.json
cargo run --locked -j 1 -p cadus-worker --example unit00_template_gate -- .tooling/unit00-candidates.json .tooling/unit00-gate
python3 scripts/authoring/unit00/publish.py --candidates .tooling/unit00-candidates.json --gate .tooling/unit00-gate
cargo run --locked -j 1 -p cadus-worker --example unit00_schema_blockers
cargo test --locked -j 1 -p cadus-worker --test unit00_pending_templates --test authoring_completion_templates
python3 -m unittest discover -s scripts/authoring/unit00 -p test_domains.py -v
cargo run --locked -j 1 -p cadus-core --example content_audit_facts -- curriculum > .tooling/unit00-audit-facts.json
python3 ../cadus2-filtered-checkpoints/foundations_content_audit.py --facts .tooling/unit00-audit-facts.json --content-root docs/content-foundations --output docs/reports/unit00-authoritative-audit-after.json
```

Grounded: worker/evaluator source, the supplied constraints, literal rejections, and unchanged authoritative audit rules. Tested: all accepted templates, exhaustive instances, digests, domain properties, importer manifest shape, and all blocker probes. Inferred: the exhaustive capability classification of the current template AST. Unverified: human pedagogical approval and database import. No task-started process was left running.
