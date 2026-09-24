# AMENDMENT K — background model grading (DRAFT, not built)

Status: draft for the owner (steer note 11 decision 3 a). Nothing here is implemented.
The only built part is the worked-solution reveal of decision 3 b (worktree amend-k).

## The requirement change

The platform's hard rule was "no model ever decides a verdict" (REQUIREMENTS.md), which
made the learner wait for a machine verdict on every answer, and gave the written-proof
item no verdict at all (the honest "ungraded"). The real requirement behind it was the
owner's: **he does not want to wait 30 seconds after an answer.**

Amendment K splits the requirement in two:

- **Code decides wherever code is able to decide, at once** — unchanged (every
  AnswerContract verdict; the grader stays deterministic and instant).
- **Where code cannot decide (a written proof, a free explanation), a model MAY grade —
  but never on the request path.** The learner's answer is saved at once, the worked
  solution is shown at once (decision 3 b builds this reveal), and the learner continues
  with the next task. The model verdict arrives later, in the background.

## IDs of REQUIREMENTS.md that change

| ID | Was | Becomes |
|---|---|---|
| L2 | the learner waits for the verdict of each answer | instant reply; a pending model verdict may arrive later for items with no deterministic checker |
| T1 | the teach/practice flow blocks on the verdict | the flow continues to the next task; the verdict event lands when the worker finishes |
| T2 | the answer route grades synchronously | the answer route saves, reveals, and enqueues a grading job when the contract has no deterministic check |
| T4 | the review screens show the verdict immediately | the review screens show "verdict pending" until the worker lands |
| C4 | content may only serve items with a deterministic check | proof/explanation items (P3-style) are servable with a background grader |
| V2 | the verdict is a fact before the next task | the verdict is eventual; mastery math reads it when it lands |
| A4 | the audit only audits machine-checkable keys | the background grader's verdicts are audited with the same audit machinery |

## The flow

1. Learner submits a written proof. The grader finds no deterministic checker.
2. The reply: answer saved, `solution` (the worked proof) shown, next task served.
   A `grading_jobs` row is written (`status: pending`, the attempt id, the problem).
3. A worker (one per app, low concurrency) takes the job and grades with:
   - **Model**: a fast cheap OpenRouter model first; the owner's local model
     (qwen-general-8bit at 10.8.0.1:8081) as the fallback.
   - **Rubric**: 5 to 8 yes/no criteria, AUTHORED WITH THE PROBLEM (by the content
     campaign's author role, stored beside the exemplar — one JSON rubric per P3 item:
     "does step 2 state the induction hypothesis", "is the base case verified", ...).
     The model answers each criterion yes/no with a quote from the learner text; the
     verdict is `pass` when all (or all but one) criteria are yes.
4. The worker folds a `verdict` event (pass / needs-revision + per-criterion detail).
   The learner sees it on the topic page and in the review; the run continues.

## The cap

Per learner, per day: **20 background gradings** (a learner writes at most a handful of
proofs a day; the cap bounds model spend and abuse). Above the cap: the reply says the
comparison will be reviewed by a human, and the P3 item counts as done-but-unverified
(it does not block the topic's progress; it never counts for mastery).

## The open question — does a background verdict count for mastery?

**For (it should count):** the proof topics are otherwise unreachable (a learner reads the
teach page but cannot complete the course); the rubric is authored, deterministic in shape,
and audited; the delay is minutes, so the mastery math reads it in the same session; the
platform already accepts model-judged content at the content boundary (the campaign's
adjudicator).

**Against (it should not count):** mastery is the currency the whole money system trusts;
a model verdict is falsifiable in a way the grader is not (a learner can phrase a proof to
satisfy rubric yes/no items without proving); the mastery math reads the verdict only when
it lands, so a mastery credit could arrive for an attempt the learner already abandoned;
the audit shows the grader is right only as often as the calibration says.

**The honest middle** (what the draft recommends): a background verdict marks the P3 item
`verified_by_model` and completes the topic, but the course's course_done requires that the
audited agreement of the background grader on that course is at or above the audit floor;
below it, the course regresses to "read the teach page" until the rubric improves.

Research background: FLOW/05-local-model-caching-jev.md.
