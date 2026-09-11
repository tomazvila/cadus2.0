# Cadus 2.0 — Requirements

Cadus 2.0 is a rewrite of Cadus, a math-learning system that implements The Math
Academy Way methodology. Version 1.0 (Python, `../cadus`) proved the pedagogy and
the data model. Version 1.0 also proved a defect: it put one synchronous LLM call
on the critical path of almost every learner action, with no latency or cost
target. Version 2.0 makes latency and cost first-class requirements, inverts the
problem-generation architecture, and moves the backend to Rust.

Requirement IDs are stable. Cite them in reviews and commits.

---

## 1. What carries over from 1.0 unchanged

- **C1 — Pedagogy.** The Hard Rules govern all tutor behavior (`../cadus/docs/PEDAGOGY.md`,
  `../cadus/docs/WEB_SERVICE.md` "Non-negotiable rules"): no answer before an attempt,
  honest structural grades, the core owns all scheduling, generation fidelity,
  LaTeX math rendering, brisk tone. The Math Academy Way PDF stays the source of truth.
- **C2 — Event sourcing.** Every state change is an event in an append-only `events`
  table. The app role has no UPDATE or DELETE on it. A wrong grade is superseded by a
  `regraded` event, never edited.
- **C3 — Multi-tenant Postgres.** Per-user accounts, row-level security, a stateless
  app tier, one-server docker-compose deployment. The app refuses to start if its DB
  role bypasses RLS.
- **C4 — Grade honesty.** `correct` reflects only mathematical correctness. Partial
  credit lives in `work_quality`. A speed optimization that risks a wrong `correct`
  verdict is rejected — correctness outranks every budget in this document.
- **C5 — Curriculum as data.** Topics, knowledge points, exemplars, and constraints
  live in reviewed, linted files under git. Content changes go through git review.
- **C6 — Evidence-backed AI review for authored content.** LLM-authored content
  (templates, problem banks, teach pages, hint ladders) is stored pending. An
  independent AI reviewer checks mathematical correctness, objective alignment,
  explanation quality, and answer leakage before approval. Decisions bind to the
  exact content digest, curriculum, and selected serving context. Unresolved
  content remains pending for AI repair and re-review; human review is optional.

## 2. New in 2.0 — the two budgets

### 2.1 Latency budget (NFR-L)

Server-side, per request, measured at p95 under single-tenant interactive load.
A change that breaks a budget does not merge.

| ID | Path | Budget | How |
|---|---|---|---|
| L1 | Serve a problem | < 150 ms | Local instantiation from an approved bank/template. No model call. |
| L2 | Grade, verifiable answer (correct or wrong) | < 300 ms | Deterministic checker. No model call. |
| L3 | Grade, full feedback prose | verdict per L2; prose arrives asynchronously | The verdict never waits on the model. See A4. |
| L4 | Teach (lesson instruction) | < 150 ms | Pre-authored, approved teach content. No model call. |
| L5 | Hint | < 150 ms | Pre-authored hint ladder. No model call. |
| L6 | Any model call that a learner waits on | none exist | Model calls run offline or asynchronously only. |

### 2.2 Token and cost budget (NFR-T)

| ID | Rule |
|---|---|
| T1 | The serve, teach, hint, and correct-answer grade paths spend **0 model tokens** at runtime. |
| T2 | Model tokens are spent in exactly two places: (a) the offline authoring pipeline, (b) optional asynchronous miss diagnosis (A4). |
| T3 | Authoring cost is amortized: one knowledge point is paid for **once**, then serves forever. Track cost per KP; alert when a KP exceeds 3 authoring attempts. |
| T4 | Async diagnosis is capped: a per-session call cap and a per-call output cap (configurable; defaults 10 calls/session, 600 output tokens/call). When the cap is hit, the learner still gets the deterministic verdict and a stock re-solve instruction. |
| T5 | Every model request sets provider prompt caching when the provider supports it, sets a reasoning-token cap **by default**, and pins provider order **by default**. 1.0 shipped these unset; 2.0 does not repeat that. |
| T6 | Every model call logs model id, cached/uncached input tokens, output tokens, reasoning tokens, latency, and purpose. Cost is a dashboard, not a surprise. |

## 3. Architecture — the inversion (A)

