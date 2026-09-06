# Integrated hint authority and submission replay

Hint reveals append `integrated_hint_revealed` before returning text, under the
existing tenant advisory lock. The idempotency key includes session, task, item
digest, field and rung. Duplicate requests append one event. An invalid or
exhausted rung records no reveal. Assistance reads only committed reveal events
for the same identity; edited content has separate reveal state.

The serve response adds `hints_used`, a field-to-count map that survives reloads.
Hint responses return the persisted field count. Answer routes discard client
counts and derive field and whole-task assistance from the log. Omitted fields
retain their assistance. A submitted final field id cannot borrow another
field's reveal state.

New integrated attempts retain a frozen grade with a serde-defaulted optional
field. A duplicate attempt reads the first row by tenant and attempt key and
returns that grade with `recorded: false`; changed answers and later hints do
not change the receipt. Legacy rows without a snapshot derive their receipt
from stored outcomes, assistance, method and credited skills. They never grade
the retried payload. Legacy events did not retain notation warnings or the
distinction between omitted and empty answers; these presentation flags use
conservative defaults. Existing legacy events still deserialize.

The projector treats reveal events as no-ops. They survive event replay without
changing skill progression or counting an extra integrated exposure. The first
persisted attempt also writes task progress done with one answered integrated
problem in the same transaction; duplicate receipts leave that progress unchanged.
A refreshed plan therefore excludes the completed task from its unfinished set.
No migration,
production write, content approval or deployment is part of this change.

Database-backed tests cover duplicate hints, out-of-range hints, refresh through
a fresh router, event decoding, session/task/digest isolation, forged and omitted
assistance counters, a forged final field id, conflicting duplicate answers,
frozen receipts and legacy receipt fallback.

Known baseline check: core projector code at 3854761 declares version 5, while
`the_fold_stamps_the_projector_version_and_the_config_hash` still expects 4.
This slice leaves that cross-lane version assertion unchanged.

Verification: all 11 integrated route tests, 21 core event tests, seven core
integrated-grade tests and 12 store state/fold tests pass. The broader projector
suite has 16 passing tests and the baseline version assertion above. The
all-target Clippy run also reaches a baseline test-only compile failure in
`crates/web/src/report/probe.rs`: an Attempt initializer lacks feedback_practice.
The root integration lane owns those two baseline fixes.
Production-library Clippy for core/store/web and focused integrated-route test
Clippy pass with warnings denied. Formatting and diff checks pass.
