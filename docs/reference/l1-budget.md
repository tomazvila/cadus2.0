# The L1 and L2 budget split

Requirements: L1 (serve a problem, p95 < 150 ms), L2 (grade a verifiable answer,
p95 < 300 ms), L3-L5 (the prose, teach and hint lines), L6 (no model call a
learner waits on), T1 (zero model tokens on both paths), D-O1 (the serve
transaction), D-O2 (the grade transaction).

Source: `docs/reference/serving-1.0-spec.md` section 10 and
`docs/reference/web-service-1.0-spec.md` section 8. This document is the split
both sections ask for. Every benchmark cites this file, and this file names
every benchmark.

M5 U12 added section 2.1 (the per-route table), the arena row of section 2, the
grade rows of section 3, and the L6 boundary test of section 5.

FIX-M5-G added section 6.3, the long-log rows of section 8, and the D-S2 rule
that section 6.3 states: a benchmark that seeds a short log measures a new
account and not a learner (M5 review 1, findings F15 and F18).

---

## 1. Why a split and not one number

L1 is a wall-clock budget of a whole HTTP request. A GitHub `ubuntu-latest`
runner is a shared machine, so one wall-clock p95 on it is a flaky gate.
HANDOVER.md section 3 makes a broken budget a merge block, so a flaky gate
blocks good work.

The fix is two benchmarks and one written split:

- **Benchmark A** measures the pure CPU work. It is deterministic, it needs no
  server, and it is a hard gate.
- **Benchmark B** measures the database round trip against a throwaway
  Postgres. It is a p95 gate and a recorded artifact.
- The rest of the budget is named headroom. Nothing measures it today.

A change that needs more than its own segment moves the split here, in a
reviewed commit. That rule is the mechanism that stops "fix it later".

---

## 2. The L1 split (serve a problem, 150 ms)

| Segment | Budget | Measured by |
|---|---|---|
| Arena traversal and scheduler decision (in memory, D1/D3) | 20 ms | Benchmark A, `benchmark_a_arena_holds_the_l1_segment` |
| Template render, answer evaluation, canonicalization, hash, ring | 5 ms | Benchmark A, `benchmark_a_instantiation_holds_the_l1_segment` |
| Postgres: model read, pool pop, claim, state write, commit | 100 ms | Benchmark B, `benchmark_b_serve_round_trip_holds_the_l1_segment` |
| axum, tokio, serde, session lookup, RLS `SET LOCAL` | 25 ms | unmeasured headroom |
| **Total** | **150 ms (L1)** | |

**M5 U12 moved the first row from 5 ms to 20 ms.** M4 wrote 5 ms into it and
measured nothing (the old section 5). The first measurement reads a p95 of
3.24 ms for one `compose_session` over the committed curriculum tree, which is
1.5 times under 5 ms: that is a flaky gate on a shared runner and not a budget.
The 15 ms comes out of the framework row, which is still headroom, so the total
is still 150 ms. Section 1 names this as the mechanism, and this paragraph is the
reviewed commit it asks for.

## 2.1 Route to L\* line (spec section 8)

Every route of the M5 service, its budget, and the split of that budget. The
style follows `serving-1.0-spec.md:540-550`, and
`docs/reference/web-service-1.0-spec.md` section 8 is the source.

