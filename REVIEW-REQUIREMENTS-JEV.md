# Review — REQUIREMENTS-JEV.md (Amendment J, draft 1)

**Reviewer:** pedagogy/requirements reviewer (independent context)
**Date:** 2026-09-16
**Authority chain applied:** The Math Academy Way (TMAW) PDF is the source of truth;
`REQUIREMENTS.md` v2.0 stable IDs; `../cadus/docs/PEDAGOGY.md` Hard Rules as captured in
`docs/reference/authoring-and-spa-1.0-spec.md:557-563` (no answer before an attempt; honest
structural grades; the core owns all scheduling; generation fidelity; LaTeX rendering; brisk tone).

**Evidence basis:** the TMAW PDF could not be text-extracted on this box (subsetted fonts, no
`pdftotext`/`pypdf` available; a raw stream extraction yielded zero searchable ASCII). The review
grounds TMAW claims on the four pre-extracted page sets in
`/home/deploy/.cache/cadus2_scripts/framework/book-p*.txt` (pp. 209-221 mastery learning +
micro-scaffolding/cognitive load; pp. 278-279 micro/macro-interleaving; pp. 375-380 diagnostic
confidence + conservative/aggressive edge of mastery) plus the well-established public teachings
(retrieval practice, attempt-before-answer, automaticity through reps, grade honesty). Any claim
outside that evidence base is flagged as [KNOWLEDGE-BASED] below.

---

## Verdict: PASS WITH FINDINGS

The amendment's architecture is pedagogically sound in its load-bearing decisions: Jev is
author-time verification only (J4), symbolic decidability stays preferred (J3), and every failure
direction is fail-safe (J1). The doc is *not* needlessly timid — the ~1,351 locked KPs are a real
pedagogical emergency and Jev is a legitimate key. But it is **not ratifiable as-is**: four
specification gaps must be closed first, and one premise (the "multipart is undecidable" routing)
is stale against the implemented 2.0 answer-contract grammar. Approve **with the blocking changes
in the final section incorporated; do not ratify until B1–B6 are in the text.**

---

## 1. Pedagogical primacy audit (TMAW)

### 1.1 Grade honesty (C4) — the central question