The 1.0 defect: problems were generated live, per serve, so no two problems repeated
and nothing was cacheable. 2.0 inverts this: **the LLM is an offline compiler, not a
runtime dependency.** Content is authored in batch, verified by machine, approved by a
human, and served from local storage.

- **A1 — Parameterized problem templates are the unit of content.** A template holds:
  the statement with placeholders, per-parameter domains, **constraints between
  parameters** (e.g. `a > b`, `a + b` must carry — the feature whose absence blocked
  templates in 1.0), the answer as an evaluable expression, a solution sketch, and a
  hint ladder. The server instantiates locally with fresh values and computes the
  answer itself. 1.0 measured this path at 200 problems in 86 ms with zero API calls.
- **A2 — Offline authoring pipeline.** A batch job (not a request handler) asks the
  model for templates, verifies each one mechanically (sample coverage at domain
  edges and mixed corners, constraint satisfaction, answer-expression agreement with
  hand-worked samples), then queues it for AI evidence review (C6). Rejected templates get
  precise, actionable retry feedback — the 1.0 lesson: the biggest yield lever is the
  quality of the rejection message.
- **A3 — Deterministic grading is the only synchronous grader.** Answer kinds are
  restricted to a machine-decidable grammar (§5). The checker returns `correct`
  instantly for right AND wrong answers — 1.0 already discarded the model's verdict
  whenever the checker had one, so nothing is lost.
- **A4 — Miss diagnosis is asynchronous and optional.** On a wrong answer the learner
  immediately gets: the verdict, the worked solution, and the mandatory unaided
  re-solve instruction (all deterministic). A model call for error-specific diagnosis
  (`error_tags`, tailored prose) runs in the background under the T4 cap; the client
  shows it when it lands. Where a template's common wrong answers are precomputable
  (distractor analysis at authoring time), the diagnosis is pre-authored too and the
  call is skipped.
- **A5 — Anti-repeat without live generation.** Fresh values per instantiation plus a
  served-instance hash log satisfy Hard Rule 4. The server rejects an instantiation
  whose hash was recently served and redraws.
- **A6 — Fallback is explicit, never silent.** A KP with no approved template serves
  its authored exemplars in rotation (the 1.0 FAKE-engine behavior) and is flagged in
  an operator dashboard. There is **no** synchronous live-generation fallback: a slow
  path that "usually doesn't happen" becomes the p95.
- **A7 — The architecture stays generation-ready (future work).** LLM generation of
  novel problems has pedagogical value — the same skill attacked from different
  angles deepens learning — and 2.0 must not close the door on it. It is deferred,
  not rejected. The seam it needs, built now:
  - Problems reach the learner only through a **serving pool**, and the pool is
    source-agnostic. Sources at launch: template instantiation (A1) and exemplar
    rotation (A6). A future LLM generator is a third source behind the same trait —
    a new module, not a redesign.
  - Every source feeds the pool through the **same verification gate** (A2 mechanics:
    machine-checked answer, decidable grammar per §5) before anything is servable.
    A generated problem obeys C6 review policy like any other authored content.
  - A future generator runs in the **worker** (R4) and fills the pool ahead of
    need — per-learner, prefetched — so serve stays L1 (< 150 ms, no model call on
    the request path) and its tokens are budgeted and logged under T6. Generation
    changes what the pool contains, never what a request waits on.
  - The pool schema records `source` per problem, so its pedagogical effect is
    measurable per source before it earns more budget.

## 4. Backend — Rust (R)

- **R1 — Language.** The backend is Rust. No Python in the runtime.
- **R2 — Stack.** `axum` for HTTP, `sqlx` (compile-time-checked queries) for Postgres,
  `tokio` runtime. Session auth and RLS semantics match `../cadus/docs/MULTIUSER_ARCHITECTURE.md`.
- **R3 — Core purity.** The scheduling/pedagogy core is a crate with no network, DB,
  or LLM dependencies — the same layering rule as 1.0's `cadus/`. Adapters depend on
  the core; never the reverse.
- **R4 — Async by construction.** Model calls (authoring, A4 diagnosis) run in a
  worker, not in request handlers. Request handlers do local CPU work and DB I/O only.
- **R5 — Migration oracle.** During the port, the Rust scheduler and checker are
  property-tested against 1.0's Python implementation on recorded event streams.
  Divergence is a bug in 2.0 until proven otherwise.

## 5. Answer checking without SymPy (V)