| Route | Line | Budget | Split |
|---|---|---|---|
| POST `/api/task/{id}/serve` | **L1** | 150 ms | arena+scheduler 20 ms · render/eval/hash/ring 5 ms · Postgres (model read, pool pop, claim, state write, commit) 100 ms · framework+session+RLS 25 ms |
| POST `/api/task/{id}/answer` | **L2** (verdict) / **L3** (prose) | 300 ms | `check`+tier+hash 5 ms · Postgres (one INSERT, two UPDATEs, one pool pop, one job INSERT) 150 ms · framework+session+RLS 40 ms · 105 ms headroom |
| POST `/api/task/{id}/teach` | **L4** | 150 ms | one `content_store` read by `(kp_id,'teach','approved')` 20 ms · in-memory digest cache hit ~0 · framework 40 ms · 90 ms headroom |
| POST `/api/task/{id}/hint` | **L5** | 150 ms | one `content_store` read (`hint_ladder`) 20 ms · state write 60 ms · framework 40 ms · 30 ms headroom |
| POST `/api/diag/answer` | **L2** | 300 ms | the same split as answer, minus the pool pop |
| GET `/api/diagnosis/{id}` | — | 100 ms | one indexed row read; it is a poll, so it must be cheap |
| GET `/api/diagnosis/stream` | — | n/a | long-lived; the budget is the *notify-to-flush* delay, target < 250 ms |
| GET `/api/status`, `/api/session/plan` | — | 300 ms | one model read + scheduler compose; not an L\* line, but the dashboard is on the critical path of every session start |
| GET `/api/graph` | — | 500 ms | about 1,100 nodes, layout precomputed server-side |
| POST `/api/session/start`, `/end` | — | 300 ms | one event append + one fold |
| GET `/api/export` | — | streamed | a full per-user scan; never in a p95 |
| GET `/api/operator/flags` | — | not on a learner path | one `operator_flags` read + at most 20 gate runs, about 10 ms of CPU each; admin only (A6) |
| **Every route above** | **L6** | **0 model calls** | asserted by `crates/web/tests/purity.rs`; section 5 |

**T1 as a merge gate:** serve, teach, hint and the whole grade path spend **0
model tokens**. The only spenders are the worker's diagnosis job and the offline
authoring pipeline (T2).

`/api/operator/flags` is the one route with no time budget. It serves an admin
account, no learner waits on it, and one request runs the template gate up to 20
times on purpose. Its cost is bounded by a count and not by a clock:
`cadus_web::operator::GATE_NOTE_LIMIT`.

## 3. The L2 split (grade a verifiable answer, 300 ms)

| Segment | Budget | Measured by |
|---|---|---|
| `answer::check` of one learner answer | 5 ms | Benchmark A, `benchmark_a_check_holds_the_l2_segment` |
| The D5 hash, `check`, and the D-M5-2 work quality tier, as the handler runs them | 5 ms | Benchmark A, `benchmark_a_grade_cpu_holds_the_l2_segment` |
| Postgres: append one event, fold it into the model, write the job row, pop the next problem, update the state row | 150 ms | Benchmark B, `benchmark_b_grade_transaction_holds_the_l2_segment` |
| axum, tokio, serde, session lookup, RLS `SET LOCAL` | 140 ms | unmeasured headroom |
| **Total** | **300 ms (L2)** | |

The two 5 ms rows measure the same checker from two sides. The first reads
`cadus_core::answer::check` alone, and it is the M4 row. The second reads
`cadus_web::grade::deterministic_grade`, the function the route calls: the same
`check`, plus the tier and the error tags of D-M5-2, plus the
`problem_text_hash` of the served statement. They share one 5 ms line, because
one request runs the second one and never both.

The verdict never waits on a model (T1, L6). The diagnosis prose of A4 arrives
asynchronously and is outside this budget.

---

## 4. Benchmark A — CPU only, deterministic, hard gate

Files: `crates/core/tests/bench_l1.rs` (three parts) and
`crates/web/tests/bench_grade_cpu.rs` (one part). Both are plain `#[test]`
files: spec section 10.1 asks for one number the gate reads, and criterion needs
a second crate in the dependency tree of the pure core (R3).

### 4.1 The instantiation half (L1) and the check half (L2)

File: `crates/core/tests/bench_l1.rs`.

- **Fixture.** `crates/core/tests/fixtures/templates/`, 20 committed template
  documents. They span the shapes: one parameter, three parameters, a choice
  domain, a choice value with a backslash (`\times`), a constrained pair, a
  rational parameter, and the `expression` answer kind. A test gates all 20 with
  `cadus_core::template::gate`, so the benchmark measures documents the pipeline
  would approve and serve (C6).
