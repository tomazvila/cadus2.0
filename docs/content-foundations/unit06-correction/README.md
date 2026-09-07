# Unit06 correction — production-gated artifacts
The Unit06 bundle contains 78 production-gated template recipes. All 78 current candidates pass the mechanical gate. Seventy-five review records remain pending human review; three records are explicitly superseded by the reviewed template19 bundle. No recipe is approved.

## Counts
| Measure | Count |
|---|---:|
| Owned knowledge points inspected | 78 |
| Authored exemplars checked | 312 |
| Template candidates gated | 78 |
| Previously absent KPs receiving valid recipes | 70 of 70 |
| Existing template records refreshed | 8 |
| Pending review records | 75 |
| Superseded review records | 3 |
| Valid distinct instances, exhaustively checked | 936 |
| Instances per template | 12 |
| Current production-gate blockers | 0 |
| Database writes, approvals, deployments, model calls | 0 |

## Scoped recheck
| Code | Baseline affected KPs | Current residual |
|---|---:|---:|
| `fewer_than_four_exemplars` | 0 | 0 |
| `missing_solution_sketch` | 0 | 0 |
| `undecidable_authored_answer` | 0 | 0 |
| `duplicate_problem_answer_family` | 7 | 0 |
| `generic_or_tautological_sketch` | 6 | 0 |
| `singleton_label_contract` | 0 | 0 |
| `absent_pending_template_recipe` | 70 | 0 |

The supplied root `audit.json` remains the immutable full-course baseline. `residuals.json` is a scoped recheck against the current Unit06 artifacts. Its checks cover the supplied markers, numeric-normalized textual families, and Rust contract/gate evidence. Full pedagogical family independence remains outside this scoped report.

## Evidence and content
- `drafts.json`: 78 local-import records with bounded parameter domains, executable answer expressions, worked samples, hints, and solution sketches.
- `pending-review.json`: 78 hash-bound production-gate records and 936 rendered instances. Each record is pending or names the template19 bundle that superseded it.
- `authored-verification.json`: all 312 current authored exemplars with their contracts and decidability results.
- `schema-blockers.json`: the empty current blocker set.
- `residuals.json`: the scoped 78-KP recheck and zero residuals across the seven tracked codes.
- `manifest.json`: current artifact hashes, all 78 gated records, and the explicit 75 pending/3 superseded review-state split.

Eight Unit06 template records in `../zero-api-completion/drafts.json` and `stored-review.json` were replaced with their verified recipes. Each replaced review record retains `supersedes_digest` and identifies offline storage. All content remains unapproved.

## Verification
Run from the repository root with one Cargo build job:
```sh
export CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true
cargo run --locked -p cadus-worker --example unit06_templates
python3 scripts/authoring/unit06_report.py
cargo test --locked -p cadus-worker --test unit06_pending_templates
cargo clippy --locked -p cadus-worker --example unit06_templates --test unit06_pending_templates -- -D warnings
cargo fmt --all --check
```
The worker regression binds every candidate to its current gate body and digest, checks all 936 rendered problems and solutions, rejects per-instance off-by-one answers, and requires every review status to be pending or explicitly superseded.