C4: "`correct` reflects only mathematical correctness… A speed optimization that risks a wrong
`correct` verdict is rejected — correctness outranks every budget." Note that Jev never touches a
learner's attempt (J4), so C4's runtime exposure is unchanged. The *only* new channel through
which a learner can be graded wrong is a **silently-wrong answer key** that the deterministic
checker then grades with perfect confidence. That is the right lens, and the doc sees it (J2's
corruption test, J-O2's staleness sweep).

**Quantified risk.** Measured single-shot false-positive rate ≈ 5% (1/20 corruptions, noul 0.96
on a corrupted eigenvector). Under the 3-vote AND-gate: if vote errors were independent, survival
is ≈ 0.05³ ≈ 1.25 × 10⁻⁴ per key. They are not independent — the doc itself identifies the
correlated failure class (vector-direction convention, "the error is in the convention, not the
wording"), which rephrasing cannot break and which only the `likely_error` escape hatch targets.
The escape hatch is **unmeasured as a mechanism** (the pilot measured single-shot noul, not the
hatch's catch rate on convention-corrupted keys). Honest bounds on residual per-key error:
worst case (fully correlated) ≈ 5%, realistic case ≈ 10⁻³–10⁻⁴. Over ~1,900 exemplars that is
**expected 0.2–2 silent bad keys** if realistic, up to ~95 keys at the unmeasured worst case.

**The two failure modes compared honestly.**

- *Mode A — a silent bad key ships.* A learner drills a wrong rule to automaticity with high
  confidence. TMAW's knowledge-graph structure makes this worse than it looks: a wrong rule
  mastered on a prerequisite poisons every post-requisite reached through encompassing relations —
  the learner confidently applies it downstream and the scheduler keeps serving "correct" practice.
  This is the single worst failure the methodology recognizes, and it is *invisible* until a human
  notices.
- *Mode B — the ~1,351 KPs never get content.* TMAW p. 211: a stuck learner "can try again
  another day, but in the meantime, they are allowed to learn other topics that don't depend on
  the problematic one." So the lockout is not total — but for 7 entire courses (calculus-1/2,
  linear algebra, multivariable, DEs, abstract algebra, category theory) the *dependent chain* is
  dark. A learner with no servable problems gets no reps; no reps means no automaticity; no
  automaticity means the topic is not mastered no matter how good the teach page is
  [KNOWLEDGE-BASED: TMAW's automaticity-through-reps thesis]. Mode B is a ~100% pedagogical loss
  on those domains; Mode A at the realistic bound is a ~0.1% per-key loss **with the mitigations
  below**, and at the unmeasured worst case it is still a bounded, detectable, repairable loss.

**Judgment: the doc's mitigation direction is sufficient *in kind*, insufficient *in evidence*.**
The 3-vote AND-gate + rephrased states + escape hatch + middle-band-to-review is the right
shape (it mirrors TMAW's own conservative/aggressive edge-of-mastery discipline: gate hard on the
dangerous direction, spend human attention in the ambiguous middle). But the only dangerous
measurement (1/20) is single-shot; the gate as a *system* has never been run. Acceptance is
therefore conditional on B3 (a fresh corruption benchmark through the full gate) and B2 (a repair
protocol for the keys that slip through), because Mode A's damage is repairable only if
discovery is designed for. A learner-facing dispute affordance ("flag this answer") is the
cheapest detection instrument that exists and is absent from the doc.

### 1.2 Attempt-before-answer, scheduler ownership, spaced repetition, mastery gating

- **Attempt-before-answer (Hard Rule 1): not weakened anywhere.** Nothing in J1–J8 reveals an
  answer before an attempt; J-O3's default (provenance operator-only) is right. ✓
- **Scheduler ownership (Hard Rule 3, C1): untouched.** Jev decides what content *earns the right
  to be served* (J4's own invariant sentence); the core still decides what to serve and when.
  J5's "teach+exemplar-practice-only" flag is a content-availability flag, not a scheduling
  decision — the correct side of the line. ✓
- **Spaced repetition / mastery gating:** Jev appears nowhere in D-S2/D-S3 mechanics; the
  amendment correctly leaves R1–R5, D1–D9, A1, A3, A5, A6 untouched. ✓
- **Velocity/coverage leak check:** the one place coverage pressure touches pedagogy is J5's
  `not_decidable → teach+exemplar-practice-only`, and the authoring report's claim that this is
  "functionally equivalent for a self-motivated learner" — it is not. Exemplar practice on
  non-grammar answers is **ungraded** at runtime (V2: "never claims a deterministic verdict"),
  which means no honest attempt outcome exists, so mastery state cannot be credited and the
  scheduler cannot schedule the topic as mastered. The doc inherits this from the original V2 but
  now makes it load-bearing for ~1,300 KPs. See Finding 1 (B1).
- **Triage stickiness (J-O4):** a single un-rephrased triage call misclassifying an authorable KP
  as `not_decidable` becomes permanent until a grammar bump. The safe direction here is the cheap
  one (re-triage is 1 call); require a manual re-triage override so an evidenced correction does
  not wait for a grammar change (Finding 5, R-scale).

### 1.3 The runtime-grade wall (C4 / L2 / T1) — does the wall hold?

**J4 is the strongest sentence in the document and should be kept verbatim**: "Jev decides what
content *earns the right to be served*; it never decides anything *about a learner's attempt at
runtime*", with build-breaking status equal to a budget breach. §7's restated non-goal and the T2
amendment (four offline places) are consistent with it. The pilot's invariant — author-time
verification only — is preserved.

**One leak, and it matters:** the §C amendment to V1 says a Jev-verified answer "is graded at
runtime by the deterministic checker's own rendering rules for that answer kind, or — where no
deterministic comparison exists (proof, free-form) — is graded per A4 and never claims a
deterministic verdict (V2)." This sentence silently assumes every Jev-verified kind *has* runtime
rendering rules. For the kinds J2 exists to unlock, there are exactly two cases:

1. **A deterministic comparison exists (or can exist)** — then Jev's role is precisely *answer-key
   verification at authoring time*, and the runtime grade stays fully deterministic and 0-token
   (L2/T1 intact). Good.
2. **No deterministic comparison exists (proof, free-form multi-step)** — then there is *no
   honest runtime verdict at all*. The amended text says "graded per A4's async path", but A4 is
   a *diagnosis* path, not a verdict path; an attempt on such content produces an ungraded
   outcome (the H-1/H-2 "not marked" class, now at scale), no foldable mastery signal, and a
   learner staring at a tile with no resolution.

The doc must say which case each unlocked KP falls into and forbid case 2 from being dressed up
as practice with grades. **Runtime Jev grading must be named forbidden explicitly in the amended
V1 text**, not merely implied by J4's placement. Verdict engine note: the working tree's
`crates/core/src/answer/contract.rs` already ships `AnswerContract::{Multipart, Coordinates,
Set, List, Label, AscendingChain, PolynomialRelation, …}` — see Finding 3.

### 1.4 Distractor pre-authoring (J7, amends A4)

Pedagogically sound and genuinely TMAW-aligned: error-specific feedback that names the *learner's*
misconception (sign slip, direction convention, thousands-comma trap per ISSUE-3) is exactly the
misconception-targeting Math Academy does instead of generic "try again" prose. It also *shrinks*
the runtime model path (T4), which is the correct direction: pedagogy served by pre-computation,
not by latency. Two requirements to add: (a) the rule for deciding "the distractor bank covers
this miss" must be deterministic string/structure matching of the learner's wrong answer to a
pre-authored distractor, with fallback to the async A4 call on no match (Finding 6);
(b) distractor prose passes the Hard Rule 1 leakage check (a distractor explanation must not
reveal `expected` — trivially satisfied for wrong-answer diagnoses, but test it).

### 1.5 Interleaving and problem variety

**Net positive, with one guard needed.** TMAW pp. 278-279: macro-interleaving works by taking a
*breadth-first* path through many bite-size topics — which requires topics to *exist and be
servable*. Unlocking 7 courses widens the breadth-first frontier; that is a direct interleaving
win, not a side effect. The variety risk is real but pre-existing: a KP served only by exemplar
rotation (A6) draws from a small fixed set, which is narrow clones by another name and strains
Hard Rule 4's anti-repeat spirit at scale (the 1.0 lesson: `MIN_SPACE_SIZE = 12` — "at least 12
are needed for randomized values and for avoidance of a recently-served problem to mean
anything"). **Proposed new requirement (J-R1):** a servable KP must clear a distinct-practice
floor — ≥ 12 distinct problems from templates, or ≥ 6 distinct exemplars in rotation — before it
enters the scheduler's servable set; and the A7 seam should explicitly admit *Jev-verified
generated variants* of an exemplar as a future third source, since J2's gate is exactly the
verification seam A7 said it needed. Jev-verified content that merely unlocks *coverage* without
*variety* would trade one TMAW failure (no practice) for a subtler one (narrow clones).

[KNOWLEDGE-BASED throughout this section where TMAW page evidence is not locally extractable:
retrieval practice/attempt-before-answer, automaticity via reps, no-credit-for-self-assessment,
the automaticity thesis. All are core, uncontroversial TMAW teachings; the page extracts above
directly confirm mastery learning, cognitive load, and interleaving claims.]

---

## 2. Internal consistency audit

- **§7 non-goals:** the restated line ("No model call on the request path, ever — including…
  Jev (J4)") is coherent and *strengthens* the original. ✓
- **T2 amended to four token sinks** — consistent with J2/J5/J6/J7; all four are offline/worker
  side. ✓ One omission: the **rephrasing generation** that the 3-vote gate requires (who writes
  the three rephrased states? a model) is a fifth, uncounted token sink — fold it under (a) or
  (b) explicitly and count it in J8's cost baseline (Finding 8).
- **NFR-L:** correctly untouched — Jev places no calls on any request path, so L1–L6 stand as
  written. NFR-T: J1 extends T6 (purposes `jev_*`) without moving the normative line. ✓
- **T5 gap:** T5 requires provider prompt caching "when the provider supports it". The OpenRouter
  *alpha* decisions endpoint's caching support is unverified, and the doc is silent. If the
  surface does not support caching, every Jev call technically violates T5 as written. One
  sentence fixes it (Finding 7): carve out — "where the surface does not support prompt caching,
  T5's caching clause is void for Jev calls and the call logs `cached=0` under T6."
- **Orphaned/dangling references:** `ISSUE-3` ✓ and `ISSUE-10` ✓ exist in `ISSUES.md`;
  `CONTENT-AUTHORING-REPORT.md` ✓ exists with the cited numbers (gate-refusal distribution
  matches; "1,300 KPs" vs the report's 1,351 — acceptable rounding). **One false citation:** the
  non-ratification note says the draft's subordinate status is "already recorded" in
  `HANDOVER-OUTSTANDING.md` — it is not; that file records only that "`REQUIREMENTS.md` still
  wins over everything" (generic, predates this draft). Trivial, but requirement docs must not
  cite evidence that does not exist (Finding 9).
- **Double definitions:** none — V1/V2/C6/T2/A7 amendments live once, in §C; J-series IDs are
  referenced, not restated. ✓
- **Stale premise (the big one):** the amendment's motivation and J5 triage assume multipart/
  tuple answers are *undecidable*. The implemented 2.0 checker disagrees: `AnswerContract`
  (`crates/core/src/answer/contract.rs`, committed, exercised by `curriculum/geometry/
  06-reasoning-proof.yaml` `answer_contract: {kind: label…}` and multipart/coordinates contracts)
  already decides multipart, coordinates, sets, lists, labels, chains, and polynomial relations
  deterministically. Routing `needs_multipart` to the probabilistic J2 gate (J5) would send
  KPs that *can* be symbolically graded for all variants to a gate that verifies *sampled*
  variants — violating J3's own stated preference. See Finding 3 (blocking).

---

## 3. Rigor audit

- **3-vote AND-gate precision:** thresholds (≥ 0.9 / < 0.5 / middle) and the all-votes condition
  are specified, and the rules define a total order *if* applied reject → review → accept. Two
  holes: (i) **escape-hatch failure has no disposition** — a noul ≥ 0.9 vote that names a likely
  error is correctly denied auto-accept, but the text does not say whether it lands in review or
  reject (it should be *review* — a named error class is diagnostic gold, not proof of a wrong
  key); (ii) whether the escape-hatch question runs per-vote or once per sample is unstated.
  (Finding 4.)
- **Vote independence:** well specified as a principle ("differ in phrasing… not merely in
  whitespace; two votes sharing a rendering count as one") but the *generator* of the three
  rephrasings is unspecified — which component produces them, what provenance is stored per vote,
  and whether value re-expression (`3/4` ↔ `0.75`) is machine-verified as mathematically
  identical. [Finding 4/R2.]
- **Jaggedness:** properly promoted from folklore to requirement — the plain-text canonical state
  rule (rendered "from the same data that feeds §5 checking, not from the raw template body") is
  exactly the right fix and is implementable. ✓ One nit: for answers *outside* §5, "the same
  data" needs a definition (who computes the proposed answer string?).
- **The deepest unspecified thing — the answer source at serve time.** For a template whose
  answer form is outside the grammar, *what entity carries the expected answer of an instantiated
  problem* (D-S5's "hidden expected answer")? If it is the model's answer string, then Jev
  verified samples but the runtime serves an unverified generalization. If it is a server-side
  evaluator, the evaluator must be part of the verified artifact. The doc must say (Finding 2).
- **Sample-selection policy:** "domain edges plus mixed corners, minimum 3 samples… drawn per KP
  not per template draft" — the "per KP not per template draft" clause is ambiguous (a
  parameterized template's sample space is per-template by definition), and 3 samples is far
  below the pilot-validated 12-distinct floor for anything parameterized. Tighten (Finding 7).
- **Middle-band queue:** **unbounded**. J2 routes middle-band items to the C6 review queue; C6's
  own semantics leave unresolved content pending indefinitely ("human review is optional") — the
  exact shape of content_store's pending pile that the review brief warns about. With ~1,900
  exemplars at the pilot's 3/35 mid-band rate, expect ~160 queue entries at full sweep with no
  cap, no aging, no downgrade, and no dashboard count specified. Finding 5 (blocking).
- **J8 guardrails:** rate limiter, per-run cap, T3's 3-attempt alert reuse — all implementable
  and correctly aimed at runaway loops rather than expected load. ✓

---

## 3. Runtime checks

Commands run (all read-only):

- TMAW PDF extraction attempt: `python3` raw FlateDecode stream extraction → 589 MB decompressed,
  0 keyword hits — **subsetted fonts, text not extractable on this box**; TMAW grounding taken
  from `framework/book-p{209-211,219-221,278-279,375-380}.txt` + [KNOWLEDGE-BASED] as flagged.
- `REQUIREMENTS.md` and `REQUIREMENTS-JEV.md` read in full; `docs/reference/web-service-1.0-spec.md`,
  `serving-1.0-spec.md`, `authoring-and-spa-1.0-spec.md` (Hard Rules capture), `HANDOVER-OUTSTANDING.md`,
  `CONTENT-AUTHORING-REPORT.md`, `ISSUES.md` (ISSUE-3, ISSUE-10) cross-checked.
- `grep -rn jev crates/ web/src/ migrations/ curriculum/` → **zero hits**: nothing is implemented
  on faith; the draft's own rule is being honored. ✓
- `git status`/`git diff` on the repo: working-tree changes in `crates/web/src/grade/verdict.rs`,
  `crates/web/src/serve/mod.rs`, `crates/web/tests/readiness_parity.rs` are rustfmt reflow + one
  test reformat — **unrelated to the amendment** (see Out-of-scope).
- `crates/core/src/answer/contract.rs` + `check.rs` + curriculum greps: confirms the live
  `AnswerContract` grammar incl. `Multipart`, `Coordinates`, `Set`, `List`, `Label` — basis of
  Finding 3.
- No Python was changed by this task; `py_compile` not applicable. No tests were run against any
  database (none required for a requirements review).

---

## Findings

1. **[BLOCKER] The runtime grading story for Jev-verified content is unresolved.**
   REQUIREMENTS-JEV.md §C (V1 amendment), J2. For answer kinds with no deterministic comparison
   (proof, free-form multi-step), the amended text routes runtime grading to "A4's async path",
   but A4 is a diagnosis path, not a verdict path — such attempts are *ungraded*, cannot fold
   into mastery (D-S3), and recreate the H-1/H-2 "not marked" dead end at the scale of ~1,300
   KPs. Fix: partition J2's unlock into (i) kinds with a deterministic runtime contract — Jev
   verifies the **answer key** at authoring time; the runtime grades deterministically, 0 tokens,
   honest verdict (this is where multipart/coordinates belong); and (ii) genuinely undecidable
   kinds (proofs) — which may serve teach + worked exemplars with **no mastery credit and no
   fabricated verdict**, and the doc must say so in the amended V1 text. Add the explicit
   sentence: "A Jev-verified answer is never graded by Jev at runtime; where no deterministic
   comparison exists the attempt is ungraded and the learner is told so."
2. **[BLOCKER] No answer-source rule for Jev-verified templates.** J2 does not say what carries
   the expected answer of an *instantiated* problem at serve time (D-S5). If the model's answer
   string is served, samples were verified but the served generalization was not. Fix (J2, new
   bullet): servable instances must carry answers computed by a server-side evaluator that is
   itself part of the verified artifact; Jev verifies the evaluator's outputs on the sample grid;
   exemplars carry their stored (Jev-verified) key as-is.
3. **[BLOCKER] J5 triage misroutes decidable kinds.** `needs_multipart → J2 path` contradicts the
   implemented `AnswerContract::Multipart/Coordinates/Set/List/Label` grammar
   (`crates/core/src/answer/contract.rs`; used by `curriculum/geometry/06-reasoning-proof.yaml`
   and others). Fix (J5): re-derive the triage enum and routing from the *current* AnswerContract
   set — `needs_multipart`/`needs_structured` routes to **Gate 1** (symbolic multipart authoring),
   Gate 2 (Jev) is reserved for forms no contract expresses; restate the motivation section
   against the current grammar state. J3's "Jev covers sampled variants" argument is the doc's
   own reason.
4. **[MAJOR] 3-vote gate gaps.** (a) Escape-hatch failure has no disposition — specify *review
   queue*, not reject (the named error class is diagnostic, J2's own rationale). (b) Specify the
   rephrasing generator (component, model/technique, provenance stored per vote under T6) and
   whether the escape-hatch question runs per-vote. (c) State the rule application order
   explicitly (reject > review > accept). (d) For parameterized templates raise the sample floor
   (≥ 3 is far under the 12-distinct Hard Rule 4 lesson) and delete or define "per KP not per
   template draft".
5. **[MAJOR] The middle-band review queue is unbounded.** J2 → C6 queue with no cap, aging, or
   downgrade; at the measured 3/35 mid-band rate a full 1,900-exemplar sweep queues ~160 items
   indefinitely ("human review is optional"). Fix (J8 or J2): per-run middle-band cap; items age
   out to `teach+exemplar-practice-only` after N review cycles or T days (reuse T3's
   surface-don't-silently-burn principle); queue depth appears on the operator dashboard.
6. **[MAJOR] No bad-key discovery/repair protocol.** Mode-A damage is repairable only if
   discovered, and nothing in the doc discovers it. Fix (new J-R2): a learner- and operator-facing
   "flag this answer" action quarantines the KP, a re-verification sweep runs, discovered bad keys
   emit C2 `regraded` events for affected attempts, learner models replay (D-O6), and downstream
   KPs are re-evaluated along the DAG. Without this, the 3-vote gate's residual risk is
   unrecoverable rather than merely small.
7. **[MAJOR] Gate validation evidence missing.** The only dangerous measurement (1/20) is
   single-shot; the 3-vote gate + escape hatch has never been run as a system. Fix (B3): before
   auto-accept is enabled for any KP class, rerun the corruption benchmark (≥ 50 perturbations
   spanning the unlocked answer kinds, explicitly including the vector-direction class) through
   the *full* gate and publish the measured residual FPR in the ratification record.
8. **[MINOR] T5/prompt-caching gap.** Fix (J1): one sentence — where the alpha decisions surface
   does not support prompt caching, T5's caching clause is void for Jev calls and the call logs
   `cached=0` (T6). Also count rephrasing-generation tokens under T2(a)/(b) and include them in
   J8's dashboard baseline.
9. **[MINOR] False citation.** Non-ratification note claims `HANDOVER-OUTSTANDING.md` "already
   records" this draft's subordinate status; it does not (it predates the draft and speaks only
   generically). Cite the draft status itself or drop the citation.
10. **[MINOR] J7 matching rule unspecified.** "Where the distractor bank covers a miss" needs a
    deterministic match definition (learner answer ≡ pre-authored distractor under the same
    canonicalization used for grading); no match → the A4 async call runs as today.
11. **[NIT] J-O2 default.** Recommend the owner adopt the **bump-triggered re-verification sweep**
    as the default (mirrors the ISSUE-10 re-stamp lesson the doc itself cites); periodic sweeps
    are unnecessary at $0.15/run.
12. **[NIT] Variety floor missing.** See §1.5 — adopt J-R1 (≥ 12 template instances or ≥ 6
    exemplars per servable KP) so the unlock does not convert a coverage hole into a
    narrow-clone hole.

---

## Out-of-scope changes

- Working-tree modifications unrelated to this review: `crates/web/src/grade/verdict.rs`,
  `crates/web/src/serve/mod.rs`, `crates/web/tests/readiness_parity.rs` — rustfmt reflow plus a
  small test edit (H-1 unit-guidance area). Not part of Amendment J; no Jev code exists anywhere
  (verified by grep), which is the correct state for an unratified draft.
- The pedagogy reference `../cadus/docs/PEDAGOGY.md` does not exist on this box (1.0 repo absent);
  Hard Rules were verified via `docs/reference/authoring-and-spa-1.0-spec.md:557-563` and
  `REQUIREMENTS.md` C1. Not a defect of the doc under review, but the amendment cites
  `../cadus/docs/PEDAGOGY.md` transitively via C1 — fine as long as C1's citation stands.

---

## Per-requirement verdict table

| ID | Verdict | Change required |
|---|---|---|
| J1 (access contract) | ACCEPT-WITH-CHANGES | T5 caching carve-out + `cached=0` logging (F8) |
| J2 (3-vote gate) | ACCEPT-WITH-CHANGES | escape-hatch disposition + per-vote spec + rule order + sample floor (F4); answer-source rule (F2); ratification-gating benchmark (F7) |
| J3 (symbolic preferred) | ACCEPT | none |
| J4 (hard wall) | ACCEPT | recommended: CI/lint enforcement of "no Jev symbols on request paths" as the build-breaking mechanism (R7) |
| J5 (triage) | ACCEPT-WITH-CHANGES | reroute `needs_multipart` to Gate 1 against the implemented AnswerContract grammar; add manual re-triage override (F3) |
| J6 (C6 bands) | ACCEPT-WITH-CHANGES | per-kind threshold table (J-O1) mandatory *before* auto-approve of teach pages; correctness and quality questions kept separate |
| J7 (distractor analysis) | ACCEPT-WITH-CHANGES | deterministic miss-match rule + fallback to A4 call (F10); leakage test |
| J8 (guardrails) | ACCEPT-WITH-CHANGES | bound the middle-band queue (F5); count rephrasing tokens (F8) |
| J-O1 (thresholds) | ACCEPT | none |
| J-O2 (model-bump sweep) | ACCEPT | recommend bump-triggered default (F11) |
| J-O2–J-O4 | ACCEPT | J-O4: add manual override path (see §1.2) |
| §C: V1/V2 amendments | ACCEPT-WITH-CHANGES | F1 (runtime disposition + explicit runtime-Jev prohibition) |
| §C: A2, C6, T2, T4, A7, §7, §8 | ACCEPT | T2: name the rephrasing sink (F8) |

**Overall:** REQUIREMENTS-JEV.md is **approved with the listed changes** — not as-is, not a
rewrite. The architecture (author-time-only Jev behind a hard wall, symbolic-first, fail-safe
degradation, digest-bound decisions) is the correct shape and is *more* conservative than the
status quo it replaces where it matters (the dangerous direction). The blocking items below are
specification gaps and one stale premise, not architectural objections.

---

## Prioritized required changes

**Blocking (must be in the text before ratification):**

- **B1 (F1):** Partition Jev-verified kinds: (i) deterministic-runtime kinds — Jev verifies the
  answer key, runtime grades deterministically, 0 tokens (C4/L2/T1 intact); (ii) undecidable
  kinds (proof/free-form) — no runtime verdict, no mastery credit, explicit "Jev never grades at
  runtime" sentence in the amended V1 text.
- **B2 (F6):** Add the bad-key discovery & repair protocol (dispute flag → quarantine → C2
  regraded events → D-O6 replay → DAG-aware downstream repair).
- **B3 (F7):** Before any auto-accept is enabled, re-run the corruption benchmark
  (≥ 50 perturbations, all unlocked answer kinds, vector-direction class included) through the
  full 3-vote gate + escape hatch; publish measured residual FPR in the ratification record.
- **B4 (F3):** Re-derive J5 triage routing from the implemented `AnswerContract` grammar;
  `needs_multipart` → Gate 1; Gate 2 reserved for forms no contract expresses.
- **B5 (F4/F5):** Specify escape-hatch disposition (→ review queue), rephrasing generator +
  provenance, rule application order, raised sample floor; bound the middle-band queue (cap,
  aging-out, dashboard count).
- **B6 (F8):** T5 caching carve-out for the alpha endpoint; rephrasing tokens counted under T2
  and J8.

**Recommended (non-blocking):**

- **R1 (F9):** fix the HANDOVER-OUTSTANDING citation.
- **R2 (F4c):** escape-hatch per-vote vs per-sample decision recorded as a J-O decision.
- **R3 (F12):** adopt the distinct-practice floor (J-R1, ≥ 12 template instances / ≥ 6 exemplars)
  and note Jev as the future verification seam for generated novel variants (A7).
- **R4 (F10):** deterministic distractor-match rule + fallback.
- **R5 (F11):** J-O2 default = bump-triggered re-verification sweep.
- **R6 (§1.2):** correct the authoring report's "functionally equivalent" framing for
  exemplar-only KPs wherever the amendment echoes it — ungraded practice does not feed mastery.
- **R7 (J4):** implement the build-breaking status as a CI check (no Jev identifiers reachable
  from any request handler crate).