- **Work per iteration.** Draw a satisfying tuple, render the statement,
  evaluate `answer_expr` on exact rationals, canonicalize the answer, compute
  `problem_text_hash`, and ask the D5 anti-repeat view about the digest.
- **Sample.** 2,000 iterations, one fixed seed, 20 templates in file order.
- **Assertions.** p95 of one iteration under 5 ms; the allocation count of the
  whole loop under a fixed literal; the count of anti-repeat hits and the last
  digest of the ring; and the pinned sequence of the next bullet.
- **The pinned sequence.** `the_measured_sequence_is_pinned` replays the 2,000
  draws outside the allocation counter and holds them to literals: the fixture,
  the rendered text, the answer, and the digest of iteration 0, iteration 9, and
  iteration 19 (the first draw of fixture 1, of fixture 10, and of fixture 20),
  and one `problem_text_hash` over all 2,000 iterations. The digest over all
  2,000 reads every one of the 20 fixtures, so a change to the draw order, to
  the renderer, to the evaluator, or to the hash moves it. The test needs no
  `CADUS_BENCH`, so `cargo test --workspace` runs it too. The benchmark itself
  ties its measured loop to the replay by the last digest, asserts the same
  sequence digest, and writes it to `benchmark-a.json`, so a CI artifact records
  what the loop rendered and not the timings alone. Before M4 review 2
  (finding 6) the file pinned the last draw and a count of set hits alone: the
  FIXM4a bracket rule changed 183 of the 2,000 statements and moved neither
  literal, so a C4 regression on 19 of the 20 fixtures passed this gate.
- **The allocation bound.** A counting global allocator counts every `alloc`,
  `alloc_zeroed`, and `realloc` of the measuring thread. The bound catches the
  regression a timing bound on a shared runner never catches: a `format!` in a
  hot loop. The bound is the measured count plus 0.5 percent, rounded down, so
  the headroom is 0.269 allocations per iteration and ONE added allocation per
  served instance fails the assertion. The old bound of 2.2 percent held 1.2
  allocations of headroom per iteration and passed that mutation (M4 review 1,
  finding 21). Section 8 holds the measured count and the rule that re-pins it.
- **L2 half.** The second test runs `answer::check` over the 3,492 answers of
  the 1.0 corpus and asserts the p95 of one check. The learner side of each pair
  is a re-spelling of the authored answer, not the authored answer itself: a
  numeric answer takes `+0`, and an expression answer goes inside parentheses
  and takes `*1`. A self-check returns at rung 2, the string-key rung, so it
  measures `normalize` and a string compare and never the parser or the exact
  canonicalizer (M4 review 1, finding 20).
- **The two guards of the L2 half.** The test counts, outside the timed loop,
  the pairs whose two normalized string keys differ (3,492 of 3,492: no measured
  call returns at rung 2) and the pairs whose two sides both reach a canonical
  form (2,985: the calls that run the exact arithmetic). Both counts are pinned
  literals. A p50 floor of 1,000 ns is the backstop: the same corpus measured a
  p50 of 460 ns while the learner side was the authored answer.

### 4.2 The arena half (L1) — new in M5 U12

Test: `benchmark_a_arena_holds_the_l1_segment`, in the same file.

- **What it measures.** One `cadus_core::selector::compose_session` over the
  COMMITTED curriculum tree: the course scope, the mastered set, the frontier,
  the due reviews, the compression, the lesson order, and the interleave. That
  is the whole in-memory half of a plan, and it is the one function
  `cadus_web::session::compose_plan` calls.
- **Fixture.** Every third topic of the arena carries a learned state, and every
  fifth of those is due at the fixed clock. The rule is a stride and not a draw,
  so the states map is a function of the committed tree alone: 1,090 topics, 364
  learned, 116 composed tasks. All three counts are pinned literals, and so are
  the first and the last `task_id` of the plan.
- **Sample.** 200 compositions, one fixed sampler seed per iteration, so all 200
  compose the same plan. The test asserts that too: a loop that composed 200
  different decisions would report one percentile over 200 different shapes.
