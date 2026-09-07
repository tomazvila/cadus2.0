# Content audit production-gate evidence

`scripts/review/foundations_content_audit.py` checks authored exemplars and every
pending template recipe through the production worker's pure preflight and
`verify_kind` gate. The offline worker adapter creates no database connection or
model client. Existing content thresholds remain unchanged.

Run the complete audit from the repository root with project-local or Nix-provided
Rust and Python:

```sh
python3 scripts/review/foundations_content_audit.py --output target/content-audit.json
```

The command builds/runs `cadus-core`'s `content_audit_facts` and `cadus-worker`'s
`content_template_gate`. Exit 0 means every KP passes; exit 1 writes a report with
findings; exit 2 means evidence could not be read or validated.

## Cached evidence and tests

`--facts` accepts the core fact adapter's JSON. `--template-gate-facts` accepts the
worker adapter's JSON. Both must have the same `curriculum_hash`; every acceptance
must match the complete normalized recipe document. The worker binary also embeds
a build-time fingerprint of the core/worker Rust sources and dependency manifests.
The audit compares it with current source files and checks a separate fingerprint
of the current curriculum files. Cached evidence from a previous gate implementation
or curriculum revision is refused even when both fact files are old.
Missing evidence or a changed
recipe produces `pending_template_production_gate_declined`. A declined sibling
recipe remains visible even when another recipe for that KP passes.

The worker adapter accepts a JSON list of `{kp_id, kind, arguments}` recipes on
stdin and the curriculum directory as its only argument. `pending_templates`
normalizes checked-in arguments and stored-body exports into that shape, removes
server-owned identity/count fields, and deduplicates identical recipes. Metadata or an incomplete payload does not count as a pending recipe; the KP remains absent until a complete importer-shaped recipe exists.

Python unit tests use explicit synthetic or cached facts and never invoke Rust,
a database, or a model. Static exemplar-only tests assert the missing-gate finding;
they cannot certify production acceptance. The Rust `content_template_gate`
integration test exercises the actual offline binary with valid and adversarial
documents and unusable database/model environment settings.

Production-gate acceptance proves import eligibility. Pending storage, semantic
objective coverage, instance diversity, human approval, and serving readiness
remain separate checks.