1.0's deterministic path leaned on SymPy. Rust has no equivalent CAS, and does not
need one, because 2.0 controls both sides of the comparison.

- **V1 — Decidable answer grammar.** Supported kinds: integers, rationals, decimals,
  simple algebraic expressions over a restricted grammar. Equivalence is decided by
  parsing + canonical normalization + exact arithmetic (arbitrary-precision rationals),
  not by heuristic simplification.
- **V2 — Authoring-time enforcement.** The pipeline (A2) rejects any template whose
  answer expression leaves the grammar. Undecidable kinds (`proof`, free-form
  multi-step) are graded per A4's async path and never claim a deterministic verdict.
- **V3 — Oracle parity.** The Rust checker is fuzz-tested against 1.0's SymPy checker
  over the curriculum's answer corpus before it grades a real attempt (R5).
- **V4 — Learner-notation tolerance carries over.** Whitespace, equivalent fraction
  forms, and the dot-thousands notation note behave as in 1.0.

## 6. Data — shapes, operations, structures, and store (D)

### 6.1 The data shapes

Six families cover everything the system holds. Shapes 1–3 carry over from 1.0's
`docs/DATA_MODEL.md`; shapes 4–5 are new in 2.0 (the A1–A7 inversion).

| # | Shape | Content | Mutability | Size |
|---|---|---|---|---|
| D-S1 | **Curriculum graph** | Topics (id, name, answer_kind), knowledge points (constraints, exemplars), prerequisite DAG with encompassing weights | Immutable at runtime; versioned in git; reloaded on deploy | ~1,100 topics, a few MB |
| D-S2 | **Event log** | Per-user append-only sequence `(user_id, seq)` → typed JSON payload; `attempt` is the load-bearing type; `regraded` supersedes, never edits | Append-only (DB grants enforce it) | Grows forever; single rows ≤ ~20 KB |
| D-S3 | **Learner model** | Per-user projection: topic → `{status, repNum, memoryBase, t0, interval_days, ability, speed}` + XP, streak, velocity, and the event cursor it was built from | Derived; rebuilt or incrementally folded; never hand-edited | One row per user, tens of KB |
| D-S4 | **Content store** | Approved templates (statement, domains, inter-parameter constraints, answer expr, hint ladder), teach pages, pre-authored diagnoses; each keyed by content digest with approval state | Written by the authoring pipeline; read-mostly | One record per KP per kind |
| D-S5 | **Serving pool** | Verified problem instances per `(user, kp)`: text, hidden expected answer, source tag (A7), instance hash | Filled by the worker; consumed by serve | Small ring per user |
| D-S6 | **Hot state + queues** | Live session state (served problem, task progress, hint count); job queues (async diagnosis, Anki, email); model-call telemetry (T6) | Transactional read-modify-write; queues are claim-and-delete | One small row per user / per job |

### 6.2 The dominant operations

The hot loop is serve → grade, once per 30–120 s per active learner. Frequencies
drive the design; the budgets of §2.1 bind each row.

| # | Operation | Touches | Frequency | Budget |
|---|---|---|---|---|
| D-O1 | **Serve**: scheduler traverses D-S1 against D-S3, pops one D-S5 instance, checks anti-repeat, writes D-S6 | 1 model read, 1 pool pop, 1 state write; graph work is in-memory | Highest | L1 |
| D-O2 | **Grade**: normalize + compare answer (pure CPU), append one D-S2 event, fold it into D-S3 incrementally, update D-S6 | 1 insert, 2 updates | Highest | L2 |
| D-O3 | **Hint / teach read**: fetch pre-authored content by KP | 1 read, cacheable in memory | High | L4/L5 |
| D-O4 | **Pool refill** (worker): instantiate template, verify, insert into D-S5 | Batch inserts | Background | — |
| D-O5 | **Queue claim** (worker): take one diagnosis/Anki/email job without blocking other workers | Concurrent claim | Background | — |
| D-O6 | **Full replay**: rebuild D-S3 from all of D-S2 (regrade, config change, `PROJECTOR_VERSION` bump) | Sequential scan of one user's events | Rare | none (offline) |

Two properties of this workload decide everything below: **every hot-path row is
keyed by `user_id` with a small working set**, and **the only global data (D-S1) is
immutable and fits in cache**.

### 6.3 In-process data structures (Rust)

