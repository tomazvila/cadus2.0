//! Generated local scaffolds retain exact proofs and production gate boundaries.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_core::instruction::ServedInstance;
use cadus_worker::authoring::{
    completion::{diagnosis_from_templates, generate},
    job::verify_kind,
    prompt::Kind,
};

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

#[test]
fn practice_template_excludes_a_sibling_exemplar_problem() {
    let mut spec = common::squares_spec();
    spec.topic_id = "perfect-square-roots".to_owned();
    spec.topic_name = "Perfect Squares and Their Roots".to_owned();
    spec.kp_id = "kp1".to_owned();
    spec.kp_name = "Squares of 1 through 15".to_owned();
    spec.constraints = Some("bases 1-15".to_owned());
    spec.exemplars = vec![
        cadus_core::curriculum::Exemplar {
            problem: "Compute $13^2$.".to_owned(),
            answer: "169".to_owned(),
            solution_sketch: None,
            answer_contract: None,
        },
        cadus_core::curriculum::Exemplar {
            problem: "Compute $15^2$.".to_owned(),
            answer: "225".to_owned(),
            solution_sketch: None,
            answer_contract: None,
        },
    ];

    let proposals = generate(&spec, &[]);
    let draft = proposals
        .drafts
        .iter()
        .find(|row| row["kind"] == "template")
        .unwrap();
    let body = verify_kind(Kind::Template, &spec, &draft["arguments"], &[]).unwrap();
    let instances = cadus_core::instruction::template_instances(&body);
    assert!(instances.len() >= 12);
    for instance in instances {
        assert!(
            spec.exemplars
                .iter()
                .all(|exemplar| exemplar.problem.trim() != instance.problem.trim()),
            "practice repeated sibling exemplar: {}",
            instance.problem
        );
    }
}

#[test]
fn an_explicit_template_distractor_can_become_gated_diagnosis_evidence() {
    let spec = common::squares_spec();
    let values: Vec<i64> = (10..=21).collect();
    let samples: Vec<_> = values
        .iter()
        .map(|value| serde_json::json!({"params":{"a":value},"expected":value * value}))
        .collect();
    let arguments = serde_json::json!({
        "statement":"Compute ${a}^{{2}}$.",
        "params":{"a":{"kind":"choice","values":values}},
        "constraints":[], "answer_expr":"a**2",
        "solution_sketch":"Multiply the value by itself.",
        "hints":["What does the exponent mean?"],
        "distractors":[{
            "answer":"a+1", "error_tag":"arithmetic-slip",
            "note":"Added one instead of multiplying the value by itself."
        }],
        "samples":samples,
    });
    let body = verify_kind(Kind::Template, &spec, &arguments, &[]).unwrap();
    let draft = diagnosis_from_templates(&spec, &[body]).unwrap();
    let stored = verify_kind(Kind::Diagnosis, &spec, &draft["arguments"], &[]).unwrap();
    assert!(stored.contains("arithmetic-slip"));
    assert!(stored.contains("Added one instead"));
}
