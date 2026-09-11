# Integrated session boundary

## Runtime behavior

`Session` tries the integrated endpoint for a multi-step task before any ordinary serve. Exactly `409 no_integrated_item` selects the per-component fallback. Authentication failures and transient failures stay on the original request path. The existing request wrapper supplies authentication handling and retry behavior, with retries bound to the current task and loading phase.

The integrated view uses synchronous phases for hint and answer requests. One in-flight hint blocks both another hint and submission until its assistance count arrives. One submission holds the gate until its receipt arrives; the successful receipt keeps the form terminal. Continue uses the parent session phase gate to advance once through the existing task-plan/session-end path. Existing ordinary-problem focus and auto-advance effects tolerate the separate integrated receipt. Late replies after unmount cause no progression.

The optional served `hints_used` map initializes field counters. A refreshed screen requests the next persisted rung and sends the current server-returned count. Older services that omit this map retain zero-initialized counters.

## Server audit at base 20888d4

- Integrated serve and answer append idempotent events keyed by session, task, and item digest. Duplicate answer requests return `recorded: false`.
- Hint progression at this base is client-directed: the hint handler reads the supplied rung index without persisting exposure, and grading reads submitted hint counts. This frontend serialization prevents ordinary click races; authoritative assistance needs persisted server state. A separate server-hints lane supplies that change. This frontend accepts its served counter map.
- `Event::IntegratedAttempt` is a no-op in the core projector at this base. No planner/session reader consumes that event elsewhere in core or web. This patch advances the in-memory session after the receipt; persistent completion across reload/replan requires a backend projection change.
- The idempotent answer route computes the retry response from the new submitted payload even when the first event already exists. Replaying the stored receipt would preserve authoritative result display across ambiguous network failures.

This patch changes no database approvals, event schema, server grading, or deployment. Cross-lane server integration needs its own tests.

## Verification

52 targeted tests passed across `integrated.test.tsx` (7), `session.integrated.test.tsx` (8), `session.advance.test.tsx` (16), and `session.lifetime.test.tsx` (21). TypeScript and full frontend lint passed. Frontend invariants passed (33/33 covered). The focused cases cover StrictMode reachability, precise fallback, transport-failure isolation, duplicate-submit and duplicate-Continue gates, final session completion, hint/submission serialization, persisted hint hydration, and expired authentication. The production browser and cross-lane server persistence behavior were not exercised.

Changed-file LOC passed: all seven TypeScript/TSX files remain below 500 lines. The optional full clone scan reported two existing duplicates in unchanged files: `api/demo.ts` with `api-demo.methods.test.ts`, and `helpers/dashboard.tsx` with `session.advance.test.tsx`. A diff against base confirmed those four files are unchanged. No new changed file appeared in the clone findings.
