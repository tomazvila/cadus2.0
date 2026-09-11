# Production-gated pending content lifecycle

## Scope
`complete_foundations` creates a deterministic review bundle for the Foundations knowledge points that pass the authoritative content audit. The generator reads curriculum plus pending or approved template rows from the selected database. It does not call an external model, update curriculum, write the database, approve content, deploy, or contact production.

The bundle covers six lifecycle needs for every eligible knowledge point:
- `teach`: a distinct worked example that passes the production teach gate.
- `hint_ladder`: a non-answer-revealing ladder that passes the production hint gate.
- `solution_feedback`: audit-clean, decidable authored solution evidence for every exemplar.
- `template`: at least 12 distinct generated practice instances that pass the production template gate.
- `assessment`: the final decidable authored exemplar, which readiness holds out when a knowledge point has at least four decidable exemplars.
- `diagnosis`: a concrete wrong answer, controlled error tag, and learner-facing note derived only from an explicit distractor on an already-gated template; the production diagnosis gate validates it.

Every emitted content-store draft passes `verify_kind` before it enters `drafts.json`. Worked examples are checked again against authored, already-stored, and newly generated practice. Assessment candidates are deduplicated against the same practice set and the worked example. Unsupported derivations remain explicit `skipped` rows with machine-readable reason codes.

## Audit boundary
The command requires the JSON output of `foundations_content_audit.py`. It refuses a report unless:
- schema version and course match;
- all 809 Foundations keys occur exactly once;
- no orphan pending-template key exists;
- at least one knowledge point has an empty issue list.

Only issue-free keys enter the bundle. `coverage.json` must contain exactly one row for each eligible key and each of the six lifecycle needs. A covered row must name its evidence source. A skipped row must name a reason code. Bundle generation fails if that matrix is incomplete or duplicated.

## Verified isolated run
Snapshot `f2be804` plus the gated lifecycle patch produced this result from an empty, migrated disposable database:

| Lifecycle need | Covered | Skipped | Evidence or skip boundary |
|---|---:|---:|---|
| teach | 3 | 74 | Production-gated pending draft; otherwise no distinct closed-rational worked example |
| hint ladder | 72 | 5 | Production-gated pending draft; otherwise no safe ladder |
| solution/feedback | 77 | 0 | Audit-clean authored derivation |
| template/practice | 41 | 36 | Production-gated pending draft; otherwise no accepted parameterized scaffold |
| assessment | 77 | 0 | Audit-clean final decidable exemplar held out by readiness |
| diagnosis | 0 | 77 | No explicit wrong-answer mapping in current gated templates |

The report contains 462 rows for 77 audit-clean knowledge points: 270 covered and 192 explicitly skipped. The review bundle contains 116 pending drafts: 41 templates, 3 teach pages, and 72 hint ladders. Two independent generations were byte-identical.

Import through the loopback local-draft endpoint stored all 116 rows with zero reported model cost in the disposable database. The database contained exactly 41 pending templates, 3 pending teach pages, and 72 pending hint ladders. A second `--missing-only` import made zero calls and stored zero rows. No approved row was created.

## Reproduction
Build the fact and audit inputs:
```sh
PATH=/home/deploy/.local/share/cadus2-tooling/gcc/bin:$PATH \
CARGO_BUILD_JOBS=1 cargo build -p cadus-worker \
  --bin content_audit_facts --example complete_foundations

target/debug/content_audit_facts curriculum > /tmp/foundations-content-facts.json
python3 scripts/review/foundations_content_audit.py \
  --facts /tmp/foundations-content-facts.json \
  --content-root docs/content-foundations \
  --output /tmp/foundations-content-audit.json
```
The audit exits nonzero while any Foundations knowledge point still has an issue, but writes the complete report. Generate the fail-closed review bundle against an explicitly selected database:
```sh
DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate \
  target/debug/examples/complete_foundations \
  /tmp/foundations-content-bundle \
  /tmp/foundations-content-audit.json
```
Validate the import path without writing:
```sh
DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate \
python3 scripts/authoring/import_local_drafts.py \
  --manifest /tmp/foundations-content-bundle/manifest.json \
  --worker target/debug/cadus-worker \
  --missing-only --dry-run
```
Remove `--dry-run` only for an explicitly selected disposable or intended review database. The importer stores accepted rows as `pending`; human digest review remains required for approval.

## Remaining work
This pipeline closes deterministic generation, production gating, pending import, evidence accounting, and honest skip reporting for the current 77 audit-clean knowledge points. It does not make the other 732 knowledge points audit-clean. It also does not manufacture teaching examples or misconception mappings where no independent source evidence exists. P2.3 remains open until usable content covers all required knowledge points and a human approves every digest intended for serving.
