# Cadus 2.0 — build progress

One entry per milestone cycle (HANDOVER.md §2). Newest first.

## M6 — offline authoring pipeline, review tooling, React + TypeScript SPA (2026-08-30, in progress)

Requirement IDs: A2, C6, T2, T3, T5, T6, C5, C1, L4, L5, O3. Plan: `docs/plans/M6.md`. Spec:
`docs/reference/authoring-and-spa-1.0-spec.md` (§7: units R1–R8, S1–S14). Owner go: 2026-08-30
(`docs/DECISIONS.md`). Pipeline workflow `wf_ca4db33a-875`: the Rust chain R1 → R2 → (R3 ∥ R4) → R5 →
(R6 ∥ R7) → R8 and the SPA chain S1 → … → S14 run in parallel; S12 waits for R5. Branches `m6/r<n>`, `m6/s<n>`.

Progress at 2026-08-30 (a process exit interrupted the run; resumed with `resumeFromRunId`): R1 prompts (`80e8104`), R2 batch
loop (`5a9de31`), R3 T3 accounting (`e434c1b`), R4 store admin path (`9f687dc`), S1 scaffold (`5e530c7`), S2 API (`15065f3`),
S3 hooks (`8407b1f`), S4 design system (`7ca9371`), S5 math input (`33bd45e`), S6 auth screens (`ab27bf6`), S7 dashboard
(`ab9b308`), S8 session loop (`8b4fbe7`), S9 diagnosis panel (`f48ba90`) — all gate green on their branches. R5 and S10 were
in flight; their partial work is on `m6/r5-wip` and `m6/s10-wip`, which the resumed units merge first.

In parallel, unit FIX-D6 (branch `d6/decimal`) implements the owner's D6 ruling of 2026-08-30 (`docs/DECISIONS.md`
row `D6-dec`): a learner decimal that equals the exact value rounded half-to-even to the typed digits is
correct-with-`notation`; exact rational arithmetic, no float; the 240 M2 divergence pairs get pinned counts.

FIX-D6 merged (`7830b47`, gate on its branch 1,610 tests): rung 5 of `cadus_core::answer::check` and the module
`cadus_core::answer::rounding`. Of the 240 pairs: 151 correct-with-notation, 5 stay wrong (three nested radicals,
two fractions), 84 name `pi` or `e` and get no verdict (a miss at the route, spec 5.1). Class-3 agreement with 1.0:
16,498/16,498. D6 follow-ups for the M6 backlog:
- A nested radical (`√(2+√3)/2` vs `0.9659258263`) canonicalizes to a `Poly` over a `sqrt` call and stays wrong although the decimal is the exact rounding; extend the rule to a symbol-free `Poly` of `sqrt` calls.
- `pi` and `e` against a decimal get no verdict; rational interval bounds for the two constants make the case decidable (`3.14` for `π` is the exact rounding to 2 digits).
- `notation_note` (the prose "Correct value. One note on form: ...") has no caller: the M5 reply carries the `notation` tag only. Wiring the note into the reply is an API change (web-service spec 2.1).

## M5 — HTTP API, session state, deterministic grading, async diagnosis, model-call log (2026-08-27 to 2026-08-30, closed)

Requirement IDs: A3, A4, C1–C4, R2, R4, L1–L6, T1–T6. Plan: `docs/plans/M5.md`. Spec:
`docs/reference/web-service-1.0-spec.md`. Owner go: 2026-08-27; D-M5-2 on the plan default.

| Unit | Branch | Result |
|---|---|---|
| U1 axum skeleton: error envelope, security headers, CSRF origin layer, route-template metrics, `/api/health`, `/api/ready` (worker liveness from `diagnosis_jobs`, migration 0007) | `m5/u1` | gate green |
| U2 auth primitives: Argon2id, password policy, tokens, email normalization, cookie writer | `m5/u2` | gate green |
| U3 `cadus_store::auth`: the five SECURITY DEFINER lookups and the bound writes | `m5/u3` | gate green |
| U6 `cadus_web::state`: the D-S6 row, the per-tenant lock, session routes, export | `m5/u6` | gate green |
| U7 serve + teach + hint: pool pop, A6 exemplar fallback, authored teach page and hint ladder | `m5/u7` | gate green |
| U4 `/api/auth/*` routes, the four paired rate rules, anti-enumeration | `m5/u4b` | gate green (continued from the interrupted U4 WIP) |
| U5 OAuth: Google + GitHub, PKCE, state, handshake cookie | `m5/u5` | gate green |
| U8 the one-transaction grade path: check, tier, `{task_id}-{n}`, ON CONFLICT no-op, incremental fold / full replay on regrade, H3, timing, caps | `m5/u8b` | gate green (continued from the interrupted U8 WIP) |
| U9 A4 client surface: `diagnosis` field, distractor lookup, job enqueue, `GET /api/diagnosis/{id}`, SSE over LISTEN/NOTIFY | `m5/u9` | gate green |
| U10 diagnosis worker: SKIP LOCKED claim, sweeps, the model client crate with T5 defaults, retries, vocabulary filter, NOTIFY | `m5/u10` | gate green |
| U11 T6 model-call ledger and `/metrics` series (migration 0009) | `m5/u11` | gate green |
| U12 budgets: per-route table, grade/serve benchmarks, L6 crate-boundary test, arena benchmark, `GET /api/operator/flags` | `m5/u12` | gate green (commit 1ee253f; release p95: serve 1.9 ms, grade 7.6 ms, arena 3.3 ms, instantiate 8.3 µs) |

