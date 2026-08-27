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

use cadus_core::curriculum::AnswerKind;
use cadus_core::curriculum::model::{Exemplar, KnowledgePoint, Slug};
use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{
    Avoid, Batch, ExemplarSource, FILL_ROUNDS, FillError, POP_CANDIDATES, PoolCounters,
    ProblemSource, REFUSAL_FLAG_PERCENT, RING_CAPACITY, Ring, SOURCES, Source,
    TASK_MEMORY_CAPACITY, TaskMemory, TemplateSource, check_instance, pick, serve,
};
use cadus_core::template::{
    Bindings, Compiled, GateSpec, Instance, Scalar, TemplateDoc, from_body, gate,
};

// --------------------------------------------------------------------------
// Fixtures
// --------------------------------------------------------------------------

/// The 1.0 perfect-squares template, in the 2.0 document shape.
///
/// The declared space is 12 tuples, which is the `MIN_SPACE_SIZE` floor of
/// specification section 9. Twelve is also the count 1.0 uses for its
/// blocked-set test (`tests/test_problem_templates.py:583-618`).
fn perfect_squares_body() -> &'static str {
    r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}^{{2}}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 12}},
      "answer_expr": "a**2",
      "solution_sketch": "${a} \\times {a}$ gives the answer.",
      "hints": ["What does squaring a number mean?"],
      "samples": [{"params": {"a": 1}, "expected": "1"},
                  {"params": {"a": 12}, "expected": "144"}]
    }"#
}

/// A document every instantiation refuses: the answer divides by zero.
fn always_refused_body() -> &'static str {
    r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 12}},
      "answer_expr": "1/(a - a)",
      "hints": ["Read the statement again."]
    }"#
}

/// A document whose two constraints cannot both hold.
fn no_satisfying_tuple_body() -> &'static str {
    r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}$ less ${b}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 12},
                 "b": {"kind": "int", "low": 1, "high": 12}},
      "constraints": [{"op": "gt", "left": "a", "right": "b"},
                      {"op": "lt", "left": "a", "right": "b"}],
      "answer_expr": "a - b",
      "hints": ["Which number is larger?"]
    }"#
}

fn doc_from(body: &str) -> TemplateDoc {
    from_body(body).expect("the fixture body reads")
}

/// Bind one whole number to one parameter name.
fn bind_int(name: &str, value: i64) -> Bindings {
    let mut bindings = Bindings::new();
    bindings.insert(name.to_string(), Scalar::Int(value).value());
    bindings
}

/// Bind two whole numbers to two parameter names.
fn bind_two(first: &str, one: i64, second: &str, two: i64) -> Bindings {
    let mut bindings = bind_int(first, one);
    bindings.insert(second.to_string(), Scalar::Int(two).value());
    bindings
}

/// The twelve statements of the perfect-squares template and their digests.
///
/// Every digest is `sha1(utf8(statement))[:12]`, worked out from the statement
/// and written here as a literal (specification section 5.1). The seventh row is
/// the row 1.0 pins in `tests/test_problem_templates.py:594-608`.
const SQUARE_STATEMENTS: [(&str, &str); 12] = [
    ("Compute $1^{2}$.", "44b34b7dc138"),
    ("Compute $2^{2}$.", "c88b03aa364f"),
    ("Compute $3^{2}$.", "e61280d48db9"),
    ("Compute $4^{2}$.", "b70f053d75ea"),
    ("Compute $5^{2}$.", "999f3a215c9b"),
    ("Compute $6^{2}$.", "4479aead909f"),
    ("Compute $7^{2}$.", "e4047cd6798e"),
    ("Compute $8^{2}$.", "46f23bdadecb"),
    ("Compute $9^{2}$.", "a60352fc67e1"),
    ("Compute $10^{2}$.", "87eb77864313"),
    ("Compute $11^{2}$.", "88df4e1cc368"),
    ("Compute $12^{2}$.", "c3d8b10562c1"),
];

/// The three exemplars of the fallback fixture, in author order.
const EXEMPLARS: [(&str, &str, &str); 3] = [
    ("Compute $3 + 4$.", "7", "2af3b1f3dd58"),
    ("Compute $10 + 6$.", "16", "f8a6a976625f"),
    ("Compute $25 + 25$.", "50", "60ddf2e09d81"),
];

