//! Benchmark A, part 1: the instantiation segment of the L1 budget.
//!
//! Requirements: L1 (serve a problem, p95 < 150 ms), L2 (grade a verifiable
//! answer, p95 < 300 ms), T1 (no model token on either path), D6 (no float in a
//! value or an equality decision).
//!
//! `docs/reference/l1-budget.md` holds the split this file measures. Three
//! segments of that table live here:
//!
//! | Segment | Budget | Measured by |
//! |---|---|---|
//! | Arena traversal and scheduler decision | 20 ms | [`benchmark_a_arena_holds_the_l1_segment`] |
//! | Template render + answer evaluation + canonicalization + hash + ring | 5 ms | [`benchmark_a_instantiation_holds_the_l1_segment`] |
//! | `answer::check` of one learner answer | 5 ms | [`benchmark_a_check_holds_the_l2_segment`] |
//!
//! M5 U12 added the arena row, which M4 left unmeasured, and moved its segment
//! from 5 ms to 20 ms in the same commit (section 2 of that document).
//!
//! The rest of L1 is Benchmark B (`crates/store/tests/bench_serve_roundtrip.rs`)
//! and the documented headroom. The grade half of the L2 CPU segment is
//! `crates/web/tests/bench_grade_cpu.rs`: the work quality tier lives in
//! `cadus-web`, and the pure core never links it.
//!
//! # Why a plain test and not a criterion harness
//!
//! Spec section 10.1 asks for one number the gate reads. Criterion reports a
//! distribution for a human and needs a second crate in the dependency tree of
//! the pure core (R3). A plain `#[test]` gives the gate the one number, keeps
//! the core dependency list unchanged, and runs on the same toolchain as every
//! other test.
//!
//! # Determinism
//!
//! The draw takes [`BENCH_SEED`] and nothing else, so the 2,000 iterations are
//! one fixed sequence of instances. [`the_measured_sequence_is_pinned`] holds
//! that sequence to literals:
//!
//! - the fixture, the rendered text, the answer, and the digest of three
//!   iterations, one per named fixture ([`PINNED_INSTANCES`]);
//! - one digest over all 2,000 iterations ([`SEQUENCE_DIGEST`]), which moves on
//!   a change to the draw, the renderer, the evaluator, or the hash of ANY of
//!   the 20 fixtures.
//!
//! [`RING_TAIL_AFTER_THE_RUN`] and [`BLOCKED_IN_THE_RUN`] pin the last draw of
//! the measured loop and the count of anti-repeat hits. They read the ring the
//! measured loop itself fills, so they tie that loop to the pinned sequence.
//! They are not the sequence guard: the tail is one digest of 2,000, and the
//! blocked count compares digests to digests, so both hold under a renderer
//! change on the other 19 fixtures (M4 review 2, finding 6).
//!
//! # How to run it
//!
//! `scripts/bench.sh` runs this file in the release profile. Without
//! `CADUS_BENCH` in the environment every timing test prints one skip line and
//! returns, so `cargo test --workspace` never runs a benchmark beside the test
//! suite (spec section 10.5).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::time::Instant;

use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_core::template::rng_from_seed;
use common::bench::{
    BENCH_SEED, BENCH_VAR, BLOCKED_IN_THE_RUN, ITERATIONS, Percentiles, RING_TAIL_AFTER_THE_RUN,
    SEQUENCE_DIGEST, TEMPLATE_COUNT, benchmarks_are_on, budget, compile_all, fixtures, profile,
    replay, sequence_digest, write_artifact,
};

// ---------------------------------------------------------------------------
// The pinned literals of the benchmark
// ---------------------------------------------------------------------------

/// The p95 budget of one instantiation, in nanoseconds: 5 ms of the 150 ms of
/// L1 (`docs/reference/l1-budget.md`).
const INSTANTIATE_P95_BUDGET_NS: u128 = 5_000_000;

