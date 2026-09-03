//! Benchmark A, part 0: the fixture of 20 approved templates and the pinned
//! measured sequence.
//!
//! [`the_measured_sequence_is_pinned`] is the determinism guard of benchmark A,
//! and it runs without `CADUS_BENCH`: a renderer, evaluator, draw, or digest
//! regression fails the ordinary `cargo test --workspace` run, not the benchmark
//! alone. See `bench_l1.rs` for the budget table and how to run the benchmark.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::curriculum::{AnswerKind, Exemplar};
use cadus_core::learner::problem_text_hash;
use cadus_core::template::{GateSpec, from_body, gate, to_body};
use common::bench::{
    ITERATIONS, PINNED_INSTANCES, RING_TAIL_AFTER_THE_RUN, SEQUENCE_DIGEST, TEMPLATE_COUNT,
    fixtures, replay, sequence_digest,
};

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
