# Framework recovery rehearsal
This report records the disposable database evidence for P5.3 and P5.5. The run used branch `framework/recovery-ops` on 2026-09-06. It touched no production database, deployment, or live Cadus container.

## Reproducible command
Run this command from the repository root:
```sh
CARGO_BUILD_JOBS=1 scripts/check_recovery.sh
```
The script refuses every container except `cadus2-testdb`. It also verifies the published address `127.0.0.1:55434`. Each database name starts with `cadus2_recovery_`. An exit trap removes each database and the run scratch data.

## Snapshot set
The source database contained five fixed test learners:
- an empty event history;
- one ordinary practice history;
- one review history;
- one integrated-task history;
- one 258-event history.

The set contained 268 events. Each learner model had projector version 6 before the backup. The integrated history included the instruction-to-independent-application state that projector version 7 introduced.

## Verified results
The run produced these results:
- All 12 migrations applied to the source database.
- `pg_dump` wrote a readable custom-format archive with event table data.
- `pg_restore` produced the same event count and event fingerprint.
- The script sent `SIGKILL` after the largest model received an uncommitted version-7 projector write.
- PostgreSQL rolled back that write. The model retained version 6, and the event fingerprint stayed unchanged.
- A retry replayed all five samples through sequence values `0`, `3`, `4`, `3`, and `258`.
- The retry persisted projector version 7 for all five samples.
- A second pass used the cache for all five samples and produced identical model JSON.
- The event count and event fingerprint stayed unchanged through both replay passes.
- The release had no migration difference from rollback commit `2ff3c1f`.
- A third database restored the retained archive and recovered all five version-6 models and all 268 events.
- Cleanup left no `cadus2_recovery_*` database and no rehearsal process.

The final line was `RECOVERY OK`.

## Scope boundary
This run closes the deterministic local recovery evidence for process loss, transaction retry, projection replay, backup restoration, and schema-compatible rollback. It does not replace these production checks:
- restore an encrypted production backup;
- replay actual production event shapes and the largest production history;
- start the retained rollback image and test authenticated routes;
- observe production health, readiness, latency, and container stability;
- complete browser checks for ordinary, review, and integrated learner flows.
