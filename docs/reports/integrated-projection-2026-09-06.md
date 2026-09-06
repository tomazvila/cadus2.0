# Integrated knowledge-point projection

## Evidence rule

An integrated attempt completes its session/task once in the projector's light task-completion index. Its first receipt determines the credited evidence; duplicate events for the same session/task add no completion or later credit.

A knowledge point becomes `passed` only when all these conditions hold:

- Its exact `topic/kp` key appears in `skills_credited`.
- A step or final field names that same key and has outcome `correct`.
- The field has an assessment contract other than `none`.
- Both the topic and its knowledge point exist in the loaded curriculum.

Malformed keys, unknown topics or KPs, incorrect fields, ungraded fields, and uncredited fields add no mastery state. The event's broad `solved` flag and reasoning prose grant no credit.

The update writes only `kp_progress`. It grants no topic status, whole-topic completion, XP, ability, repetitions, scheduling interval, or neighboring-topic propagation. Assisted correct fields receive the same KP marker that an ordinary passed assisted lesson receives, with the narrower scope of their explicit field skills. Assistance remains in the event. Only completely unaided events and fields write the skill-specific practice index that closes independent confirmation work.

## Replay and cache behavior

Task-completion and eligible practice indices rebuild during light replay. KP markers write only during full/new-event replay. The cached learner document persists these markers. Projector version 6 invalidates caches created when integrated attempts were ignored; old integrated logs then rebuild their evidence.

The web session plan reads completion from the separate persisted `WebState.tasks` document. This core patch maintains the projector index and KP model; the integrated answer transaction needs the corresponding web task-progress write for reload/replan completion. The separate server lane supplies that atomic write and its reload/replan database test; root integration composes the two commits. No web-state, hint, event-schema, or deployment file changes in this patch.

## Verification

57 distinct targeted tests passed: eight new integrated projection regressions, seven existing projector unit tests, 17 projector integration tests, eight retention tests, ten outcome tests, and seven event-parity tests. New regressions cover serialized model restoration, every incremental split, duplicate task receipts, partial/final credit, malformed keys, ungraded and uncredited fields, assistance, and light replay. Core all-target Clippy passed with warnings denied. Formatting, changed-file LOC, whole-repository Rust dead-code checks, and new-module complexity checks passed. The optional full clone scan reported 15 existing clones across unchanged files; none involve this patch's files. Database reload/replan behavior belongs to the separately tested server transaction change.
