# The L1 and L2 budget split

Requirements: L1 (serve a problem, p95 < 150 ms), L2 (grade a verifiable answer,
p95 < 300 ms), T1 (zero model tokens on both paths), D-O1 (the serve
transaction), D-O2 (the grade transaction).

Source: `docs/reference/serving-1.0-spec.md` section 10. This document is the
split that section asks for. Both benchmarks cite this file, and this file names
both benchmarks.

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
| Arena traversal and scheduler decision (in memory, D1/D3) | 5 ms | Benchmark A (M3 selector; see section 5) |
| Template render, answer evaluation, canonicalization, hash, ring | 5 ms | Benchmark A, `benchmark_a_instantiation_holds_the_l1_segment` |
| Postgres: model read, pool pop, claim, state write, commit | 100 ms | Benchmark B, `benchmark_b_serve_round_trip_holds_the_l1_segment` |
| axum, tokio, serde, session lookup, RLS `SET LOCAL` | 40 ms | unmeasured headroom |
| **Total** | **150 ms (L1)** | |

## 3. The L2 split (grade a verifiable answer, 300 ms)

| Segment | Budget | Measured by |
|---|---|---|
| `answer::check` of one learner answer | 5 ms | Benchmark A, `benchmark_a_check_holds_the_l2_segment` |
| Postgres: append one event, fold it into the model, update the state row | 150 ms | Benchmark B shape (M5 wires the grade transaction) |
| axum, tokio, serde, session lookup, RLS `SET LOCAL` | 145 ms | unmeasured headroom |
| **Total** | **300 ms (L2)** | |

The verdict never waits on a model (T1, L6). The diagnosis prose of A4 arrives
asynchronously and is outside this budget.

---

## 4. Benchmark A — core only, deterministic, hard gate

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

## 5. What benchmark A does not yet measure

The first row of the L1 table, the arena traversal and the scheduler decision,
has its own 5 ms segment and no benchmark in M4. M3 asserts the selector against
the 1.0 oracle for correctness, not for time. M5 adds the row to this harness
when it wires the serve handler. The segment stays in the table so the sum
stays honest.

## 6. Benchmark B — the store round trip

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

---

## 7. How to run the benchmarks

```sh
CADUS_TEST_DATABASE_URL=postgresql://test:test@127.0.0.1:55434/cadus2_gate \
    scripts/bench.sh
```

`scripts/gate.sh` runs the same script after `cargo test`. The two never run
together: parallel suites contend on one box, and a contended benchmark measures
the scheduler (spec section 10.5).

Without `CADUS_BENCH` in the environment every timing test prints one skip line
and returns, so `cargo test --workspace` stays a test run. `scripts/bench.sh`
sets that variable, and it runs both benchmarks in the release profile, because
the numbers below are release numbers. A debug run holds a budget ten times
wider, which is the rule `crates/core/tests/answer_check.rs` already carries for
L2.

`CADUS_BENCH_DIR` moves the artifact directory. The default is `target/bench`.

`scripts/bench.sh` runs one more step between the two benchmarks:
`CADUS_RELEASE_BENCH=1 cargo test --release -p cadus-core --test answer_check`.
That file holds the M2 budgets of the checker — 5 ms per check and 1 s per
corpus pass in a release build, ten times wider without the variable. Before
M4 review 1 (finding 20) no script set the variable, so the release budgets of
the checker ran nowhere.

---

## 8. The measured numbers

Build box, 2026-08-27, release profile, one test thread. The numbers are one
run, not a promise; the artifacts of each CI run carry the trend.

| Benchmark | Segment | p50 | p95 | p99 | max | Budget |
|---|---|---|---|---|---|---|
| A | instantiate, evaluate, canonicalize, hash, ring | 3,590 ns | 8,340 ns | 8,810 ns | 13,320 ns | 5 ms |
| A | `answer::check`, 3,492 re-spelled corpus pairs | 3,010 ns | 24,969 ns | 52,219 ns | 220,505 ns | 5 ms |
| B | serve transaction, 500 samples | 1,790,191 ns | 2,051,004 ns | 8,755,455 ns | 18,645,335 ns | 100 ms |

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

The instantiation p95 is 599 times under its segment. The check p95 is 200 times
under its segment. The serve transaction p95 is 48 times under its segment. 1.0
measured the same instantiation path in Python at a p95 of 0.171 ms (spec
section 1), so the Rust path is about 20 times faster than the path the A1 claim
rests on.
