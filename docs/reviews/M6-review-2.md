# M6 review, verification round (2026-08-31)

Workflow `wf_49d536f8-225` (restarted once after a process exit), one round, same six lenses. Raised 16, confirmed 12, nine distinct defects. Reviewed tree: `main` at `f5b20c0` (1,800 Rust tests, web check green).

The twenty round-1 defects hold at the route except three: V1 (the F2/F15 gate is inert on a fresh deployment: it reads APPROVED templates and the single documented pass authors everything before review), V2/V11 (the teach half of F15 was never implemented: `gate_teach` ignores `instance_answers`), and V9 (the F12 `caddy reload` re-reads a stale inode after `git pull`). The rest are new or test-quality: the quiz/placement Retry gate (V4, V5, V7 — the F10 fix reached `Session.tsx` only), the restartable quiz clock (V6), the CSP audit blind to inline scripts (V8), the last-rung-only gate pin (V10), and the unpinned diagnosis timing literals (V12).

## Rulings and fix units (wave 2)

### FIX2-M6-A (V1, V2/V11, V10)

- **V1** [blocker] `crates/worker/src/authoring/job.rs:1045` (C1, L5, C6, T3) — The default authoring pass gates every hint ladder against an EMPTY served-answer set, so a give-away rung reaches content_store and the L5 route serves it

- **V2** [major] `crates/core/src/instruction.rs:263` (C1, L4, C6) — gate_teach never reads InstructionSpec::instance_answers, so a teach page that works a served template instance down to its answer is stored

- **V11** [major] `crates/worker/src/authoring/job.rs:490` (C1, L4) — The teach half of the F15 fix is unimplemented and unpinnable: gate_teach never reads instance_answers, so passing them is an equivalent mutant

- **V10** [major] `crates/core/src/instruction.rs:360` (C1, L5) — The L5 give-away gate is pinned only on the LAST rung: a mutant that skips every earlier rung passes the whole Rust suite


**Ruling:** Fix (blocker). Three parts. (1) The gate reads the served material regardless of review status: `served_answers` reads templates with status IN (approved, pending) — a pending template is the material the reviewer is about to approve, so its instance answers gate the ladder and the teach page authored in the same pass. (2) `gate_teach` reads `instance_answers`: a worked example whose final step or answer text names a served instance answer of a DIFFERENT problem than the one it works is rejected (the page may work its own problem to its own answer; the rule refuses a page whose text names another served answer). Thread the same set through `verify_teach`; a mutant that ignores the field must go red. (3) The approve route re-gates: approving a TEMPLATE re-runs the hint and teach gates of that knowledge point against the new answer set and moves a now-failing pending ladder or page to status `rejected` with the gate message as the reason (an approved ladder is not touched; the reviewer sees the rejection in the queue). Tests: the single-pass scenario end to end (author all kinds, approve the template, assert the give-away ladder is rejected, not served); give-away on the FIRST and a MIDDLE rung (kill the last-rung mutant V10).

### FIX2-M6-B (V3)

- **V3** [major] `crates/worker/src/authoring/job.rs:939` (T3, C6, A2) — A stale row whose re-author reproduces the same body never clears its prompt_digest, so the pass pays for the same model call on every run, forever, and prints none of it


**Ruling:** Fix. When the re-author of a stale row reproduces the identical body (same content digest), the pass UPDATES the prompt_digest of the stored row to the current digest instead of dropping the write, reports `Outcome::Refreshed`, counts it in `duplicate`, and the row leaves the stale set; the batch summary prints the count. A test runs the pass twice after a prompt edit and pins one model call total and an empty `--stale` list after the first run.

### FIX2-M6-C (V4, V5, V7)

- **V4** [major] `web/src/views/Quiz.tsx:196` (O3, F-37-1c, F-36-1b) — The quiz Retry re-posts a graded problem_id and eats the next question

- **V5** [major] `web/src/views/Diagnostic.tsx:171` (O3, F-37-1c, F-36-1b) — The placement Retry re-posts an answered probe and drags the view back into feedback

- **V7** [major] `web/src/views/Quiz.tsx:196` (O3, T12, QUIZ-timeout, F-36-1b) — FIX-M6-D1's retry gate never reached the quiz and placement submits: a stale Retry re-posts a spent problem_id and re-arms forever


**Ruling:** Fix. Quiz.submit, Quiz.timeUp and Diagnostic.send pass `retryGate` and `onFail` to useCall exactly as Session.tsx does; a Retry after the probe or question was graded is refused with a non-actionable toast. Fake-timer tests pin: a stale quiz Retry posts nothing and the on-screen question is not consumed; a stale placement Retry posts nothing and the view stays on the live probe.

### FIX2-M6-D (V6)

- **V6** [major] `web/src/views/Quiz.tsx:156` (O3, QUIZ-budget) — The whole-quiz clock restarts at the full budget on every Quiz mount, and the topbar Map is a one-click reset


**Ruling:** Fix. The whole-quiz clock derives from server state, not component state: the D-S6 quiz buffer holds `started_at` (the serve wrote it in M5); the mount effect computes left = budget minus (now minus started_at) and a re-mount resumes the running clock; at or under zero the timeout path runs immediately. A test mounts, advances 30 s, unmounts, remounts, and pins the remaining seconds and the answered count.

### FIX2-M6-E (V8, V12)

- **V8** [major] `web/scripts/check-bundle-csp.mjs:214` (O3, SEC-cookie, spec-4.3, spec-7-S1) — The CSP output gate never checks for an inline script, so a build that the deployed CSP blocks passes npm run check

- **V12** [major] `web/src/views/session/useDiagnosis.ts:46` (O3, DIAG-poll, DIAG-30s) — The SSE-to-poll fallback pins only the RATIO of its two spec literals: doubling both survives the whole web check chain


**Ruling:** Fix. The CSP audit refuses an inline script body (a script tag without src whose content is non-empty) and an inline event handler attribute in every emitted .html; the selftest proves it on a known-bad file. The diagnosis timing tests pin the LITERALS: 2000 ms poll and 30000 ms deadline asserted as numbers (not via the imported constants), and one test asserts the client deadline equals the server PENDING_DEADLINE_SECS read from a shared constant; doubling either goes red.

### FIX2-M6-F (V9)

- **V9** [major] `scripts/deploy.sh:202` (D9, O3) — The F12 fix is inert: `caddy reload` re-reads a stale file after a git pull, and deploy.sh prints DEPLOY OK with the old routing in force


**Ruling:** Fix. Mount the DIRECTORY deploy/ into the container (a directory bind follows file replacement) and point Caddy at the file inside it, or copy the file in with docker compose cp before the reload; state which in the runbook. check_ops gains the failing probe: replace the file with a new inode in the sandbox, reload, and assert the loaded config changed (mutate the guard, confirm red).


## Not fixed

None. Every confirmed finding maps to a fix unit.
