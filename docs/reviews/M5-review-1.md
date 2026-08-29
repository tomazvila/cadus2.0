# M5 review, round 1 and round 2 (2026-08-29)

Workflow `wf_5ce9fa2b-e1e`: six lenses (grade correctness, tenant isolation, auth security, async diagnosis and budgets, state/serve, test quality), one independent refuter per finding, major and blocker only. Round 1: 18 raised, 13 confirmed. Round 2: 10 raised, 5 confirmed. Total 28 raised, 18 confirmed. The 18 confirmed findings hold 12 distinct defects; the duplicates are grouped below.

Reviewed tree: `main` at `1dd3c4f` (U1-U12, 1,534 tests).

## Rulings and fix units

### FIX-M5-A (F1, F12)

- **F1** [blocker] `crates/web/src/grade.rs:918` (C2, C4, D-O2, FR-14) — attempt_id is derived from the loss-tolerant D-S6 serve counter, so POST /api/enroll inside an open session makes every later attempt of a task collide and go unrecorded

- **F12** [major] `crates/web/tests/grade_route.rs:297` (C2, D-O2) — The `{task_id}-{n}` attempt_id rule is unpinned past n=1: a mutant that fixes the index at 1 keeps the whole M5 suite green and drops every attempt after the first


**Ruling:** Fix. `n` comes from the event log: the count of `attempt` events with prefix `{task_id}-` plus one, read inside the grade transaction, so a cleared scratch row cannot repeat an id. The `already_recorded` branch answers with the STORED event (`correct`, `work_quality`), as the spec says ("reply with the state read"). New tests chain serve -> answer -> serve -> answer and pin `-2`; an enroll between two answers pins `-2` too.

### FIX-M5-B (F2/F11, F10/F16)

- **F2** [major] `crates/web/src/serve.rs:525` (A3, D-M5-3) — install_next hard-codes solution_sketch: None, so no graded reply and no H3 rework reply ever carries the worked solution

- **F11** [major] `crates/web/src/serve.rs:525` (A4, A6, D-S6) — `solution_sketch` is never populated, so the A4 worked solution never reaches the learner

- **F10** [major] `crates/web/src/serve.rs:520` (D-O3, D-S6, L5, A4) — A component review question reads the hint ladder and the diagnosis of the wrong knowledge point

- **F16** [major] `crates/web/src/serve.rs:520` (D-O3, A4, L5, C6) — A component-interleaved review serves the parent topic's hint ladder and the parent topic's pre-authored diagnosis


**Ruling:** Fix. `ServedProblem` gains `solution_sketch` (from the pool row) and `serve_topic` (the `Target.serve` topic). The hint ladder and the diagnosis lookup build the serving key from `serve_topic`; the attempt still records against `topic`. A graded reply and an H3 rework reply carry the sketch.

### FIX-M5-C (F3, F4, F7, F9)

- **F3** [blocker] `crates/web/src/lib.rs:218` (C3, A3, A4) — No layer ever inserts `Tenant`, so every learner route answers 401 with a valid session

- **F4** [blocker] `crates/web/src/lib.rs:218` (C3, A3, A4, auth) — No layer inserts `Tenant`, so every learner route answers 401 to a valid session

- **F7** [blocker] `crates/web/src/lib.rs:180` (A4, C3, D-M5-1, auth) — No layer ever inserts `Tenant`, so every session-required route — the A4 poll and the A4 stream included — answers 401 to a valid session

- **F9** [blocker] `crates/web/src/lib.rs:218` (C3, D-S6, D-O1, D-O3) — No layer puts `Tenant` into the request extensions, so every learner route answers 401


**Ruling:** Fix (blocker). A new layer `auth::layer::tenant_layer` runs `current_user`; on success it inserts `Tenant(user_id)` and `Authed` into the request extensions, on a missing or invalid credential it passes the request through with no `Tenant`, so the extractor answers 401 as before. The layer sits inside the CSRF layer. Route tests drive the layer with a real session cookie; the hand-inserted `Tenant` in tests is removed.

### FIX-M5-D (F5, F14)