Gate on `main` after U1–U12: 1,534 tests, 0 failed, live oracle enabled (log gate-m5-3). Gate after the fix wave: 1,572 tests, 0 failed (log gate-m5-4).

Review rounds 1 and 2 (2026-08-29, `docs/reviews/M5-review-1.md`): 28 raised, 18 confirmed, 12 distinct defects (three blockers: the `Tenant` layer that no route had, the `attempt_id` counter reset, the worker compose block without the model variables). Fix wave `wf_84cbaac3-726` (2026-08-29/30): units FIX-M5-A..F and T in parallel, then G and H in sequence; all nine gate green on their branches; merged through `m5/fix-h`. Changes: the attempt number comes from the event log; `ServedProblem` carries `solution_sketch` and `serve_topic`; the tenant layer (`auth::layer`) binds a live session on every route; `safe_next` rejects control bytes and a provider link to an unverified account clears its password; `/api/auth/*` body rejections answer the envelope; compose forwards the seven model variables to the worker; a quiz route test; one event-log read per request behind a cached session view (migration 0010, benchmark `bench_long_log` at 20,000 events: serve p95 < 100 ms, grade p95 112.9 ms < 150 ms); the serve appends `task_served` (ruling D-M5-8).

Verification round (2026-08-30, `docs/reviews/M5-review-2.md`): the twelve round-1 fixes hold; 19 raised, 13 confirmed, ten distinct defects (three from the fix wave: the serve leaves the fold cursor one line behind the log, plan/status drop a running drill, the repeat-fail rule reads only the session window; seven pre-existing: the Path rejection outside the envelope, no email length cap, the DEL byte in `next`, the worker loop skips diagnosis when refill is absent, three tests that measure the wrong thing or flake under load). Fix wave 2 `wf_53264f5d-8dd`: FIX2-M5-A, B, D, E in parallel, then C.

U12 items for the M5 review (from the unit report):
- The L1 arena segment moved from 5 ms to 20 ms (`docs/reference/l1-budget.md` §2). M4 wrote 5 ms without a measurement; the first measurement is 3.24 ms p95. The 150 ms total is unchanged. The review rules on the split.
- `GET /api/operator/flags` reports `pool_depth` and `last_source` for the calling admin's tenant only (`serving_pool` is under RLS); `approved_templates` and `needs_template` are deployment-wide. `source_exhausted` is always false (the refill backoff map lives in the worker process). `docs/SELF_HOST.md` states both limits.
- The route runs `cadus_core::template::gate` at request time (at most 20 runs, about 200 ms CPU). No L* line covers the route; R4 holds. Two merge
seams were resolved by hand (additive: the web purity list, the store error variants and
module list); one load-sensitive debug timing test now uses the bomb budget.

### M5 close (2026-08-30)

Fix wave 2 (`wf_53264f5d-8dd`): five units, all gate green on their branches, merged through `m5/fix2-c`. Changes: the serve folds and saves after it appends `task_served`, so `through_seq` stands at the log head after every route; `GET /api/session/plan`, `GET /api/status` and the serve path compose from one session view (`session::view_for_open_session`); the repeat-fail rule reads a `lesson_failures` map in the cached session view; the worker tick runs the diagnosis pass without a refill job; `ApiPath` maps the path rejection into the envelope; the email field is capped at 254 bytes after normalization; `safe_next` accepts bytes `0x21..=0x7e` only; benchmark B (grade) folds its attempt, the pool tail test accepts `None` at the tail, the histogram test asserts shape.

Gate on `main` at `da01837`: 1,593 tests, 0 failed (log gate-m5-5). Release benchmarks: serve p95 1.9 ms, grade p95 7.6 ms, 20,000-event log serve p95 2.7 ms and grade p95 104.6 ms, arena p95 3.3 ms.

Review totals for M5: round 1 and 2 raised 28, confirmed 18 (12 distinct); the verification round raised 19, confirmed 13 (10 distinct). All 22 distinct defects are fixed with a red-then-green test each and a mutation check. No third review round: the verification round re-opened none of the round-1 fixes, and no blocker appeared in wave 2.

Open items carried to M6 (`docs/plans/M6.md` backlog):
- The FIRST serve of a task in a session pays one whole-log read and fold (114–167 ms at 20,000 events, once per task per session): `project_incremental` replays the earlier events for the light indices (xp, streak, velocity, quiz, remediation). A cached light-index document removes the last whole-log term.
- `SessionContext::with_open_multistep_components` has no caller, so the R6 stable-component branch reads an empty slice; the `task_served` event now carries `component_topics`, so the read is one lookup in `compose_plan`.
- The tenant layer runs `current_user`, and `/api/auth/me`, `logout`, `logout-all` and `/api/operator/flags` run it again (a cost, not a defect).
- The client-facing `index` restarts after `POST /api/enroll`; the recorded attempt number does not.
- `JobPayload.topic` carries the record topic beside the serve topic's knowledge point.
- The refill pass has no test through `run_with` (every refill test calls `refill_once`).
- The pool tail test's lower bound (`>= 192` serves) rests on an argument, not a proof.
- The 1.0 quiz batch reveal route (`service.complete_task`) is not ported; the quiz test proves the reveal material only.
- `GET /api/operator/flags`: `pool_depth` is per calling tenant, `source_exhausted` is always false, the route runs the template gate at request time.