fn exemplar_fixture() -> Vec<Exemplar> {
    EXEMPLARS
        .iter()
        .map(|(problem, answer, _)| Exemplar {
            problem: (*problem).to_string(),
            answer: (*answer).to_string(),
            solution_sketch: None,
        })
        .collect()
}

/// The twelve instances of the perfect-squares template, in `a` order.
///
/// The order is fixed, so a test that asserts "the LAST candidate" names a
/// literal statement and never re-derives one.
fn squares_in_order(compiled: &Compiled<'_>) -> Vec<Instance> {
    (1..=12)
        .map(|a| {
            compiled
                .instantiate(bind_int("a", a))
                .expect("the perfect-squares template instantiates")
        })
        .collect()
}

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
// Acceptance 3: the exemplar rotation (A6)
// --------------------------------------------------------------------------

#[test]
fn an_exemplar_knowledge_point_cycles_through_all_exemplars() {
    // Specification section 6.1: 2.0 fills the pool with the whole exemplar list
    // and lets the D5 ring choose, so a knowledge point with three exemplars gets
    // a real three-cycle instead of 1.0's per-task `index % len` restart.
    let exemplars = exemplar_fixture();
    let source = ExemplarSource::new("adding-two-digits", &exemplars);
    let candidates = source
        .fill("adding-two-digits", 8, 0)
        .expect("the fill runs");

    assert_eq!(
        candidates.len(),
        3,
        "every exemplar enters the pool at once"
    );
    assert_eq!(candidates[0].text, "Compute $3 + 4$.");
    assert_eq!(candidates[1].text, "Compute $10 + 6$.");
    assert_eq!(candidates[2].text, "Compute $25 + 25$.");
    assert_eq!(candidates[0].instance_hash, "2af3b1f3dd58");
    assert_eq!(candidates[1].instance_hash, "f8a6a976625f");
    assert_eq!(candidates[2].instance_hash, "60ddf2e09d81");
    assert_eq!(candidates[0].answer, "7");
    assert_eq!(candidates[2].answer, "50");

    let mut ring = Ring::new();
    let mut task = TaskMemory::new();
    let mut counters = PoolCounters::new();

    let mut served = Vec::new();
    for _ in 0..3 {
        let instance =
            serve(&candidates, &mut ring, &mut task, &mut counters).expect("an exemplar is served");
        served.push(instance.text.clone());
    }
    assert_eq!(
        served,
        [
            "Compute $3 + 4$.".to_string(),
            "Compute $10 + 6$.".to_string(),
            "Compute $25 + 25$.".to_string()
        ],
        "the three serves walk the whole exemplar list in author order"
    );
    assert_eq!(counters.served, 3);
    assert_eq!(
        counters.pool_exhausted, 0,
        "the cycle never repeats inside its own length"
    );

    // The fourth serve has nowhere to go: three exemplars cannot fill a ring of
    // 20. It repeats the last one and raises the A6 count.
    let fourth = serve(&candidates, &mut ring, &mut task, &mut counters).expect("a repeat");
    assert_eq!(fourth.text, "Compute $25 + 25$.");
    assert_eq!(counters.pool_exhausted, 1);
}

#[test]
fn the_exemplar_source_reports_its_tag_and_its_ring_shortfall() {
    let exemplars = exemplar_fixture();
    let source = ExemplarSource::new("adding-two-digits", &exemplars);

    assert_eq!(source.source(), Source::Exemplar);
    assert_eq!(source.source().as_str(), "exemplar");
    assert_eq!(source.kp_id(), "adding-two-digits");
    assert_eq!(
        source.content_digest(),
        None,
        "an exemplar has no content_store row"
    );
    assert_eq!(source.len(), 3);
    assert!(!source.is_empty());
    assert!(
        !source.covers_ring(),
        "3 exemplars cannot fill a ring of 20 (A6 flag)"
    );
    assert!(source.refusals().is_empty());
}

