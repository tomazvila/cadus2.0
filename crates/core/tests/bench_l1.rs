//! Benchmark A: the core-only segment of the L1 and L2 budgets.
//!
//! Requirements: L1 (serve a problem, p95 < 150 ms), L2 (grade a verifiable
//! answer, p95 < 300 ms), T1 (no model token on either path), D6 (no float in a
//! value or an equality decision).
//!
//! `docs/reference/l1-budget.md` holds the split this file measures. Two
//! segments of that table live here:
//!
//! | Segment | Budget | Measured by |
//! |---|---|---|
//! | Template render + answer evaluation + canonicalization + hash + ring | 5 ms | [`benchmark_a_instantiation_holds_the_l1_segment`] |
//! | `answer::check` of one learner answer | 5 ms | [`benchmark_a_check_holds_the_l2_segment`] |
//!
//! The rest of L1 is Benchmark B (`crates/store/tests/bench_serve_roundtrip.rs`)
//! and the documented headroom.
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
//! one fixed sequence of instances. [`RING_TAIL_AFTER_THE_RUN`] and
//! [`BLOCKED_IN_THE_RUN`] pin that sequence: a change to the draw, the renderer,
//! the evaluator, or the hash moves one of the two literals.
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

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Instant;

use cadus_core::answer::{Outcome, check};
use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_core::template::{
    Compiled, GateSpec, TemplateDoc, from_body, gate, rng_from_seed, to_body,
};

// ---------------------------------------------------------------------------
// The pinned literals of the benchmark
// ---------------------------------------------------------------------------

/// The count of committed fixture templates (spec section 10.1).
const TEMPLATE_COUNT: usize = 20;

/// The count of measured instantiations (spec section 10.1).
const ITERATIONS: usize = 2_000;

/// The seed of every draw of the run. The benchmark is a function of this
/// number and the committed fixtures alone.
const BENCH_SEED: u64 = 20_260_827;

/// The p95 budget of one instantiation, in nanoseconds: 5 ms of the 150 ms of
/// L1 (`docs/reference/l1-budget.md`).
const INSTANTIATE_P95_BUDGET_NS: u128 = 5_000_000;

/// The p95 budget of one `check`, in nanoseconds: 5 ms of the 300 ms of L2.
const CHECK_P95_BUDGET_NS: u128 = 5_000_000;

/// The debug-profile multiplier of both time budgets.
///
/// The exact arithmetic of the checker runs about ten times slower without
/// optimization, and `crates/core/tests/answer_check.rs` already carries the
/// same ten-times rule for the L2 budget. The gate runs this file in the
/// release profile, so the 5 ms literals above are the numbers that bind.
const DEBUG_SLOWDOWN: u128 = 10;

/// The allocation bound of the measured loop (spec section 10.1).
///
/// The counting allocator counts one for every `alloc`, `alloc_zeroed`, and
/// `realloc` of the measuring thread while the loop runs. The measured run
/// allocates 107,581 times for 2,000 instantiations, which is 53 per instance:
/// the drawn `BigRational` values, the rendered statement, the evaluated tree,
/// the canonical form, the digest, and the two anti-repeat views. The count is
/// the same number in the debug profile and in the release profile, so this
/// bound binds in both.
///
/// The bound is 110,000, which is 2.2 percent above the measured number. The
/// mutation check chose it: one `format!` around the rendered value in
/// `template::render` moves the count to 111,881 and fails this assertion,
/// while the p95 of the same mutated run stays at 8,230 ns, deep inside the
/// 5 ms budget. That is the regression spec section 10.1 asks this bound to
/// catch and a timing bound on a shared runner never catches.
const ALLOCATION_BOUND: u64 = 110_000;

/// The last digest of the ring after the measured loop.
///
/// The literal is the instance hash of the 2,000th draw. It pins the whole
/// sequence: the draw order, the rendered text, and `problem_text_hash`.
const RING_TAIL_AFTER_THE_RUN: &str = "c027d35bab27";

/// The count of iterations whose digest the anti-repeat view already held.
///
/// The 20 fixtures include small spaces (12 distinct instances for the perfect
/// squares and for the square roots), so a repeat inside the 20-digest ring and
/// the 12-digest task memory is expected, and the count is a fixed number of
/// this fixed sequence.
const BLOCKED_IN_THE_RUN: usize = 47;

/// The count of answers in the 1.0 corpus (`docs/plans/M2.md`).
const CORPUS_ANSWERS: usize = 3_492;

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
// The fixtures
// ---------------------------------------------------------------------------

/// The directory of the committed template fixtures.
fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates")
}