## M4 — serving pool, template instantiation, anti-repeat, L1/L2 benchmarks (2026-08-27)

Requirement IDs: A1, A5, A6, A7, D5, D-S4, D-S5, D-O1, D-O4, L1, L2, C6, T1, V2. Plan:
`docs/plans/M4.md`. Spec: `docs/reference/serving-1.0-spec.md`.

| Unit | Branch | Result |
|---|---|---|
| U1 template core: document, constraint language, renderer, exact evaluator over the M2 AST, domains, `space_size`, seeded draws | `m4/u1` | 26 tests; `a**2` renders the 1.0 statement, hash `e4047cd6798e`; `a > b` holds on 10,000 draws; 7 mutations red |
| U2 verification gate: the 28 checks with literal 1.0 messages + the 2.0 additions | `m4/u2` | 38 tests; the four live 1.0 rejections reproduce byte-for-byte |
| U3 pool sources: `ProblemSource` trait (A7), template and exemplar sources, ring 20, task memory 12, candidate rule | `m4/u3` | 32 tests |
| U4 store pool ops (batch insert, pop with `SKIP LOCKED` + ring filter + claim in one transaction, `operator_flags`) and the worker refill job | `m4/u4` | 33 tests; 2×100 concurrent pops give 200 distinct rows; RLS holds |
| U5 benchmarks A and B, `docs/reference/l1-budget.md`, `scripts/bench.sh`, CI artifact | `m4/u5` | A: p95 8.45 µs (segment 5 ms), 107,581 allocations at U5 (107,680 after the fix waves, bound 108,218); B: p95 1.9 ms (segment 100 ms) on this box |

Gate on integrated `main`: all steps PASS (debug + release parity + benchmarks), live oracle enabled.

### Adversarial review round 1 (M4)

Six lenses, two find/refute rounds: 40 raised, 22 confirmed, 6 blockers
(`docs/reviews/M4-review-1.md`). Blockers: the refill inserted instances the gate never
checked (the gate samples a fixed 4,096 tuples above the limit; the 1.0 incident the
spec records); a parameter used only in the answer let one statement carry several
answers; the image ships no curriculum, so the worker refilled nothing; a pair that can
never fill starved the refill queue. Rulings in the record. Fix units FIXM4a–c fixed all 22 (commit d717818): every
instance is re-checked by `template::check_instance` before it enters the pool; a
hidden answer-only parameter is refused; the sampled walk continues past a spent draw and
reads coverage from the satisfying sample; constraint literals read `n/d`; values keep
their authored spelling (new `decimal` domain kind); rendered negatives and fractions are
parenthesized; the answer writer brackets nested powers; the image carries
`/app/curriculum` and the worker exits 2 without a curriculum; unfillable pairs leave the
refill queue with a 15-minute backoff; the batch nonce is the clock; an undecodable pool
row is retired and the pop continues; benchmark B runs the production insert/pop; A's L2
half bypasses the string rung. Gate: 1,148 tests, benchmarks A p95 8.3 µs / B p95 1.9 ms.
A verification round (round 2): 18 raised, 11 confirmed, 0 blockers
(`docs/reviews/M4-review-2.md`) — two tuples could render one statement with different
answers; the space estimator and the walk used different draw budgets; choice coverage
read the declared list; pool rows were served after an approval was revoked; benchmark
literals were stale. Fix units FIXM4d–f fixed all 11: the gate refuses a statement that two tuples answer
differently (`statement-collision`) and the fill refuses a colliding digest; one walk
counts the distinct satisfying tuples (the 4,096-draw estimator is gone); choice coverage
and axis ends read the satisfying sample where a constraint names the axis and the
declared ends otherwise; the pop joins `content_store` and serves a template row only
while its digest is approved, and the refill retires rows of a revoked digest; a pair with
two empty fills is backed off 60 minutes and flagged; benchmark A pins three rendered
instances and the bound is measured + 0.5% (108,218).

### M4 close (2026-08-27)

Final gate on `main`: all steps PASS in both profiles, live oracle enabled, benchmarks in
the gate (A instantiate p95 8.3 µs / 5 ms; A check p95 25 µs / 5 ms; B serve p95 3.7 ms /
100 ms). What M4 delivers: `cadus_core::template` (document with inter-parameter
constraints, exact evaluator over the M2 AST, renderer, domains incl. `decimal`, seeded
draws, `space_size`), the 28-check verification gate with the 1.0 messages plus the 2.0
rules (grammar membership, canonical round-trip, hidden parameter, statement collision),
`cadus_core::pool` (`ProblemSource` seam, template and exemplar sources, per-instance
re-check on every fill, ring 20 + task memory 12), `cadus_store::pool` (batch insert,
pop with `SKIP LOCKED` + ring + approval join + claim in one transaction, retirement,
`operator_flags`), the worker refill job with backoff and clock nonces, the image with
`/app/curriculum`, benchmarks A/B and `docs/reference/l1-budget.md`. Review: two rounds,
22 + 11 confirmed, all fixed. M4 closes on the two-round cap.

Open for M5: `source_exhausted` is worker-process state (a durable per-pair table needs a
migration); two approved digests for one KP both stay servable until one is rejected;
`ExemplarSource` dedups by digest without the answer check (an authored-content lint item).

