# CADUS 2.0 — Learner Evaluation Issues Log

> **Fix status (2026-09-14, commit `fa6d18fa`, deployed):**
> ISSUE-0…5 fixed by the review agents (commits `fc93a25c`, `adda454b`,
> `9d935270`, `8bdc8063`); ISSUE-6, 7, 9, 10 fixed in `fa6d18fa`
> (quiz progress reconciliation, approval sibling re-stamp, two-letter
> session-id wrap, boot currency sweep). ISSUE-8 fixed in the deployment
> `.env` (model `deepseek/deepseek-chat`, provider order
> `deepinfra,fireworks`) — the repo cannot carry that file. All fixes
> reviewed: fmt clean, clippy `--workspace --all-targets` 0 warnings,
> cadus-core / cadus-store / cadus-web suites green against a migrated
> throwaway database. The grind continues under
> `/tmp/orchestrator.py` (log: `/tmp/orchestrator.log`).


Evaluator: fresh learner persona (no prior maths knowledge).
Method: created a new user (`learner.eval@example.com`), worked the full
learner journey through the app over three sessions (`s_2026-09-14a/b/c`):
signup → verification workaround → placement diagnostic → reviews,
10 + 8 + 1 lessons, an 8-question quiz, 4 × 20-question drills, hints,
problem reports, export, and the curriculum map. Issues logged in order.
Also: redeployed the stack to HEAD mid-evaluation (see NOTE C).

## Issue format

Each entry: ID, date, area, severity (blocker/major/minor/note), description,
steps to reproduce, expected vs actual.

## Issues

### ISSUE-0 — Review task progress counter shows `index 2 / total 1`

- **Date:** 2026-09-14
- **Area:** Session UI / serve payload
- **Severity:** Minor
- **Description:** On the `review-multiplication-tables` confirm task
  (`n_problems: 1`), after a wrong answer the next serve payload reports
  `index: 2, total: 1`. A UI reading these literally renders "2 of 1".
- **Repro:** `POST /api/task/{id}/serve` then a wrong `/answer`, read
  `next.index` / `next.total`.
- **Expected:** Index ≤ total, or the UI computes its own counter.
- **Actual:** `index: 2`, `total: 1`.

<!-- Entries appended in order as the walkthrough progresses. -->

### ISSUE-1 — New-user signup is a dead end: email verification can never complete

- **Date:** 2026-09-14
- **Area:** Auth / onboarding
- **Severity:** Blocker (on this deployment)
- **Description:** A new user signs up (`POST /api/auth/signup`) and gets
  `verification_required`. The verification token is stored in `auth_tokens`
  as a SHA-256 hash only. There is no mail delivery (`crates/web/src/auth/routes/mod.rs`
  says: "M5 has no unit that delivers mail"), `email_outbox` stays empty
  (verified: 0 rows after signup), and there is no admin route to list/spend
  verify tokens or to mark a user verified. A real user can never verify their
  address and therefore can never sign in.
- **Repro:** Sign up with any address → try to log in → `401 invalid_credentials`
  ("Invalid email or password." — also see ISSUE-2) → no path forward.
- **Expected:** Verification link delivered (mail or outbox drain), or an
  operator path to verify a user.
- **Actual:** Dead end; operator had to `UPDATE users SET email_verified_at = now()`
  directly in the database to continue the evaluation.

### ISSUE-2 — Quotient-remainder answers: lowercase `r` graded "incorrect" with no guidance

- **Date:** 2026-09-14
- **Area:** Answer checker / lesson UX
- **Severity:** Major (learner-unfair grading)
- **Description:** The lesson problem text says "Give quotient and remainder."
  A learner typing the extremely common form `23 r 14` gets plain
  `incorrect`, even when 23 remainder 14 is exactly the authored answer (the
  shown worked solution confirms the same numbers). The parser accepts only
  `remainder`, `R`, `R2`-attached forms (`9 R2`, `9R2`) and tuples like
  `(23,14)` — lowercase `r` is deliberately never a marker
  (`crates/core/src/answer/parse/mod.rs`, `at_remainder_marker`).
- **Repro:** Answer a "Compute $750 \\div 32$. Give quotient and remainder."
  problem with `23 r 14` → `outcome: incorrect`; answer `12 R 27` → correct.
- **Expected:** Either accept lowercase `r` (case-insensitive marker), or show
  an input-format hint / a distinct "cannot parse your answer" message so the
  learner knows the format, not the maths, was the problem.
- **Actual:** Silent flat-out incorrect verdict.

