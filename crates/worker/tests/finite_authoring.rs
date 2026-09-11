//! Finite authoring specs bind prompts and production gates to reviewed policy.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use cadus_core::curriculum::{
    AnswerKind, Exemplar, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase,
    FiniteObjectiveDomain, Slug,
};
use cadus_worker::authoring::{
    job::{verify, verify_hint_ladder, verify_teach},
    prompt::{AuthoringSpec, FiniteAuthoringPolicy, Kind, user_message},
};
use serde_json::{Value, json};

fn policy() -> FiniteObjectiveDomain {
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:reviewed-finite-index".to_owned(),
        cases: (1..=3)
            .map(|value| FiniteObjectiveCase {
                id: Slug::new(format!("case-{value}")).unwrap(),
                role: FiniteCaseRole::PracticeFresh,
                variants: vec![FiniteCaseVariant {
                    problem: format!("Return {value}."),
                    answer: value.to_string(),
                    answer_contract: None,
                }],
            })
            .chain(std::iter::once(FiniteObjectiveCase {
                id: Slug::new("reserved-secret").unwrap(),
                role: FiniteCaseRole::ReservedAssessment,
                variants: vec![FiniteCaseVariant {
                    problem: "Reserved assessment secret.".to_owned(),
                    answer: "99".to_owned(),
                    answer_contract: None,
                }],
            }))
            .collect(),
    }
}

fn spec() -> AuthoringSpec {
    AuthoringSpec {
        kp_id: "kp1".to_owned(),
        kp_name: "Finite index".to_owned(),
        topic_id: "finite-index".to_owned(),
        topic_name: "Finite index".to_owned(),
        answer_kind: AnswerKind::Numeric,
        difficulty_target: None,
        constraints: None,
        exemplars: (1..=3)
            .map(|value| Exemplar {
                problem: format!("Return {value}."),
                answer: value.to_string(),
                answer_contract: None,
                solution_sketch: None,
            })
            .chain(std::iter::once(Exemplar {
                problem: "Reserved assessment secret.".to_owned(),
                answer: "99".to_owned(),
                answer_contract: None,
                solution_sketch: None,
            }))
            .collect(),
        finite: Some(FiniteAuthoringPolicy::new("finite-index/kp1", &policy()).unwrap()),
    }
}

fn arguments(values: &[i64]) -> Value {
    let samples: Vec<Value> = values
        .iter()
        .map(|value| json!({"params":{"a":value},"expected":value.to_string()}))
        .collect();
    json!({
        "statement":"Return {a}.",
        "params":{"a":{"kind":"choice","values":values}},
        "constraints":[],
        "answer_expr":"a",
        "solution_sketch":"Return the given index.",
        "hints":["Read the displayed index."],
        "distractors":[],
        "samples":samples
    })
}

#[test]
fn finite_prompt_names_roles_and_gate_requires_the_complete_reviewed_set() {
    let spec = spec();
    let prompt = user_message(Kind::Template, &spec, None);
    assert!(prompt.contains("fingerprint="));
    assert!(prompt.contains("Case case-1 role \"practice_fresh\""));
    assert!(prompt.contains("must exhaust exactly every practice_fresh"));
    assert!(!prompt.contains("Reserved assessment secret"));

    verify(&spec, &arguments(&[1, 2, 3])).expect("the complete reviewed set passes");
    let rejection = verify(&spec, &arguments(&[1, 2])).unwrap_err();
    assert_eq!(rejection.code, "finite-case-missing");
}

#[test]
fn authoring_refuses_a_stale_policy_fingerprint_before_the_template_gate() {
    let mut spec = spec();
    spec.finite.as_mut().unwrap().fingerprint.push('0');
    let rejection = verify(&spec, &arguments(&[1, 2, 3])).unwrap_err();
    assert_eq!(rejection.code, "finite-policy");
    assert!(rejection.message.contains("stale"));
}

#[test]
fn instruction_gates_refuse_a_policy_changed_after_its_fingerprint_was_bound() {
    let mut spec = spec();
    spec.finite
        .as_mut()
        .unwrap()
        .domain
        .review_ref
        .push_str("-changed");

    for rejection in [
        verify_teach(&spec, &json!({}), &[]).unwrap_err(),
        verify_hint_ladder(&spec, &json!({}), &[]).unwrap_err(),
    ] {
        assert_eq!(rejection.code, "finite-policy");
        assert!(rejection.message.contains("stale"));
    }
}