- **Why a fifth and not all of them.** A fixture in which every learned topic is
  due measures 11.2 ms on this box, because the compression walks 364 topics.
  That is the worst case of a learner who let a course lapse for months, not the
  day. The fixture models a learner mid-course, and section 8 records both
  numbers.

### 4.3 The grade half (L2) — new in M5 U12

File: `crates/web/tests/bench_grade_cpu.rs`.

- **What it measures.** The two CPU steps of one submission, in the order the
  handler runs them: `cadus_core::learner::problem_text_hash` of the served
  statement, then `cadus_web::grade::deterministic_grade`, which is `check` plus
  the D-M5-2 tier plus the error tags. The measured function is the PRODUCTION
  function: a tier rule that grows a `format!` moves this p95 and not a copy of
  it.
- **Fixture.** The same 1.0 answer corpus, 3,492 rows. The learner side is built
  from each row by a stride, so all three tiers are measured: every tenth row is
  blank (`poor`, `blank-answer`), the next one is a wrong value
  (`nearly_passable`), and the other eight are the re-spelling
  (`nearly_perfect`). The three input counts and the three verdict counts are
  pinned literals, so a tier rule that reclassifies one row fails the test.
- **The guard.** The same p50 floor of 1,000 ns as the check half. A p50 under it
  says the loop short-circuited before the canonicalizer.

## 5. The L6 crate boundary — 0 model calls on any route

L6 is the one budget with no milliseconds in it: no model call may sit on a path
a learner waits on. Spec section 8 asks for it as a crate boundary, so it is a
failed test and not a review note.

File: `crates/web/tests/purity.rs`. Four guards, and M5 U12 added the fourth:

1. `web_normal_closure_carries_no_http_client_or_model_sdk` — no HTTP client and
   no model SDK anywhere in the resolved normal dependency closure of
   `cadus-web`. `cadus-model-client` is on that list.
2. The two direct-list tests pin the DIRECT normal dependencies of `cadus-web`
   and of `cadus-store`, so a new name in either manifest is a reviewable diff.
3. `web_and_worker_sources_hold_no_socket_or_process_call` reads the SOURCE of
   both tiers: `std::net` and `std::process` are dependencies of nothing.
4. `no_request_path_crate_links_the_model_client` walks the closures of BOTH
   `cadus-web` and `cadus-store` for `cadus-model-client`, and
   `no_request_path_manifest_names_the_model_client` reads the two manifests
   under every dependency kind and every target.

**The positive control.** `the_l6_walk_finds_the_model_client_in_the_worker`
runs the same walk over `cadus-worker`, which declares `cadus-model-client`, and
requires the answer `true`. Without it a broken walk — a lost edge, an empty
resolve, a renamed package — would make guard 4 pass while proving nothing. One
crate must hold the client, and two must not.

`crates/worker/tests/purity.rs` holds the mirror of guards 1 to 3 for the worker
side. The worker is the tier that MAY call a model, so its list allows
`cadus-model-client` and refuses every other client.

---

## 6. Benchmark B — the store round trips

Two files, one per transaction: `crates/store/tests/bench_serve_roundtrip.rs`
(the D-O1 serve transaction, L1) and
`crates/store/tests/bench_grade_transaction.rs` (the D-O2 grade transaction,
L2). Both run against a throwaway Postgres, and both measure the `cadus_store`
functions the handlers call.

### 6.1 The serve transaction (L1)

File: `crates/store/tests/bench_serve_roundtrip.rs`.

- **What it measures.** The D-O1 transaction of spec section 7.2, with the
  tenant bound and row-level security on (C3): bind the tenant, read the learner
  model row, pop at most 8 pool rows with `FOR UPDATE SKIP LOCKED`, reject the
  digests the D5 ring holds, claim the survivor, write the D-S6 state document,
  and commit.