- **D1 — Curriculum arena.** Load D-S1 once into an immutable arena shared via
  `Arc`: topics in a `Vec`, external string ids interned to `u32` indices, the
  prerequisite DAG as compressed adjacency lists (CSR), topological order
  precomputed. The graph fits in L2 cache; scheduler traversals are index walks with
  no allocation. A graph database is explicitly the wrong tool for a static ~1,100-node
  graph.
- **D2 — Index/id boundary.** Arena indices are valid only within one curriculum
  build. Everything **persisted** (events, models, pool rows) uses external string
  ids; everything **in-flight** uses indices. Conversion happens at the storage
  boundary, nowhere else.
- **D3 — Learner model in memory.** A dense `Vec<Option<TopicState>>` parallel to
  the arena during a request; serialized to JSON keyed by string ids (D2). Due-review
  selection is a linear scan — at n ≈ 1,100 a scan is microseconds, and no heap or
  index structure is justified below n = 100,000 (decision rule, not taste).
- **D4 — Incremental fold.** D-S3 stores the `seq` cursor it was built from; a grade
  folds one event forward (D-O2). Full replay (D-O6) is reserved for regrades and
  version bumps, exactly as 1.0's projector does.
- **D5 — Anti-repeat set.** Per `(user, topic)`: a fixed-size ring of recent instance
  hashes plus a `HashSet` view, persisted inside the D-S6 state row.
- **D6 — Exact values.** Answers are arbitrary-precision rationals plus a small
  expression AST (§5). No floats in any equality decision.

### 6.4 Database decision

**PostgreSQL, one instance, and nothing else** — confirmed by the workload analysis
above, not only inherited from C3.

- **D7 — Why Postgres fits.** The workload is small-row OLTP keyed by tenant. Postgres
  covers every shape with a built-in: RLS for tenant isolation (C3); role grants with
  no UPDATE/DELETE for the append-only guarantee (C2); JSONB for event payloads and
  template documents; `SELECT … FOR UPDATE SKIP LOCKED` for the serving pool pop
  (D-O1) and all queue claims (D-O5); `LISTEN/NOTIFY` to push a finished async
  diagnosis (A4) to the waiting client; one transaction around append-event +
  fold-projection so D-S2 and D-S3 cannot diverge.
- **D8 — Rejected alternatives, so they are not re-litigated.**
  - *SQLite*: no roles, no RLS, single-writer — incompatible with C2's grant-enforced
    append-only rule and a stateless multi-process app tier.
  - *Redis or a message broker*: a second stateful system to operate and back up, for
    queues Postgres already serves at this scale with `SKIP LOCKED`; 1.0's
    `REDIS_URL` knob was never consumed.
  - *Document stores*: the grade path needs cross-table transactions and grant-level
    append-only enforcement.
  - *Graph databases*: D-S1 is static, versioned in git, and cached in memory (D1).
  - *Dedicated event-store products*: `events` + grants + the projector already give
    the guarantee; a new system adds operational surface, not safety.
- **D9 — Schema lineage.** The 2.0 schema starts from 1.0's `DATA_MODEL.md` §9 tables
  (users/auth, `events`, `learner_models`, per-user scratch, queues) and adds three:
  `content_store` (D-S4), `serving_pool` (D-S5), and `model_call_log` (T6). Queries go
  through `sqlx` with compile-time checking (R2).

## 7. Non-goals

- No chat tutor, no CLI (both removed in 1.0; not resurrected).
- No model call on the request path, ever — including generation (A6/A7). Generation
  itself is NOT a non-goal: A7 reserves the architecture for it as future work; only
  the *synchronous* form of it is permanently out.
- No generator implementation in the 2.0 launch scope (A7 is the seam, not the feature).
- No migration of 1.0's Python code; the port re-implements against the docs and the
  R5 oracle.
- No new pedagogy. 2.0 changes *how fast and how cheaply* the same pedagogy is served.

## 8. Open decisions (owner)

- **O1 — Event history.** Migrate 1.0's `events` rows into 2.0, or start learners
  fresh? (The 1.0 log includes regrade-superseded history; migration must preserve it.)
- **O2 — Diagnosis model + budget numbers.** T4's defaults (10 calls/session, 600
  output tokens) and the model choice need owner sign-off.
- **O3 — Frontend.** Keep the 1.0 SPA (`static/`, Tokyo Night design system) against
  the new API, or rewrite it in the same pass?