#[test]
fn an_exemplar_source_reads_a_knowledge_point() {
    let kp = KnowledgePoint {
        id: Slug::new("adding-two-digits").unwrap(),
        name: "Add two two-digit numbers".to_string(),
        key_prerequisites: Vec::new(),
        exemplars: exemplar_fixture(),
        constraints: None,
    };
    let source = ExemplarSource::from_knowledge_point(&kp);
    assert_eq!(source.kp_id(), "adding-two-digits");
    assert_eq!(source.exemplars().len(), 3);
    let filled = source
        .fill("adding-two-digits", 1, 7)
        .expect("the fill runs");
    assert_eq!(filled.len(), 1, "the fill stops at the asked-for count");
    assert_eq!(filled[0].text, "Compute $3 + 4$.");
}

#[test]
fn the_exemplar_fill_drops_a_repeated_statement() {
    // The pool carries UNIQUE (user_id, kp_id, instance_hash), so a batch with
    // two equal digests loses a row to a conflict.
    let exemplars = vec![
        Exemplar {
            problem: "Compute 1 + 1.".to_string(),
            answer: "2".to_string(),
            solution_sketch: None,
        },
        Exemplar {
            problem: "Compute 1 + 1.".to_string(),
            answer: "2".to_string(),
            solution_sketch: None,
        },
    ];
    let source = ExemplarSource::new("fallback", &exemplars);
    let filled = source.fill("fallback", 8, 0).expect("the fill runs");
    assert_eq!(filled.len(), 1);
    assert_eq!(filled[0].text, "Compute 1 + 1.");
    assert_eq!(filled[0].instance_hash, "18f88183c820");
}

#[test]
fn an_undecidable_exemplar_answer_is_named_and_skipped() {
    // Specification section 6.1: record the fact rather than hide it. One broken
    // exemplar must not take the whole knowledge point off the air.
    let exemplars = vec![
        Exemplar {
            problem: "Prove that the sum of two even numbers is even.".to_string(),
            answer: "See the write-up.".to_string(),
            solution_sketch: None,
        },
        Exemplar {
            problem: "Compute $3 + 4$.".to_string(),
            answer: "7".to_string(),
            solution_sketch: None,
        },
    ];
    let source = ExemplarSource::new("mixed", &exemplars);

    let refusals = source.refusals();
    assert_eq!(refusals.len(), 1);
    assert_eq!(refusals[0].index, 0);
    assert_eq!(refusals[0].answer, "See the write-up.");

    let filled = source
        .fill("mixed", 8, 0)
        .expect("the good exemplar survives");
    assert_eq!(filled.len(), 1);
    assert_eq!(filled[0].text, "Compute $3 + 4$.");
}

#[test]
fn a_knowledge_point_with_no_usable_exemplar_refuses_the_fill() {
    let exemplars: Vec<Exemplar> = Vec::new();
    let source = ExemplarSource::new("empty", &exemplars);
    let refusal = source.fill("empty", 4, 0).expect_err("the fill refuses");
    assert_eq!(refusal, FillError::NoExemplar);
    assert_eq!(
        refusal.to_string(),
        "no exemplar of this knowledge point has an answer the checker can decide"
    );
}

// --------------------------------------------------------------------------
// Acceptance 4: the ring keeps 20 and drops the oldest
// --------------------------------------------------------------------------

#[test]
fn the_ring_keeps_twenty_and_drops_the_oldest() {
    let mut ring = Ring::new();
    for index in 0..25 {
        ring.push(&format!("hash{index:02}"));
    }
    assert_eq!(ring.len(), 20);
    assert_eq!(ring.hashes()[0], "hash05", "the newest 20 start at hash05");
    assert_eq!(ring.hashes()[19], "hash24", "the newest entry is last");
    assert!(!ring.contains("hash04"), "hash04 fell out of the window");
    assert!(ring.contains("hash05"));
    assert!(ring.contains("hash24"));

    ring.clear();
    assert!(ring.is_empty());
    assert_eq!(ring.len(), 0);
}

#[test]
fn the_task_memory_keeps_twelve_and_drops_the_oldest() {
    let mut task = TaskMemory::new();
    for index in 0..15 {
        task.push(&format!("hash{index:02}"));
    }
    assert_eq!(task.len(), 12);
    assert_eq!(task.hashes()[0], "hash03");
    assert_eq!(task.hashes()[11], "hash14");
    assert!(!task.contains("hash02"));
    assert!(task.contains("hash03"));
}

