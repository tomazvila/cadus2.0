# Verification — 2026-09-06

Implementation and curriculum snapshot: `db54aac`, based on `780def6`. Database: disposable `cadus2_manual_20260906` on the local test cluster at port 55434. Production was not changed.

## Verified execution

- Focused generation regressions: 2 passed. Exact source answers, separate worked/practice operands, unsupported word models, and answer-leaking hints exercised.
- Importer Python tests: 7 passed, including missing-only selection and loopback cleanup.
- Targeted strict Clippy: passed. Formatting and worker build passed.
- First real local-fixture import: 806 stored, 0 skipped, 0 declined, 806 loopback calls, zero model cost.
- Second import: 0 stored, 806 skipped, 0 declined, 0 calls.
- Database afterward: 1,014 pending rows across 795 knowledge points; 0 approved rows.
- `stored-review.json` contains the 806 actual stored bodies and their pending digests for review.

The importer stops its loopback server on completion. No processes were intentionally left running.

## Remaining coverage and readiness

| Measure | Count |
|---|---:|
| Knowledge points without a pending/approved teach page | 681 |
| Knowledge points without a pending/approved hint ladder | 15 |
| Knowledge points without a pending/approved template | 720 |
| Approved-ready knowledge points | 0 |
| Blocked knowledge points | 809 |
| Teachable blockers | 809 |
| Practicable blockers | 809 |
| Assessable blockers | 746 |
| Hint blockers | 809 |
| Solution blockers | 426 |
| Prerequisite blockers | 803 |
| Visual blockers | 77 |
| Knowledge points with serve/render/grade contract failures | 183 |

Blocker counts overlap. Pending pair coverage does not establish adequate template family variety or approval. Solution proposals cover 91 knowledge points and assessment candidates cover 75; these proposals remain unapplied. Existing solution sketches were not replaced. Human review must establish objective alignment, explanations, operand bounds, and held-out family independence.

## Reproducible evidence

Scratch evidence on the same host:

- `/home/deploy/.cache/cadus2_orchestration/zero-api-import-first.log`
- `/home/deploy/.cache/cadus2_orchestration/zero-api-import-second.log`
- `/home/deploy/.cache/cadus2_orchestration/readiness-content-completion.json`
- `/home/deploy/.cache/cadus2_orchestration/readiness-content-completion.md`

The counts are grounded in this checkout and database snapshot. Later curriculum or content changes require another readiness run. Pedagogical completeness and human approval remain unverified.
