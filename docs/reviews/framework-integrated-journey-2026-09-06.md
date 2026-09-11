# Integrated instruction and delayed application
## Behavior
The production readiness policy requires approved instruction before an authored integrated application. The existing teach endpoint reads the first credited skill's concept and worked example, persists its hand-off, and the UI waits for the learner to continue before serving the scenario. Delayed assessments skip reteaching.

An instructed, correct, unassisted integrated application starts a seven-day delay. The planner selects a distinct unseen authored item that covers the same component topics. No alternate item means no fabricated assessment. The assessment carries its source item and delay in both serve and attempt events. An assessment is a whole integrated task; its component-serving endpoint refuses the request.

The projector rebuilds a compact journey index from integrated events. Served assessments stay pinned through replay. Completed assessment receipts remain reachable in the same session; the plan derives their completed status from the replayable index. Projector version 7 invalidates older cached models.

## Verified
- Database-backed HTTP journey: approved instruction, independent source success, no early assessment, test-clock advance by eight days, a different authored scenario, and one recorded delayed assessment.
- Recovery simulation: restart the application and delete rebuildable model/UI state after the assessment hand-off and after the committed answer. Event replay restores the item and completion; retrying a different answer returns the original receipt and adds no event.
- Core journey: seven-day boundary, unseen-item requirement, instruction/independence prerequisites, full and incremental replay.
- Core regression counts: library 137; outcome events 10; event parity 7; projector 17; regrade 14; retention probes 8.
- Web regression counts: integrated routes 14; teaching routes 3; UI integrated session 9. TypeScript and selected ESLint passed.
- Core and selected web Clippy passed with warnings denied. Formatting and diff checks passed.

## Scope
The seven-day integrated delay is an operational, uncalibrated first assessment. Instruction comes from existing approved skill content. The deployment still needs an approved instruction page and an unseen authored alternative covering the component set. This change approves no content and performs no live deployment or browser walk. Recovery uses deterministic database-state loss and router restart, rather than killing a production process.