Open notes: the serving key is `"<topic_id>/<kp_id>"` (a KP id is unique inside its topic
only) — M5/M6 must use the same spelling; the refill target list derives from existing
pool rows, so the M5 serve path writes the first row on a pool miss; M5 adds the L1
arena-traversal and the L2 grade-transaction benchmarks.

## M3 — core scheduler/projector port, incremental fold, parity (2026-08-27)

Requirement IDs: R5, D3, D4, C2, C4. Plan: `docs/plans/M3.md`. Spec:
`docs/reference/projector-1.0-spec.md`. Oracle: `scripts/oracle/dump_projector_1_0.py`.

| Unit | Branch | Result |
|---|---|---|
| U1 events (16 types), learner model, config (`config_hash` preimage byte-exact), numeric helpers (Neumaier sum, round-half-even, correctly-rounded `round_dp`, `local_day`) | `m3/u1` | 57 tests; `serde_json` needed `float_roundtrip`; 5 mutations red |
| U2 FIRe + XP | `m3/u2` | 56 tests; every pinned 1.0 literal |
| U3 projector: fold, `apply_regrades`, `finalize`, `project`, `project_incremental`, canonical blob | `m3/u3` | 38 tests; stream 1 digest `ba128459…` matches 1.0; incremental == full at every split |
| U4 selector: `compose_session` with every ordering rule, `compress`, `order_lessons`, interleave, multistep, task ids, `QuizSampler` (2.0 RNG, not CPython parity — trap T11) | `m3/u4` | 44 tests; L1 core cost 2.5 ms in release against a 5 ms budget |
| U5 stream generator (seeds 2–20), 1.0 digests, property tests, selector oracle, measured coverage | `m3/u5b` | 20/20 streams match the 1.0 digest in UTC, America/New_York, and without regrades; selector plan equals 1.0 on 10 seeded states; 4 mutations red (one after a generator extension) |

Gate on `main` after U1–U5: all steps PASS, live oracle enabled.

### Adversarial review round 1 (M3)

Six lenses, two find/refute rounds: 25 raised, 15 confirmed (`docs/reviews/M3-review-1.md`).
Blocker: in a release build LLVM rewrites `0.5.powf(x)` to `exp2(-x)`, one ulp off glibc
`pow`, so the optimized fold diverged from 1.0 on 10% of streams — the gate tested the
debug profile only. Rulings: `black_box` the base, add a release-profile parity step to
the gate; event `Slug`s strip whitespace as 1.0 does; overflow and out-of-range integers
become fold errors; boundary tests at equality; the selector oracle compares with n = 40.
Fix units FIXM3a–b fixed all 15 (commit below); the gate now runs the parity tests in
the release profile too.

### M3 close (2026-08-27)

Final gate on `main`: all steps PASS in both profiles, live oracle enabled. What M3
delivers: `cadus_core::{event, learner, config, numeric, fire, xp, projector, selector}`
— the 16 event types on the 1.0 wire shape, the learner model with a 2.0 `through_seq`
cursor outside the parity blob, `config_hash` byte-exact, the FIRe engine and XP with the
CPython numeric semantics (Neumaier sum, half-even rounding, correctly-rounded `round_dp`,
glibc `pow` through `black_box`), the projector with `apply_regrades`, full and incremental
projection, and the selector with every 1.0 ordering rule (quiz sampling is 2.0's own
RNG — trap T11). Parity: 20 seeded streams fold to the 1.0 digests in three timezone
settings; the selector matches 1.0 on 10 states with n = 40; incremental-fold divergence
classes are pinned to 1.0's digests. Review: one round, 15 confirmed, all fixed.
M3 closes on the milestone cap (the owner asked for M4 next).

Ruling recorded here: 1.0's `project_incremental` does not re-fold a `regraded` event
that supersedes an already-cached grade (1.0 `projector.py:794-830`); 1.0's service
layer forces a full replay when the new events hold a `regraded` (spec §5). The port
reproduces the primitive exactly (`incremental_1_0.json` pins the divergent splits and
digests). M5 requirement: `project_and_save` forces a full replay on any `regraded`
event, as 1.0 does.

## M2 — answer checker: grammar, exact arithmetic, oracle fuzz (2026-08-26)

Requirement IDs: V1–V4, D6, A3, C4, L2, R5. Plan: `docs/plans/M2.md`. Spec:
`docs/reference/checker-1.0-spec.md`. Corpus: `crates/core/tests/fixtures/answers/`.

| Unit | Branch | Result |
|---|---|---|
| U1 normalize + lexer + recursive-descent parser (grammar §8.1 + intervals), 4,000-char cap, no panic on any input (10 s fuzz) | `m2/u1` | corpus split 3,214 parse / 278 undecidable (committed as `undecidable_1_0.jsonl`); 20 tests; 4 mutations red |
| U2 canonical forms (BigRational, squarefree radicals, sparse polynomials with signed exponents and Inverse atoms, sets/tuples/lists/intervals) + `check` with the rung order and the dot-thousands reading; MAX_STEPS work bound | `m2/u2` | every pinned 1.0 pair of spec §6 gives the 1.0 verdict; 38 documented divergences pinned to the 2.0 verdict; 4 mutations red |
| U3 oracle harness (`scripts/oracle/check_1_0.py`), 36 notation generators, committed 1.0 verdicts, residue report | `m2/u3` | 14,875 pairs: class 3 comparable 13,924 with 100% agreement, class 4 documented 16, class 1 outside grammar 935; `docs/reference/undecidable-answers.md` lists the 278 residue answers |