/// Read the committed templates in file-name order.
fn fixtures() -> Vec<(String, TemplateDoc)> {
    let dir = fixture_dir();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let body = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
            let doc = from_body(&body).unwrap_or_else(|err| panic!("{name} does not read: {err}"));
            (name, doc)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The percentile rule
// ---------------------------------------------------------------------------

/// The `percent` percentile of a sorted sample, by the nearest-rank rule.
///
/// The rank is `ceil(percent * n / 100)`, counted from one, and the function
/// reads the value at that rank. The arithmetic is integer arithmetic: no float
/// enters a reported number (D6).
fn percentile(sorted: &[u128], percent: u128) -> u128 {
    assert!(!sorted.is_empty(), "a percentile needs a sample");
    let count = sorted.len() as u128;
    let rank = (percent * count).div_ceil(100).max(1);
    let index = usize::try_from(rank - 1).unwrap_or(0);
    sorted[index.min(sorted.len() - 1)]
}

/// The p50, p95, p99, and maximum of a sample of nanosecond durations.
struct Percentiles {
    p50: u128,
    p95: u128,
    p99: u128,
    max: u128,
}

impl Percentiles {
    /// Read the percentiles of one sample. The function sorts its own copy.
    fn of(samples: &[u128]) -> Self {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        Self {
            p50: percentile(&sorted, 50),
            p95: percentile(&sorted, 95),
            p99: percentile(&sorted, 99),
            max: *sorted.last().unwrap(),
        }
    }

    /// The JSON body of the four numbers.
    fn json(&self) -> String {
        format!(
            "{{\"p50_ns\": {}, \"p95_ns\": {}, \"p99_ns\": {}, \"max_ns\": {}}}",
            self.p50, self.p95, self.p99, self.max
        )
    }
}

/// The time budget of this build profile.
///
/// A release build holds the literal of the budget table. A debug build holds
/// ten times that number, because the exact arithmetic runs about ten times
/// slower without optimization.
fn budget(release_ns: u128) -> u128 {
    if cfg!(debug_assertions) {
        release_ns * DEBUG_SLOWDOWN
    } else {
        release_ns
    }
}

/// The name of the profile, for the artifact.
fn profile() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    }
}

// ---------------------------------------------------------------------------
// The artifact
// ---------------------------------------------------------------------------

/// The environment variable that moves the artifact directory.
const ARTIFACT_DIR_VAR: &str = "CADUS_BENCH_DIR";

/// The environment variable that turns the benchmarks on.
const BENCH_VAR: &str = "CADUS_BENCH";

/// Whether this run asks for the benchmarks.
fn benchmarks_are_on() -> bool {
    std::env::var_os(BENCH_VAR).is_some()
}

/// Write one benchmark artifact and print its path.
fn write_artifact(name: &str, body: &str) {
    let dir = match std::env::var_os(ARTIFACT_DIR_VAR) {
        Some(value) => PathBuf::from(value),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/bench"),
    };
    std::fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join(name);
    std::fs::write(&path, body).unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
    println!("artifact: {}", path.display());
}

// ---------------------------------------------------------------------------
// The fixture is 20 approved templates
// ---------------------------------------------------------------------------

/// The fixture holds the 20 templates spec section 10.1 asks for, and every one
/// of them passes the U2 gate.
///
/// The benchmark is only honest if it measures documents the pipeline would
/// serve. A document that the gate refuses is never in `content_store` with
/// status `approved`, so it never reaches the worker or the serve path (C6).
#[test]
fn the_fixture_holds_twenty_gated_templates() {
    let fixtures = fixtures();
    assert_eq!(
        fixtures.len(),
        TEMPLATE_COUNT,
        "the benchmark fixture is 20 templates"
    );
    let no_exemplars: [Exemplar; 0] = [];
    for (name, doc) in &fixtures {
        let spec = GateSpec {
            answer_kind: doc.answer_kind,
            exemplars: &no_exemplars,
        };
        let verified = gate(doc, &spec)
            .unwrap_or_else(|rejection| panic!("{name} does not pass the gate: {rejection}"));
        assert!(
            verified.space.count() >= 12,
            "{name} has a space of {}, and the floor is 12",
            verified.space.count()
        );
    }
}

/// Every fixture body round-trips byte for byte through the document type.
///
/// A body that does not round-trip has a field the reader drops, and a dropped
/// field is a template the store and the benchmark disagree about.
#[test]
fn every_fixture_body_round_trips() {
    for (name, doc) in fixtures() {
        let written = to_body(&doc).unwrap_or_else(|err| panic!("{name} does not write: {err}"));
        let again =
            from_body(&written).unwrap_or_else(|err| panic!("{name} does not re-read: {err}"));
        assert_eq!(doc, again, "{name} does not round-trip");
    }
}