- **The measured code is the production code.** The pop is
  `cadus_store::pool::pop_with_ring_tx`, the function the M5 serve path calls,
  and the seed is `cadus_store::pool::insert_batch`, the function the D-O4
  worker calls. An earlier version carried its own SELECT over its own fixture
  documents; those documents did not decode through the production reader, and
  the inline SELECT diverged from the pop in its ORDER BY and in its claim
  (M4 review 1, finding 11). The benchmark now asserts the decoded statement and
  the decoded expected answer of every sample.
- **Fixture.** One seeded user, one knowledge point, and 200 unclaimed pool rows
  written by two `insert_batch` calls. One call is one statement, so 100 rows
  share one `created_at`: the pop reads the two batches in age order and breaks
  the tie inside a batch by `id`, which is the sort every production pop
  performs. The fixture reads the first three digests in that same
  `created_at, id` order and puts them into a ring of 20, so the pop skips three
  rows on every sample and the measured transaction carries the skip loop.
- **Sample.** 50 untimed warm-ups, then 500 timed transactions on one
  connection, with no concurrency.
- **Gate policy.** The build fails at p95 above 100 ms. It never fails on p50
  and never fails on one slow sample.
- **Artifact.** Every run writes `benchmark-b.json` next to the benchmark A
  files. CI uploads the directory on a green run and on a red run.

### 6.2 The grade transaction (L2) — new in M5 U12

File: `crates/store/tests/bench_grade_transaction.rs`.

- **What it measures.** The D-O2 transaction of spec section 4.3, with the
  tenant bound and row-level security on (C3): take the tenant's advisory lock,
  read the log, fold it, read the D-S6 document, append ONE attempt event, write
  the `learner_models` row, insert ONE `diagnosis_jobs` row, pop and claim the
  next problem, write the state row, and commit.
- **The measured code is the production code.** Every statement runs through the
  function `cadus_web::grade::answer` calls: `lock_web_state`, `load_events`,
  `project_current`, `load_web_state`, `append_event`, `project_and_save`,
  `diagnosis::enqueue`, `pool::pop_with_ring_tx`, and `save_web_state`. A
  benchmark with its own SQL measures its own SQL (M4 review 1, finding 11).
- **Fixture.** One seeded user, one `session_start` and 200 attempt events, the
  committed curriculum tree for the fold, 64 unclaimed pool rows, and the D-S6
  row. The log length is part of what the number describes, so it is a named
  constant.
- **The fixture stays stationary.** One sample appends one event, writes one job
  row, and claims one pool row. Left alone, sample 200 would fold a log 200
  events longer than sample 1. After each timed sample the harness restores the
  fixture with the admin pool, OUTSIDE the measured window: it deletes the event
  and the job row, releases the pool row, and rewrites `learner_models` to the
  snapshot it took before the run. Three pinned literals hold that: every sample
  folds the same event count, appends at the same `seq`, and takes the
  incremental branch and not the full replay.
- **Sample.** 20 untimed warm-ups, then 200 timed transactions on one
  connection, with no concurrency.
- **Gate policy.** The build fails at p95 above 150 ms, and never on p50 or on
  one slow sample.
- **Artifact.** `benchmark-b-grade.json`.

### 6.3 The lifetime log (L1 and L2) — new in FIX-M5-G

File: `crates/store/tests/bench_long_log.rs`.

- **Why it exists.** Sections 6.1 and 6.2 seed 0 and 201 events. D-S2
  (`REQUIREMENTS.md:158`) says the event log grows forever and nothing deletes a
  row from it, so those two numbers describe a NEW ACCOUNT and not a learner.
  M5 review 1 finding F15 and finding F18 both come from that gap: every learner
  route read the whole log and folded it, twice on the serve path and three
  times on the grade path, and the cost grew with the lifetime event count.
- **Fixture.** 20,000 events: 200 sessions of 100 events each. Every session but
  the last one is closed. The last one is OPEN and carries 100 events, so the
  CURRENT session is short while the account is long. That is the shape D-S2
  produces, and it is the shape a per-request whole-log read fails on.
