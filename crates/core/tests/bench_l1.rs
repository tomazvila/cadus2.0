//! Benchmark A: the core-only segment of the L1 and L2 budgets.
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

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

use cadus_core::answer::{Outcome, canonical_form, check, normalize};
use cadus_core::config::Config;
use cadus_core::curriculum::{AnswerKind, Curriculum, Exemplar, load_curriculum};
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::{TopicState, problem_text_hash};
use cadus_core::pool::{Avoid, Ring, TaskMemory};
use cadus_core::selector::{SeededSampler, SessionContext, SessionPlan, compose_session};
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

/// The last digest of the ring after the measured loop.
///
/// The literal is the instance hash of the 2,000th draw alone. It pins the last
/// iteration and nothing else, and iteration 1,999 reads fixture 20, because
/// `1999 % TEMPLATE_COUNT == 19`. [`SEQUENCE_DIGEST`] is the guard over all
/// 2,000 iterations and all 20 fixtures (M4 review 2, finding 6).
const RING_TAIL_AFTER_THE_RUN: &str = "c027d35bab27";

/// The count of iterations whose digest the anti-repeat view already held.
///
/// The 20 fixtures include small spaces (12 distinct instances for the perfect
/// squares and for the square roots), so a repeat inside the 20-digest ring and
/// the 12-digest task memory is expected, and the count is a fixed number of
/// this fixed sequence.
const BLOCKED_IN_THE_RUN: usize = 47;

/// One pinned iteration of the measured sequence.
///
/// The four fields are the whole record of one draw: the fixture the loop read,
/// the statement the learner reads, the answer the pool row carries, and the
/// digest the D5 anti-repeat view holds.
struct PinnedInstance {
    /// The index of the iteration inside the 2,000-iteration loop.
    iteration: usize,
    /// The file name of the fixture the iteration draws from.
    fixture: &'static str,
    /// The rendered statement.
    text: &'static str,
    /// The expected answer, inside the M2 grammar.
    answer: &'static str,
    /// `problem_text_hash` of `text`.
    hash: &'static str,
}

/// Three pinned iterations: the first draw of fixture 1, of fixture 10, and of
/// fixture 20 (M4 review 2, ruling on findings 6, 7 and 11).
///
/// `index % TEMPLATE_COUNT` picks the fixture, so iteration 0 draws the first
/// fixture in file-name order, iteration 9 draws the tenth, and iteration 19
/// draws the twentieth. These three literals are the readable half of the
/// sequence guard: a reviewer reads the fixture, the statement, and the answer
/// and compares them by eye. [`SEQUENCE_DIGEST`] is the half that covers the
/// other 17 fixtures.
///
/// [`the_measured_sequence_is_pinned`] prints the current values before it
/// asserts, so a deliberate change re-pins these literals from the printed
/// lines.
const PINNED_INSTANCES: [PinnedInstance; 3] = [
    PinnedInstance {
        iteration: 0,
        fixture: "01-perfect-squares.json",
        text: "Compute $3^{2}$.",
        answer: "9",
        hash: "e61280d48db9",
    },
    PinnedInstance {
        iteration: 9,
        fixture: "10-distribute-over-a-sum.json",
        text: "Expand $5(x + 3)$.",
        answer: "15 + 5*x",
        hash: "54a94abb69bd",
    },
    PinnedInstance {
        iteration: 19,
        fixture: "20-two-digit-difference.json",
        text: "Compute $48 - 34$.",
        answer: "14",
        hash: "1287954c19c6",
    },
];

/// The digest of the whole measured sequence.
///
/// [`sequence_digest`] folds the fixture name, the rendered text, the answer,
/// and the instance hash of all 2,000 iterations into one `problem_text_hash`.
/// The literal therefore moves on a change to the draw order, to the renderer,
/// to the evaluator, or to the digest of ANY of the 20 fixtures.
///
/// This literal is the answer to M4 review 2 finding 6. Before it, the file
/// pinned the last draw and a count of set hits alone, and the FIXM4a bracket
/// rule changed 183 of the 2,000 statements (fixtures 07 and 08) while both
/// literals held byte for byte.
const SEQUENCE_DIGEST: &str = "c767c554c277";

/// The count of answers in the 1.0 corpus (`docs/plans/M2.md`).
const CORPUS_ANSWERS: usize = 3_492;

