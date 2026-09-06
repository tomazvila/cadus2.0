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
- The immediate feedback loop covers lessons, reviews, drills, and legacy per-component multi-step tasks. Supplemental practice stays outside the original assessment count and weighted score. The configured terminal lesson-failure rule records its failed result and remediation once; supplemental fresh practice remains open until independently answered. Quizzes retain blind receipts and provide fresh practice after their explicit batch reveal.
- Confirmation delay counts completed lesson, review, quiz, and drill events. A task type without a result event supplies no task-close evidence.
- This code supplies engineering evidence. It establishes no empirical retention or mastery guarantee.
## Verification
- Eight core regressions cover trajectory conflict, per-KP attribution, legacy wire fields, independent confirmation, and duplicate task-close evidence.
- Twenty-nine HTTP regressions cover fresh same-KP practice, exhausted-pool recovery, pass rules, immutable event shape, real review close, and targeted follow-up events.
- The complete frontend suite passes: 56 files, 880 tests. TypeScript and ESLint pass.
- Clippy passes for core, store, and web across all targets. The final changed test targets also pass.
- Rust source limits pass: zero clones, zero unused public items, no checked source file at 500 lines, and no function above the configured complexity limits. The separate web clone scan reports two pre-existing fixture/object duplicates outside the quiz-practice changes.
- The initial full core suite passes. The broad web run was stopped after successful completed binaries so the integration owner runs the consolidated workspace gate once.
- No live learner data, deployment, model API call, or content approval changed.

## Supplemental assessment practice
- The server assigns `Attempt.feedback_practice` from persisted feedback metadata. Request-body fields do not assign this evidence.
- A supplemental answer supplies no original review question or trajectory weight. The last original assessment answer determines its quality tier. A final miss holds the task open until fresh independent practice succeeds.
- Supplemental drill questions have no countdown. The UI labels them Independent practice instead of adding them to the assessment count.
- Exhausted pools return `fresh_practice_unavailable`. Feedback names the block and preserves a retry action and the saved answer.
- An early targeted confirmation leaves the delayed obligation intact. Its resolution requires the specified number of distinct task closes.
- Added checks cover original weighted score after three successful practice items, final-slot review/drill recovery, server-owned evidence, explicit unavailable/refill recovery, and blind quiz receipts.

## Quiz result and independent practice
The final original answer now appends one server-owned `QuizResult`. Receipts remain blind three-field acknowledgements, including the final receipt. `POST /api/task/{task_id}/quiz-result` releases the recorded batch only after that event exists. The same endpoint accepts `practice: true` after studying and starts a persistent, deduplicated missed-skill queue. Each supplemental item must have a fresh text digest and carries the server-owned practice marker. Supplemental attempts leave the original quiz count, result, XP and FIRe evidence unchanged. The quiz UI hides all studied solutions during untimed practice and resumes that mode on reload.
An unknown answer makes the batch inconclusive. Its result earns zero XP and changes no FIRe, practice stamp, high-score streak or retake classification. The reveal names the pending verdict and the individual unknown answer. Historical quiz events default to a conclusive result.
Verification: raw HTTP blind-receipt tests; close/reveal authorization gate and fresh-practice lifecycle; unchanged original result and question count after supplemental success; unknown-answer batch; historic event compatibility; core projection non-effects; quiz UI reveal/hide/practice and 40 prior clock, reveal, timeout and card regressions. Route fixture is regenerated from the live Axum router.
Terminal lesson failure also retains a fresh-practice obligation. Its original failed result and remediation remain unchanged, and supplemental success closes the practice with no additional result or XP. The session plan restores pending practice after a reload. A focused HTTP regression verifies the original failed close, fresh same-KP item, replan recovery, one result/remediation, and no extra XP.


## Drill completion evidence
A dedicated `DrillResult` records the stable task ID once the original batch and required independent practice finish. It adds no XP or grading claim. Replay deduplicates completion by task ID, so three distinct completed drills satisfy the delayed-confirmation spacing and repeated event application cannot shorten that spacing. A full 20-original-item HTTP drill verifies that its final miss creates no completion, fresh independent success creates exactly one, and a repeated submission creates none. The new event follows the existing timestamp, session and schema-version readers.
