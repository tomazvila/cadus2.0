//! M4 U3 acceptance, part 3: the exemplar source and the template source
//! (A1, A6, A7).
//!
//! Every statement, digest, wire tag, and refusal message below is a literal
//! from `docs/reference/serving-1.0-spec.md` sections 6 and 9, or a SHA-1 digest
//! worked out by hand from the statement.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::model::{Exemplar, KnowledgePoint, Slug};
use cadus_core::pool::{
    ExemplarSource, FillError, PoolCounters, ProblemSource, Ring, Source, TaskMemory,
    TemplateSource, serve,
};
use common::pool_fixtures::{
    SQUARE_STATEMENTS, doc_from, exemplar, exemplar_fixture, perfect_squares_body,
};

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
fn twenty_exemplars_cover_the_ring_and_none_is_empty() {
    let twenty: Vec<Exemplar> = (0..20)
        .map(|index| exemplar(&format!("Problem {index}."), "1"))
        .collect();
    let full = ExemplarSource::new("adding-two-digits", &twenty);
    assert_eq!(full.len(), 20);
    assert!(full.covers_ring(), "20 exemplars fill a ring of 20");

    let none = ExemplarSource::new("adding-two-digits", &[]);
    assert!(none.is_empty());
    assert_eq!(none.len(), 0);
}

#[test]
fn an_exemplar_source_reads_a_knowledge_point() {
    let kp = KnowledgePoint {
        id: Slug::new("adding-two-digits").unwrap(),
        name: "Add two two-digit numbers".to_string(),
        key_prerequisites: Vec::new(),
        exemplars: exemplar_fixture(),
        constraints: None,
        finite_objective_domain: None,
        visuals: Vec::new(),
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
        exemplar("Compute 1 + 1.", "2"),
        exemplar("Compute 1 + 1.", "2"),
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
        exemplar(
            "Prove that the sum of two even numbers is even.",
            "See the write-up.",
        ),
        exemplar("Compute $3 + 4$.", "7"),
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

#[test]
fn an_exemplar_fill_of_zero_instances_returns_an_empty_batch() {
    let exemplars = exemplar_fixture();
    let source = ExemplarSource::new("adding-two-digits", &exemplars);
    let batch = source
        .fill("adding-two-digits", 0, 0)
        .expect("a fill of zero runs");
    assert!(batch.is_empty());
    assert!(batch.refusals().is_empty());
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

/// The exemplar source refuses a knowledge point it does not fill, the same as
/// the template source.
#[test]
fn an_exemplar_source_refuses_a_knowledge_point_it_does_not_fill() {
    let exemplars = exemplar_fixture();
    let source = ExemplarSource::new("adding-two-digits", &exemplars);
    let refusal = source.fill("subtraction", 4, 0).expect_err("refuses");
    assert_eq!(
        refusal,
        FillError::UnknownKp {
            have: "adding-two-digits".to_string(),
            want: "subtraction".to_string(),
        }
    );
}

/// A document that does not compile is no source, and a constraint the draw
/// cannot decide refuses the fill.
#[test]
fn a_document_the_instantiator_refuses_is_no_source() {
    let uncompilable = doc_from(
        r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}$.",
      "params": {"a": {"kind": "int", "low": 1, "high": 12}},
      "answer_expr": "a +",
      "hints": ["Read the statement again."]
    }"#,
    );
    assert!(TemplateSource::new("perfect-squares", &uncompilable).is_err());

    let undecidable = doc_from(
        r#"{
      "v": 1,
      "topic_id": "perfect-squares",
      "answer_kind": "numeric",
      "statement": "Compute ${a}$ {b}.",
      "params": {"a": {"kind": "int", "low": 1, "high": 12},
                 "b": {"kind": "choice", "values": ["\\times", "+"]}},
      "constraints": [{"op": "gt", "left": "a", "right": "b"}],
      "answer_expr": "a",
      "hints": ["Read the statement again."]
    }"#,
    );
    let source = TemplateSource::new("perfect-squares", &undecidable).expect("the source compiles");
    let refusal = source.fill("perfect-squares", 3, 0).expect_err("refuses");
    assert!(
        matches!(refusal, FillError::Instantiate(_)),
        "it gave {refusal:?}"
    );
}
