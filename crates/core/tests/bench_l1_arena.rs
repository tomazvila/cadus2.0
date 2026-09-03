//! Benchmark A, part 3: the arena traversal and the scheduler decision of the
//! L1 budget, over the COMMITTED curriculum tree.
//!
//! Requirement L1 (serve a problem, p95 < 150 ms). The 20 ms segment is the
//! first row of the L1 table of `docs/reference/l1-budget.md`. See
//! `bench_l1.rs` for why the benchmark is a plain test, and for how to run it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

use cadus_core::config::Config;
use cadus_core::curriculum::{Curriculum, load_curriculum};
use cadus_core::event::{Timestamp, TopicStatus};
use cadus_core::learner::TopicState;
use cadus_core::selector::{SeededSampler, SessionContext, SessionPlan, compose_session};
use common::bench::{BENCH_VAR, Percentiles, benchmarks_are_on, budget, profile, write_artifact};

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