/// The count of corpus pairs whose two sides both reach a canonical form.
///
/// The L2 half compares each authored answer against a re-spelled equivalent
/// (see [`respell`]). A pair of this count runs the parser and the exact
/// canonicalizer on BOTH sides, so this literal is the count of measured calls
/// that reach the arithmetic the 5 ms segment pays for. It is the counter that
/// proves the string rung did not answer the run: 2,985 of the 3,492 calls take
/// the parse-and-canonicalize path, where the self-check of M4 review 1
/// finding 20 took it zero times.
///
/// The other 507 pairs split in two groups, and both of them still run the
/// parser:
///
/// - 265 authored answers sit outside the decidable grammar (`7 L/min`,
///   `18 degrees Celsius`, `x <= -1`), so the expected side alone refuses (V2);
/// - 242 authored answers are collections (`(6, 4)`, `[-3, 3]`,
///   `3/8, 1/2, 5/8`). The grammar reads the collection and then refuses the
///   arithmetic of the re-spelling with "arithmetic on a collection", so the
///   canonicalizer runs on the learner side and stops inside it.
const CANONICALIZED_PAIRS: usize = 2_985;

/// The count of re-spelled pairs the checker decides correct.
const RESPELLED_CORRECT: usize = 2_985;

/// The count of re-spelled pairs the checker decides wrong.
///
/// A wrong verdict here is a canonicalizer that does not see `(e)*1` as `e` or
/// `n+0` as `n`. The literal pins that count.
const RESPELLED_WRONG: usize = 0;

/// The count of re-spelled pairs the checker refuses (V2).
const RESPELLED_UNDECIDABLE: usize = 507;

/// The p50 floor of the L2 half, in nanoseconds.
///
/// The floor is the second guard against a silent return to the string rung.
/// Finding 20 of M4 review 1 measured the self-check at a p50 of 450 ns and the
/// re-spelled check at a p50 of 2,620 ns on this box. A p50 under this floor
/// means the loop stopped reaching the canonicalizer, so the reported number is
/// not the number of the grade path. The floor is deliberately far under the
/// measured p50: it catches a rung-2 short circuit, not a fast machine.
///
/// The count assertions above are the primary proof. This floor is the backstop
/// for a change that keeps the counts and drops the work.
const CHECK_P50_FLOOR_NS: u128 = 1_000;

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
// The pinned sequence
// ---------------------------------------------------------------------------

/// One replayed iteration of the measured sequence.
struct Replayed {
    /// The file name of the fixture the iteration drew from.
    fixture: String,
    /// The rendered statement.
    text: String,
    /// The expected answer.
    answer: String,
    /// `problem_text_hash` of `text`.
    hash: String,
}

/// Replay the measured sequence outside the allocation counter.
///
/// The replay draws the same 2,000 instances as
/// [`benchmark_a_instantiation_holds_the_l1_segment`]: the same seed, the same
/// fixtures, the same order, the same production `draw`. The measured loop keeps
/// no text and no digest of its own, because a kept `String` is an allocation
/// [`ALLOCATION_BOUND`] pays for; this function keeps all four fields of every
/// iteration and costs the measured numbers nothing.
///
/// The ring and the task memory of the measured loop read the digests and never
/// feed the draw, so the two loops walk one sequence. The benchmark asserts that
/// tie: its ring tail equals the last digest of this replay.
fn replay() -> Vec<Replayed> {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), TEMPLATE_COUNT);
    let compiled: Vec<Compiled<'_>> = fixtures
        .iter()
        .map(|(name, doc)| {
            Compiled::new(doc).unwrap_or_else(|err| panic!("{name} does not compile: {err}"))
        })
        .collect();

    let mut rng = rng_from_seed(BENCH_SEED);
    let mut run: Vec<Replayed> = Vec::with_capacity(ITERATIONS);
    for index in 0..ITERATIONS {
        let slot = index % TEMPLATE_COUNT;
        let instance = match compiled[slot].draw(&mut rng) {
            Ok(instance) => instance,
            Err(err) => panic!("iteration {index} did not instantiate: {err}"),
        };
        run.push(Replayed {
            fixture: fixtures[slot].0.clone(),
            text: instance.text,
            answer: instance.answer,
            hash: instance.instance_hash,
        });
    }
    run
}