- **What it measures.** Two transactions, both built out of the production store
  functions the handlers call. The serve half is `cadus_web::serve::open` plus
  the pool pop, the `task_served` append of a task served for the FIRST time,
  the fold and save that append needs, and the state write; the grade half is the
  same shape with the attempt append, and it folds and saves on every sample.
- **The serve half has two shapes** (M5 review 2, findings V1 and V8). The first
  serve of a task appends the cadence line and folds the whole log ONCE, the
  term section 8 names on the grade row. The 19 hand-offs after it append
  nothing, fold nothing, and read the open session's window alone. The run times
  the one first serve on its own and prints it, and the p50 and p95 of the table
  describe the repeated hand-off.
- **Gate policy.** The file carries two gates. The COUNTING gate runs always: it
  holds the literal row count the open read decodes (101: the open session's 100
  events and the one cadence line), the literal `seq` of that cadence line, and
  the literal cursor the fold reaches, so a whole-log read that comes back fails
  it, and no clock enters the assertion. The TIMING gate runs under
  `CADUS_BENCH`, like every other benchmark of this document, and fails at p95
  above 100 ms (serve) and 150 ms (grade). A debug build holds a budget ten
  times wider. The one first serve carries NO timing gate: see the open question
  in section 8.
- **Artifacts.** `benchmark-b-long-log-serve.json` and
  `benchmark-b-long-log-grade.json`.

**The fix this benchmark gates.** `learner_models.session_view` (migration 0010)
caches the whole-log maps beside the model — the enrollment stack, `learned_at`,
`last_drill_at`, the closed task ids, the active study days, and the open
session — and folds forward from the same `through_seq` cursor the model uses.
`cadus_store::state::load_events_after` is the range read that makes the forward
fold cheap. A request that appends nothing therefore reads NO event row beyond
the one cursor line, and the grade path reads the whole log ONCE instead of
three times.

---

## 7. How to run the benchmarks

```sh
CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate \
    scripts/bench.sh
```

`scripts/gate.sh` runs the same script after `cargo test`. The two never run
together: parallel suites contend on one box, and a contended benchmark measures
the scheduler (spec section 10.5). CI runs that one gate script in ONE job, so
the same order holds there, and `scripts/check_ops.sh` check (i) fails a gate
script that runs the benchmarks first and a workflow that gives them a job of
their own (M5 U12).

Without `CADUS_BENCH` in the environment every timing test prints one skip line
and returns, so `cargo test --workspace` stays a test run. `scripts/bench.sh`
sets that variable, and it runs every benchmark in the release profile, because
the numbers below are release numbers. The order is fixed: benchmark A (core),
benchmark A (grade CPU), the M2 release budgets, benchmark B (serve), benchmark
B (grade). Two transaction benchmarks never run together, for the reason two
suites never do. A debug run holds a budget ten times
wider, which is the rule `crates/core/tests/answer_check.rs` already carries for
L2.

`CADUS_BENCH_DIR` moves the artifact directory. The default is `target/bench`.

`scripts/bench.sh` runs `crates/store/tests/bench_long_log.rs` after benchmark
B (grade), so the TIMING gate of section 6.3 (serve p95 < 100 ms, grade p95
< 150 ms on a 20,000-event log) is part of every gate run. The COUNTING gate of
section 6.3 runs inside `cargo test --workspace`.

`scripts/bench.sh` runs one more step between the two benchmarks:
`CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test answer_check`.
That file holds the M2 budgets of the checker — 5 ms per check and 1 s per
corpus pass in a release build, ten times wider without the variable. Before
M4 review 1 (finding 20) no script set the variable, so the release budgets of
the checker ran nowhere.

---

## 8. The measured numbers

Build box, 2026-08-29, release profile, one test thread, one `scripts/gate.sh`
run. The numbers are one run, not a promise; the artifacts of each CI run carry
the trend. The two serve rows of the 20,000-event fixture come from a
`scripts/gate.sh` run of 2026-08-30 on the same box, after the M5 review 2 fix
wave gave the serve its append (findings V1 and V8). That box carried three
other builds during the run, so both rows read higher than the quiet numbers the
bullets below name.

