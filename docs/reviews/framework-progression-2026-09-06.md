# Independent practice and review evidence
## Behavior
- Lesson feedback records the original helped or incorrect attempt.
- Helped answers supply no independent pass-rule evidence.
- The study step hides the solution before the next answer. It resets the server clock through the serve route.
- The next item uses the same topic and KP. A bounded draw rejects every problem digest in the current feedback chain. An exhausted pool reports unavailable and retains the pending practice request.
- A fresh unassisted answer records `independent_after_feedback`. A correct answer creates a replay-derived confirmation obligation after the original task and three other task closes.
- A complete review appends `review_result`. Ungraded answers and help create uncertainty. A conflict between the weighted trajectory and final answer creates uncertainty. Skill-level evidence prevents a strong result on one KP from hiding weak evidence on another KP.
- An inconclusive review awards zero XP and changes no FIRe state. Each uncertain topic/KP receives a one-item confirmation task. Its own identity and KP survive plan persistence.
## Policy rationale
| Rule | Why | Calibrate |
|---|---|---|
| Weighted threshold 0.65 and positional weights | Preserve the configured trajectory signal while the new conflict check prevents unsupported binary decisions. | yes |
| Independent pass-rule evidence | A displayed solution supplies support, so an answer after that support requires a fresh item. | yes |
| Three intervening tasks | D-F8 specifies separation from immediate feedback. Task closes supply observable evidence of that separation. | yes |
| Eight draw attempts | Bound request work when the exemplar pool has no fresh item. This is an engineering limit. | no |
## Compatibility and boundaries
- Legacy review events omit `confirmation_skills` and `inconclusive` on serialization.
- Historical attempt evidence remains unchanged. New attempts identify skills with stable `topic/KP` keys.
- Existing valid rework scratch remains readable. It supplies no fresh independent evidence.
- The immediate feedback loop covers lessons. The configured terminal lesson-failure rule still closes a failed task and queues its existing remediation. Reviews use targeted confirmations; quizzes and other task types retain their current feedback flow.
- Confirmation delay counts completed lesson, review, and quiz events. A task type without a result event supplies no task-close evidence.
- This code supplies engineering evidence. It establishes no empirical retention or mastery guarantee.
## Verification
- Eight core regressions cover trajectory conflict, per-KP attribution, legacy wire fields, independent confirmation, and duplicate task-close evidence.
- Twenty-nine HTTP regressions cover fresh same-KP practice, exhausted-pool recovery, pass rules, immutable event shape, real review close, and targeted follow-up events.
- The complete frontend suite passes: 56 files, 880 tests. TypeScript and ESLint pass.
- Clippy passes for core, store, and web across all targets. The final changed test targets also pass.
- Source limits pass: zero clones, zero unused public items, no checked source file at 500 lines, and no function above the configured complexity limits.
- The initial full core suite passes. The broad web run was stopped after successful completed binaries so the integration owner runs the consolidated workspace gate once.
- No live learner data, deployment, model API call, or content approval changed.
