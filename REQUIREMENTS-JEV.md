STATUS: PARKED 2026-09-18 — not necessary for launch.

# Cadus 2.0 — Requirements Amendment J: the Jev decision model

> **STATUS: DRAFT — NOT RATIFIED.** This amendment is pending review by the
> pedagogy reviewer (against The Math Academy Way). Until that review lands,
> `REQUIREMENTS.md` remains the sole ratified authority. Nothing here is
> implemented-on-faith; implement only what the reviewer passes.

- **Version:** REQUIREMENTS.md v2.0 + Amendment J (draft 1)
- **Date:** 2026-09-16
- **Amends:** V1, V2, A2, C6, T2, A7 (extends); T4 (note); §7 Non-goals, §8 Open decisions (restated lines)
- **Adds:** J1–J11, J-O1–J-O5, B2-O1–B2-O3, B5-O1
- **Motivation (one sentence):** the decidable answer grammar (§5/V1) blocked
  ~1,300 knowledge points (multipart, tuple, proof, and calculus answer forms —
  see `CONTENT-AUTHORING-REPORT.md`); the Jev-verified author-time gate unlocks
  those KPs without moving a single token onto the runtime grade path, so
  runtime determinism (L2, T1, C4) is untouched.

Requirement IDs are stable. Cite them in reviews and commits. `REQUIREMENTS.md`
§1–§8 carry over verbatim except where the cleanup section (§C below) amends them.

---

## J. The Jev decision model (TypeSafe "System One")

Jev is a judgment service: given a `state` and a set of `questions`, it returns,
per question, a probability distribution plus a confidence value. It answers
"how likely is this state to be correct/adequate per criterion", not "write me
content". That makes it a verifier, triage tool, and reviewer — and nothing at
runtime.

Measured on real CADUS content during the pilot (cited as fact below):

- Reachable through the **existing** OpenRouter integration: one endpoint, one
  model id, no new vendor account, no new key.
- Correct answer → noul **0.99**; wrong answer → noul **0.01**.
- 35-exemplar verification run: **28 auto-accept / 3 mid-band / 3
  false-rejections — every error was in the safe direction** (Jev doubted
  correct keys; it never blessed a wrong one).
- Corruption test: 20 correct answers silently perturbed (±1/±2) → **16/20
  caught** (noul < 0.5), 3 mid-band, **1 missed** (noul 0.96). Single-shot
  false-positive rate ≈ **5%**, in the dangerous direction. This drives the
  3-vote AND-gate of J2.
- All observed errors had two causes: **LaTeX-heavy state formatting**
  ("jaggedness" — Jev accuracy degrades with complex markup in `state`) and
  **vector direction conventions**. Both are mitigations, not mysteries (J2).
- Cost: pilot ≈ **$0.0007** (35 calls + adjudications + corruption test);
  extrapolated full 3-vote verification of ~1,900 exemplars ≈ **$0.15**.
- Limits: 250k tok/s, 1200 req/min, output tokens free, 64k context (32k for
  `state`).

### J1 — Jev access contract

- Endpoint: `POST https://openrouter.ai/api/alpha/decisions` (OpenRouter alpha
  surface). Model id: `~typesafe/jev-latest` (currently resolves to
  `typesafe/jev-1.13-20260917`; log the resolved id, not the alias).
- Auth: the existing `OPENROUTER_API_KEY`. **No direct typesafe.ai dependency** —
  OpenRouter is the integration surface. If OpenRouter drops the alpha route,
  Jev-verified gates degrade to the C6 review queue (fail-safe, never fail-open);
  a replacement surface is a new amendment, not a hotfix.
- Request shape: `{"model", "state": string-or-JSON, "questions": {name:
  {type: "noul" | "choice" | "score", instructions, criteria}}}`. All questions
  in one call evaluate in parallel against the one state — batch questions
  aggressively; a call is the unit of cost.
- Answer objects carry probability distributions + confidence; noul answers
  carry a single 0–1 value.
- **T6 applies unchanged**: every Jev call logs model id (resolved), tokens,
  latency, cost, and purpose (`jev_verify`, `jev_triage`, `jev_review`,
  `jev_distractor`), into `model_call_log`.
- Rationale: one vendor surface, one key, one telemetry row shape — the pilot's
  $0.0007 must stay auditable at 1,900-exemplar scale.

**T5 applicability (B6 decision — no exemption):** T5 applies to Jev calls
**as written**. Caching: the route does not support it, so the clause has no
effect. Reasoning limit: Jev makes no reasoning tokens, so the clause has no
effect. Provider order: one provider serves the model, so the clause has no
effect. Do the applicability check again when OpenRouter moves the route out
of alpha, and on each change of the resolved model id. **The Jev client sends
no caching, reasoning, or provider parameters** — verified this session: the
route *silently ignores* unknown parameters (a probe with `provider_order`,
`reasoning`, and a question-level cache key returned a normal answer, no
error), so a client sending them would falsely believe caching is active.

**Route facts (verified this session; J1 and this decision cite them):**