/// The fixture spans the shapes spec section 10.1 names.
#[test]
fn the_fixture_spans_the_named_shapes() {
    use cadus_core::template::Domain;

    let fixtures = fixtures();
    let mut one_param = 0;
    let mut three_params = 0;
    let mut choice = 0;
    let mut constrained = 0;
    let mut rational = 0;
    let mut expression = 0;
    for (_, doc) in &fixtures {
        match doc.params.len() {
            1 => one_param += 1,
            3 => three_params += 1,
            _ => {}
        }
        if doc
            .params
            .values()
            .any(|domain| matches!(domain, Domain::Choice { .. }))
        {
            choice += 1;
        }
        if doc
            .params
            .values()
            .any(|domain| matches!(domain, Domain::Rational { .. }))
        {
            rational += 1;
        }
        if !doc.constraints.is_empty() {
            constrained += 1;
        }
        if doc.answer_kind == AnswerKind::Expression {
            expression += 1;
        }
    }
    assert_eq!(one_param, 3, "three fixtures declare one parameter");
    assert_eq!(three_params, 4, "four fixtures declare three parameters");
    assert_eq!(choice, 3, "three fixtures declare a choice domain");
    assert_eq!(rational, 2, "two fixtures declare a rational domain");
    assert_eq!(constrained, 6, "six fixtures carry constraints");
    assert_eq!(expression, 2, "two fixtures answer an expression");
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
    let compiled: Vec<Compiled<'_>> = fixtures
        .iter()
        .map(|(name, doc)| {
            Compiled::new(doc).unwrap_or_else(|err| panic!("{name} does not compile: {err}"))
        })
        .collect();

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
             \"p95_budget_ns\": {}\n}}\n",
            profile(),
            TEMPLATE_COUNT,
            ITERATIONS,
            BENCH_SEED,
            times.json(),
            allocations,
            allocations / ITERATIONS as u64,
            ALLOCATION_BOUND,
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

// ---------------------------------------------------------------------------
// Benchmark A, part 2 — the M2 check on the corpus (L2)
// ---------------------------------------------------------------------------

/// One row of `crates/core/tests/fixtures/answers/corpus_1_0.jsonl`.
#[derive(serde::Deserialize)]
struct CorpusRow {
    answer: String,
    answer_kind: String,
}

/// Read the 1.0 answer corpus.
fn corpus() -> Vec<CorpusRow> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/answers/corpus_1_0.jsonl");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    text.lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|err| panic!("row {line}: {err}")))
        .collect()
}

/// The answer kind of one corpus row.
fn kind_of(row: &CorpusRow) -> AnswerKind {
    match row.answer_kind.as_str() {
        "numeric" => AnswerKind::Numeric,
        "expression" => AnswerKind::Expression,
        other => panic!("the corpus holds only verifiable kinds, and this row is {other}"),
    }
}

/// Benchmark A: `check` over the whole 1.0 corpus holds the 5 ms segment of L2.
///
/// The grade path of M5 runs one `check` per attempt (A3, D-O2). The corpus is
/// every authored answer of the 1.0 curriculum, so its p95 is the number the L2
/// budget table cites.
#[test]
fn benchmark_a_check_holds_the_l2_segment() {
    if !benchmarks_are_on() {
        println!("SKIPPED benchmark A (check): {BENCH_VAR} is not set");
        return;
    }
    let corpus = corpus();
    assert_eq!(corpus.len(), CORPUS_ANSWERS, "the corpus is 3,492 answers");
    let mut samples: Vec<u128> = Vec::with_capacity(corpus.len());
    let mut decided = 0_usize;
    for row in &corpus {
        let start = Instant::now();
        let outcome = check(&row.answer, &row.answer, kind_of(row));
        samples.push(start.elapsed().as_nanos());
        if matches!(outcome, Outcome::Decided(verdict) if verdict.correct) {
            decided += 1;
        }
    }
    assert_eq!(
        decided, CORPUS_ANSWERS,
        "every authored answer must equal itself"
    );

    let times = Percentiles::of(&samples);
    let budget = budget(CHECK_P95_BUDGET_NS);
    println!(
        "benchmark A check ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} answers",
        profile(),
        times.p50,
        times.p95,
        times.p99,
        times.max,
        corpus.len()
    );
    write_artifact(
        "benchmark-a-check.json",
        &format!(
            "{{\n  \"benchmark\": \"A-check\",\n  \"profile\": {:?},\n  \"answers\": {},\n  \
             \"check_ns\": {},\n  \"p95_budget_ns\": {}\n}}\n",
            profile(),
            corpus.len(),
            times.json(),
            budget,
        ),
    );

    assert!(
        times.p95 < budget,
        "the p95 check took {} ns, and the budget is {budget} ns",
        times.p95
    );
}