/// The digest of a whole replayed run.
///
/// The preimage holds one line per iteration. One line is the fixture name, the
/// rendered text, the answer, and the instance hash, in that order, separated by
/// the unit separator `U+001F`. No field of the four carries that character or a
/// newline, so one preimage reads back as one sequence.
///
/// The function hashes the preimage with `problem_text_hash`, the production
/// digest of a problem text, so the fold needs no second hash definition
/// (trap T17).
fn sequence_digest(run: &[Replayed]) -> String {
    let mut preimage = String::new();
    for row in run {
        preimage.push_str(&row.fixture);
        preimage.push('\u{1f}');
        preimage.push_str(&row.text);
        preimage.push('\u{1f}');
        preimage.push_str(&row.answer);
        preimage.push('\u{1f}');
        preimage.push_str(&row.hash);
        preimage.push('\n');
    }
    problem_text_hash(&preimage)
}

/// The measured sequence is the pinned sequence.
///
/// This test is the determinism guard of benchmark A, and it runs without
/// `CADUS_BENCH`: a renderer, evaluator, draw, or digest regression fails the
/// ordinary `cargo test --workspace` run, not the benchmark alone.
///
/// It asserts two things:
///
/// - the fixture, the text, the answer, and the digest of three iterations, one
///   per fixture the M4 review 2 ruling names ([`PINNED_INSTANCES`]);
/// - one digest over all 2,000 iterations ([`SEQUENCE_DIGEST`]), which reads all
///   20 fixtures.
///
/// The test prints every pinned value before it asserts, so a deliberate change
/// re-pins the literals from the printed lines (`--nocapture` shows them).
#[test]
fn the_measured_sequence_is_pinned() {
    let run = replay();
    assert_eq!(run.len(), ITERATIONS, "every iteration is replayed");

    for pin in &PINNED_INSTANCES {
        let row = &run[pin.iteration];
        println!(
            "pinned iteration {}: fixture {:?} text {:?} answer {:?} hash {:?}",
            pin.iteration, row.fixture, row.text, row.answer, row.hash
        );
    }
    let digest = sequence_digest(&run);
    println!("sequence digest: {digest:?}");

    for pin in &PINNED_INSTANCES {
        let row = &run[pin.iteration];
        let at = pin.iteration;
        assert_eq!(
            row.fixture, pin.fixture,
            "iteration {at} reads a different fixture"
        );
        assert_eq!(
            row.text, pin.text,
            "iteration {at} renders a different statement"
        );
        assert_eq!(
            row.answer, pin.answer,
            "iteration {at} evaluates a different answer"
        );
        assert_eq!(row.hash, pin.hash, "iteration {at} hashes to a new digest");
        // The digest of a pool row is `problem_text_hash` of the statement and
        // of nothing else (D-S5). The pinned text and the pinned hash therefore
        // hold each other: a rewrite of one of the two alone fails here.
        assert_eq!(
            problem_text_hash(pin.text),
            pin.hash,
            "the pinned text of iteration {at} does not hash to the pinned digest"
        );
    }

    assert_eq!(
        digest, SEQUENCE_DIGEST,
        "the fixture, the text, the answer, or the digest of one of the 2,000 iterations moved"
    );
    assert_eq!(
        run[ITERATIONS - 1].hash,
        RING_TAIL_AFTER_THE_RUN,
        "the last replayed digest is the ring tail the benchmark pins"
    );
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

/// The re-spelling of one corpus answer: the same value, a different string.
///
/// `check` returns at rung 2 when the two normalized string keys agree, so a
/// self-check measures `normalize` and a string compare and never the parser or
/// the canonicalizer (M4 review 1, finding 20). The learner side of the L2 half
/// is therefore a re-spelling:
///
/// - a `numeric` answer takes `+0`;
/// - an `expression` answer goes inside parentheses and takes `*1`.
///
/// Both rewrites keep the value and change the string key, so every measured
/// call walks past rung 2 into the parser and the exact arithmetic. A learner
/// types the same kind of variant, so the measured cost is the cost of the M5
/// grade path.
fn respell(row: &CorpusRow) -> String {
    // Re-spell the reader source, not the authored text. The author wraps an
    // answer in `$...$`, and `normalize` strips that pair at the two ends only;
    // a `$` in the middle of `($3\pi$)*1` leaves the grammar. The source string
    // is what a learner types, so the rewrite starts from it.
    let source = normalize(&row.answer).source;
    match kind_of(row) {
        AnswerKind::Numeric => format!("{source}+0"),
        AnswerKind::Expression => format!("({source})*1"),
        // `kind_of` panics on every other kind, so this arm is unreachable. It
        // keeps the match exhaustive without a wildcard that hides a new kind.
        other => panic!("the corpus holds only verifiable kinds, and this row is {other:?}"),
    }
}

/// Benchmark A: `check` over the whole 1.0 corpus holds the 5 ms segment of L2.
///
/// The grade path of M5 runs one `check` per attempt (A3, D-O2). The corpus is
/// every authored answer of the 1.0 curriculum, and the learner side of each
/// pair is the [`respell`] variant of it, so the p95 this test reports is the
/// p95 of a check that runs the parser and the canonicalizer.
#[test]
fn benchmark_a_check_holds_the_l2_segment() {
    if !benchmarks_are_on() {
        println!("SKIPPED benchmark A (check): {BENCH_VAR} is not set");
        return;
    }
    let corpus = corpus();
    assert_eq!(corpus.len(), CORPUS_ANSWERS, "the corpus is 3,492 answers");

    // Build both sides before the loop. The `format!` of the re-spelling is
    // fixture work, and fixture work never enters a measured sample.
    let pairs: Vec<(&str, String, AnswerKind)> = corpus
        .iter()
        .map(|row| (row.answer.as_str(), respell(row), kind_of(row)))
        .collect();

    // Guard 1: no measured call stops at the string rung. Rung 2 returns when
    // the two normalized string keys agree, so every pair of this run carries
    // two different keys.
    let different_keys = pairs
        .iter()
        .filter(|(expected, learner, _)| {
            normalize(expected).string_key != normalize(learner).string_key
        })
        .count();
    assert_eq!(
        different_keys, CORPUS_ANSWERS,
        "a pair with two equal string keys returns at rung 2 and measures no arithmetic"
    );

    // Guard 2: the count of pairs whose two sides both reach a canonical form.
    // Those are the calls that run the exact arithmetic of the L2 budget. The
    // count runs outside the measured loop, so it costs the reported numbers
    // nothing.
    let canonicalized = pairs
        .iter()
        .filter(|(expected, learner, _)| {
            canonical_form(expected).is_ok() && canonical_form(learner).is_ok()
        })
        .count();
    assert_eq!(
        canonicalized, CANONICALIZED_PAIRS,
        "the count of pairs that reach the canonicalizer changed"
    );

    let mut samples: Vec<u128> = Vec::with_capacity(pairs.len());
    let mut correct = 0_usize;
    let mut wrong = 0_usize;
    let mut undecidable = 0_usize;
    for (expected, learner, kind) in &pairs {
        let start = Instant::now();
        let outcome = check(expected, learner, *kind);
        samples.push(start.elapsed().as_nanos());
        match outcome {
            Outcome::Decided(verdict) if verdict.correct => correct += 1,
            Outcome::Decided(_) => wrong += 1,
            Outcome::Undecidable(_) => undecidable += 1,
        }
    }

    let times = Percentiles::of(&samples);
    let budget = budget(CHECK_P95_BUDGET_NS);
    // Report first, then assert, for the same reason the instantiation half
    // reports first: the review cycle reads a failing run as well.
    println!(
        "benchmark A check ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} answers \
         ({} correct, {} wrong, {} undecidable, {} canonicalized)",
        profile(),
        times.p50,
        times.p95,
        times.p99,
        times.max,
        pairs.len(),
        correct,
        wrong,
        undecidable,
        canonicalized,
    );
    write_artifact(
        "benchmark-a-check.json",
        &format!(
            "{{\n  \"benchmark\": \"A-check\",\n  \"profile\": {:?},\n  \"answers\": {},\n  \
             \"learner_side\": \"respelled\",\n  \"canonicalized_pairs\": {},\n  \
             \"verdicts\": {{\"correct\": {}, \"wrong\": {}, \"undecidable\": {}}},\n  \
             \"check_ns\": {},\n  \"p95_budget_ns\": {},\n  \"p50_floor_ns\": {}\n}}\n",
            profile(),
            pairs.len(),
            canonicalized,
            correct,
            wrong,
            undecidable,
            times.json(),
            budget,
            CHECK_P50_FLOOR_NS,
        ),
    );

    assert_eq!(
        correct, RESPELLED_CORRECT,
        "the count of re-spellings the checker accepts changed"
    );
    assert_eq!(
        wrong, RESPELLED_WRONG,
        "the checker called a re-spelling of an answer a different answer"
    );
    assert_eq!(
        undecidable, RESPELLED_UNDECIDABLE,
        "the count of pairs outside the decidable grammar changed"
    );
    assert!(
        times.p95 < budget,
        "the p95 check took {} ns, and the budget is {budget} ns",
        times.p95
    );
    assert!(
        times.p50 > CHECK_P50_FLOOR_NS,
        "the p50 check took {} ns, under the {CHECK_P50_FLOOR_NS} ns floor, so the loop \
         short-circuited before the canonicalizer",
        times.p50
    );
}

// ---------------------------------------------------------------------------
// Benchmark A, part 3 — the arena traversal and the scheduler decision (L1)
// ---------------------------------------------------------------------------

/// The p95 budget of one plan composition, in nanoseconds: 20 ms of the 150 ms
/// of L1 (`docs/reference/l1-budget.md`, the first row of the L1 table).
///
/// M4 wrote 5 ms into that row and measured nothing. This test is the first
/// measurement, and it reads a p95 of 3.24 ms on the build box: 1.5 times under
/// 5 ms, which is a flaky gate on a shared runner and not a budget. M5 U12
/// therefore moves the split, which is the mechanism section 1 of that document
/// names: the row takes 20 ms and the unmeasured framework row gives them up.
/// The total of L1 stays 150 ms.
const ARENA_P95_BUDGET_NS: u128 = 20_000_000;

/// The count of measured compositions.
const ARENA_ITERATIONS: usize = 200;

/// The clock of every composition. It is `T` of the 1.0 selector suite
/// (`crates/core/tests/common/mod.rs`, `T_US`).
const ARENA_T_US: i64 = 1_784_030_400_000_000;

/// The session id the composed task ids are keyed on.
const ARENA_SESSION: &str = "s_2026-08-29a";

/// The quiz sampler seed of every composition.
const ARENA_SEED: u64 = 20_260_829;

/// Every `ARENA_LEARNED_STRIDE`-th topic of the arena carries a learned state.
const ARENA_LEARNED_STRIDE: usize = 3;

/// The `memoryBase` of a learned state that is DUE for review at [`ARENA_T_US`].
const ARENA_MEMORY_DUE: f64 = 0.35;

/// The `memoryBase` of a learned state that is not due at [`ARENA_T_US`].
const ARENA_MEMORY_FRESH: f64 = 0.95;

/// Every `ARENA_DUE_STRIDE`-th learned topic is due for review.
///
/// The fixture models a learner mid-course: about a third of the tree is
/// learned, and a fifth of that is due on this day. A fixture in which EVERY
/// learned topic is due is the worst case and not the day: it puts 364 topics
/// into the compression and measures 11.2 ms on the build box, which
/// `docs/reference/l1-budget.md` section 8 records beside the number this test
/// gates.
const ARENA_DUE_STRIDE: usize = 5;

/// The count of topics the committed curriculum tree holds.
const ARENA_TOPICS: usize = 1_090;

/// The count of topics the fixture puts into the states map.
const ARENA_LEARNED_TOPICS: usize = 364;

/// The count of tasks every composition returns.
const ARENA_TASKS: usize = 116;

/// The `task_id` of the first task of every composition.
const ARENA_FIRST_TASK: &str = "s_2026-08-29a-review-adding-subtracting-rational-expressions";

/// The `task_id` of the last task of every composition.
const ARENA_LAST_TASK: &str = "s_2026-08-29a-drill-subtracting-integers";

/// The committed curriculum tree.
fn real_curriculum() -> Curriculum {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (graph, _) = load_curriculum(&root)
        .unwrap_or_else(|err| panic!("the curriculum at {} did not load: {err}", root.display()));
    graph
}

/// One learned topic state, in the shape the 1.0 `_learned` builder writes.
fn learned_state(memory: f64) -> TopicState {
    TopicState {
        status: TopicStatus::Learning,
        rep_num: 3.0,
        memory_base: memory,
        t0: Some(Timestamp::from_micros(ARENA_T_US)),
        interval_days: 10.0,
        ability: 0.6,
        speed: 1.2,
        ..TopicState::default()
    }
}

/// The fixture states map: every third topic of the arena, in arena order.
///
/// The rule is a stride and not a draw, so the map is a function of the
/// committed tree alone and the count below is a literal of that tree.
fn arena_states(graph: &Curriculum) -> BTreeMap<String, TopicState> {
    graph
        .topics()
        .iter()
        .enumerate()
        .filter(|(index, _)| index % ARENA_LEARNED_STRIDE == 0)
        .enumerate()
        .map(|(learned, (_, topic))| {
            let memory = if learned % ARENA_DUE_STRIDE == 0 {
                ARENA_MEMORY_DUE
            } else {
                ARENA_MEMORY_FRESH
            };
            (topic.id.as_str().to_owned(), learned_state(memory))
        })
        .collect()
}

/// Benchmark A: 200 plan compositions hold the first 5 ms segment of L1.
///
/// M4 left this row of the L1 table unmeasured: M3 asserts the selector against
/// the 1.0 oracle for correctness and never for time (section 5 of
/// `docs/reference/l1-budget.md`). One iteration is the whole in-memory half of
/// a plan: walk the course scope, read the mastered set and the frontier, take
/// the due reviews, compress them, order the lessons, and interleave the result.
/// That is `compose_session`, the one function `cadus_web::session::compose_plan`
/// calls, over the COMMITTED curriculum tree and not a fixture graph.
#[test]
fn benchmark_a_arena_holds_the_l1_segment() {
    if !benchmarks_are_on() {
        println!("SKIPPED benchmark A (arena): {BENCH_VAR} is not set");
        return;
    }
    let graph = real_curriculum();
    let states = arena_states(&graph);
    let cfg = Config::default();
    let no_test_prep: BTreeSet<String> = BTreeSet::new();
    let no_closed: BTreeSet<String> = BTreeSet::new();
    let ctx = SessionContext::default()
        .with_session_id(ARENA_SESSION)
        .with_test_prep(&no_test_prep)
        .with_multistep(0, &no_closed);

    let mut samples: Vec<u128> = Vec::with_capacity(ARENA_ITERATIONS);
    let mut plans: Vec<SessionPlan> = Vec::with_capacity(ARENA_ITERATIONS);
    for _ in 0..ARENA_ITERATIONS {
        // The sampler is built inside the loop, so every iteration composes the
        // same plan from the same seed and the 200 samples measure one shape.
        let mut sampler = SeededSampler::new(ARENA_SEED);
        let start = Instant::now();
        let plan = compose_session(&states, &graph, &cfg, ARENA_T_US, &mut sampler, &ctx);
        samples.push(start.elapsed().as_nanos());
        plans.push(plan);
    }

    let times = Percentiles::of(&samples);
    let budget = budget(ARENA_P95_BUDGET_NS);
    let first = plans[0].tasks.first().map(|task| task.task_id.clone());
    let last = plans[0].tasks.last().map(|task| task.task_id.clone());
    // Report first, then assert, for the reason the other two halves report
    // first: the review cycle reads the failing run too.
    println!(
        "benchmark A arena ({}): p50 {} ns, p95 {} ns, p99 {} ns, max {} ns over {} topics, \
         {} learned, {} tasks, first {:?}, last {:?}",
        profile(),
        times.p50,
        times.p95,
        times.p99,
        times.max,
        graph.topics().len(),
        states.len(),
        plans[0].tasks.len(),
        first,
        last,
    );
    write_artifact(
        "benchmark-a-arena.json",
        &format!(
            "{{\n  \"benchmark\": \"A-arena\",\n  \"profile\": {:?},\n  \"topics\": {},\n  \
             \"learned_topics\": {},\n  \"iterations\": {},\n  \"tasks\": {},\n  \
             \"compose_ns\": {},\n  \"p95_budget_ns\": {}\n}}\n",
            profile(),
            graph.topics().len(),
            states.len(),
            ARENA_ITERATIONS,
            plans[0].tasks.len(),
            times.json(),
            budget,
        ),
    );

    assert_eq!(
        samples.len(),
        ARENA_ITERATIONS,
        "every iteration is measured"
    );
    assert_eq!(
        graph.topics().len(),
        ARENA_TOPICS,
        "the committed curriculum tree changed its topic count"
    );
    assert_eq!(
        states.len(),
        ARENA_LEARNED_TOPICS,
        "the fixture states map changed its size"
    );
    assert_eq!(
        plans[0].tasks.len(),
        ARENA_TASKS,
        "the composed plan changed its task count"
    );
    assert_eq!(first.as_deref(), Some(ARENA_FIRST_TASK));
    assert_eq!(last.as_deref(), Some(ARENA_LAST_TASK));
    // Every iteration composes the same plan. Without this the loop could be
    // measuring 200 different decisions and reporting one percentile over them.
    for (index, plan) in plans.iter().enumerate() {
        assert_eq!(
            plan.tasks, plans[0].tasks,
            "iteration {index} composed a different plan"
        );
    }
    assert!(
        times.p95 < budget,
        "the p95 composition took {} ns, and the budget is {budget} ns",
        times.p95
    );
}
