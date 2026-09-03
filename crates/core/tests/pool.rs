//! M4 U3 acceptance: the pool sources and the anti-repeat rule (A5, A6, A7, D5).
//!
//! Every expected value in this file is a literal. The statements, the digests,
//! the window sizes, the wire tags, and the refusal messages are written out, and
//! none of them is read back from the code under test. The literals come from
//! four places:
//!
//! - `docs/reference/serving-1.0-spec.md`, section 5 (anti-repeat), section 6
//!   (the fallback), section 7 (the serve path), and section 9 (pinned literals);
//! - `/home/deploy/dev/cadus/cadus/projector.py` and `cadus_web/state.py`, the
//!   1.0 files that hold the two window sizes;
//! - `migrations/0005_content.sql`, which holds the three `source` wire values;
//! - SHA-1 digests worked out by hand from the statements, so a wrong hash
//!   function fails the test instead of agreeing with itself. 1.0 learned that
//!   lesson the hard way (specification section 5.1: two spellings once made the
//!   whole guard a silent no-op, and the covering test still passed).
//!
//! No test in this file calls a model, opens a socket, or reads a clock (T1, R3).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{
    Avoid, FILL_ROUNDS, POP_CANDIDATES, PoolCounters, ProblemSource, RING_CAPACITY, Ring, SOURCES,
    Source, TASK_MEMORY_CAPACITY, TaskMemory, TemplateSource, pick, serve,
};
use cadus_core::template::{Compiled, Instance};
use common::pool_fixtures::{
    SQUARE_STATEMENTS, bind_int, doc_from, perfect_squares_body, squares_in_order,
};

// --------------------------------------------------------------------------
// The pinned sizes and wire values
// --------------------------------------------------------------------------

#[test]
fn the_two_window_sizes_are_twenty_and_twelve() {
    // `LAST_PROBLEMS_WINDOW` (`cadus/projector.py:92`) and `SERVED_TEXT_MEMORY`
    // (`cadus_web/state.py:133`), specification section 5.5.
    assert_eq!(RING_CAPACITY, 20, "the D5 ring keeps 20 instance hashes");
    assert_eq!(
        TASK_MEMORY_CAPACITY, 12,
        "the per-task memory keeps 12 statement hashes"
    );
    assert_eq!(Ring::capacity(), 20);
    assert_eq!(TaskMemory::capacity(), 12);
}

#[test]
fn the_serve_pops_at_most_eight_candidates() {
    // The M4 pool decision: "Serve pops at most 8 candidates with FOR UPDATE
    // SKIP LOCKED", specification section 5.5.
    assert_eq!(POP_CANDIDATES, 8);
}

#[test]
fn a_fill_walks_at_most_eight_candidate_streams() {
    assert_eq!(FILL_ROUNDS, 8);
}

#[test]
fn the_three_source_tags_are_the_three_wire_values_of_the_column() {
    // `migrations/0005_content.sql`: CHECK (source IN ('template','exemplar','generator')).
    assert_eq!(SOURCES, ["template", "exemplar", "generator"]);
    assert_eq!(Source::Template.as_str(), "template");
    assert_eq!(Source::Exemplar.as_str(), "exemplar");
    assert_eq!(Source::Generator.as_str(), "generator");
    assert_eq!(Source::Exemplar.to_string(), "exemplar");
    assert_eq!(
        serde_json::to_string(&Source::Exemplar).unwrap(),
        "\"exemplar\""
    );
    assert_eq!(
        serde_json::from_str::<Source>("\"generator\"").unwrap(),
        Source::Generator
    );
    assert_eq!(Source::from_wire("template"), Some(Source::Template));
    assert_eq!(Source::from_wire("exemplar"), Some(Source::Exemplar));
    assert_eq!(Source::from_wire("generator"), Some(Source::Generator));
    assert_eq!(
        Source::from_wire("Template"),
        None,
        "a value outside the check constraint decodes to nothing"
    );
}

// --------------------------------------------------------------------------
// The instance hash (specification section 5.1, trap 1)
// --------------------------------------------------------------------------