/// The allocation bound of the measured loop (spec section 10.1).
///
/// The counting allocator counts one for every `alloc`, `alloc_zeroed`, and
/// `realloc` of the measuring thread while the loop runs. The measured run
/// allocates 107,680 times for 2,000 instantiations, which is 53.84 per
/// instance:
/// the drawn `BigRational` values, the rendered statement, the evaluated tree,
/// the canonical form, the digest, and the two anti-repeat views. The count is
/// the same number in the debug profile and in the release profile, so this
/// bound binds in both.
///
/// The bound is the measured count plus 0.5 percent, rounded down:
/// floor(107,680 x 1.005) = 108,218. Per iteration the bound is 54.109
/// allocations against the measured 53.84, so the headroom is 538 allocations
/// over the loop, which is 0.269 per iteration. One added heap allocation per
/// instance therefore moves the loop to 109,680 and fails this assertion. That
/// is the regression spec section 10.1 asks this bound to catch and a timing
/// bound on a shared runner never catches. The old bound of 110,000 held 1.2
/// allocations of headroom per iteration and passed the same mutation (M4
/// review 1, finding 21).
///
/// # How to re-pin this literal
///
/// A change that alters the count on purpose moves this literal in the same
/// commit. Follow these steps:
///
/// 1. Run the measurement command:
///
/// ```sh
/// CADUS_BENCH=1 cargo test --release -p cadus-core --test bench_l1 -- \
///     --test-threads=1 --nocapture benchmark_a_instantiation
/// ```
///
/// 2. Read `n` from the printed `<n> allocations` field.
/// 3. Set this literal to `floor(n * 1005 / 1000)`, the rule of the M4 review 1
///    ruling on finding 21.
/// 4. Record `n`, `n / 2000`, and the new bound in the paragraph above and in
///    `docs/reference/l1-budget.md` section 8.
///
/// The measurement behind the literal below is 107,680, taken on the M4 review 2
/// tree (commit cd59434) in the release profile and in the debug profile, five
/// runs, one test thread. M4 review 2 findings 7 and 11 are the record of what a
/// stale measurement costs.
///
/// NOTE: FIXM4d changes the gate and the template source in the same fix wave.
/// If the merged tree prints a different count, repeat the four steps above once
/// after the merge, and re-run [`the_measured_sequence_is_pinned`] as well: a
/// change that moves the drawn tuples moves those literals too.
const ALLOCATION_BOUND: u64 = 108_218;

// ---------------------------------------------------------------------------
// The counting allocator
// ---------------------------------------------------------------------------

thread_local! {
    /// Whether the current thread counts its allocations.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    /// The allocations of the current thread since the last reset.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Count one allocation of the current thread.
///
/// The counter is thread-local, so a second test in this binary never adds to
/// the count of the measured loop. `try_with` keeps the allocator total: a
/// thread whose local storage is gone counts nothing instead of panicking.
fn bump() {
    let _ = COUNTING.try_with(|on| {
        if on.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get().saturating_add(1)));
        }
    });
}

/// The system allocator with a thread-local counter in front of it.
struct Counting;

// SAFETY: every method forwards to the system allocator with the same
// arguments and returns its pointer unchanged. `bump` touches a `Cell<u64>` in
// thread-local storage and allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        bump();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        bump();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Run `body` with the allocation counter on, and return its allocations.
fn count_allocations<T>(body: impl FnOnce() -> T) -> (T, u64) {
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|on| on.set(true));
    let out = body();
    COUNTING.with(|on| on.set(false));
    let total = ALLOCATIONS.with(Cell::get);
    (out, total)
}

// ---------------------------------------------------------------------------
// Benchmark A, part 1 — instantiate, evaluate, canonicalize, hash, ring
// ---------------------------------------------------------------------------