- **F5** [major] `crates/web/src/auth/oauth.rs:365` (auth) — OAuth `next` open redirect: `safe_next` passes a tab, and browsers strip it back to `//host`

- **F14** [major] `crates/web/src/auth/oauth_routes.rs:301` (auth) — The OAuth merge activates a password nobody proved, so a pre-registered address becomes an account takeover


**Ruling:** Fix. `safe_next` accepts only a path that starts with `/`, whose second byte is not `/` or `\\`, and that holds no byte below 0x21 and no `\\`. A provider link that resolves to an existing account whose email was not verified clears `password_hash`, deletes every session of that account, and then stamps `email_verified_at`; the link to a verified account keeps its password.

### FIX-M5-E (F6)

- **F6** [major] `crates/web/src/auth/routes.rs:416` (auth) — The body-limit rejection on every `/api/auth/*` POST breaks the section 2 error envelope


**Ruling:** Fix. The ten `/api/auth/*` handlers take a `LimitedBody` extractor (own module `auth::body`) that maps the axum rejection to the envelope: `413 payload_too_large` over the limit, `422 invalid_request` for a body that cannot be read.

### FIX-M5-F (F8)

- **F8** [blocker] `docker-compose.yml:205` (A4, T2, T4, T5) — docker-compose.yml forwards none of the seven diagnosis variables to the worker, so A4 calls no model in the shipped stack


**Ruling:** Fix. `docker-compose.yml` forwards `OPENAI_API_KEY`, `OPENAI_BASE_URL`, `OPENAI_MODEL`, `OPENROUTER_PROVIDER_ORDER`, `DIAGNOSIS_OUTPUT_TOKENS`, `DIAGNOSIS_REASONING_MAX_TOKENS`, `DIAGNOSIS_CALLS_PER_SESSION` to the worker (`${VAR:-}` form). `scripts/check_ops.sh` gains an invariant that `docker compose config` shows the seven keys on the worker service.

### FIX-M5-T (F13)

- **F13** [major] `crates/web/src/grade.rs:802` (D-O2, W7) — No test answers a quiz over HTTP: the quiz branch of the one-transaction grade path is dead in the suite, and a mutant that removes it leaves the suite green


**Ruling:** Fix. New test file `crates/web/tests/quiz_route.rs` answers a quiz over HTTP: the bare receipt, no verdict before the batch reveal, the reveal after the last answer. Mutation: remove the quiz branch, confirm red.

### FIX-M5-G (F15, F18)

- **F15** [major] `crates/store/src/state.rs:176` (L1, L2, L4, L5) — Every learner route reads and deserializes the learner's whole event log, so L1/L4/L5 fail on a log the benchmarks never build

- **F18** [major] `crates/web/src/serve.rs:392` (L1, L4, L5, D-O1, D-O3, D4) — Serve, teach and hint read and fold the learner's whole event log twice per request, breaking L1/L4/L5 on a normal-length log


**Ruling:** Fix (phase 2). One event-log read per request. The whole-log maps (`enrollment_stack`, `learned_at`, `last_drill_at`, `closed_task_ids`, `active_study_days`, `current_session`) move into a cached `session_view` document beside the learner model, folded incrementally from `through_seq` with a full replay on `regraded`. `load_events` gains `load_events_after(seq)`. A benchmark seeds 20,000 events and pins serve p95 < 100 ms and grade p95 < 150 ms.

### FIX-M5-H (F17)

- **F17** [major] `crates/web/src/session.rs:973` (C1, R5, D-S2) — No route ever appends `task_served`, so the 3.5-day drill cadence never suppresses a drill


**Ruling:** Fix (phase 2, after G). The serve route appends `task_served` the first time a task is served in a session (idempotent: no second row for one task id), so `last_drill_at` fills and the 3.5-day cadence holds. Ruling D-M5-8: 2.0 appends at first serve, 1.0 appended at plan composition; `GET /api/session/plan` stays pure (trap W3).


## Not fixed

None. Every confirmed finding maps to a fix unit.

## Refuted (10 of 28)

The refuters' records are in the workflow journal `subagents/workflows/wf_5ce9fa2b-e1e/journal.jsonl`.