- `usage` fields returned: `input_tokens`, `output_tokens`, `cost` (USD).
  No cached/reasoning token fields exist.
- Resolved model id form: `typesafe/jev-1.13-20260917` (versioned, dated).
- Unknown parameters: silently ignored, at both request and question level.

**T6 logging for Jev rows (B6 decision — no schema change):** the existing
`model_call_log` columns stand (additive-only amendment; making
`input_tokens_cached`/`reasoning_tokens` nullable is rejected). Per Jev row:

- `input_tokens_uncached`: all input tokens; `input_tokens_cached`: 0;
  `reasoning_tokens`: 0. The `purpose` value identifies a Jev row, so a zero
  is not ambiguous.
- `output_tokens`: log the count the API returns (vendor bills 0; the log
  records what happened — measured 20–48 per call).
- `cost_usd`: computed from input tokens and the price table ($0.042/Mtok
  input); NULL if the price is unknown.
- `model_id`: the resolved versioned id from the response, never the alias.
- `provider`: the provider name (`TypeSafe`). `request_id`: the OpenRouter
  request id (`gen-dec-…`). `user_id`, `session_id`: NULL (offline call).

**Purpose values (underscores, matching J1):** `jev_verify`, `jev_review`,
`jev_distractor`, `jev_triage` (post-B4, applies only to the optional advice
call), and **`jev_benchmark`** for J10 runs — benchmark calls never change
production counts. On ratification, update the two comments in
`crates/worker/src/model_log.rs` (`PURPOSE_DIAGNOSIS`/`PURPOSE_AUTHORING`,
"T2 names no third spender") **in the same commit** as the new constants.

**Dashboard rule (B6 decision):** the cached-token-ratio panel excludes rows
with a `jev_` purpose — otherwise every Jev run reads as a caching regression.

**Model pinning (B3 decision):** every call uses the **versioned model id**
(`jev-1.13.0`, or the resolved OpenRouter id such as
`typesafe/jev-1.13-20260917`), never the rolling alias. The resolved id is
logged per call (T6) and recorded in content_store metadata. Acceptance
results are valid per resolved id; a model bump re-triggers J-O2 and the J10
benchmark.

### J2 — The Jev-verified answer gate (new author-time acceptance path)

**Scope (B1 decision):** the J2 gate applies **only** to answer kinds that have
a deterministic runtime grading rule in the V1 partition table (see C-V1).
Tuple, coordinates, vector, matrix, set, and list are a **V1 grammar
extension** under this amendment, not Jev-only kinds. Proof-format,
free-form multi-step, and expression forms outside the extended grammar go to
teach-plus-exemplar only and never reach this gate.

**Division of labor (B1 decision):** at authoring time, **code does the
arithmetic check of the answer key** — component-wise comparison,
substitution, or the SymPy oracle (V3). **Jev votes only on whether the stem
agrees with the formal specification and on `likely_error`; Jev does not vote
on "this number is correct".** Rationale: the jev-1.13 model card lists
mathematics and numeric precision as known failure modes — "Jev is not a
calculator". The corruption pilot measured its numeric edge (5% single-shot
false-positive); the gate must not depend on it for arithmetic truth.

Answers **outside** the decidable grammar (§5/V1) may be accepted at authoring
time as **"Jev-verified"** when, for every rendered sample checked:

**Verification unit (B5 decision):** the **content digest** (the template),
not the KP — the answer expression being verified belongs to a digest. Each
digest gets **two sample sets**, seeded deterministically from the digest so a
re-run selects the same samples:

- **Code samples:** ≥ 200 per template (they are free), covering every
  constraint boundary and every corner of the parameter space. This is the
  stage that finds arithmetic errors (B1) — 3 samples would be the wrong
  number for it.
- **Jev samples:** one per distinct constraint boundary plus 2 interior;
  **minimum 3, maximum 8**.

**Vote composition (B5 decision).** One Jev request evaluates all its
questions in parallel against one state, so the `likely_error` Choice rides in
each of the three calls at no extra call cost. A **vote passes** only if:
noul ≥ 0.9, the `likely_error` choice is `none`, **and** the Choice's
confidence is at or above its own threshold — set separately in the J10
benchmark (the vendor advises against reusing a Noul threshold for a Choice).
The `likely_error` option list carries an **`other`** option (the vendor
recommends one whenever the list may be incomplete). A **sample passes** only
if all three votes pass.

**Disposition (B5 decision):**

| Result | Destination |
|---|---|
| All three votes pass | **Auto-accept** |
| One or more votes with noul < 0.5 | **Reject** — the rejection message goes to the generator (A2); T3's 3-attempt limit applies |
| All other results: a vote in the middle band, a named error, or low Choice confidence | **Review queue** — the record carries each noul, each choice, each probability map, and the three states |

**Rule order (B5 decision):**

1. Contract validation (J5/B4).
2. Sample selection (the two sets above).
3. Deterministic key check on **all** code samples.
4. Render three states for each Jev sample.
5. One Jev call per state, all questions in that call.
6. Apply the AND rule.