#[test]
fn an_over_long_stored_window_loads_to_a_legal_window() {
    let over_long: Vec<String> = (0..30).map(|index| format!("hash{index:02}")).collect();
    let ring = Ring::from_hashes(over_long.clone());
    assert_eq!(ring.len(), 20);
    assert_eq!(ring.hashes()[0], "hash10");

    let task = TaskMemory::from_hashes(over_long);
    assert_eq!(task.len(), 12);
    assert_eq!(task.hashes()[0], "hash18");
}

#[test]
fn the_ring_keeps_a_repeated_digest_the_way_the_one_zero_fold_does() {
    // `cadus/projector.py:213-224` appends and truncates. It does not deduplicate.
    let mut ring = Ring::new();
    ring.push("e4047cd6798e");
    ring.push("e4047cd6798e");
    assert_eq!(ring.len(), 2);
    assert_eq!(ring.hashes(), ["e4047cd6798e", "e4047cd6798e"]);
}

// --------------------------------------------------------------------------
// The D-S6 documents
// --------------------------------------------------------------------------

#[test]
fn the_two_windows_are_the_d_s6_serde_documents() {
    let ring = Ring::from_hashes(["e4047cd6798e", "c3d8b10562c1"]);
    let body = serde_json::to_string(&ring).unwrap();
    assert_eq!(body, r#"{"hashes":["e4047cd6798e","c3d8b10562c1"]}"#);
    assert_eq!(serde_json::from_str::<Ring>(&body).unwrap(), ring);

    let task = TaskMemory::from_hashes(["44b34b7dc138"]);
    let body = serde_json::to_string(&task).unwrap();
    assert_eq!(body, r#"{"hashes":["44b34b7dc138"]}"#);
    assert_eq!(serde_json::from_str::<TaskMemory>(&body).unwrap(), task);

    // A missing array reads as an empty window, so a state row written before
    // this milestone still loads.
    assert_eq!(serde_json::from_str::<Ring>("{}").unwrap(), Ring::new());
    assert_eq!(
        serde_json::from_str::<TaskMemory>("{}").unwrap(),
        TaskMemory::new()
    );

    // An unknown key is a refusal, not a silently dropped instruction.
    assert!(serde_json::from_str::<Ring>(r#"{"texts":[]}"#).is_err());
}

#[test]
fn the_counters_round_trip_as_a_document() {
    let mut counters = PoolCounters::new();
    assert_eq!(counters, PoolCounters::default());
    counters.record(pick(&["aaaaaaaaaaaa"], &Avoid::none()).unwrap());
    assert_eq!(counters.served, 1);
    assert_eq!(counters.blocked, 0);
    assert_eq!(counters.pool_exhausted, 0);

    counters.record_undecodable(2);
    assert_eq!(counters.pool_row_undecodable, 2);

    let body = serde_json::to_string(&counters).unwrap();
    assert_eq!(
        body,
        r#"{"served":1,"blocked":0,"pool_exhausted":0,"pool_row_undecodable":2}"#
    );
    assert_eq!(
        serde_json::from_str::<PoolCounters>(&body).unwrap(),
        counters
    );
}

// --------------------------------------------------------------------------
// The template source (A1, A7)
// --------------------------------------------------------------------------

#[test]
fn the_template_source_reports_its_tag_and_its_digest() {
    let doc = doc_from(perfect_squares_body());
    let source = TemplateSource::new("perfect-squares", &doc)
        .expect("the fixture compiles")
        .with_digest("498eee5fb77c1db4");

    assert_eq!(source.source(), Source::Template);
    assert_eq!(source.source().as_str(), "template");
    assert_eq!(source.kp_id(), "perfect-squares");
    assert_eq!(source.content_digest(), Some("498eee5fb77c1db4"));
    assert!(
        source.walks_whole_space(),
        "12 declared tuples is under the exhaustive limit"
    );
    assert_eq!(source.doc().topic_id, "perfect-squares");
}

#[test]
fn a_fill_returns_distinct_instances_of_the_whole_space() {
    let doc = doc_from(perfect_squares_body());
    let source = TemplateSource::new("perfect-squares", &doc).expect("the fixture compiles");
    let filled = source
        .fill("perfect-squares", 12, 0)
        .expect("the fill runs");

    assert_eq!(filled.len(), 12);
    let mut texts: Vec<String> = filled.iter().map(|i| i.text.clone()).collect();
    texts.sort();
    let mut want: Vec<String> = SQUARE_STATEMENTS
        .iter()
        .map(|(text, _)| (*text).to_string())
        .collect();
    want.sort();
    assert_eq!(texts, want, "the fill covers the whole 12-tuple space");

    // The space holds 12 distinct instances, so a request for 20 gets 12.
    let short = source
        .fill("perfect-squares", 20, 0)
        .expect("the fill runs");
    assert_eq!(short.len(), 12);
}

#[test]
fn a_fill_is_reproducible_from_its_recorded_seed() {
    // Specification section 3.2: a reviewer reproduces any served instance from
    // the seed and the document.
    let doc = doc_from(perfect_squares_body());
    let source = TemplateSource::new("perfect-squares", &doc).expect("the fixture compiles");
    let first = source
        .fill("perfect-squares", 4, 20_260_827)
        .expect("fills");
    let again = source
        .fill("perfect-squares", 4, 20_260_827)
        .expect("fills");
    assert_eq!(first, again, "one seed gives one batch");
    assert_eq!(first.len(), 4);
}

#[test]
fn a_fill_of_zero_instances_returns_an_empty_batch() {
    let doc = doc_from(perfect_squares_body());
    let source = TemplateSource::new("perfect-squares", &doc).expect("the fixture compiles");
    assert!(source.fill("perfect-squares", 0, 0).unwrap().is_empty());
}

#[test]
fn a_source_refuses_a_knowledge_point_it_does_not_fill() {
    let doc = doc_from(perfect_squares_body());
    let source = TemplateSource::new("perfect-squares", &doc).expect("the fixture compiles");
    let refusal = source.fill("adding-fractions", 4, 0).expect_err("refuses");
    assert_eq!(
        refusal,
        FillError::UnknownKp {
            have: "perfect-squares".to_string(),
            want: "adding-fractions".to_string(),
        }
    );
    assert_eq!(
        refusal.to_string(),
        "this source fills knowledge point perfect-squares and the caller asked for adding-fractions"
    );
}

#[test]
fn a_template_no_instance_survives_carries_the_one_zero_message() {
    let doc = doc_from(always_refused_body());
    let source = TemplateSource::new("perfect-squares", &doc).expect("the source compiles");
    let refusal = source.fill("perfect-squares", 3, 0).expect_err("refuses");
    assert!(matches!(refusal, FillError::NoValidInstance { .. }));
    assert_eq!(
        refusal.to_string(),
        "no instance of this template produced a usable answer — it should not have passed the gate, and it must not be served"
    );
}

#[test]
fn a_constraint_set_with_no_satisfying_tuple_refuses_the_fill() {
    let doc = doc_from(no_satisfying_tuple_body());
    let source = TemplateSource::new("subtraction", &doc).expect("the source compiles");
    let refusal = source.fill("subtraction", 3, 0).expect_err("refuses");
    assert_eq!(refusal, FillError::NoSatisfyingTuple);
    assert_eq!(
        refusal.to_string(),
        "no tuple of the declared domains satisfies the constraints"
    );
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

// --------------------------------------------------------------------------
// The per-instance re-check (C4, C6; review round 1, findings #1, #2, #15)
// --------------------------------------------------------------------------

/// The reviewer's template: 10,000 declared tuples, one of which breaks the
/// envelope.
///
/// The declared space is above `EXHAUSTIVE_SPACE_LIMIT`, so the gate reads a
/// 4,096-tuple sample from its constant seed and never meets `a = 100, b = 100`.
/// That tuple answers `-1`, and every authored answer of the knowledge point is a
/// non-negative whole number.
fn big_subtraction_body() -> &'static str {
    r#"{
      "v": 1,
      "topic_id": "big-subtraction",
      "answer_kind": "numeric",
      "statement": "Compute $9999 - {a} \\times {b}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 100},
                 "b": {"kind": "int", "low": 1, "high": 100}},
      "answer_expr": "9999 - a*b",
      "hints": ["What is the product first?"],
      "samples": [{"params": {"a": 1, "b": 1}, "expected": "9998"},
                  {"params": {"a": 100, "b": 1}, "expected": "9899"},
                  {"params": {"a": 1, "b": 100}, "expected": "9899"}]
    }"#
}

/// The two authored exemplars of that knowledge point.
///
/// Both answers are non-negative whole numbers, so the envelope is
/// `{non_negative: true, integral: true}`.
fn big_subtraction_exemplars() -> Vec<Exemplar> {
    vec![
        Exemplar {
            problem: "Compute $9999 - 1 \\times 12$.".to_string(),
            answer: "9987".to_string(),
            solution_sketch: None,
        },
        Exemplar {
            problem: "Compute $9999 - 10 \\times 12$.".to_string(),
            answer: "9879".to_string(),
            solution_sketch: None,
        },
    ]
}

/// C4: the fill refuses `a = 100, b = 100`, and no pool row ever carries it.
///
/// Seed 1459 is the seed the review round 1 report names: at that seed the batch
/// draws the one violating tuple of the 10,000. Every number below is a literal.
#[test]
fn the_fill_refuses_the_instance_the_gates_sample_never_read() {
    let doc = doc_from(big_subtraction_body());
    let exemplars = big_subtraction_exemplars();
    let source = TemplateSource::new("big-subtraction", &doc)
        .expect("the fixture compiles")
        .with_exemplars(&exemplars);

    // The gate ACCEPTS this document: its 4,096-tuple sample from the constant
    // GATE_SEED never meets the violating tuple of the 10,000.
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &exemplars,
    };
    let verified = gate(&doc, &spec).expect("the gate accepts the document");
    assert!(
        !verified.exhaustive,
        "10,000 declared tuples is above the exhaustive limit"
    );
    assert_eq!(verified.instances_checked, 4096);

    let batch = source
        .fill("big-subtraction", 24, 1459)
        .expect("the template fills");

    assert_eq!(batch.len(), 24, "the batch still fills to the depth asked");
    assert_eq!(
        batch.refusals().len(),
        1,
        "exactly one candidate of this batch breaks the envelope"
    );

    let refused = &batch.refusals()[0];
    assert_eq!(refused.code, "envelope-sign");
    assert_eq!(
        refused.message,
        "instance {'a': 100, 'b': 100} answers '-1', but every authored answer for this knowledge \
         point is non-negative — narrow the domains so no instance goes below zero"
    );
    assert_eq!(
        refused.text.as_deref(),
        Some("Compute $9999 - 100 \\times 100$."),
        "the refusal names the statement that must not be served"
    );

    // The refused statement is in NO instance of the batch.
    for instance in batch.instances() {
        assert_ne!(
            instance.text, "Compute $9999 - 100 \\times 100$.",
            "the refused instance must not reach the pool"
        );
        assert_ne!(instance.answer, "-1", "no served answer is negative");
    }
}

/// The same batch without the exemplars keeps the instance.
///
/// The envelope is the exemplars' rule, so a source that carries none has no
/// envelope to apply. The test states the boundary of the fix: the exemplars are
/// what the fill must be given.
#[test]
fn a_source_without_exemplars_has_no_envelope_to_apply() {
    let doc = doc_from(big_subtraction_body());
    let source = TemplateSource::new("big-subtraction", &doc).expect("the fixture compiles");

    let batch = source
        .fill("big-subtraction", 24, 1459)
        .expect("the template fills");

    assert!(batch.refusals().is_empty());
    assert_eq!(source.exemplars().len(), 0);
    assert_eq!(
        batch
            .instances()
            .iter()
            .filter(|instance| instance.answer == "-1")
            .count(),
        1,
        "with no envelope the negative instance stays in the batch"
    );
}

/// The refusal counters of one batch.
#[test]
fn a_batch_reports_its_refusal_rate() {
    let doc = doc_from(big_subtraction_body());
    let exemplars = big_subtraction_exemplars();
    let source = TemplateSource::new("big-subtraction", &doc)
        .expect("the fixture compiles")
        .with_exemplars(&exemplars);

    let batch = source
        .fill("big-subtraction", 24, 1459)
        .expect("the template fills");
    assert_eq!(batch.checked(), 25, "24 kept and 1 refused");
    assert_eq!(batch.refusal_percent(), 4, "1 of 25 is 4 percent");
    assert!(
        !batch.is_flagged(),
        "4 percent is under the 10 percent limit"
    );
    assert_eq!(REFUSAL_FLAG_PERCENT, 10);

    // A batch of 4 kept and 1 refused is 20 percent, which is above the limit.
    let flagged = Batch::new(
        batch.instances()[..4].to_vec(),
        batch.refusals()[..1].to_vec(),
    );
    assert_eq!(flagged.checked(), 5);
    assert_eq!(flagged.refusal_percent(), 20);
    assert!(flagged.is_flagged());

    let empty = Batch::default();
    assert_eq!(empty.checked(), 0);
    assert_eq!(empty.refusal_percent(), 0);
    assert!(!empty.is_flagged());
}

/// `check_instance` writes the gate's own message for each per-instance rule.
#[test]
fn check_instance_refuses_a_negative_answer_with_the_gate_message() {
    let doc = doc_from(big_subtraction_body());
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    let exemplars = big_subtraction_exemplars();
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &exemplars,
    };

    let mut bindings = Bindings::new();
    bindings.insert("a".to_string(), Scalar::Int(100).value());
    bindings.insert("b".to_string(), Scalar::Int(100).value());
    let instance = compiled.instantiate(bindings).expect("it instantiates");
    assert_eq!(instance.text, "Compute $9999 - 100 \\times 100$.");
    assert_eq!(instance.answer, "-1");

    let refusal = check_instance(&doc, &spec, &instance).expect_err("the envelope refuses it");
    assert_eq!(refusal.code, "envelope-sign");
    assert_eq!(
        refusal.message,
        "instance {'a': 100, 'b': 100} answers '-1', but every authored answer for this knowledge \
         point is non-negative — narrow the domains so no instance goes below zero"
    );

    // The same instance with no exemplar passes: there is no envelope to read.
    let bare = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &[],
    };
    assert!(check_instance(&doc, &bare, &instance).is_ok());
}

