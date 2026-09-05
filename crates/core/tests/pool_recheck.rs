//! M4 U3 acceptance, part 4: the per-instance re-check of the fill
//! (C4, C6; review round 1, findings #1, #2, #15; review round 2, finding 1).
//!
//! Every statement, tuple, count, and refusal message below is a literal from
//! the two review reports. No expected value is read back from the code under
//! test.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

mod common;

use cadus_core::curriculum::AnswerKind;
use cadus_core::curriculum::model::Exemplar;
use cadus_core::learner::problem_text_hash;
use cadus_core::pool::{
    Batch, ProblemSource, REFUSAL_FLAG_PERCENT, TemplateSource, check_instance,
};
use cadus_core::template::{Bindings, Compiled, GateSpec, Instance, Scalar, gate};
use cadus_testkit::fixtures::BIG_SUBTRACTION_BODY;
use common::pool_fixtures::{bind_int, bind_two, doc_from, exemplar};

/// The two authored exemplars of that knowledge point.
///
/// Both answers are non-negative whole numbers, so the envelope is
/// `{non_negative: true, integral: true}`.
fn big_subtraction_exemplars() -> Vec<Exemplar> {
    vec![
        exemplar("Compute $9999 - 1 \\times 12$.", "9987"),
        exemplar("Compute $9999 - 10 \\times 12$.", "9879"),
    ]
}

/// C4: the fill refuses `a = 100, b = 100`, and no pool row ever carries it.
///
/// Seed 1459 is the seed the review round 1 report names: at that seed the batch
/// draws the one violating tuple of the 10,000. Every number below is a literal.
#[test]
fn the_fill_refuses_the_instance_the_gates_sample_never_read() {
    let doc = doc_from(BIG_SUBTRACTION_BODY);
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
    let doc = doc_from(BIG_SUBTRACTION_BODY);
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
    let doc = doc_from(BIG_SUBTRACTION_BODY);
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

    // A batch of 9 kept and 1 refused is exactly 10 percent, which is not above
    // the limit.
    let at_limit = Batch::new(
        batch.instances()[..9].to_vec(),
        batch.refusals()[..1].to_vec(),
    );
    assert_eq!(at_limit.refusal_percent(), 10);
    assert!(!at_limit.is_flagged());

    let empty = Batch::default();
    assert_eq!(empty.checked(), 0);
    assert_eq!(empty.refusal_percent(), 0);
    assert!(!empty.is_flagged());
}

/// `check_instance` writes the gate's own message for each per-instance rule.
#[test]
fn check_instance_refuses_a_negative_answer_with_the_gate_message() {
    let doc = doc_from(BIG_SUBTRACTION_BODY);
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

/// The batch of 200 asked-for instances of the adjacent-parameter document.
fn adjacent_fill() -> Batch {
    let doc = doc_from(adjacent_product_body());
    let source = TemplateSource::new("two-digit-codes", &doc).expect("the source compiles");
    source
        .fill("two-digit-codes", 200, 0)
        .expect("the fill runs")
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
    let filled = adjacent_fill();

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
    let filled = adjacent_fill();
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
