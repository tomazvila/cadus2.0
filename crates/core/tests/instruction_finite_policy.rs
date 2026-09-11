//! Trusted teaching roles allow rehearsal without exposing fresh/reserved cases.
#![allow(clippy::unwrap_used)]
use cadus_core::curriculum::{
    Exemplar, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
};
use cadus_core::instruction::{
    InstructionSpec, ServedInstance, gate_hint_ladder, gate_teach, gate_teach_with_policy,
    regate_with_policy,
};

fn policy() -> FiniteObjectiveDomain {
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:reviewed-finite-instruction-fixture".into(),
        cases: [
            ("fresh", FiniteCaseRole::PracticeFresh, "Compute 7+1.", "8"),
            ("teach", FiniteCaseRole::TeachOnly, "Compute 2+3.", "5"),
            (
                "reserved",
                FiniteCaseRole::ReservedAssessment,
                "Compute 8+1.",
                "9",
            ),
            (
                "rehearsal",
                FiniteCaseRole::TaughtRehearsal,
                "Compute 3+3.",
                "6",
            ),
        ]
        .into_iter()
        .map(|(id, role, problem, answer)| FiniteObjectiveCase {
            id: Slug::new(id).unwrap(),
            role,
            variants: vec![FiniteCaseVariant {
                problem: problem.into(),
                answer: answer.into(),
                answer_contract: None,
            }],
        })
        .collect(),
    }
}
fn body(problem: &str, final_step: &str) -> String {
    serde_json::json!({"concept":"Add the two quantities.","worked_example":{"problem":problem,"steps":["Combine the quantities.",final_step]}}).to_string()
}
fn exemplars(policy: &FiniteObjectiveDomain) -> Vec<Exemplar> {
    policy
        .cases
        .iter()
        .flat_map(|case| case.variants.iter())
        .map(|variant| Exemplar {
            problem: variant.problem.clone(),
            answer: variant.answer.clone(),
            answer_contract: None,
            solution_sketch: None,
        })
        .collect()
}

#[test]
fn only_explicit_teaching_and_rehearsal_roles_allow_the_worked_collision() {
    let policy = policy();
    let exemplars = exemplars(&policy);
    let spec = InstructionSpec {
        exemplars: &exemplars,
        instance_answers: vec![],
    };
    for (problem, answer) in [("Compute 2+3.", "5"), ("Compute 3+3.", "6")] {
        let page = body(problem, answer);
        assert!(gate_teach(&page, &spec).is_err());
        assert!(gate_teach_with_policy(&page, &spec, Some(&policy)).is_ok());
        assert!(regate_with_policy("teach", &page, &spec, Some(&policy)).is_none());
    }
    for problem in ["Compute 7+1.", "Compute 8+1."] {
        assert_eq!(
            gate_teach_with_policy(&body(problem, "The sum follows."), &spec, Some(&policy))
                .unwrap_err()
                .code,
            "teach-finite-role"
        );
    }
    assert_eq!(
        gate_teach_with_policy(&body("Compute 4+1.", "5"), &spec, Some(&policy))
            .unwrap_err()
            .code,
        "teach-finite-case"
    );
}

#[test]
fn teaching_one_case_does_not_allow_disclosing_a_different_case_or_hint_answer() {
    let policy = policy();
    let exemplars = exemplars(&policy);
    let spec = InstructionSpec {
        exemplars: &exemplars,
        instance_answers: vec![ServedInstance {
            problem: "Compute 7+1.".into(),
            answer: "8".into(),
        }],
    };
    let page = body("Compute 2+3.", "Also, 7+1 = 8.");
    assert_eq!(
        gate_teach_with_policy(&page, &spec, Some(&policy))
            .unwrap_err()
            .code,
        "teach-answer"
    );
    let hint = r#"{"hints":["The answer is 6."]}"#;
    assert!(gate_hint_ladder(hint, &spec).is_err());
    assert!(regate_with_policy("hint_ladder", hint, &spec, Some(&policy)).is_some());
}

#[test]
fn reviewed_aliases_share_the_teaching_exception_but_ambiguous_catalogs_fail() {
    let mut policy = policy();
    policy.cases[3].variants.push(FiniteCaseVariant {
        problem: "Evaluate 3+3.".into(),
        answer: "6".into(),
        answer_contract: None,
    });
    let exemplars = exemplars(&policy);
    let spec = InstructionSpec {
        exemplars: &exemplars,
        instance_answers: vec![],
    };
    assert!(
        gate_teach_with_policy(&body("Evaluate 3+3.", "3+3 = 6."), &spec, Some(&policy)).is_ok()
    );
    policy.cases[1].variants.push(FiniteCaseVariant {
        problem: "Evaluate 3+3.".into(),
        answer: "7".into(),
        answer_contract: None,
    });
    assert_eq!(
        gate_teach_with_policy(&body("Evaluate 3+3.", "6"), &spec, Some(&policy))
            .unwrap_err()
            .code,
        "finite-policy"
    );
}

#[test]
fn one_case_cannot_assign_conflicting_answers_to_the_same_worked_problem() {
    let mut policy = policy();
    let original = policy.cases[1].variants[0].clone();
    policy.cases[1].variants.push(FiniteCaseVariant {
        problem: format!(" {} ", original.problem),
        answer: "6".into(),
        answer_contract: None,
    });
    assert!(policy.validate().is_err());
    let spec = InstructionSpec {
        exemplars: &[],
        instance_answers: vec![],
    };
    assert_eq!(
        gate_teach_with_policy(&body(&original.problem, "5"), &spec, Some(&policy))
            .unwrap_err()
            .code,
        "finite-policy"
    );
}