Stage 5 (oracle parity): the committed verdict fixture came from the live 1.0 oracle;
the live re-run runs in the gate with `CADUS_ORACLE_PYTHON` set.

Gate on integrated `main`: 276 tests, 0 failed; all gate steps PASS.

### Adversarial review round 1 (M2)

Six lenses (false-positive, false-negative, correctness/safety, parity/oracle, test
quality, spec/docs), two find/refute rounds: 39 raised, 21 confirmed
(`docs/reviews/M2-review-1.md`). Blockers (C4 false positives): `2⅓` read as `2*(1/3)`;
the `x =` label strip dropped the variable so `x = 4` equaled `y = 4`; `cos 2x` bound one
atom (`x*cos(2)`); a percent rescaled the whole body; an unbounded LCM cost 8.4 s on one
in-grammar answer; a space-grouped numerator read as a mixed number. Rulings are in the
record. Fix units FIXM2a–c.

Fix units FIXM2a–c fixed all 21 plus the known items: mixed numbers with vulgar
glyphs (`2⅓` = 7/3), labels as `Ast::Assign` with symmetric tolerance and name
comparison, juxtaposed function arguments (`cos 2x` = `cos(2x)`), percent bound to its
number, times-`x`/`X` between numerals, multi-letter runs split (with prose still
refused), `ln`≡`log`, `Atom::Exp`, a width-charged work bound, debug/release timing
rules, a killable oracle worker. Regenerated: corpus split 3,227 parsed / 265
undecidable; oracle set 14,989 pairs, class 3 agreement 14,037/14,037, 21 documented
divergences. Gate with the live oracle green.

### Adversarial review round 2 (M2)

One find/refute round on the fixed tree: 20 raised, 17 confirmed, 7 blockers
(`docs/reviews/M2-review-2.md`). The round-1 mixed-number reading covered only the
glued glyph: `2 ½`, `2\frac{1}{2}` still read as products (`check("1", "2 ½")` was true);
the juxtaposed function argument stopped at an explicit `*` (`cos 2*x` = `x*cos(2)`);
`\sqrt{}` after a letter glued into one name; a space-grouped number after a factor
invented a value; canon kept two forms for `e^(x+2)` and for reciprocals of powers; the
oracle harness had a catch-all divergence reason. Ruling: one mixed-number rule in the
parser for every spelling; the argument chain continues through `*`; one `Inverse` per
monomial; no catch-all reason. Fix units FIXM2d–e, regeneration FIXM2f, then one
verification round, then M2 closes with any leftover recorded.

Fix units FIXM2d–f fixed all 17: one mixed-number rule in the parser for every spelling
(literal-fraction token `⟦b/c⟧`), the argument chain through `*`, the `\sqrt` product
sign, space-grouped numbers refused after a factor, negated times-x, `Atom::E` folding,
one `Inverse` per monomial, no catch-all divergence reason, four more generator
families (case flip, ten significant digits, product reorder, algebraic refactor).
Regenerated: 17,047 oracle pairs, class 3 agreement 15,940/15,940, 176 documented
divergences (156 of them the D6 class: a decimal approximation of an exact value is
wrong in 2.0 — see "Decision for the owner" under M2). Gate with the live oracle green.

### Adversarial review round 3 (M2)

One find/refute round: 18 raised, 14 confirmed, 5 blockers (`docs/reviews/M2-review-3.md`):
a space inside `\frac{ 1}{2}` defeated the mixed-number token; the percent rewrite
`(N)/100` re-associated under `/`; a juxtaposed argument swallowed a following function
name (`sec x tan x`). Ruling: LaTeX and glyph constructs become lexer tokens with parsed
structure (no string rewrites); canon replaces `Inverse` atoms with an expanded
numerator/denominator pair (no GCD). Fix units FIXM2g–i, then verification round 4. If
round 4 confirms another C4 blocker, M2 stops for an owner decision on the grammar design.

Fix units FIXM2g–i fixed all 14: LaTeX and glyph constructs are lexer tokens
(`\frac` with recursive brace lexing, `\sqrt`, `^{}`, `\cdot`, `\times`, `%` postfix,
vulgar glyphs, `√`, superscripts, `°`); a percent node wraps exactly its primary; the
juxtaposed argument stops at a function name; `-0 1/2` keeps its sign; canon uses
`Canon::Value {num, den}` (expanded, no polynomial GCD; `x/x = 1` by exponents); `sqrt` of
a rational reduces; `Atom::E` folds the integer part. Oracle set regenerated with a
SymPy-generated rational-rewrite family: 17,874 pairs, class 3 agreement
16,554/16,554, 355 documented divergences in 14 specific classes (D6 240, bare `e` 46,
times-x 24, no-GCD 6, no-radical-rationalization 6, …). Gate green with the live oracle.

### Adversarial review round 4 (M2)

One find/refute round on the restructured tree: 9 raised, 4 confirmed, 0 blockers
(`docs/reviews/M2-review-4.md`): a `b/c` fraction after a `/`- or `^`-consumed number
took the product reading; one 4,000-char in-grammar answer cost 378 ms in release (the
work bound did not charge sum rebuilds); two missing negative tests. Fix unit FIXM2j;
then M2 closes (four rounds: 21, 17, 14, 4).