### ISSUE-3 — Ordering problems containing thousands-separated numbers are permanently "ungraded"

- **Date:** 2026-09-14
- **Area:** Answer checker / lesson UX
- **Severity:** Major
- **Description:** On "Order from least to greatest: $712$, $89$, $1{,}205$,
  $698$" the learner's natural answer `89, 698, 712, 1,205` (exactly the
  problem's own displayed notation) is undecidable — commas are both the list
  separator and the thousands separator — so the checker returns
  `outcome: ungraded` (`correct: null`, no solution shown). The learner sees
  "Not marked". There is no hint about an alternative format, and every
  re-expression that follows the problem's own notation repeats the loop.
  (Same problem text displays the number WITH the comma, so "type it without"
  is not discoverable.)
- **Repro:** Answer any ordering problem whose items include a 4+ digit number
  with a thousands comma, using a comma-separated list.
- **Expected:** Accept the displayed notation, or guide the learner ("write
  large numbers without the comma") when a submission is ambiguous.
- **Actual:** Silent "Not marked" loop.

### ISSUE-4 — Remediation lesson is scheduled but cannot be served (`no_instruction`)

- **Date:** 2026-09-14
- **Area:** Scheduler / remediation path
- **Severity:** Blocker for the remediation path
- **Description:** After a failed confirmation, the next session schedules
  "lesson | Multiplication Tables | why: remediation (confirm_failed);
  peel-back lesson". Serving that task answers `409`-class
  `{"error":{"code":"no_instruction","message":"Only a lesson with an approved
  teach page has a worked example."}}` — the teach call returns nothing and
  serve refuses. The learner is assigned a task they cannot open at all.
  Session a correctly *blocked* contentless lessons with the `teachable`
  blocker; the replanner/remediation path apparently skipped that check.
- **Repro:** Fail a confirm task → start the next session → `POST
  /api/task/{remediation-lesson-id}/serve`.
- **Expected:** The plan only schedules lessons with approved teach pages (or
  falls back to practice-only remediation).
- **Actual:** Unschedulable task occupying the top of the plan; a struggling
  learner — exactly the user remediation targets — hits a wall first thing.
  (It disappeared from the plan after the session was replanned, so the
  impact is transient — but the wall is hit first by the learners who most
  need the remediation.)

### ISSUE-5 — Lesson dead-ends mid-task when only some KPs have teach pages

- **Date:** 2026-09-14
- **Area:** Scheduling gate / serve route / content readiness
- **Severity:** Major (unfinishable task; plan permanently incomplete)
- **Description:** `opposites-of-integers` defines kp1 and kp2, but only
  `opposites-of-integers/kp1` has an approved teach page. The lesson passed
  the `teachable` gate and was scheduled. I answered two kp1 problems
  (correct); the second returned `task_status: kp_advance, next: null`, and
  every later `serve`/`teach` call now answers `409 no_instruction`. The task
  shows `done: False` in the plan and can never be completed — the learner
  hits a wall mid-lesson at the first KP boundary without coverage.
  Reproduced a second time on `equivalent-fractions` (curriculum defines
  kp1–kp3, only kp1 has an approved teach page). Sessions keep re-scheduling
  both dead lessons (`s_2026-09-14c` lists them again), so the wall is
  permanent until content is authored.
- **Repro:** Enroll fresh learner → work the opposites lesson past kp1 →
  `POST /api/task/.../serve` → `no_instruction`.
- **Expected:** The teachable gate requires an approved teach page for EVERY
  KP the lesson may serve, or serve falls back to practice-only and completes
  the task.
- **Actual:** Dead end; plan task unfinishable.

## Additional observations (lower severity)

- **NOTE A — Completed drill's last answer reports `task_status: continue`.**
  The final answer of every drill (20/20) returned `task_status: continue,
  xp: null` instead of `task_passed`; the plan then showed `done: true`. The
  UI may never show the completion moment for drills.
- **NOTE B — Hint ladders unapproved for many KPs.** `POST /api/task/{id}/hint`
  on a subtraction-with-borrowing review answered `no_hint_ladder` ("This
  knowledge point has no approved hint ladder."). Hints are a core lifeline
  for a novice; coverage should mirror teach-page coverage.
- **NOTE C — Deployment was one commit behind HEAD.** The running web image
  predated the commit that added `POST /api/task/{id}/report`, so that learner
  flow 404'd ("This path serves nothing") until I rebuilt and redeployed
  (`cadus2-web`, `cadus2-worker`, `cadus2-edge`, `cadus2-report-worker`). The
  repo's `scripts/deploy.sh` cannot run on this box (it expects the repo's own
  compose + `.env`; the live stack runs from `~/homelab/services/cadus2` with
  hand-built images), so deploys are manual and can silently lag HEAD. After
  redeploy the report flow worked end to end (queued → producer → critic →
  completed, resolution `no_issue_found`), though it took several minutes and
  one retry; the verifier container logged a `BrokenPipeError` mid-response.
  Worth watching, not a confirmed defect.
- **NOTE D — Ungraded attempts have no learner-side resolution.** After the
  two "Not marked" answers of ISSUE-3, `status.ungraded` stays 2 and the
  dashboard shows a "not marked" tile, but no learner action exists to
  resolve them (the regrade path is admin-only, `/api/admin/ungraded`).
- **NOTE E — Honest ETA may read demotivating.** After a first session with a
  perfect quiz the dashboard extrapolates an ETA ~2 years out
  (`eta: "2028-08-12"` at 2.75 topics/week; it improves to 2027 as pace
  builds). Honest, but consider framing (range, or "pace so far").
- **NOTE F — Lesson ordering quirk.** The dashboard promises "up to 40
  questions, about 2 minutes" for the placement; it stopped after 7 probes —
  faster than promised (good), just misaligned copy.
- **NOTE G — Placement claims credit broadly.** One correct decimal probe
  inferred ~14 topics as "placed", which then needed per-topic confirmations
  anyway. Coherent with the D-F6 "credit, not evidence" design, but a novice
  sees "practiced 0 / inferred 17" and several surprise confirmation tasks.

## What worked well

- Placement diagnostic: adaptive, stopped early, honest (no answer reveal).
- The core loop (serve → teach → answer → feedback) is consistent; worked
  examples are correct; KP advancement (kp1→kp2→kp3) visibly ramps difficulty.
- Quiz gives no per-question feedback by design, and the results screen
  reconstructs every item with a solution sketch afterwards.
- Drills enforce timing (`time_budget_secs: 6`, `countdown: true`) and the
  timing claim/judgement machinery behaved sensibly.
- Ungraded attempts are neutral ("Not marked"), never counted as wrong.
- Dynamic replanning between sessions is coherent: failed confirmations
  produce remediation, then confirmation reviews, and frontier lessons reorder
  by dependent count.
- Export (`GET /api/export`) streams the full append-only event log; the
  curriculum map graph endpoint serves 285 nodes/815 edges.

### ISSUE-6 — Interrupted quiz deadlocks: serve repeats question 1 forever, answers say "already_recorded"

- **Date:** 2026-09-14
- **Area:** Quiz state / session lifecycle
- **Severity:** Major (task unrecoverable; learner cannot finish the quiz)
- **Description:** After a quiz is partially answered and the session is left
  and reopened, the quiz task deadlocks: every `serve` returns the SAME first
  question (`index: 1`, same `problem_id`), every `answer` responds
  `task_status: "already_recorded"` (attempt `...-quiz-1`), and the quiz's
  recorded-answers array in `web_states.quizzes.{task}.answers` is EMPTY
  (length 0). `quiz_complete` can therefore never fire; the plan keeps the
  quiz as not-done and the session cannot finish it. Reproduced repeatedly
  across sessions `s_2026-09-14w/x/y/z` (each recorded two answers, then hit
  `unknown_problem` on the third).
- **Repro:** Start a quiz, answer 1-2 questions, end/reopen the session (or
  interrupt mid-quiz), then serve + answer: same question forever, no
  progress, `remaining` never reaches 0.
- **Expected:** The quiz resumes at the first un-answered question (or restarts
  cleanly); recorded answers must stay consistent between the event log and
  the session state.
- **Actual:** Serve/answer disagree about what is recorded; the quiz is
  unfinishable in that session.

### ISSUE-7 — Approving any content for a topic silently stales the topic's other approved teach pages, blocking lessons

- **Date:** 2026-09-14
- **Area:** Content review pipeline (C6) / readiness currency
- **Severity:** Major (silent content lockout; operator-hostile recovery)
- **Description:** A teach page's approval stamps
  `approved_template_context_digest` at approval time. Later approvals of
  OTHER documents for the same topic (hint ladders, further teach pages)
  change the topic's template-bank context, so the older teach page's stamp
  no longer matches `cadus_template_context(...)` — the teach page silently
  drops out of the readiness index and the lesson becomes unschedulable
  (`blocked: ['teachable']`) with NO signal to the reviewer or operator.
  Observed: after approving hint ladders for `gcf-lcm`, its already-approved
  teach pages went stale and the lesson blocked; same for
  `equivalent-fractions/kp1` after kp2/kp3 approvals. Recovery is not
  discoverable: the admin GET echoes the stale stamp as "the context", and
  re-approving only the obviously-missing KPs does not fix the start-KP's
  stamp — every KP's teach page had to be re-approved (twice) to unblock.
- **Repro:** Approve teach+template for a KP → lesson serves. Approve a hint
  ladder for the same topic → lesson blocks `teachable` with no error
  anywhere.
- **Expected:** Approval re-stamps sibling documents of the topic (or the
  context check is per-document, not per-bank), and the review surface flags
  content it invalidated.
- **Actual:** Silent lockout; operator had to brute-force re-approve every
  teach page of the topic.

### ISSUE-8 — Deployment's authoring model does not exist; automatic content pipeline is dead

- **Date:** 2026-09-14
- **Area:** Deployment / authoring pipeline (A2) / worker config
- **Severity:** Major (blocks all curriculum completion)
- **Description:** The worker daemon runs with
  `OPENAI_MODEL=deepseek/deepseek-v4-pro`, which no longer exists on OpenRouter
  (every call answers 404 "No endpoints found"). The online refill tick
  therefore never authors (`targets=0, inserted=0, skipped_starved=95`), and
  the offline `cadus-worker author` fails identically with the deployed env.
  The curriculum ships content for only ~250 of ~1,090 topics, so every course
  dead-ends at the first unauthored frontier (ISSUE-4/5 were the visible tips).
  Manual CLI passes work only after overriding the model (z-ai/glm-5.3-flash,
  deepseek/deepseek-chat) and provider order; OpenRouter's shared pools then
  429 for long stretches, so bulk authoring is throughput-bound.
- **Expected:** The configured model id must resolve, or startup must fail
  loudly; the curriculum must either ship full content or the pipeline must
  reliably author the rest.
- **Actual:** Silent no-op authoring; courses are uncompletable as shipped.

### ISSUE-9 — Session ids cap at `z` and reuse the id (and its task ids) for every later session that day

- **Date:** 2026-09-14
- **Area:** Session id generation / event dedup
- **Severity:** Major under automated/heavy use; minor for a human day
- **Description:** `View::new_session_id` walks `a`..`z` and falls back to `z`
  when a day has used all 26 (documented as 1.0 carry-over). From the 27th
  session of the day, every `session_start` reuses `s_<date>z` — including its
  task ids. A quiz/lesson task id that already has answered attempts in the
  event log then collides: answers dedup as `already_recorded` while the
  session state restarts empty, producing the ISSUE-6 deadlock for every later
  session that day. Also interacts with ISSUE-6 to make an interrupted quiz
  unrecoverable for the rest of the day.
- **Repro:** Open and end 26 sessions in one day; the 27th reuses `z` and any
  previously answered task id inside it is poisoned.
- **Expected:** Wrap to a two-letter scheme (`aa`, `ab`, …) or refuse and roll
  to the next day.
- **Actual:** Silent id reuse; cross-session state collisions.

### ISSUE-10 — Engine-digest change (any deploy) silently stales EVERY prior content approval

- **Date:** 2026-09-14
- **Area:** Content review currency (C6) / deployment procedure
- **Severity:** Blocker-scale (found during the grind; root of the sweep below)
- **Description:** `approved_review_engine_digest` is a fingerprint of the
  executable renderer/evaluator/gate source. The ISSUE-2/3 fixes changed that
  source, so ALL ~1,464 approvals made before the redeploy (the entire bulk
  content batch of 2026-09-13) stopped matching `read_index` — the whole
  authored curriculum silently became `teachable=false` and every course
  locked. Nothing in the deployment, logs, or review surface flags it; the
  operator discovers it as "all lessons blocked". Recovery required a
  hand-written SQL sweep re-stamping 1,464 rows (the API path would need
  ~800 GET+POST pairs).
- **Expected:** A deploy that changes the engine digest must re-gate or
  re-stamp approvals as a migration step, and report the count; the review
  surface must surface mass staleness.
- **Actual:** Whole-curriculum silent lockout after a routine code deploy.

## Open questions / observations

- Mail delivery: `email_outbox` exists but nothing drains it (documented in
  `crates/web/src/auth/routes/mod.rs`). Verification links must be read from
  the database by hand — except the raw token is never stored, so the outbox
  alone would not help either; a mailer must send at mint time.
