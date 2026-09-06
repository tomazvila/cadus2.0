//! Generated local scaffolds retain exact proofs and production gate boundaries.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_core::instruction::ServedInstance;
use cadus_worker::authoring::{completion::generate, job::verify_kind, prompt::Kind};

#[test]
fn exact_square_variants_keep_worked_and_practice_operands_separate() {
    let spec = common::squares_spec();
    let out = generate(&spec, &[]);
    assert!(!out.solutions.is_empty());
    let teach = out
        .drafts
        .iter()
        .find(|row| row["kind"] == "teach")
        .unwrap();
    assert_ne!(
        teach["arguments"]["worked_example"]["problem"],
        spec.exemplars[0].problem
    );
    for row in &out.drafts {
        let kind = Kind::from_wire(row["kind"].as_str().unwrap()).unwrap();
        verify_kind(kind, &spec, &row["arguments"], &[]).unwrap();
    }
    let template = out
        .drafts
        .iter()
        .find(|row| row["kind"] == "template")
        .unwrap();
    let body = verify_kind(Kind::Template, &spec, &template["arguments"], &[]).unwrap();
    let served = cadus_core::instruction::template_instances(&body);
    verify_kind(Kind::Teach, &spec, &teach["arguments"], &served).unwrap();
}

#[test]
fn word_problem_models_are_never_guessed_and_hint_leaks_remain_refused() {
    let mut spec = common::squares_spec();
    spec.exemplars[0].problem = "A rectangle has unknown sides. What is its area?".to_owned();
    let out = generate(
        &spec,
        &[ServedInstance {
            problem: "A served problem".to_owned(),
            answer: "grouped".to_owned(),
        }],
    );
    assert!(out.solutions.is_empty());
    assert!(out.assessment.is_empty());
    assert!(
        !out.drafts
            .iter()
            .any(|row| row["kind"] == "teach" || row["kind"] == "template")
    );
    assert!(
        out.refusals
            .iter()
            .any(|reason| reason.contains("hint-answer"))
    );
}
