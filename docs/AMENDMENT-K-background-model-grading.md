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

---

## STATUS: DECIDED BY THE OWNER (steer note 114, 24 Sep 2026). The design below is the owner's own.

The note-113 five-point discussion is WITHDRAWN. The owner asked why his two suggestions
(the cache and the additional free local-model check of a deterministic "wrong"/"cannot
parse") were never implemented. Answer: the draft above was parked behind the content
run. They are scheduled now.

## The owner's design (note 114, verbatim intent)

1. **The deterministic grader runs first, unchanged.** When it says "wrong" or "cannot
   parse", the app asks the owner's LOCAL model (`qwen-general-8bit` at
   `http://10.8.0.1:8081/v1`, free) ONE question with the problem text, the stored key,
   the answer contract and the learner's text: "Is the learner's answer mathematically
   equivalent to the key? Reply EQUIVALENT or NOT and one line why." A strict prompt,
   temperature 0, one short reply.

2. **Cache.** A table keyed by (item digest, normalized learner text) → verdict, reason,
   model, time. A repeat costs nothing and answers at once. Normalization (the cache key
   only; the model still sees the raw text): whitespace, case, unicode minus, `x^2`/`x²`,
   decimal comma.

3. **Latency budget.** If the local model answers within the request budget (measured on
   staging; see `FLOW/05-local-model-caching-jev.md` §1 for the research numbers), the
   verdict is applied in the SAME reply. If not, the reply says the answer is being
   checked, the next task is served, and the verdict lands in the background (the
   Amendment K flow above: a job row, a worker, a `verdict` event on the topic page).
   The owner's words: "I don't want to wait 30 seconds after an answer."

4. **An EQUIVALENT verdict counts as correct** — for the topic and for mastery (it IS the
   same answer). A NOT verdict leaves the deterministic "wrong" and shows the model's
   one-line reason next to the worked solution.

5. **Logging, audit, cap, fallback.** Every model verdict is logged (`model_call_log`)
   and sampled by the audit like a key (A4 of the draft). Per-learner daily cap:
   **200** equivalence checks (the model is free; the cap bounds abuse). The OpenRouter
   fallback stays OFF unless the owner says otherwise.

6. **Proof items (P3)** keep the rubric flow as drafted above — same worker, same cache.

Implementation: GLM1, on a branch off `main`, after the 84 b staging PASS. Unit tests
with a fake model: the owner's case (the learner text "not a solution, 5" against the key
`verdict = contradicts; D = 5`) returns EQUIVALENT through the fake model; a cache hit
skips the model; the timeout path serves the next task and folds the verdict later.
Staging proof with the real local model; then production per note 95.
