# Historical v1 miss review and integer remainder contracts

## Read-only recovery scan

`scripts/review/historical_misses_export.sql` exports one complete tenant event snapshot in a repeatable-read, read-only transaction. It applies both the tenant session setting and an explicit tenant filter. Export includes all events so later corrections remain available to the reviewer.

Use the existing secure PostgreSQL connection configuration and an explicit learner UUID in the learner_uuid shell variable:

```sh
umask 077
psql -X -qAt -v ON_ERROR_STOP=1 -v user_id="$learner_uuid" \
  -f scripts/review/historical_misses_export.sql > /private/path/history.jsonl &&
python3 scripts/review/historical_misses.py /private/path/history.jsonl \
  > /private/path/historical-review.json
```

The scanner validates the exported count/high-water mark, dense unique sequences, tenant identity, stored/payload version agreement, JSON structure and attempt identity before emitting a report. A missing or partial header, truncated stream, unsupported version, mixed tenant or malformed outcome fails closed with no report on standard output.

Every stored schema-v1 incorrect attempt without an explicit outcome receives `human_review_required`. This includes apparently ordinary arithmetic misses: the recorded fields cannot establish whether the old checker was certain. Explicit outcomes remain explicit; v2 attempts are counted separately. A missing payload version uses the exported database version as evidence and is marked as absent in the payload.

The report preserves stored payload export text and its SHA-256, candidate sequence/identity, and later regrade evidence. Sorting by sequence makes output deterministic even when input event rows are reordered. The snapshot digest includes every event and its version. Later corrections do not clear the original missing-provenance flag: the report gives their evidence to the reviewer without inferring a review decision.

PostgreSQL stores payloads as JSONB. Exported text is preserved exactly by the scanner; pre-storage whitespace, key order and other lexical distinctions are already unavailable. The scanner never rewrites events, changes projection state, approves content, calls an LLM or submits a regrade.

## Recovery boundary

A3.8 remains open. This tool supplies a conservative operator review queue; it cannot recover uncertainty that v1 never stored. No production snapshot or historical-miss scan was performed in this slice. The existing `/api/admin/ungraded` path handles originally ungraded attempts and does not provide a historical-incorrect-to-ungraded correction workflow. A reviewed append-only correction remains a separate explicit operator action; the original observation must stay intact.

The existing replay regression demonstrates that an explicitly appended ungraded correction preserves source observations and restores projected uncertainty. That fixture does not claim the original v1 producer wrote uncertainty fields.

## Proven remainder annotations

Eight arithmetic-core exemplars now carry `quotient_remainder` with their literal positive divisor. `scripts/review/foundations_remainders.py` independently derives the quotient and remainder using integer division, checks the complete authored prompt/answer, and emits `foundations-reviewed-remainders.jsonl`. The Rust regression checks each policy against current curriculum and rejects a same-total pair whose remainder exceeds the divisor.

The reviewed cohort consists of four division-with-remainders items, two long-division-one-digit items and two long-division items. One division-with-remainders prompt asks for verification; its solution sketch explicitly checks the quotient/remainder decomposition. Reasoning prose remains outside deterministic correctness.

Four polynomial/synthetic-division remainder exemplars remain unannotated. Their polynomial quotient/divisor cannot satisfy the existing integer quotient/remainder contract. This is a representation gap, not missing integer arithmetic.

Foundations now has 322 explicit policies out of 1,695 exemplars: 206 exact, 71 closed label, 27 unit, 10 required form and eight quotient/remainder. The 1,373 residual candidates include four polynomial remainder forms. No original answer, problem wording, solution or legacy corpus fixture was changed.

## Verification

- Scanner regressions: 14 passed, including the pinned 94-event historical fixture with four review candidates.
- Focused core contract/inventory/historical replay tests: six passed.
- Curriculum parity and live-oracle tests: 18 passed.
- Disposable database export-to-report test: one passed. App/admin exports agree, another tenant stays excluded, a write inside the export transaction fails with SQLSTATE `25006`, and exported payloads remain unchanged.
- Focused core/store Clippy passes with warnings denied. Formatting, owned-file LOC (five files), complexity (43 functions), and the workspace public-dead-item scan pass. The whole-tree physical LOC check still finds three unchanged baseline files over 500 lines: crates/core/src/config.rs (599), crates/core/src/projector/handlers.rs (517), crates/core/src/retention/state.rs (504). This slice does not claim a passing whole-tree quality gate.
- The eight-row remainder manifest reproduces byte-for-byte. Removing exactly those eight new policies from the canonical dump independently reproduces the previous 4,098,390-byte dump and SHA-256 `483a1016a66eacc8088b2b4625f9ddc383e027ea6c3deadc64c802557f6395df`.

Current canonical dump: 4,098,872 bytes excluding the CLI newline; SHA-256 `150681b167808c756f25ec11fdbb9674d120650879c00466790d6e98808142b7`. Both parity pins are refreshed. Annotation changes leave the canonicalizer input answers unchanged.

The historical fixture source SHA-256 is `824d35e2ed810ba5aea0b3e7404a03635cd488de37f4e6c37885456e8629b028`; its deterministic report SHA-256 is `71550c00e5cede9b378adfd333850b82d6bff1eb9257d380cb61cc4e70566972`. These are test-fixture observations, not production findings.

No full integration gate, production scan, content approval, deployment or learner-data correction was performed in this slice.
