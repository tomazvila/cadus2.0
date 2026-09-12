# Template19 production-gate repair — 2026-09-07
## Result
The exact 19 previously declined knowledge points now pass the real worker gate and a pending-only import into a clean, migrated disposable database. The importer stored 19 rows, declined zero, and reported zero model cost. A missing-only replay skipped all 19 rows with zero calls. The checked-in cohort has 236 material instances and exhaustive worked samples.

No production import, approval, push, or deployment was performed. Pending content still requires human review before serving.

## Root cause
At framework HEAD `a3c6c5f5`, worker preflight rejected a multi-step template unless its knowledge-point exemplars supplied one shared decidable contract. These KPs intentionally contain heterogeneous or legacy exemplar policies. The explicit typed contract inside the incoming recipe had not yet reached the worker, so preflight declined the request before reading it. Independently, assembly could discard that supplied contract. Replacing recipe prose alone could not repair this path.

The repaired worker allows an unshared multi-step template to reach contract-aware validation. It preserves the submitted typed contract when there is no shared reviewed policy, retains the reviewed policy when one exists, and rejects malformed, none, conflicting, or wrong-shape contracts. Proof remains preflight-refused. The pure offline companion calls this same preflight and production verification path.

## Content and contracts
Ten recipes required content repairs in addition to propagation; nine retained their existing content. The changes preserve the current KP objectives and constraints, including exact equation conversion, true/false judgments and chains, contextual inverse trigonometry, and substitution checks after squaring.

| Knowledge point | Exhaustive instances | Content |
|---|---:|---|
| `basic-absolute-value-inequalities/kp3` | 16 | Existing precise recipe retained |
| `comparing-integers/kp2` | 12 | Computed judgment plus typed ascending chain |
| `converting-to-vertex-form/kp3` | 12 | Vertex triple tightened to coordinates of arity 3 |
| `law-of-sines-cosines/kp1` | 16 | Existing precise recipe retained |
| `logarithm-basics/kp1` | 12 | Existing precise recipe retained |
| `logarithm-basics/kp2` | 12 | Existing precise recipe retained |
| `parabola-vertex-form/kp3` | 12 | Existing precise recipe retained |
| `percentages/kp3` | 12 | Both find-the-part and find-the-percent computations |
| `pythagorean-converse/kp3` | 12 | 12 distinct similarity classes; acute, right and obtuse |
| `quadratic-applications/kp1` | 12 | Existing precise recipe retained |
| `quadratic-applications/kp2` | 12 | Existing precise recipe retained |
| `quadratic-graphs-vertex/kp2` | 12 | Existing precise recipe retained |
| `radical-equations-basic/kp1` | 12 | Existing precise recipe retained |
| `radical-equations-basic/kp2` | 12 | Isolate the radical, solve and substitute |
| `radical-equations-basic/kp3` | 12 | Computed empty/singleton real solution sets; both RHS signs |
| `ratio-tables-equivalent-ratios/kp3` | 12 | Both common-volume counts and stronger-mix decision |
| `trig-applications/kp3` | 12 | 12 distinct incline ratios; certified one-decimal inverse tangent |
| `understanding-ratios/kp3` | 12 | Missing equivalent-ratio part and equivalence judgment |
| `unit-rates/kp2` | 12 | Both unit prices and cheaper-pack decision |

The pure core adds a contract-specific `ascendingchain` writer and accepts the mathematically valid empty set in the existing set grammar. The chain writer permits 2–16 distinct rational values and requires the ascending-chain policy. Empty solution sets use the existing typed set checker. No constant-output classifier or expanded catch-all vocabulary was introduced.

Existing logarithm/exponential equation writers retain their bounded, role-checked vocabularies. They independently validate base, exponent, and argument; a solved number, reversed conversion, wrong equation, or malformed answer fails the contract. Coordinates, units, multipart answers, inequality classifications, and approximate angle policies retain their specific shapes.