A failure in step 1 or step 3 rejects the content **before any Jev call**.
After the first vote below 0.5, **do not stop**: send all remaining calls for
that sample anyway — the cost is a fraction of a cent, the full record makes
the A2 rejection message, and it yields the correlation data for J10/J-O1.

Rationale: the pilot's only dangerous measurement was the ≈5% single-shot
false-positive (1/20 corruptions at noul 0.96). Three votes with rephrased
states require the corruption to survive three renderings of the same
mathematics; the escape hatch catches states that are *consistently* misread
(the vector-direction failure mode, which rephrasing alone cannot break, since
the error is in the convention, not the wording).

**Correlation caveat (B3 decision):** the p³-style safety claim holds only if
votes fail independently. The vendor measures σ = 0.0102 on repeat calls and
calls Jev "extremely consistent" on similar inputs — so correlated failures
are the *expected* case, not the exception. The J10 benchmark therefore
measures P(all three votes pass | one vote passes) per negative; if that
number is **≥ 0.5**, the three votes are treated as **one vote plus a
rephrase-sensitivity check** and this rationale is rewritten to say so. The
claim is never stated without the measured correlation beside it.

**The rephraser is code, not a model (B3 decision).** The three states are
rendered from the same template data by three fixed phrasing patterns;
re-expressed values (`3/4` ↔ `0.75` ↔ `6/8`) are computed exactly in code. A
code test proves all three states carry the same parameters and the same
answer. **No model writes a state** — an LLM rephraser could silently change
the mathematics and make the three votes test three different problems.

**Statistical reporting rule (B3 decision):** a benchmark reports a rate with
its 95% upper bound — never "safe". Zero false accepts in 200 trials bounds
the rate at ~1.5% (rule of three); "fewer than 1 in 7,000" needs ~21,000
zero-false-accept negatives. Auto-accept enablement is a threshold decision
against measured bounds, with the B2 escape-count loop as the control for the
remainder.