### M2 close (2026-08-27)

FIXM2j fixed the four round-4 findings (a `b/c` fraction after a `/`- or `^`-consumed
number is Undecidable; the work bound charges sum rebuilds — the 3,267-char review case
went from 369 ms decided to 0.8 ms refused; the corpus worst case is 104 µs in release;
two negative tests). Final gate on `main` with the live oracle: all steps PASS.

What M2 delivers: `cadus_core::answer` — normalization (six whole-string steps), a lexer
with LaTeX and glyph tokens, a recursive-descent parser for the decidable grammar
(integers, decimals, fractions, mixed numbers in five spellings, radicals, polynomials,
function applications, tuples/sets/lists, intervals and chained inequalities, percent,
value labels), exact canonical forms (BigRational, squarefree radicals, sparse
polynomials, `Canon::Value {num, den}`), `check(expected, learner, kind) → Decided{correct,
notation} | Undecidable`, a work bound, and a 1.0 oracle harness with 47 generator
families: 17,874 pairs, class-3 agreement 16,554/16,554, 355 documented divergences in
14 specific classes. Corpus: 3,227 of 3,492 answers decidable; the 265 residue rows are
listed for re-kinding in `docs/reference/undecidable-answers.md`.

Review record: four rounds, 21 + 17 + 14 + 4 confirmed, all fixed and mutation-checked.

### Decision for the owner (M5, before U8 starts)

The tier a deterministic miss records (`docs/plans/M5.md` D-M5-2): the plan uses
`nearly_passable` (XP ×0.3, below the pass line) for a decided wrong answer and `poor`
for a blank; the async diagnosis adds tags and prose but never moves the tier. The 1.0
model chose the tier per miss. Say if you want a different tier.

### Decision taken (M2, ruling `D6-dec`, 2026-08-30)

The D6 rule made a decimal approximation of an exact value WRONG in 2.0: `0.3333333333`
for `1/3`, `11.31370850` for `8√2` (240 generated pairs, 1.0 accepted them at 1e-6).
The owner took the decidable alternative, and `docs/DECISIONS.md` row `D6-dec` records
it. A learner decimal is correct when it equals the exact value rounded half-to-even to
the digits the learner typed (exact rational arithmetic, no float), and the verdict
carries the `notation` tag, the way the dot-thousands reading does. FIX-D6 ships it as
rung 5 of `cadus_core::answer::check`, with the module `cadus_core::answer::rounding`.
Of the 240 pairs, 151 are correct with the tag, 5 stay wrong (three nested radicals and
two fractions), and 84 name `pi` or `e` and get no verdict (V2).

Known items before the M2 fix wave were: the lexer refused a multi-letter run (`3xy^2`, 11
corpus answers on 9 topics) — split unknown letter runs into single-letter variables
except function names and differentials; `e` and `E` both read as Euler's number (1.0
reads lowercase `e` as a symbol); spec §8.2/§8.3 counts to annotate with the measured
split; new 1.0 defects found by the fuzz (the Python tokenizer reads `2j` as an imaginary
literal, so 1.0 marks the live answers `$3i - 2j$` wrong).

## M1 — curriculum arena, loader, lint port, parity (2026-08-26)

Requirement IDs: D1, D2, C5, R5. Plan: `docs/plans/M1.md`. Spec:
`docs/reference/curriculum-1.0-spec.md`. The 1.0 curriculum tree is copied into
`curriculum/` (89 YAML files, 13 courses, 1090 topics, 3138 knowledge points).

| Unit | Branch | Result |
|---|---|---|
| U1 loader: types with `deny_unknown_fields`, `Slug`, findings, parse stage; YAML crate `serde_norway` | `m1/u1` | 17 tests; literal counts; fixture messages generated by the 1.0 loader; 5 mutations red |
| U2 arena: interning, CSR both ways, `enc`/`enc_rev` with the 1.0 asymmetry, topo order (Kahn, min-heap on load index), cycle, closures, `encompassing_weight` (LIFO replay), mastery floor | `m1/u2` | 21 tests; 3281/3200/3282 literal; 3 oracle weights exact; 5 mutations red (one equivalent mutant: LIFO→FIFO gives the same f64 on the tree) |
| U3 lint port: 16 codes + `empty`, exact 1.0 message text, `expected.json` per fixture generated by the 1.0 lint | `m1/u3` | every fixture equals the committed 1.0 output; clean tree 0 findings; 5 mutations red |
| U4 canonical dump, `curriculum_hash`, `dump_curriculum` bin, live parity test | `m1/u4` | Rust dump byte-identical to the 1.0 dump: sha256 `f121f9ba…a8b9e`, 4,052,882 bytes |

Stage 5 (oracle parity): the live comparison runs in the gate when
`CADUS_ORACLE_PYTHON=/home/deploy/dev/cadus/.venv/bin/python` is set; without it the
test pins the hash and the length literals.

Gate on integrated `main` (commit 128b032): 182 tests, 0 failed; all gate steps PASS.

### Adversarial review round 1 (M1)

Five lenses (parity, correctness, purity/budgets, test quality, data/spec), two
find/refute rounds: 30 raised, 27 confirmed (`docs/reviews/M1-review-1.md`). Blockers:
NaN passed the 0..=1 range check; hidden and symlinked unit files were dropped. Ruling:
2.0 rejects YAML 1.1-only forms (`yes`, octal, `1_200`, merge keys, duplicate keys)
with explicit findings and does not emulate PyYAML; the spec §7 "2.0 strictness" section
lists every deliberate difference. Fix units FIXM1a–c fixed all 27 (commit 7d49756);
gate with the live oracle: 205 tests, 0 failed.