/// A hint rung that names the answer of ONE instance refuses that instance.
#[test]
fn check_instance_refuses_a_hint_that_names_the_answer() {
    let body = r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "What is the square of the number after ${a}$?",
      "params": {"a": {"kind": "int", "low": 1, "high": 12}},
      "answer_expr": "(a + 1)**2",
      "hints": ["Add 1 to get 4, then square it."]
    }"#;
    let doc = doc_from(body);
    let compiled = Compiled::new(&doc).expect("the fixture compiles");
    let spec = GateSpec {
        answer_kind: AnswerKind::Numeric,
        exemplars: &[],
    };

    // a = 1 answers 4, and the rung reads `4`.
    let refused = compiled
        .instantiate(bind_int("a", 1))
        .expect("it instantiates");
    assert_eq!(refused.answer, "4");
    let refusal = check_instance(&doc, &spec, &refused).expect_err("the hint gives the answer");
    assert_eq!(refusal.code, "hint-answer");
    assert_eq!(
        refusal.message,
        "hint 0 reads 'Add 1 to get 4, then square it.' for {'a': 1}, which names the answer '4' \
         — a hint is a question, never the final step (Hard Rule 3)"
    );

    // a = 2 answers 9, which the rung does not name, so the instance passes.
    let kept = compiled
        .instantiate(bind_int("a", 2))
        .expect("it instantiates");
    assert_eq!(kept.answer, "9");
    assert!(check_instance(&doc, &spec, &kept).is_ok());
}