[`drafts.json`](../content-foundations/template19-production-gate/drafts.json) is an exact importer-shaped cohort. [`worked-samples.json`](../content-foundations/template19-production-gate/worked-samples.json) contains all 236 rendered problems, expected answers, and worked solutions. The source map identifies every existing catalog mirror. Three old unit06 review snapshots are explicitly superseded while preserving their original body and digest. `sync_template19_recipes.py --check` rejects generator drift with a pure local test.

## Production import evidence
| Run | Stored | Skipped | Declined | Endpoint calls | Reported model cost |
|---|---:|---:|---:|---:|---:|
| Unmodified HEAD reproduction | 0 | 0 | 19 | 0 | 0 micro-USD |
| Final pending-only import | 19 | 0 | 0 | 19 | 0 micro-USD |
| Final missing-only replay | 0 | 19 | 0 | 0 | 0 micro-USD |

The 19 successful endpoint calls were served by the local deterministic draft endpoint. They were not live model calls. The disposable database applied all 12 migrations; the dry-run planned the exact 19 keys. Worker tests also assert pending-only storage and unchanged idempotent replay counts.

### Evidence log hashes
| Local log | SHA-256 |
|---|---|
| `baseline-head-import.log` | `7bae16f4bb103ad42dd3c71cade6d76d98ce23ac8a86c092ce5b4d4eb927fed4` |
| `final-dry-run.log` | `1d1febdc63c8e6e6e62f15fc366fb560f740f112aaff3df4e0deb5d7c5dc873d` |
| `final-import.log` | `227e8c5d813157fdd8f32ba8a01c4aedead388d107d8888e0cdb337d0999aa58` |
| `final-replay.log` | `eb51e628765e9061a794d324aeb5db5459bd7b61df979c69271385a502e777db` |
| `final-db.log` | `ec8ddee5edd59a0243d1bcbcc6e5c3a9cf572e31854772525866c8c62dc83d7a` |

## Regression and audit verification
- Focused core run recorded 14 passing tests across set grammar, exhaustive catalog/gate checks, adversarial contracts, and materiality. A subsequent renderer-evidence test was added; final rerun count and collision totals are pending root reconciliation.
- Focused worker run recorded 12 passing tests across existing contract propagation, preflight refusals, exact-19 database import/replay, and the pure production adapter.
- The isolated Python review suite recorded 38 passing tests. The new source-mirror/worked-sample check also passed without a database or model.
- Collision validation walks nested checked-in arguments/body records, excludes superseded snapshots, normalizes whitespace and math wrappers, and checks target instances against other recipe instances, every curriculum exemplar, and diagnostics. It retains minimum catalog/instance coverage assertions.
- Content audit requires production-gate results bound to the current source fingerprint and curriculum. Missing, stale, rejected, or malformed gate evidence cannot produce a green audit. Python tests exercise this through local fixtures and subprocess stubs; production Rust validation is a separate offline companion, with no live database or model dependency.

## Whole-catalog audit result
The final source-bound audit reports zero issues for all exact 19 target KPs. Across the whole course, 1,008 normalized pending documents cover 807 keys: 972 pass the production gate and 36 are declined. The 809-KP course audit flags 38 KPs: those 36 gate-declined KPs and two preexisting missing-source KPs. The wider catalog therefore remains non-green; this repair makes those production refusals visible instead of counting them as usable templates. These remaining keys are outside the requested exact-19 cohort.

## Final quality reconciliation
Full formatting and Clippy checks passed before the final evidence-only additions. LOC recorded 1,131 source files and zero at or above 500 lines; the newest count and full release-gate run remain pending root reconciliation. Root will also record the renderer-test rerun, collision coverage, final commit and process cleanup. Earlier failed formatting/Clippy attempts are not pass evidence.

## Verification boundary
Grounded: exact declined keys, current curriculum objectives/constraints, contract and worker implementation, local import logs. Tested: focused core/worker suites, clean migrated disposable-DB import and replay, isolated Python audit/source checks. Inferred: unchanged nine recipes need propagation repair only, supported by their passing exhaustive gate samples. Not verified here: production serving readiness, human approval, full release/browser gate, deployment or rollback.