### Adversarial review round 2 (M1) and close

One find/refute round on the fixed tree: 15 raised, 15 confirmed
(`docs/reviews/M1-review-2.md`): the float text broke shortest-digit ties away from zero
where CPython rounds to even; `-0.0` sorted below `0.0`; YAML 1.2-only numeric spellings
(`1e3`, `0o17`, `08`) loaded where 1.0 refused; fixture gaps. Ruling: an integer field
accepts a plain decimal integer or a plain decimal float only; every other spelling is a
`schema` finding found by a line-level pre-scan (spec §7 lists the limits). Fix units
FIXM1d–e fixed all 15; the float text now matches CPython `repr` over a 494,972-value
sweep (0 mismatches). M1 closes on the two-round cap.

Open notes for later milestones:
- `encompassing_weight` runs one relaxation per call (1.0 memoizes per source); M3
  calls `reach_weights` once per source.
- On a tree with several cycles the port names the cycle in load-index order; 1.0
  names one in hash-set order. Not observable on the checked-in tree.
- The `yaml` finding message embeds the YAML library's own text; the lint oracle
  compares it after normalization.

## M0 — workspace scaffold, schema v1, RLS proof (2026-08-25)

Requirement IDs: R1–R4, C2, C3, D9. Plan: `docs/plans/M0.md`.
Owner decisions: O1/O2/O3 unanswered; the build uses the HANDOVER.md defaults
(build the migrator but do not run it; T4 defaults; keep the 1.0 SPA).

### Environment set up on this box

- rustup stable (rustc 1.98), rustfmt, clippy, sqlx-cli 0.9 in `~/.cargo/bin`; C linker
  from `nix build nixpkgs#gcc` (no sudo on the box).
- Throwaway Postgres 16 container `cadus2-testdb` on `127.0.0.1:55434`, trust auth.

### Units and outcome

| Unit | Branch | Result |
|---|---|---|
| U1 scaffold + gate + core purity test | `m0/u1` | gate green; purity test fails when `sqlx` is added to core (mutation-checked) |
| U2 migrations 0001–0006 + `docs/SCHEMA.md` | `m0/u2` | applied twice on a fresh DB; `cadus_app` UPDATE/DELETE on `events` → 42501; 10 tables RLS-forced, 6 exempt |
| U3 store crate: config, pool, migrate, boot guard, `begin_tenant`, test support, 5 RLS tests, `cadus-migrate` bin | `m0/u3` | gate green |
| U4 web crate: `/api/health`, `/api/ready`, boot guard exit 3, SIGTERM shutdown, 6 tests | `m0/u4` | gate green |
| U5 worker skeleton: heartbeat loop, SIGTERM, 2 tests | `m0/u5` | gate green |
| U6 ops: CI workflow, `scripts/check_migrations.sh`, Dockerfile, compose, Caddyfile, `docs/SELF_HOST.md` | `m0/u6` | actionlint clean; migration check fails on a numbering gap (checked) |
| Mutation check of U3 tests | `m0/mut-u3` | 5/6 mutants killed; survivor M4 (policy `WITH CHECK` removal, DML-equivalent) → fix unit FIX1 |

Orchestrator glue: migration 0001 absorbs a lost `CREATE ROLE` race
(`duplicate_object OR unique_violation`); verified with three concurrent migrations.
The policy text uses `nullif(current_setting('app.user_id', true), '')::uuid` so a
`RESET` GUC fails closed (zero rows) instead of raising 22P02.

### Gate on integrated `main` before review (commit 48be9e9)

```
cargo fmt --all --check                                  ok
cargo clippy --all-targets --workspace -- -D warnings    ok
cargo test --workspace                                   15 passed, 0 failed
cargo sqlx prepare --check --workspace -- --all-targets  ok
scripts/check_migrations.sh                              name/fresh/rerun/drop PASS
```

### Adversarial review round 1 (stage 4)

Six lenses, four find/refute rounds: 104 findings raised, 46 confirmed by independent
refuters (`docs/reviews/M0-review-1.md`). Blockers: CI never migrated its gate database;
`cadus_app` could delete another tenant's rows through the `users` FK cascade; the
transaction-local scope of the tenant GUC and the BYPASSRLS half of the boot guard had
no test; the RLS tests read cluster role state instead of the migration. Fix units
FIX1–FIX5 address all 46; round 2 runs on the fixed tree.

Cost note: round 1 used 128 agents. Rounds 2 and 3 are capped at two find/refute
rounds each, major+ only.

### Adversarial review round 2

On the tree after FIX1–FIX5: 32 raised, 16 confirmed (`docs/reviews/M0-review-2.md`).
Blocker: FIX5 removed trust auth from the CI Postgres service, so the passwordless
`cadus_app` test pool cannot authenticate in CI. Majors: `cadus_app` kept table-wide
UPDATE on `users` (cross-tenant `password_hash`/`is_admin` writes), no privilege on
`model_call_log` and `content_store` was revoked, the superuser half of the boot guard
had no test, `pool.close()` and the boot probes ran outside the shutdown select, no
statement timeout on the pool. Fix units FIX7a–c address all 16.

