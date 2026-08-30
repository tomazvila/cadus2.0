# M5 review, verification round (2026-08-30)

Workflow `wf_a9c52bf3-35f`, one round after the fix wave (`docs/reviews/M5-review-1.md`), same six lenses and refuters. Raised 19, confirmed 13, ten distinct defects. Reviewed tree: `main` at `d944d10` (1,572 tests).

The twelve defects of round 1 and 2 hold at the route: no reviewer re-opened one. Three of the new defects come from the fix wave (V1/V8, V3/V9 from FIX-M5-H; V2 from FIX-M5-G). Seven are pre-existing and were reached through the flagged items list.

## Rulings and fix units (wave 2)

### FIX2-M5-A (V1/V8, V3/V9)

- **V1** [major] `crates/web/src/serve.rs:630` (L1, L4, L5, D4, D-S2) — The serve route appends `task_served` and never advances the fold cursor, so every following request re-reads the whole log

- **V8** [major] `crates/web/src/serve.rs:630` (L1, L4, L5, D-O1, D4) — The serve appends `task_served` and never folds it, so the next learner request re-reads and re-folds the whole event log (F15/F18 re-opened)

- **V3** [major] `crates/web/src/session.rs:820` (C1, R5, D-S2) — `GET /api/session/plan` drops the drill the learner is working on, because it composes without `forget_own_drills`

- **V9** [major] `crates/web/src/session.rs:820` (D-S6, C1, R5) — A drill in progress disappears from GET /api/session/plan and GET /api/status while POST /serve keeps serving it


**Ruling:** Fix. The serve calls `project_and_save` after `record_first_serve` inside the same transaction, so `through_seq` stands at the log head after every serve; `bench_long_log.rs` `serve_once` drives the real first serve and pins the cursor at the head afterwards (the old assertion "a serve appends nothing" is removed). The drill cadence repair moves out of `serve::open` into one shared helper (`session::view_for_open_session` or equivalent) that reads the open session's `task_served` rows and is called by `serve::open`, `GET /api/session/plan` and `GET /api/status`, so the three routes compose from one view. A test drives serve of drill question 1, then pins that the plan lists the drill and `drill_due` is true.

### FIX2-M5-B (V2)

- **V2** [major] `crates/web/src/grade.rs:434` (R5) — The repeat-fail peel-back can never fire: `lesson_failed` scans only the open-session window for the prior failure


**Ruling:** Fix. The repeat test of `lesson_failed` reads history, not the session window: the session view (migration 0010 document) gains a `lesson_failures` map `topic -> [kp]` folded from every `lesson_result` with `failed_at_kp`, full replay on `regraded`; `grade.rs` reads `repeat` from that map. A test seeds a failed `lesson_result` in an EARLIER session and pins `REMEDIATION_REPEAT_FAIL` on the second failure; mutation: drop the map read, confirm red.

### FIX2-M5-D (V7)

- **V7** [major] `crates/worker/src/lib.rs:261` (A4, D-O5, R4) — run_with skips the whole diagnosis pass whenever the refill job is absent


**Ruling:** Fix. The worker tick runs the diagnosis pass when `refill` is `None`: step 4 runs only if a refill job exists, step 5 runs whenever a diagnosis job exists. A test runs `run_with(None, Some(diagnosis))` for two ticks against a queued job on the fake server and pins one ledger row.

### FIX2-M5-E (V10, V11, V12)

- **V10** [major] `crates/store/tests/bench_grade_transaction.rs:500` (L2, D-O2, U12, C2) — Benchmark B (grade) restores a cursor the log does not hold, so no sample folds the graded attempt

- **V11** [major] `crates/store/tests/pool.rs:147` (D7, D-O1) — The D7 SKIP LOCKED acceptance test asserts a premise the pop cannot hold, and fails 5 runs in 6 under load

- **V12** [major] `crates/web/tests/skeleton.rs:373` (U1) — A metric-shape test hides a 5 ms latency budget, so the U1 histogram check fails on a loaded runner


**Ruling:** Fix (tests only). `bench_grade_transaction.rs` takes its snapshot BEFORE the first warm-up grade and restores both the log and the model row from it, so every sample folds the appended attempt (pin `folded_through == head` after a sample). `pool.rs` `two_concurrent_pops_never_return_the_same_row` accepts `claimed: None` at the tail and asserts the D7 property (no row served twice) on the rows that were served, plus at least 200 - 8 serves. `skeleton.rs` histogram test asserts the literal count `1` on the `+Inf` bucket and the count series, and asserts monotone non-decreasing counts across the bounds.

### FIX2-M5-C (V4, V5, V6/V13)

- **V4** [major] `crates/web/src/auth/oauth_routes.rs:195` (auth) — The axum Path rejection answers plain text, so six routes break the section 2 error envelope

- **V5** [major] `crates/web/src/auth/routes.rs:428` (auth) — The email field of the four public credential routes has no length cap, so a 3 KB address answers 500 from the rate counter

- **V6** [major] `crates/web/src/auth/oauth.rs:380` (auth) — safe_next keeps byte 0x7f, so the OAuth callback answers 500 after committing the session and dropping both Set-Cookie headers

- **V13** [major] `crates/web/src/auth/oauth.rs:380` (auth, U5) — safe_next accepts the DEL byte, so an OAuth login writes the account, link and session and then answers 500 with no cookie


**Ruling:** Fix (phase 2, after A, B, D, E merge). A crate extractor `ApiPath<T>` (`crates/web/src/path.rs`) maps the axum path rejection to `422 invalid_request` in the envelope; the seven handlers take it. The email field of signup, login, forgot and resend is capped at 254 bytes AFTER normalization; over the cap answers `422 invalid_request` before any counter or lookup, with the same reply for a known and an unknown address. `safe_next` accepts a byte only in `0x21..=0x7e` and never a backslash; the test enumerates every byte 0x00..=0xff once and pins the accept set against `HeaderValue::from_str`.


## Flagged items with no finding

The reviewers raised no finding on: the double session lookup of the tenant layer (a cost, not a defect); the client-facing `index` restart after an enroll; `JobPayload.topic`; the uncalled `with_open_multistep_components` (the R6 stable-component branch stays a documented gap for M6, `docs/plans/M6.md`).

## Not fixed

None.
