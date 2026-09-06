//! Preserve the equation-translation objective in each authored exemplar.
#![allow(clippy::unwrap_used)]

#[path = "common/unit03_translation.rs"]
mod semantic;

use cadus_core::{
    answer::{Outcome, check_contract},
    curriculum::load_curriculum,
};
use std::{collections::BTreeSet, path::Path};

#[test]
fn visible_sentences_reconstruct_six_distinct_unsolved_equations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../curriculum");
    let (curriculum, findings) = load_curriculum(&root).unwrap();
    assert!(findings.is_empty());
    let topic = curriculum
        .topics_in_course("foundations")
        .iter()
        .filter_map(|index| curriculum.topic(*index))
        .find(|topic| topic.id.as_str() == "translating-sentences-to-equations")
        .unwrap();
    let kp = topic
        .knowledge_points
        .iter()
        .find(|kp| kp.id.as_str() == "kp2")
        .unwrap();
    assert_eq!(kp.exemplars.len(), 6);
    let mut answers = BTreeSet::new();
    let altered_forms = [
        ("n = 7", "3*n = 21"),
        ("y = 48", "y = 12*4"),
        ("x = 4", "4*x + 28 = 44"),
        ("x = 33", "x - 27 = 6"),
        ("x = 6", "2*x = 12"),
        ("x = 26", "x - 5 = 21"),
    ];
    for (exemplar, (solved, rearranged)) in kp.exemplars.iter().zip(altered_forms) {
        let (expected, wrong_order) = semantic::reconstruct(&exemplar.problem);
        assert_eq!(exemplar.answer, expected);
        assert!(answers.insert(expected.clone()));
        assert!(exemplar.solution_sketch.as_ref().unwrap().len() > 40);
        let contract = exemplar.answer_contract.clone().unwrap();
        for correct in [
            expected.clone(),
            expected.replace(' ', ""),
            reverse_commutative_sum(&expected),
        ] {
            assert!(
                matches!(check_contract(&expected, &correct, contract.clone()),
                Outcome::Decided(v) if v.correct)
            );
        }
        let left = expected.split(" = ").next().unwrap();
        for wrong in [
            wrong_order,
            format!("{left} = 999"),
            left.to_owned(),
            "x = 7".to_owned(),
            "7".to_owned(),
            "yes".to_owned(),
            solved.to_owned(),
            rearranged.to_owned(),
        ] {
            assert!(
                !accepted(&expected, &wrong, contract.clone()),
                "{expected}: {wrong}"
            );
        }
    }
}

fn accepted(expected: &str, learner: &str, contract: cadus_core::answer::AnswerContract) -> bool {
    matches!(check_contract(expected, learner, contract), Outcome::Decided(v) if v.correct)
}

fn reverse_commutative_sum(equation: &str) -> String {
    if equation == "3*n + 5 = 26" {
        "5 + 3*n = 26".to_owned()
    } else {
        equation.to_owned()
    }
}

#[test]
fn relation_form_preserves_reviewed_unsolved_structure_and_refuses_broad_use() {
    use cadus_core::{
        answer::AnswerContract,
        template::{Bindings, Scalar, answer_for_contract, parse_answer_expr},
    };
    let bindings: Bindings = [
        ("a".to_owned(), Scalar::Int(3).value()),
        ("b".to_owned(), Scalar::Int(5).value()),
        ("c".to_owned(), Scalar::Int(26).value()),
    ]
    .into_iter()
    .collect();
    let contract: AnswerContract = serde_json::from_value(serde_json::json!({
        "kind":"relation_setup"
    }))
    .unwrap();
    let ast = parse_answer_expr("relationform((a*x+b,c),0)").unwrap();
    assert_eq!(
        answer_for_contract(&ast, &bindings, Some(&contract))
            .unwrap()
            .text,
        "3*x + 5 = 26"
    );
    let wrong_shape = parse_answer_expr("relationform((a*(x+b),c),0)").unwrap();
    let wrong = answer_for_contract(&wrong_shape, &bindings, Some(&contract)).unwrap();
    assert!(!accepted("3*x + 5 = 26", &wrong.text, contract.clone()));
    let numeric = parse_answer_expr("relationform((a+b,c),0)").unwrap();
    assert!(answer_for_contract(&numeric, &bindings, Some(&contract)).is_err());
    let inequality = parse_answer_expr("relationform((a*x+b,c),-1)").unwrap();
    assert_eq!(
        answer_for_contract(&inequality, &bindings, Some(&contract))
            .unwrap()
            .text,
        "3*x + 5 <= 26"
    );
    let unreviewed = parse_answer_expr("relationform((a*x+b,c),0)").unwrap();
    assert!(answer_for_contract(&unreviewed, &bindings, Some(&AnswerContract::Exact)).is_err());
    for bad in [
        "relationform((a*x+b,c),3)",
        "relationform((a*x+b,c),a/2)",
        "relationform((a*x+b,x),0)",
        "relationform((a*x+b),0)",
    ] {
        let ast = parse_answer_expr(bad).unwrap();
        assert!(
            answer_for_contract(&ast, &bindings, Some(&contract)).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn relation_setup_accepts_notation_variants_and_rejects_changed_setups() {
    use cadus_core::answer::AnswerContract;
    let contract: AnswerContract = serde_json::from_value(serde_json::json!({
        "kind":"relation_setup"
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(&contract).unwrap(),
        serde_json::json!({"kind":"relation_setup"})
    );
    for (expected, equivalent) in [
        ("3*x + 5 = 26", "5 + 3x=26"),
        ("x/4 - 7 = 9", "x / 4 - 7=9"),
        ("2*x + 5 <= 17", "5+2x<=17"),
    ] {
        assert!(accepted(expected, equivalent, contract.clone()));
    }
    for wrong in [
        "x = 7",
        "3*x = 21",
        "3*(x + 5) = 26",
        "3*y + 5 = 26",
        "3*x + 5 < 26",
        "26 = 3*x + 5",
        "3*x + 5",
        "3*x + 5 = 26 = 26",
    ] {
        assert!(
            !accepted("3*x + 5 = 26", wrong, contract.clone()),
            "{wrong}"
        );
    }
    let oversized = format!("{}x = 1", "1+".repeat(300));
    assert!(!accepted("x = 1", &oversized, contract));
}

#[test]
fn relation_setup_is_additive_to_existing_contract_serialization() {
    use cadus_core::answer::AnswerContract;
    for json in [
        serde_json::json!({"kind":"exact"}),
        serde_json::json!({"kind":"polynomial_relation"}),
        serde_json::json!({"kind":"inequality_union"}),
        serde_json::json!({"kind":"label","options":[["yes"],["no"]]}),
    ] {
        let contract: AnswerContract = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(contract).unwrap(), json);
    }
}