### Adversarial review round 3

On the tree after FIX7 (commit ffe0f62): 31 raised, 16 confirmed
(`docs/reviews/M0-review-3.md`). Several are follow-on defects of earlier fixes: the
orchestrator's advisory lock in `cadus-migrate` was database-scoped, not cluster-wide
(reproduced 8/8); the new 5 s statement timeout also bound the migration run; `users`
INSERT still wrote `is_admin`, and SELECT still exposed every `password_hash`; the web
shutdown spent its deadline twice (20.01 s against a 20 s grace period). Fix units
FIX8a–c address all 16.

### Adversarial review round 4

On the tree after FIX8 (commit 1bdfdf1): 28 raised, 14 confirmed
(`docs/reviews/M0-review-4.md`). Blocker: the three auth tables carry `user_id`, hold
no RLS, and keep full DML for `cadus_app`, so a bound tenant forges a session for any
account. The round-3 login functions returned any account's `password_hash` to any
caller and declared `search_path = public` without `pg_temp`. Fix units FIX9a–c
address all 14. The loop did not go dry after four rounds (16, 16, 14 confirmed);
see "Decision for the owner" below.

### Final gate on `main` after FIX9 (M0 close)

```
cargo fmt --all --check                                  ok
cargo clippy --all-targets --workspace -- -D warnings    ok
cargo test --workspace                                   96 passed, 0 failed
cargo sqlx prepare --check --workspace -- --all-targets  ok
scripts/check_migrations.sh   name, frozen, fresh, rerun, drop   PASS
scripts/check_ops.sh          compose, build, binaries, commands, deploy, invariants   PASS
```

What M0 delivers: workspace `core`/`store`/`web`/`worker`; migrations 0001–0006 with
frozen checksums; 15 RLS-scoped tables with pinned policy text, a literal privilege
matrix for `cadus_app` (tables, columns, sequences, functions, FK delete actions);
the append-only `events` proof (UPDATE/DELETE → 42501); the C3 boot guard (superuser
and BYPASSRLS, exit 3); `cadus-migrate` with a cluster-wide role lock, password rule,
and signal handling; `/api/health`, `/api/ready`; worker heartbeat loop; CI workflow;
Dockerfile, compose stack, `scripts/deploy.sh`, `docs/SELF_HOST.md`, `docs/SCHEMA.md`.

### Decision for the owner — review loop did not go dry

HANDOVER.md §2 stage 4 loops until two consecutive review rounds find nothing new.
After four rounds the count per round was 46, 16, 16, 14 confirmed findings; every
confirmed finding was fixed and the fix was mutation-checked. Tokens spent by
subagents: implement waves ≈ 2.4 M; review rounds ≈ 7.7 M + 3.9 M + 3.8 M + 3.8 M.
Each further round costs about 4 M tokens and, on the evidence of rounds 2–4, finds
10–16 more findings, most of them second-order effects of earlier fixes on the
grants/RLS surface. The orchestrator stopped after round 4 and asks the owner to
choose: (a) continue the loop on M0 at this cost, or (b) accept M0 with the open
findings below and let M1/M2 proceed, with M5 (auth) as the milestone that revisits
the `users`/auth-table policies with real handler code.

### Owner answers (2026-08-26)

O1: start fresh, no event migration. O2 (2026-08-27): DeepSeek V4 via OpenRouter now, local Qwen 3.6 later; no T4 caps (single owner-user, not public). O3: React + TypeScript rewrite in M6. Review
loop: stop after four rounds, fix the accepted-not-fixed items, continue with M1. See
`docs/DECISIONS.md`.

### Open findings

- (M5 contract) The auth layer must call the five SECURITY DEFINER lookups
  (`auth_user_by_email`, `auth_user_by_id`, `auth_session_by_token_hash`,
  `auth_token_by_hash`, `oauth_account_lookup`) BEFORE binding a tenant, then bind and
  write through the policies. `docs/SCHEMA.md` "The M5 auth contract" has the call order.
- (closed 2026-08-26, FIX10) The accepted-not-fixed items of M0 are fixed: default
  `BIND_ADDR` pinned by a pure function test; R4 purity scans handler sources for socket
  and process tokens; a client-side query bound `DB_CLIENT_TIMEOUT_MS` (sqlx 0.9 has no
  TCP keepalive); CI binds the service port on 127.0.0.1; one shared deaf-Postgres test
  server; the `tuple concurrently updated` retry has unit tests; `cadus-store` has a
  purity test; shellcheck runs in the gate; `scripts/deploy.sh` ran end to end with caddy
  on `CADDY_HTTP_PORT=18080`.
- (closed 2026-08-26) The citext function-count pin is replaced by a per-function check.

- (M3) `events.payload` is `jsonb` (D7). 1.0 stored `json` because its diagnostic
  projection read key order. The 2.0 projector must not depend on key order; the M3
  parity test must cover diagnostic events.
- (deploy) `citext` needs the Postgres contrib package; the official `postgres:16`
  image ships it.
- (tests, closed by FIX6) `TestDb::with` is the only entry point. It drops the
  `cadus2_t_*` database of a test body that panics. `TestDb::create` and
  `TestDb::drop` are gone from the public API.
- (docs drift, 1.0) `web_states` payload column is `doc` in 1.0 DDL, `state` in 1.0
  docs; 2.0 follows the DDL.