- **Sample-selection policy** (consistent with A2's machine verification):
  samples at domain edges plus mixed corners of the parameter space, minimum
  3 samples per answer, drawn per KP not per template draft.
- **State-formatting rule:** `state` is a **plain-text canonical rendering** of
  the problem and proposed answer. LaTeX is stripped or reduced to its plainest
  readable form (`x^2 + 3x`, unicode operators); the sample-agreement checker
  renders it from the same data that feeds §5 checking, not from the raw
  template body. Rationale: the jaggedness finding — Jev accuracy degrades with
  complex markup in `state`; every avoidable source of disagreement must go.
- **Vote-independence rule:** the three states must differ in *phrasing* (word
  order, reformulated stem, re-expressed values where mathematically identical,
  e.g. `3/4` ↔ `0.75` ↔ `6/8`), not merely in whitespace. Two votes sharing a
  rendering count as one. A perturbation of the stored answer must survive all
  three renderings to pass — that is the whole point.
- Accepted content is stored in `content_store` (D-S4) tagged
  `verification: jev` with the full vote record (per-vote noul, confidence,
  resolved model id) bound to the content digest.

### J3 — Symbolic decidability stays preferred

The symbolically-decidable path (V1) **remains the preferred acceptance path**
wherever the answer form is expressible in the grammar. Jev-verified is the
**fallback for topics the grammar cannot express** — multipart/tuple answers,
variable exponents, proof and multi-step forms — not a replacement.

Rationale: a symbolic proof covers **all** variants of a template forever and
grades in microseconds; Jev covers **sampled** variants and costs calls. When
the grammar later gains multipart/proof support (the follow-up issue already
filed per the authoring report), Jev-verified KPs migrate to V1.

### J4 — Hard wall: Jev never touches a request path

Jev runs at **authoring time only**. It appears on **no** request path — not
serve (L1), not grade (L2), not hint (L5), not teach (L4), not any asynchronous
learner-facing call (T4 diagnosis stays model-authored prose under the existing
cap; Jev does not join it).

This upholds L1–L6, T1, and C4 exactly as written. **A violation is
build-breaking with the same status as a budget breach** (§2.1): a change that
puts a Jev call on any request path does not merge. The invariant is one
sentence: Jev decides what content *earns the right to be served*; it never
decides anything *about a learner's attempt at runtime*.

**Prohibition sentence (B1 decision, binding in V1 and J4):**

> Jev never grades at runtime, for any answer kind, with no exceptions.
> A change that adds a Jev call, or any model call, to the grade path does
> not merge.

### J5 — Pre-authoring triage (B4 decision)

Triage classifies each KP **against the implemented grammar** — the
`AnswerContract` variants in `crates/core/src/answer/contract.rs` and the B1
partition table — not against historical refusal reasons. Each verdict is a
triple:

1. **Answer kind** — one B1 table row: numeric / label / tuple-family
   (tuple, coordinates, vector, matrix, set, list) / expression-in-grammar /
   expression-out-of-grammar / proof-format.
2. **Acceptance gate** — Gate 1 (symbolic) / Gate 2 (Jev-verified, J2) / none
   (teach-plus-exemplar only).
3. **Runtime rule** — the exact V1 table row that will grade it.

Invariant: **Gate 2 or "served for grading = Yes" is only permitted when item
3 exists.** A kind the contract enum does not recognize gets the conservative
bucket (teach-plus-exemplar only), never an optimistic guess.

**Classification is code-only and deterministic.** The verdict derives from
the KP's declared `answer_contract` and exemplar shapes in the YAML — no Jev
call is spent on classification. A Jev classification call exists only as a
tiebreaker for KPs whose declaration and exemplars disagree, or whose
declaration is missing; its verdict is recorded but never overrides a
deterministic one.

The item list:

1. **Input.** The KP's YAML declaration (`answer_contract`, `answer_kind`) and
   its exemplars.
2. **Parse.** Map the declared contract to the concrete `AnswerContract`
   variant. Unmapped or absent → conservative bucket.
3. **Exemplar check.** Every decidable exemplar must fit the declared kind. A
   mismatch is a contradiction.
4. **Authoring-fix queue.** A KP with a contradiction, an unmapped contract,
   or an authored answer that yields `Outcome::Undecidable` under its own
   contract **fails validation** and goes to the authoring-fix queue — it
   consumes no generation calls.
5. **Verdict.** The triple (kind, gate, runtime rule), derived by the mapping
   table below.
6. **Invariant, with a test.** The grade dispatch in
   `crates/core/src/answer/contract/evaluate.rs` has two wildcard arms
   (`_ => return None` and `_ => decided(same_answer(…)))`). A new
   `AnswerContract` variant thus compiles, and the grader then silently uses
   exact comparison for it. Add a function `triage_verdict(&AnswerContract)`
   with an **exhaustive match and no wildcard arm** — a new variant then
   breaks the build until it gets a verdict. Add a test that each variant
   with "served for grading = Yes" has a fixture with one correct answer and
   one incorrect answer that the grader decides.
7. **Gate assignment.** A kind with a V1 runtime rule and a contract variant
   → Gate 1 (symbolic). A kind with a runtime rule whose key resists
   mechanical checking → Gate 2 (J2), per the J2 scope rule. No runtime rule
   → teach-plus-exemplar only.
8. **Reporting.** The triage emits per-course counts per verdict, so the
   authoring plan states how many KPs each gate will process **before** any
   generation spend.
9. **`Outcome::Undecidable` at runtime.** The grader already has a third
   result: `Outcome::Undecidable(reason)`, returned for a learner answer that
   does not parse or that exceeds the input limit. Rule for this amendment:
   **Undecidable is a deterministic result, and it never starts a model
   call.** The learner sees a prompt to enter the answer again in the correct
   format, and the attempt does not count as wrong. At authoring time, if the
   authored answer of a KP gives `Undecidable` under its own contract, the KP
   fails validation and goes to the authoring-fix queue (item 4). *(Item 9
   also closes a hole in the B1 text: B1 said "same input, same verdict" but
   did not name this third result — it is part of B1.)*

**Mapping table (contract variant → verdict):** every variant in
`contract.rs` maps to exactly one triple; the table lives beside
`triage_verdict` and is covered by the item-6 test. New variants are a build
error until mapped.

### J6 — Confidence-gated C6 review

Jev extends the C6 reviewer with Score/Noul questions — mathematical
correctness, grade-level fit, answer leakage — over teach pages and
Jev-verified content. Three-band routing:

- **auto-approve** at noul/score ≥ threshold (measured default **0.9**;
  per-content-kind thresholds are a J-O1 decision);
- **review queue** in the middle band;
- **reject** below the low band.

Every **auto-approval decision stores the probabilities, confidence, and
resolved model id in `content_store` metadata**, bound to the content digest
and review context — the C6 digest-binding rule (decisions bind to the exact
content digest, curriculum, and serving context) applies unchanged to Jev
decisions. An auto-approval without a stored decision record is not an
approval.

### J7 — Author-time distractor analysis (extends A4)

At authoring time, Jev choice-questions over rendered samples enumerate common
wrong answers: *"what would a learner who misreads X answer?"* — the vector
direction convention, sign slips, off-by-one in counting, the thousands-comma
trap (ISSUE-3's class). Results are pre-authored into the diagnosis path
(D-S4's pre-authored diagnoses).

Consequence for T4: where the distractor bank covers a miss, the runtime async
diagnosis call is skipped (this was already A4's intent for precomputable
misses; Jev makes the precomputation mechanical instead of a manual authoring
chore). Expected effect: T4's async diagnosis calls **shrink or disappear** on
Jev-covered KPs. T4's cap and semantics are unchanged.

### J8 — Cost and rate guardrails

- Client-side limiter at **1200 req/min** (the measured limit) with margin;
  exponential backoff honoring `retry-after` on 429.
- Per-run call cap (configurable; a verification run of ~1,900 exemplars at 3
  votes + adjudication is bounded well under it by construction — the cap exists
  for runaway loops, not for expected load).

**Middle-band review queue (B5 decision):**

- **Limit 200.** At 200 items the pipeline stops new Jev-stage work. Symbolic-path
  content continues untouched (it never enters this queue).
- **Forecast before a run.** The J10 benchmark's middle-band rate × the planned
  template count must stay under the limit — change the questions or thresholds
  first rather than starting a run designed to fill the queue.
- **Ordering (not FIFO):** templates of a KP with **zero approved content**
  first (that KP is not served); templates that only add variety to an
  already-served KP last.
- **Aging:** after 30 days an item becomes `expired` and leaves the queue.
  Expiry is **not permanent**: the digest re-enters only through a new
  verification run, caused by a new digest, a changed Jev model id, or a new
  contract variant. (A permanent downgrade would make a second pile nobody
  reads; the grammar and the model improve.)
- **Serving in the interval:** the KP serves its other approved templates; if
  it has none, it is teach-plus-exemplar only (B1). A template in the queue is
  **never served**.
- **Dashboard** (adjacent to the T6 cost panel): queue depth, oldest item's
  age, weekly inflow/outflow, expired count.

**Reviewer of record (B5 decision):** a subagent or a person — C6 stands
(human review optional). The reviewer answers **only the specification
question** (code already proved the arithmetic in step 3), **never changes a
key** — a key change makes a new digest that starts again at step 1. Every
decision stores the reviewer identity (and model id for a subagent), bound to
the content digest per C6.
- Per-KP verification attempt cap: reuse **T3's 3-attempt alert**. A KP that
  fails Jev verification three times stops consuming calls and lands in the C6
  queue (or is downgraded to teach+exemplar-practice-only) — the T3 lesson: a
  stuck KP must surface, not silently burn budget.
- Measured baseline for the dashboard: pilot ≈ $0.0007; full 3-vote
  verification of ~1,900 exemplars ≈ $0.15. Jev's output tokens are free
  (measured); input dominates.

### J9 — Bad-key discovery and repair (B2 decision)

The unit of action is the **content digest**, not the KP. A dispute, a
quarantine, and a revocation each name one digest. The stored record does not
change; a correction makes a **new digest**, and the old digest gets the state
`revoked`.

**Dispute sources** (each writes a `content_disputed` event):

| Source | Rule |
|---|---|
| Operator | Always permitted. |
| Re-verification sweep (J-O2) | Deterministic check fails or a vote fails → dispute. |
| Anomaly counter (code only, no model) | N distinct learners give the same "wrong" answer to one digest → dispute; also when high-ability learners fail the digest above a limit. |
| Learner ("I am sure my answer is correct") | Permitted after a `wrong` verdict only, per-learner rate-limited. The verdict does not change at dispute time. |

**Quarantine.** One operator, sweep, or anomaly dispute quarantines the digest
immediately; learner disputes quarantine at **M** distinct learners. In
quarantine: serving stops, the worker removes the digest's instances from the
serving pool (D-S5), the KP's other templates continue, and a KP with no other
approved content stops — **the learner model takes no penalty for the stop**.
There is no "verdicts pending" serving state: C4 does not permit a verdict the
system itself doubts.

**Resolution (offline, worker-side).** The deterministic check from B1
recomputes the key; an operator confirms. Key correct → digest leaves
quarantine, dispute closes. Key wrong → digest becomes `revoked`; corrected
content gets a **new digest** and goes through the **full acceptance path** —
no fast lane.

**Regrade.** For each attempt event naming the revoked digest, the
deterministic grader re-runs against the corrected key; a changed verdict
appends one `regraded` event, **in both directions** (wrong→correct and
correct→wrong). **Precondition (verified against the code):** `Attempt` stores
`given_answer` and the full `problem.text` (instance hash derivable via
`problem_text_hash`), but does **not** store the content digest — an optional
`content_digest` field on `AttemptProblem` is a required schema change before
any J2 content ships.

**Replay.** Every learner with a regraded event gets the **full replay**
(D-O6). No partial replay: the sequential per-user scan is already the
regrade rule.

**Cascade through the DAG.** The replay *is* the cascade — no separate rule.
The learner model is a projection of the event log; after replay, falsely
mastered topics lose their mastery, implicit repetitions from the false
verdict disappear, and **real downstream evidence stays**. The scheduler then
gives the repaired topic priority above new downstream work (unmet
prerequisite).

**Learner-facing messages.** wrong→correct: "We graded your answer to
[problem] incorrectly. It is now marked correct." correct→wrong: "One problem
in [topic] had an incorrect answer key. The topic returns to your review
list." No blame on the learner. A closed dispute where the key was correct
shows the worked solution.

**Hard rules.** No step calls a model on a request path; steps run in the
worker; every step writes an event and every `regraded` event names the
revoked digest as its cause.

**Feedback loop.** Revoked-digest counts are tracked **per verification path**
(symbolic vs jev) — this is the measured escape rate, and it is the data the
owner uses to set J-O1 thresholds.

### J10 — The acceptance benchmark (B3 decision)

The assembled J2 gate is benchmarked on known ground truth **before any
learner sees Jev-verified content**, and re-run on each change of the resolved
Jev model id before auto-accept continues (partial answer to J-O2).

| Item | Rule |
|---|---|
| Model | Pinned versioned id (J1); logged per call. |
| Positive set | ~200 hand-written exemplars with known-correct keys, **each carrying an audited formal specification (J11)** — model-drafted, person-audited. No unaudited spec verifies anything. |
| Negative class A (arithmetic) | ±1, ±2, sign slip, swapped digits — made **by code**; ≥ 2,000 samples. |
| Negative class B (specification/convention) | Swapped vector components, wrong direction convention, wrong unit, answer to a different question, one wrong component in a multipart answer — ≥ 1,000 samples (first run may start at 300; the bound is then ~1%, and auto-accept stays off for kinds whose bound the owner rejects). |
| Split | Thresholds tuned on one half, results reported on the other. Never tuned and reported on the same data. |
| Pipeline | The full assembled path: render → three code-rendered states → deterministic stage → three votes + `likely_error` → AND rule. |
| Report | Accept / reject / middle-band rates **per class**, each with its 95% upper bound, plus the vote-correlation number. |

**Pass criteria, one per stage:**

- **Deterministic stage:** rejects 100% of class A negatives and 0 positives.
  Any failure is a **software defect** — fix it, re-run the benchmark.
- **Jev stage, class B:** zero false accepts on ≥ 1,000 negatives in the
  report half (upper bound ~0.3%). Pass → auto-accept starts. Fail →
  auto-accept stays **off**; all Jev-verified content routes to the C6 review
  queue until a changed design passes.
- **False rejects on positives:** reported, not gated — the middle band and
  the C6 queue absorb them.

**Residual risk:** a 0.3% bound is not zero. The B2 loop (J9) is the control
for the remainder; the per-path escape count feeds J-O1.

**Cost:** corruption by code is free; ~3,200 samples × 3 calls ≈ 10,000 calls,
under $1. Class B construction needs a person or a model (code cannot write a
realistic "wrong convention" corruption) — it is the expensive part of the
benchmark.

### J11 — Formal answer specification (new; unblocks J2/J10/J9)

The deterministic stage (B1 step 3) computes the answer key from a **formal
specification** — but a hand-written exemplar is only a LaTeX problem string
and an answer string. It has no parameters and no answer expression, so the
deterministic stage has **no input** for the exemplar path. Without this
item, a benchmark run on exemplars sends only stem + answer to Jev — the old
design B1 and B3 stopped.

**The specification format.** One JSON document per exemplar/answer, stating
the operation and its inputs, from which code computes the key exactly:

```
{ "kind": "matrix_add", "a": [[1,2],[3,4]], "b": [[0,1],[-1,2]] }
{ "kind": "vector_from_points", "p": [1,2], "q": [4,6] }
{ "kind": "solve_linear", "a": 2, "b": 3, "c": 11 }   // ax + b = c
```

Rules:

1. **One `kind` per answer family**, implemented once in code, with a
   V3-style fuzz test against a known-good oracle (SymPy at authoring time)
   before it verifies anything.
2. **Every kind maps to exactly one B1 table row** — the spec's computed key
   is graded at runtime by that row's rule.
3. **Ground-truth protocol for the J10 positive set:** a model writes the 200
   specifications from the exemplars; **a person audits each one** (spec
   against the exemplar's problem and answer). After the audit, the positive
   set is ground truth. No unaudited spec verifies anything.
4. **Authoring pipeline use:** templates produced by A2 must emit this format
   for their answer keys — the same structure serves template verification
   (B1 step 3) and exemplar verification (this item). A template that cannot
   state its key as a specification fails validation (J5 item 4).
5. **Regrade use (J9):** the deterministic regrade of a revoked digest
   re-computes from the stored specification — not from the old LaTeX string.
   The spec is stored in `content_store` metadata, digest-bound.

**Matrix decision (owner, delegated):** the `AnswerContract` enum gains a
**`Matrix` variant** (dimensions + entry-wise comparison), rather than
representing matrices as Multipart-of-rows. Reasons: B1's table already
defines the matrix runtime rule; a Multipart encoding conflates row structure
with answer semantics and has no dimension concept; B4's `triage_verdict`
exhaustive-match invariant then forces the verdict, fixtures, and fuzz test
to exist. Sequenced **before** the J10 positive set is built — otherwise a
large share of the 200 positives has no runtime rule.

**Open decision J-O6:** the initial `kind` registry — which operations the
first implementation covers (proposal: linear solve, vector from points,
matrix add/scale/multiply, determinant 2×2/3×3, eigen-verify, permutation
count, basic probability, mean/variance/z-score). Owner signs off the list;
each kind is a small, separately testable unit.

---

## C. Cleanup — per-ID disposition of REQUIREMENTS.md

- **C1–C5** — unchanged.
- **L1–L6, T1, T3, T5, T6** — unchanged. (T6 gains Jev purposes per J1; T3's
  alert is reused by J8; neither normative line moves.)
- **V1 — amended (B1 decision).** The runtime grading rule is selected by the
  answer kind. The verification tag (symbolic or `jev`) has **no effect at
  runtime; the grader does not read the tag.** Serving rule: a KP is served
  for graded practice only if its answer kind has a deterministic comparison
  rule below; otherwise it is teach-plus-exemplar only and the learner gets no
  verdict.

  | Answer kind | Runtime grading rule | Served for grading |
  |---|---|---|
  | numeric | V1 as written: parse, normalize, exact rational comparison | Yes |
  | label | Exact match after V4 normalization against the stored label set | Yes |
  | tuple, coordinates, vector | Parse `(a, b, …)`; arity must be equal; numeric rule per component in sequence; order significant | Yes |
  | matrix | Tuple of rows; dimensions must be equal; numeric rule per entry | Yes |
  | set, list | Parse components; for a set, sort canonical forms and remove duplicates before comparison; for a list, order significant; numeric rule per component | Yes |
  | expression inside the V1 grammar | V1 as written | Yes |
  | expression outside the V1 grammar (variable exponent, integral notation) | No rule | No — teach plus exemplar only, until a grammar extension adds a rule |
  | proof-format, free-form multi-step | No rule | No — teach plus exemplar only; A4 stays as written; Amendment J adds no proof grading |

  Prohibition sentence (binding in V1 and J4): *"Jev never grades at runtime,
  for any answer kind, with no exceptions. A change that adds a Jev call, or
  any model call, to the grade path does not merge."*

  Test requirement (B1 decision): every row marked "Yes" gets a V3-style fuzz
  test before it grades a real attempt — this stops two different
  implementations of one rule.

  Acknowledged cost (B1 decision): most calculus KPs (expression answers
  outside the grammar) lose graded practice under this table; Jev verifies
  their content but the system does not grade them, and they are removed from
  the "1,300 KPs unlocked" count. A runtime equivalence check at fixed rational
  sample points (deterministic but inexact) is **out of B1 scope** and is
  filed as open decision **J-O5**. Jev-verified acceptance (J2) is the
  author-time acceptance path for forms inside the table; the grammar remains
  preferred wherever expressible (J3).*
- **V2 — amended.** New text: *"The authoring pipeline (A2) enforces two
  acceptance gates. Gate 1 (symbolic): any template whose answer expression
  stays inside the V1 grammar (including the B1 extension kinds: tuple,
  coordinates, vector, matrix, set, list) is verified mechanically as today
  — code checks the arithmetic of the answer key. Gate 2 (Jev-verified, J2):
  answers whose kind has a runtime rule but whose form resists mechanical
  key-checking may be accepted only through the 3-vote AND-gate — and even
  there Jev votes on stem-vs-specification agreement and `likely_error`, never
  on numeric truth. Which gate applies is decided by the J5 triage
  classification, not by the template author's preference; a KP the grammar can
  express never routes to Gate 2. Undecidable kinds graded per A4's async path
  never claim a deterministic verdict — unchanged."*
- **A2 — amended.** The verification pipeline gains the Jev stage (J2 gate, J5
  triage, J6 review bands). New normative sentence: *"Rejection feedback now
  includes the Jev vote record (per-vote noul, confidence, model id) alongside
  the mechanical gate diagnostics — the 1.0 lesson stands for Jev: the quality
  of the rejection message is the yield lever, and a bare noul number is not a
  rejection message; it must name which vote failed, the likely error class,
  and the rephrasing attempted."*
- **C6 — amended.** Add: *"The reviewer may route through Jev confidence bands
  (J6): auto-approve at or above the kind-specific threshold, review queue in
  the middle band, reject below. The existing digest-binding applies unchanged:
  a Jev auto-approval binds to the content digest + curriculum + serving
  context exactly as a reviewer approval does, and stores its probabilities,
  confidence, and model id in content_store metadata. Human review remains
  optional."*
- **T2 — amended.** New text: *"Model tokens are spent in exactly four places:
  (a) offline authoring generation, (b) offline Jev verification, triage, and
  review (J2/J5/J6), (c) optional asynchronous miss diagnosis (A4), (d)
  author-time distractor analysis (J7). All four are offline or worker-side;
  none touch a request path (T1, J4). Jev calls: the vendor bills input tokens
  only, and T6 logs them. Code renders the three states (B3), and that
  rendering costs no tokens. The optional subagent reviewer from B5 is a model
  call under sink (a), not a Jev call."*
- **T4 — unchanged in spirit.** Caps, defaults, and semantics stand. Note
  added: *"J7 distractor pre-authoring may reduce this path's necessity on
  covered KPs; the call is skipped where a pre-authored diagnosis exists,
  exactly as A4 already specified."*
- **A7 — amended (extension only).** Add to the source list: *"A third
  realistic source is now on the near horizon: LLM-generated novel problems
  verified by the Jev gate (J2). Generation stays offline and worker-side (R4);
  A7's future-work framing for the generator itself stands. What changes is
  that the verification seam it needs is no longer aspirational — the same
  gate that admits multipart/proof authored answers (J2) admits generated ones,
  and the pool's `source` tag already measures them separately."* The launch
  scope still contains no generator (§7).
- **§7 Non-goals — restated affected line:** *"No model call on the request
  path, ever — including generation (A6/A7) and including Jev (J4). Jev is
  author-time verification only; its absence from every request path is
  build-breaking (J4), same status as budget breaches. Generation itself is
  NOT a non-goal: A7 reserves the architecture for it as future work; only the
  synchronous form of it is permanently out."* All other non-goal lines
  unchanged.
- **§8 Open decisions — O2 partially answered.** O2 (diagnosis model + budget
  numbers) is partially answered by the distractor-analysis direction (J7):
  pre-authored distractors displace the diagnosis model on covered KPs, so the
  open part of O2 narrows to the residual, uncovered-miss budget. O1, O3
  unchanged. Jev adds its own open decisions below (J-O).
- **R1–R5, D1–D9, A1, A3, A5, A6, V3, V4** — unchanged. (Jev is a worker-side
  offline call and fits R4's model-call placement without amendment; D-S4/D-S6
  records gain fields per J2/J6, which is schema additive under D9, not a
  shape change.)

---

## J-O. Open decisions (owner) — Jev

- **J-O1 — Auto-approve thresholds per content kind.** 0.9 is the measured
  auto-accept floor for answer verification (J2). Teach pages and J6 review
  bands may want different thresholds (leakage and grade-fit questions are not
  correctness questions). Owner sets the table: kind → auto-approve threshold →
  reject threshold.
- **J-O2 — Re-verification sweep on model bump.** When the resolved Jev model
  changes (jev-1.13-20260917 → next), do Jev-verified answers get a periodic or
  bump-triggered re-verification sweep, or do they stand? Analogous to the
  engine-digest re-stamp sweep (ISSUE-10) — a decision here prevents a repeat
  of the ISSUE-10 class of silent staleness for the Jev-verified tag.
- **J-O3 — Tag visibility.** Is the `verification: jev` tag learner-visible
  ("this answer type is verified by a second system") or operator-only
  metadata? Default proposal: operator-only; the learner sees approved content,
  not its provenance.
- **J-O4 — Triage skip policy.** May `not_decidable` triage results be
  overridden (human asserts a KP is authorable after a grammar extension), or
  is triage sticky until the grammar itself changes? Default proposal: sticky
  per grammar version; a grammar bump re-triages flagged KPs automatically
  (1 call each).
- **J-O5 — Runtime equivalence check for out-of-grammar expressions (parked
  by B1).** A deterministic but inexact runtime check for expression answers
  outside the grammar (variable exponents, integral notation): evaluate the
  learner's expression and the key at a fixed set of rational sample points
  and require agreement at all of them. Deterministic (no model call), but
  not exact — two distinct expressions can agree on all sampled points. If
  adopted, it becomes a V1 table row of its own with its own V3-style fuzz
  test and an explicit "inexact by construction" note in the learner-facing
  material. Owner decides whether the pedagogical gain (graded calculus
  practice) outweighs the inexactness.
- **B2-O1 — Anomaly and dispute counts.** N (same-wrong-answer learners),
  M (learner disputes to quarantine), and the learner dispute rate limit.
  Starting proposal: M = 2; N tuned from pilot data.
- **B2-O2 — XP after correct→wrong regrade.** Visible XP does not decrease;
  a compensation event keeps the total, because the error was the system's.
- **B2-O3 — Successor review after repair (optional stricter rule).** An
  automatic review task on each direct successor of a repaired topic, from
  TMAW's encompassing relations. Adds learner work; needs no new mechanism.
  Owner decides whether the extra rigor is worth the added reps.

- **B5-O1 — Operator audit of subagent approvals.** Audit a fixed percentage
  of subagent approvals — proposal: **5%**. Audit results feed the same escape
  count as B2 (J9), measuring the subagent reviewer, which no other control
  measures.

---

## Non-ratification note

This document is a **DRAFT**. It binds nothing until the pedagogy reviewer
signs off against The Math Academy Way, at which point the J-series IDs join
REQUIREMENTS.md's stable-ID registry and the §C dispositions take effect.
Until then, REQUIREMENTS.md v2.0 wins over everything, as HANDOVER-OUTSTANDING.md
already records.

**Ratification addendum (owner decisions, this session):**

- Ratification with default values for all open decisions (J-O1–J-O6,
  B2-O1–B2-O3, B5-O1) is acceptable; the J10 benchmark data and the B2 escape
  counts adjust them later.
- R1–R6 (reviewer non-blocking recommendations) are folded in **with** the
  ratification.
- **R7 is a gate, not a rider:** the CI check enforcing J4 (no Jev symbols
  reachable from any request-handler crate) must **exist before auto-accept
  starts**. J4 says a violation breaks the build; without the check, no build
  breaks.
