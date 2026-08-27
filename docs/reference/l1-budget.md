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
  whole loop under a fixed literal; and two pinned literals of the drawn
  sequence (the count of anti-repeat hits and the last digest of the ring).
- **The allocation bound.** A counting global allocator counts every `alloc`,
  `alloc_zeroed`, and `realloc` of the measuring thread. The bound catches the
  regression a timing bound on a shared runner never catches: a `format!` in a
  hot loop.
- **L2 half.** The second test runs `answer::check` over the 3,492 answers of
  the 1.0 corpus and asserts the p95 of one check.

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
- **Fixture.** One seeded user, one knowledge point, 200 unclaimed pool rows
  with distinct `created_at` values, and a ring of 20 digests that overlaps the
  three oldest pool rows. The pop therefore skips three rows on every sample, so
  the measured transaction carries the skip loop.
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

---

## 8. The measured numbers

Build box, 2026-08-27, release profile, one test thread. The numbers are one
run, not a promise; the artifacts of each CI run carry the trend.

| Benchmark | Segment | p50 | p95 | p99 | max | Budget |
|---|---|---|---|---|---|---|
| A | instantiate, evaluate, canonicalize, hash, ring | 3,760 ns | 8,450 ns | 10,230 ns | 24,809 ns | 5 ms |
| A | `answer::check`, 3,492 corpus answers | 460 ns | 1,560 ns | 2,160 ns | 14,090 ns | 5 ms |
| B | serve transaction, 500 samples | 1,589,154 ns | 1,889,478 ns | 4,859,173 ns | 4,893,662 ns | 100 ms |

Benchmark A allocates 107,581 times for 2,000 iterations, which is 53 per
instance. The bound is 110,000. The count is the same number in the debug
profile and in the release profile.

The instantiation p95 is 592 times under its segment. The check p95 is 3,205
times under its segment. The serve transaction p95 is 53 times under its
segment. 1.0 measured the same instantiation path in Python at a p95 of
0.171 ms (spec section 1), so the Rust path is about 20 times faster than the
path the A1 claim rests on.