#[test]
fn instance_hash_is_problem_text_hash_of_the_rendered_statement() {
    let doc = doc_from(perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    let instance = compiled
        .instantiate(bind_int("a", 7))
        .expect("a = 7 instantiates");

    // The literals. A wrong hash function, a strip, or a case fold fails here.
    assert_eq!(instance.text, "Compute $7^{2}$.");
    assert_eq!(instance.answer, "49");
    assert_eq!(instance.instance_hash, "e4047cd6798e");

    // The same digest the one exported function produces. 1.0 pins the identical
    // equality at `tests/test_problem_templates.py:594-608`, after a second
    // spelling of the digest turned its avoidance into a silent no-op.
    assert_eq!(
        instance.instance_hash,
        problem_text_hash("Compute $7^{2}$.")
    );
}

#[test]
fn every_square_statement_hashes_to_its_pinned_digest() {
    let doc = doc_from(perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    for (index, (statement, digest)) in SQUARE_STATEMENTS.iter().enumerate() {
        let a = i64::try_from(index).unwrap() + 1;
        let instance = compiled
            .instantiate(bind_int("a", a))
            .expect("instantiates");
        assert_eq!(&instance.text, statement, "statement of a = {a}");
        assert_eq!(&instance.instance_hash, digest, "digest of a = {a}");
    }
}

// --------------------------------------------------------------------------
// Acceptance 1: eleven of twelve blocked
// --------------------------------------------------------------------------

#[test]
fn eleven_of_twelve_blocked_serves_the_free_instance_over_ten_seeds() {
    // 1.0 pins the same behavior: with 11 of 12 blocked, the serve returns
    // `Compute $12^{2}$.` on 10 seeds (specification section 9,
    // `tests/test_problem_templates.py:583-618`).
    let doc = doc_from(perfect_squares_body());
    let source = TemplateSource::new("perfect-squares", &doc).expect("the fixture compiles");

    let blocked: Vec<&str> = SQUARE_STATEMENTS
        .iter()
        .take(11)
        .map(|(_, digest)| *digest)
        .collect();
    assert_eq!(blocked.len(), 11);

    for seed in 0_u64..10 {
        let candidates = source
            .fill("perfect-squares", 12, seed)
            .expect("the template fills");
        assert_eq!(
            candidates.len(),
            12,
            "seed {seed}: the whole space is 12 distinct instances"
        );

        let mut ring = Ring::from_hashes(blocked.iter().copied());
        let mut task = TaskMemory::new();
        let mut counters = PoolCounters::new();
        let served =
            serve(&candidates, &mut ring, &mut task, &mut counters).expect("a candidate is served");

        assert_eq!(
            served.text, "Compute $12^{2}$.",
            "seed {seed}: the one free instance is served"
        );
        assert_eq!(served.instance_hash, "c3d8b10562c1", "seed {seed}");
        assert_eq!(served.answer, "144", "seed {seed}");
        assert_eq!(counters.served, 1, "seed {seed}");
        assert_eq!(
            counters.pool_exhausted, 0,
            "seed {seed}: the pool was not exhausted"
        );
        assert_eq!(ring.len(), 12, "seed {seed}: the ring took the served hash");
        assert_eq!(task.hashes(), ["c3d8b10562c1"], "seed {seed}");
    }
}

#[test]
fn a_blocked_candidate_is_skipped_and_counted() {
    let doc = doc_from(perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    let candidates = squares_in_order(&compiled);

    // Block the first three statements only.
    let ring = Ring::from_hashes(["44b34b7dc138", "c88b03aa364f", "e61280d48db9"]);
    let task = TaskMemory::new();
    let avoid = Avoid::new(&ring, &task);
    let chosen = pick(&candidates, &avoid).expect("a candidate survives");

    assert_eq!(chosen.index, 3);
    assert_eq!(chosen.skipped, 3);
    assert!(!chosen.exhausted);
    assert_eq!(candidates[chosen.index].text, "Compute $4^{2}$.");
}

#[test]
fn the_task_memory_blocks_a_candidate_the_ring_allows() {
    let doc = doc_from(perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    let candidates = squares_in_order(&compiled);

    let ring = Ring::new();
    let task = TaskMemory::from_hashes(["44b34b7dc138"]);
    let avoid = Avoid::new(&ring, &task);
    assert!(avoid.blocks("44b34b7dc138"));
    assert_eq!(avoid.len(), 1);

    let chosen = pick(&candidates, &avoid).expect("a candidate survives");
    assert_eq!(chosen.index, 1);
    assert_eq!(candidates[chosen.index].text, "Compute $2^{2}$.");
}

// --------------------------------------------------------------------------
// Acceptance 2: fully blocked
// --------------------------------------------------------------------------

#[test]
fn fully_blocked_serves_the_last_candidate_and_counts_pool_exhausted() {
    // Specification section 5.4, step 6, quoting `problem_templates.py:388-392`:
    // "A repeat is a far smaller failure than no problem."
    let doc = doc_from(perfect_squares_body());
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    let candidates = squares_in_order(&compiled);
    assert_eq!(candidates.len(), 12);

    let all_twelve: Vec<&str> = SQUARE_STATEMENTS
        .iter()
        .map(|(_, digest)| *digest)
        .collect();
    let mut ring = Ring::from_hashes(all_twelve);
    let mut task = TaskMemory::new();
    let mut counters = PoolCounters::new();

    let served =
        serve(&candidates, &mut ring, &mut task, &mut counters).expect("a repeat is still served");

    assert_eq!(
        served.text, "Compute $12^{2}$.",
        "the LAST candidate is served"
    );
    assert_eq!(served.instance_hash, "c3d8b10562c1");
    assert_eq!(counters.pool_exhausted, 1);
    assert_eq!(counters.served, 1);
    assert_eq!(counters.blocked, 12);
    assert_eq!(ring.len(), 13, "the repeat costs its own ring slot");
}

#[test]
fn an_empty_candidate_list_serves_nothing() {
    // Specification section 7.2: on an empty pool the handler instantiates an
    // exemplar and raises the A6 flag. The rule reports the miss; it never draws.
    let candidates: Vec<Instance> = Vec::new();
    let mut ring = Ring::new();
    let mut task = TaskMemory::new();
    let mut counters = PoolCounters::new();

    assert!(serve(&candidates, &mut ring, &mut task, &mut counters).is_none());
    assert_eq!(counters.served, 0);
    assert_eq!(counters.pool_exhausted, 0);
    assert!(ring.is_empty());
    assert!(task.is_empty());
}

// --------------------------------------------------------------------------
// The candidate rule on the storage seam
// --------------------------------------------------------------------------

#[test]
fn the_rule_runs_on_bare_digests_for_the_store_seam() {
    // U4 pops rows, not instances. The rule reads a digest through `Candidate`,
    // so the store runs the identical rule on its own row type.
    let rows = [
        "44b34b7dc138".to_string(),
        "c88b03aa364f".to_string(),
        "e61280d48db9".to_string(),
    ];
    let ring = Ring::from_hashes(["44b34b7dc138"]);
    let task = TaskMemory::new();
    let avoid = Avoid::new(&ring, &task);

    let chosen = pick(&rows, &avoid).expect("a row survives");
    assert_eq!(chosen.index, 1);
    assert_eq!(chosen.skipped, 1);
    assert!(!chosen.exhausted);

    let empty: [String; 0] = [];
    assert!(pick(&empty, &avoid).is_none());
}

#[test]
fn a_ring_only_view_ignores_the_task_memory() {
    let ring = Ring::from_hashes(["44b34b7dc138"]);
    let view = Avoid::from_ring(&ring);
    assert!(view.blocks("44b34b7dc138"));
    assert!(!view.blocks("c88b03aa364f"));
    assert_eq!(view.len(), 1);

    let nothing = Avoid::none();
    assert!(nothing.is_empty());
    assert!(!nothing.blocks("44b34b7dc138"));
}
