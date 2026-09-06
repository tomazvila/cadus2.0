# Unit06 correction — schema-blocked result
The interrupted dirty fixes are preserved. The seven named duplicate-family KPs and six named sketch KPs have targeted repairs. The current contracts support 53 pending template recipes; 25 candidate recipes have exact production-gate blockers.

## Counts
| Measure | Count |
|---|---:|
| Owned knowledge points inspected | 78 |
| Authored exemplars accepted by current answer contracts | 312 |
| Template candidates attempted | 78 |
| Previously absent KPs receiving valid pending recipes | 45 of 70 |
| Existing template records refreshed | 8 |
| Total pending templates | 53 |
| Valid distinct instances, exhaustively checked | 636 |
| Instances per pending template | 12 |
| Authored collisions, including owned diagnostics | 0 |
| Cross-template sibling text collisions | 0 |
| Corrupted-sample production-gate controls rejected | 53 |
| Per-instance off-by-one answers rejected | 636 |
| Remaining missing recipes or semantic exclusions | 25 |
| Database writes, approvals, deployments, model calls | 0 |

## Seven-code scoped recheck
| Code | Baseline affected KPs | Scoped residual |
|---|---:|---:|
| `fewer_than_four_exemplars` | 0 | 0 |
| `missing_solution_sketch` | 0 | 0 |
| `undecidable_authored_answer` | 0 | 0 |
| `duplicate_problem_answer_family` | 7 | 0 |
| `generic_or_tautological_sketch` | 6 | 0 |
| `singleton_label_contract` | 0 | 0 |
| `absent_pending_template_recipe` | 70 | 25 |

The supplied root `audit.json` remains an unchanged full-course baseline. Its original auditor executable is absent from this checkout. `residuals.json` is explicitly a scoped recheck, using the two supplied sketch markers, numeric-normalized textual families, and fresh Rust contract/gate evidence. It does not certify full pedagogical independence across all inherited exemplar sets or claim an authoritative full-course audit rerun.

## Exact blockers
`schema-blockers.json` retains the literal production rejection, answer kind, shared contract, and KP key for each failed candidate. Rejected candidates are confined to the worker test fixture and do not appear in `drafts.json` or the pending evidence.

| Blocker | KPs |
|---|---|
| Variable exponents: `grammar`, “an exponent that is not a whole number” | `exponent-product-rule/kp1`, `/kp2`, `/kp3`; `exponent-quotient-rule/kp1`, `/kp2`, `/kp3`; `exponent-product-quotient-rules/kp1`, `/kp3`; `power-of-a-power-rule/kp1`, `/kp2`, `/kp3`; `simplifying-negative-exponent-expressions/kp1`; `simplifying-radicals-variables/kp1`, `/kp2`; `radical-exponent-conversion/kp1`, `/kp2`; `rational-exponents/kp3` |
| Label output: `grammar`, “a name that is not a function or variable” | `pythagorean-converse/kp1` (`yes`), `pythagorean-converse/kp2` (`no`) |
| Missing shared deterministic contract: `answer-kind`, “answer kind multi-step is not symbolically decidable” | `pythagorean-converse/kp3`; `radical-equations-basic/kp1`, `/kp2`, `/kp3` |
| Bounded cube domain: `space-floor`, “only 10 distinct problem(s); at least 12 are needed” | `cube-roots/kp1` |
| Semantic-family exclusion: every generated comparison has the same winning side | `estimating-square-roots/kp3` |

The exponent grammar parses literal powers before parameter binding. The label grammar cannot emit text labels. The worker derives a template contract only when all exemplars carry the same explicit deterministic contract; the four multi-step KPs do not. The positive perfect cubes through 1000 supply ten radicands; admitting zero would supply eleven. Numeric renaming, padding contexts, and changing the unit's objective bounds were not used to conceal these limits.

## Evidence and content
- `drafts.json`: 53 real local-import records with parameter domains, constraints, executable answer expressions, worked samples, topic-specific hints, and solution sketches.
- `pending-review.json`: production-gated bodies, production document digests, all 636 rendered instances and solutions, and negative-control evidence. Status is offline `pending`; no database row is claimed.
- `authored-verification.json`: all 312 current authored exemplars with their actual contracts and decidability results.
- `schema-blockers.json`: 24 exact production refusals plus one semantic exclusion.
- `residuals.json`: all 78 scoped KP results and seven-code counts.
- `manifest.json`: artifact hashes and counts.

Eight unit06 template records in `../zero-api-completion/drafts.json` and `stored-review.json` were replaced with the corresponding verified recipes. Each replaced review record retains `supersedes_digest` and explicitly identifies offline storage. All other records were preserved byte-for-byte. Earlier historical import counts and reviews remain historical observations; these replacements have not been imported.

## Verification
38 focused tests passed:
- Core unit readiness and diagnostics: 2.
- Core sign, domain, radical, notation, and label negative controls: 7.
- Worker current-contract templates, exact refusals, import partition, and all-instance negative checks: 3.
- Python correction, math, bounds, determinism, scope, and rendered-answer checks: 9.
- Existing independent exponent/radical recipe checks: 17.

Production export, targeted strict Clippy, workspace formatting, and whitespace checks passed. Changed/new code files are below 500 lines and functions are at most 70 lines. The existing unit06 negative-control test received formatting-only changes to clear workspace formatting.

`scripts/gate.sh` was attempted and stopped with `GATE FAILED: set CADUS_TEST_DATABASE_URL`. The full database-dependent gate was not run because database work is excluded from this task. No test process was left running.

## Reproduction
Run from this exact worktree with a Nix-provided Rust toolchain meeting the repository MSRV. This run used Rust/Cargo 1.95.0, a project-local `target/cargo-home`, and one Cargo build job. No Cargo manifest, lockfile, toolchain pin, or global configuration changed.
```sh
export CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true
export CARGO_HOME="$PWD/target/cargo-home"
python3 scripts/authoring/unit06_templates.py
cargo run --locked -p cadus-worker --example unit06_templates
python3 scripts/authoring/unit06_refresh_evidence.py
python3 scripts/authoring/unit06_report.py
cargo test --locked -p cadus-worker --test unit06_pending_templates
cargo test --locked -p cadus-core --test exponents_radicals_negative_controls --test exponents_radicals_unit_recipes
python3 -m unittest discover -s scripts/authoring -p 'test_unit06_correction.py'
python3 -m unittest discover -s scripts/authoring -p 'test_exponents_radicals_recipes.py'
cargo clippy --locked -p cadus-worker --example unit06_templates --test unit06_pending_templates -- -D warnings
cargo fmt --all --check
```

Grounded: current curriculum, supplied baseline audit, and production contracts/gates. Tested: the counts and controls above. Inferred: the schema limits explain these candidate refusals. Unverified: full pedagogical family independence, the unavailable original auditor, database import, and production behavior.