/// Benchmark A: 2,000 instantiations hold the 5 ms segment of L1.
///
/// One iteration is the whole core-only half of a serve: draw a satisfying
/// tuple, render the statement, evaluate the answer expression on exact
/// rationals, canonicalize the answer, hash the statement, and ask the D5
/// anti-repeat view about the digest. The assertion is on the p95 of the
/// per-iteration time and on the allocation count of the whole loop.
#[test]
fn benchmark_a_instantiation_holds_the_l1_segment() {
    if !benchmarks_are_on() {
        println!("SKIPPED benchmark A (instantiation): {BENCH_VAR} is not set");
        return;
    }
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), TEMPLATE_COUNT);
    let compiled = compile_all(&fixtures);

    let mut rng = rng_from_seed(BENCH_SEED);
    let mut ring = Ring::new();
    let mut task = TaskMemory::new();
    let mut samples: Vec<u128> = Vec::with_capacity(ITERATIONS);
    let mut blocked = 0_usize;
    let mut answers = 0_usize;

    let ((), allocations) = count_allocations(|| {
        for index in 0..ITERATIONS {
            let start = Instant::now();
            let instance = match compiled[index % TEMPLATE_COUNT].draw(&mut rng) {
                Ok(instance) => instance,
                Err(err) => panic!("iteration {index} did not instantiate: {err}"),
            };
            let hit = {
                let avoid = Avoid::new(&ring, &task);
                avoid.blocks(&instance.instance_hash)
            };
            ring.push(&instance.instance_hash);
            task.push(&instance.instance_hash);
            let elapsed = start.elapsed().as_nanos();
            samples.push(elapsed);
            if hit {
                blocked += 1;
            }
            if !instance.answer.is_empty() {
                answers += 1;
            }
        }
    });

    let times = Percentiles::of(&samples);
    let budget = budget(INSTANTIATE_P95_BUDGET_NS);
    // Replay the same 2,000 draws outside the counter, and fold them into one
    // digest. `the_measured_sequence_is_pinned` holds that digest to a literal,
    // and the artifact below carries it, so a CI run records what the loop
    // rendered and not the timings alone (M4 review 2, finding 6).
    let replayed = replay();
    let replayed_digest = sequence_digest(&replayed);
    // Report first, then assert. A failed budget must still print the numbers
    // and leave the artifact behind, because the review cycle reads the trend
    // of the failing run as well as of the passing one (spec section 10.2).
    println!(
        "benchmark A instantiate ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns, \
         {} allocations, {} blocked, ring tail {:?}",
        profile(),
        times.p50,
        times.p95,
        times.p99,
        times.max,
        allocations,
        blocked,
        ring.hashes().last().map(String::as_str).unwrap_or_default(),
    );
    write_artifact(
        "benchmark-a.json",
        &format!(
            "{{\n  \"benchmark\": \"A\",\n  \"profile\": {:?},\n  \"templates\": {},\n  \
             \"iterations\": {},\n  \"seed\": {},\n  \"instantiate_ns\": {},\n  \
             \"allocations\": {{\"total\": {}, \"per_iteration\": {}, \"bound\": {}}},\n  \
             \"sequence_digest\": {:?},\n  \"p95_budget_ns\": {}\n}}\n",
            profile(),
            TEMPLATE_COUNT,
            ITERATIONS,
            BENCH_SEED,
            times.json(),
            allocations,
            allocations / ITERATIONS as u64,
            ALLOCATION_BOUND,
            replayed_digest,
            budget,
        ),
    );

    assert_eq!(samples.len(), ITERATIONS, "every iteration is measured");
    assert_eq!(answers, ITERATIONS, "every iteration produced an answer");
    assert_eq!(
        blocked, BLOCKED_IN_THE_RUN,
        "the anti-repeat view blocked a different count, so the sequence changed"
    );
    assert_eq!(
        ring.hashes().last().map(String::as_str),
        Some(RING_TAIL_AFTER_THE_RUN),
        "the last drawn instance changed"
    );
    // Tie the measured loop to the pinned sequence. `replay` draws the same
    // instances in the same order, and `the_measured_sequence_is_pinned` holds
    // that replay to literals, so this equality carries every one of those
    // literals onto the loop the numbers above measure. The replay runs after
    // the measured loop and outside the allocation counter, so it costs the
    // reported count nothing.
    assert_eq!(
        ring.hashes().last().map(String::as_str),
        replayed.last().map(|row| row.hash.as_str()),
        "the measured loop and the pinned replay walked different sequences"
    );
    assert_eq!(
        replayed_digest, SEQUENCE_DIGEST,
        "the measured loop rendered a sequence the pinned digest does not hold"
    );
    assert!(
        times.p95 < budget,
        "the p95 instantiation took {} ns, and the budget is {budget} ns",
        times.p95
    );
    assert!(
        allocations < ALLOCATION_BOUND,
        "the measured loop allocated {allocations} times, and the bound is {ALLOCATION_BOUND}"
    );
}
