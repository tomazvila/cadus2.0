# Foundations zero-API review bundle

This bundle contains pending-review scaffolds derived from the curriculum and a disposable content-store snapshot. It makes no approval decision. The importer uses a loopback fixture endpoint with a placeholder key and records zero model cost.

## Contents

- `drafts.json`: 806 missing-pair candidates that passed the existing template, teaching, or hint gate: 46 templates, 47 teaching pages, and 713 hint ladders.
- `review.json`: all 809 knowledge points, authored constraints, existing-row counts, draft argument digests, refusals, source solution proposals, assessment candidates, and diagnostic decidability.
- `summary.json`: generation counts and source inventory, with pending and approved statuses kept separate.
- `manifest.json`: input to the pending-only importer.

The exact-answer derivation accepts closed rational calculations whose authored answer verifies under its typed contract. A template has at least twelve distinct operand choices; worked examples use separate choices. Production gates enforce template coverage, answer agreement, hint secrecy, and worked-problem identity. Unsupported word-problem models and symbolic derivations remain explicit refusals.

## Review requirements

These are instructional scaffolds. A gate pass verifies the implemented structural and mathematical contracts; a reviewer must still check objective alignment, operand bounds, explanations, and useful hint progression. Source solution sketches are proposals and do not update curriculum. Assessment variants require explicit held-out designation and independent-family review. Their existence does not establish assessment readiness. Generic method text does not establish pedagogical completeness.

The argument digests identify proposal inputs. Approval must bind the actual stored document digest through the existing approval workflow after review. This tool never sets `approved` and never changes authored curriculum.

## Reproduce and import

Set `DATABASE_URL` to the explicit disposable database and use the checkout's normal Rust environment. Generation reads that database and writes a new output directory:

```
cargo run -p cadus-worker --example complete_foundations -- /path/to/output
python3 scripts/authoring/import_local_drafts.py \
  --manifest /path/to/output/manifest.json \
  --worker /path/to/cadus-worker --missing-only --concurrency 4
```

The importer skips every `(knowledge point, kind)` already pending or approved. Repeat the import to verify zero new rows and zero fixture calls. Rerun `cadus-worker readiness --course foundations --json ... --md ...` for approved-serving readiness; pending drafts do not satisfy approval gates.