/// The reviewer's adjacent-parameter document (M4 review 2, finding 1).
///
/// The statement writes the two numbers next to each other, so `a = 1, b = 12`
/// and `a = 11, b = 2` render ONE statement, `$112$`, and the product of the two
/// tuples is 12 and 22. The gate refuses the document; this fixture is the
/// document reaching the fill anyway, which is what a hand-written body or a
/// gate defect gives the refill job.
fn adjacent_product_body() -> &'static str {
    r#"{
      "v": 1,
      "topic_id": "two-digit-codes",
      "answer_kind": "numeric",
      "statement": "A code is written as ${a}{b}$. What is the product of the two numbers?",
      "params": {"a": {"kind": "int", "low": 1, "high": 12},
                 "b": {"kind": "int", "low": 1, "high": 12}},
      "answer_expr": "a * b",
      "solution_sketch": "Read the two numbers apart and multiply them.",
      "hints": ["Which two numbers were written down?"],
      "samples": [{"params": {"a": 1, "b": 1}, "expected": "1"},
                  {"params": {"a": 12, "b": 12}, "expected": "144"}]
    }"#
}

/// M4 review 2, finding 1: one statement carries one answer, in the fill too.
///
/// `serving_pool` keys a row by `instance_hash`, so two tuples that render one
/// statement give ONE row. The fill kept whichever tuple it met first and threw
/// the other away in silence, so a learner read `$112$` and the row answered 22
/// while the learner's own reading of the code answered 12 (C4).
///
/// The 144 tuples render 142 distinct statements. `$111$` comes from `a = 1,
/// b = 11` and from `a = 11, b = 1`, and both tuples answer 11, so the fill
/// keeps one row and counts nothing. `$112$` comes from `a = 1, b = 12` and from
/// `a = 11, b = 2`, and the two answers differ, so the second tuple is a refusal
/// the batch reports.
#[test]
fn a_statement_with_a_second_answer_is_refused_by_the_fill_and_counted() {
    let doc = doc_from(adjacent_product_body());
    let source = TemplateSource::new("two-digit-codes", &doc).expect("the source compiles");
    let filled = source
        .fill("two-digit-codes", 200, 0)
        .expect("the fill runs");

    assert_eq!(filled.instances().len(), 142);
    assert_eq!(filled.refusals().len(), 1);
    assert_eq!(filled.checked(), 143);

    let refused = &filled.refusals()[0];
    assert_eq!(refused.code, "statement-collision");
    assert_eq!(
        refused.text.as_deref(),
        Some("A code is written as $112$. What is the product of the two numbers?")
    );
    assert_eq!(
        refused.message,
        "statement 'A code is written as $112$. What is the product of the two numbers?' already answers '22' and this tuple answers '12' — one statement carries one answer"
    );
    assert_eq!(refused.bindings, bind_two("a", 1, "b", 12));

    // The instance the batch kept is the one the digest names, and no instance
    // of the batch carries the refused answer.
    let colliding: Vec<&Instance> = filled
        .instances()
        .iter()
        .filter(|instance| {
            instance.text == "A code is written as $112$. What is the product of the two numbers?"
        })
        .collect();
    assert_eq!(colliding.len(), 1);
    assert_eq!(colliding[0].answer, "22");

    // One statement is one digest, so the pool insert of U4 would have dropped
    // the refused row on its unique index and kept no record of it.
    assert_eq!(
        problem_text_hash("A code is written as $112$. What is the product of the two numbers?"),
        colliding[0].instance_hash
    );

    // One refusal in 143 candidates is under the flag rate.
    assert_eq!(filled.refusal_percent(), 0);
    assert!(!filled.is_flagged());
}

/// Two tuples with one statement and ONE answer are still one row, in silence.
///
/// The rule reads the answers. `$111$` renders from two tuples that both answer
/// 11, so the batch holds one instance for them and counts no refusal.
#[test]
fn a_repeated_statement_with_one_answer_is_kept_once_and_not_counted() {
    let doc = doc_from(adjacent_product_body());
    let source = TemplateSource::new("two-digit-codes", &doc).expect("the source compiles");
    let filled = source
        .fill("two-digit-codes", 200, 0)
        .expect("the fill runs");
    let repeated: Vec<&Instance> = filled
        .instances()
        .iter()
        .filter(|instance| {
            instance.text == "A code is written as $111$. What is the product of the two numbers?"
        })
        .collect();
    assert_eq!(repeated.len(), 1);
    assert_eq!(repeated[0].answer, "11");
    assert!(
        filled
            .refusals()
            .iter()
            .all(|refused| refused.text.as_deref()
                != Some("A code is written as $111$. What is the product of the two numbers?")),
        "a statement with one answer is never a refusal"
    );
}