| Benchmark | Segment | p50 | p95 | p99 | max | Budget |
|---|---|---|---|---|---|---|
| A | instantiate, evaluate, canonicalize, hash, ring | 3,900 ns | 8,330 ns | 10,940 ns | 31,439 ns | 5 ms |
| A | `answer::check`, 3,492 re-spelled corpus pairs | 2,870 ns | 24,090 ns | 50,809 ns | 209,775 ns | 5 ms |
| A | hash + `deterministic_grade`, 3,492 corpus rows | 3,320 ns | 24,300 ns | 51,559 ns | 221,215 ns | 5 ms |
| A | `compose_session` over 1,090 topics, 200 samples | 3,225,251 ns | 3,303,850 ns | 3,395,587 ns | 4,799,996 ns | 20 ms |
| B | serve transaction, 500 samples | 1,775,822 ns | 1,928,299 ns | 3,523,444 ns | 5,085,911 ns | 100 ms |
| B | grade transaction, 200 samples | 6,721,186 ns | 7,587,387 ns | 10,069,463 ns | 10,216,820 ns | 150 ms |
| B | serve transaction, 20,000-event log, 100 samples | 4,246,350 ns | 8,087,918 ns | 10,316,572 ns | 10,857,669 ns | 100 ms |
| B | FIRST serve of a task, 20,000-event log, 1 sample | 167,190,613 ns | — | — | — | none yet |
| B | grade transaction, 20,000-event log, 100 samples | 99,521,747 ns | 112,881,445 ns | 119,705,726 ns | 119,715,831 ns | 150 ms |

The four M5 U12 measurements read this way:

- The grade CPU costs what the checker costs. The tier and the hash add about
  400 ns at the p50 and nothing the p95 can see: `deterministic_grade` is one
  `check` and one small match.
- The arena row is the tight one, at 6 times under its 20 ms segment. Every other
  row is 19 to 600 times under. Section 2 records why the segment moved.
- The same fixture with EVERY learned topic due — 364 topics into the
  compression instead of 73 — measured a p95 of 11,185,977 ns, which is 3.4
  times the number in the table and still under 20 ms. That is the worst case,
  and it is why the segment is 20 ms and not 10.
- The grade transaction folds a log of 201 events per sample. It is 19 times
  under its 150 ms segment.

The two long-log rows are the FIX-M5-G measurement, on the same box and in the
same profile. They read this way:

- **Before the fix**, the same two transactions over the same 20,000-event
  fixture measured a serve p95 of 389,378,016 ns and a grade p95 of
  639,033,961 ns. Both passed their whole L\* budget on the log read alone, and
  the serve number is 3.9 times its 100 ms segment.
- **After the fix**, the serve p95 is 8,087,918 ns, which is 12 times under its
  segment and 48 times faster than before. The serve transaction reads no event
  row beyond the cursor line and the open session's own 101 events, so the number
  no longer grows with the log. That row is the repeated hand-off: the second to
  twentieth question of a task, and every re-serve of a live problem. Two runs of
  2026-08-30 read a p95 of 2,886,748 ns and 8,087,918 ns, and the table records
  the WORSE of the two; the higher run shared this box with three other builds.
- **The FIRST serve of a task is a different transaction**, and the M5 review 2
  fix wave made it so (findings V1 and V8). It appends the `task_served` line of
  the drill cadence, so it folds and saves in the same transaction and the fold
  cursor stays on the head of the log. That fold takes the incremental branch of
  `project_current`, which reads the whole log ONCE — the same read plus fold the
  grade row pays. The two runs read 113,995,523 ns and 167,190,613 ns, and the
  table again records the worse one. It is 20 times the cost of the hand-off
  beside it, and it runs once per task and per session, which is once per 20
  questions.
- **The first serve carries no timing gate, and that is an OPEN QUESTION.** The
  number stands 1.67 times OVER the 100 ms Postgres segment of section 2 (1.14
  times on the quiet run), so a learner 20,000 events deep passes the whole
  150 ms L1 budget on the first hand-off of each task. The two ways out both
  need code no M5 fix unit owns: a `cadus_store::state` entry point that SAVES
  the projection a caller
  already folded, instead of re-reading and re-folding inside
  `project_and_save`; or the cached light-index document inside the projector
  crate that the grade row below also asks for. Either one removes the whole-log
  term from both rows. Until then the benchmark prints the number, the artifact
  records it, and no assertion holds it: no code in this tree holds that budget
  today, and a budget the tree fails is not a gate. The old serve row asserted
  "a serve appends nothing"; that assertion is gone, because the serve appends.
- The grade p95 is 112,881,445 ns, which is 1.33 times under its 150 ms segment.
  Two runs on this box read 103,942,116 ns and 112,881,445 ns, and the table
  records the WORSE of the two. It is the TIGHTEST row of this table, and the
  reason is named:
  `cadus_core::projector::project_incremental` seeds the FIRe states from the
  cache but replays the EARLIER events for their light indices, so the append
  path still reads and folds the whole log ONCE. The read is about 77 ms of the
  104 ms and the fold is about 25 ms. Removing the last whole-log term needs a
  cached light-index document inside the projector crate, which FIX-M5-G does
  not own. Until then this row is the one to watch on a slower runner.
- The grade row and the first-serve row grow with the log, so both are a function
  of the 20,000 events the fixture seeds. A learner ten times deeper would move
  those two rows and not the serve hand-off row.

Benchmark A allocates 107,680 times for 2,000 iterations, which is 53.84 per
instance. The bound is `ALLOCATION_BOUND = 108_218`, the measured count plus 0.5
percent, rounded down: floor(107,680 x 1.005). The headroom is 538 allocations
over the loop, which is 0.269 per iteration. The count is the same number in the
debug profile and in the release profile, and five runs on this box gave the same
number, so it is a deterministic literal, unlike the timings in the table above.

The count was measured on 2026-08-27 on the merged M4 tree (commit cd59434).
Before that measurement both this document and `crates/core/tests/bench_l1.rs`
recorded 107,581, the count of the tree before FIXM4a, and the bound 108,118 was
measured plus 0.407 percent (M4 review 2, findings 7 and 11).

**To re-pin the allocation count, do these steps:**

1. Run the measurement:

   ```sh
   CADUS_BENCH=1 cargo test --release -p cadus-core --test bench_l1 -- \
       --test-threads=1 --nocapture benchmark_a_instantiation
   ```

2. Read `n` from the printed `<n> allocations` field.
3. Set `ALLOCATION_BOUND` in `crates/core/tests/bench_l1.rs` to
   `floor(n * 1005 / 1000)`.
4. Record `n`, `n / 2000`, and the new bound in this section and in the docstring
   of `ALLOCATION_BOUND`.

NOTE: the FIXM4d fix unit changes the gate and the template source in the same
fix wave as this measurement. If the merged tree prints a different count, repeat
the four steps once after the merge. If the merged tree draws different tuples,
re-pin the literals of `the_measured_sequence_is_pinned` in the same commit; the
test prints its current values before it asserts.

The L2 row of the table moved with the fix of finding 20. The same corpus,
measured with the authored answer on both sides, gave a p50 of 460 ns and a p95
of 1,560 ns; those numbers were the cost of `normalize` and a string compare.
The re-spelled pairs cost 6.5 times the p50 and 16 times the p95, and 2,985 of
the 3,492 calls reach the exact canonicalizer.

The instantiation p95 is 600 times under its segment. The check p95 and the
grade-CPU p95 are 207 and 205 times under theirs. The serve transaction p95 is 51
times under its segment, the grade transaction p95 is 19 times under its, and the
plan composition p95 is 6 times under its. 1.0 measured the same instantiation
path in Python at a p95 of 0.171 ms (spec section 1), so the Rust path is about
20 times faster than the path the A1 claim rests on.
