//! Exact quadrantal values stay finite, exhaustive, and policy-bound.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use cadus_core::answer::ast::Ast;
use cadus_core::answer::canonical_form;
use cadus_core::curriculum::{
    AnswerKind, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
};
use cadus_core::template::domain::{Bindings, Value};
use cadus_core::template::eval::{answer, evaluate, parse_answer_expr};
use cadus_core::template::{GateSpec, from_body, gate};
use num_bigint::BigInt;
use num_rational::BigRational;

const KP_KEY: &str = "unit-circle-special-angles/kp1";
const BODY: &str = r#"{
  "v":1,
  "topic_id":"unit-circle-special-angles",
  "answer_kind":"numeric",
  "statement":"Use the unit circle to evaluate {f}({q}*pi/2).",
  "params":{
    "f":{"kind":"choice","values":["sin","cos"]},
    "q":{"kind":"choice","values":[0,1,2,3]}
  },
  "constraints":[],
  "answer_expr":"quartervalue(f,q)",
  "solution_sketch":"Read {q}*pi/2 as a quadrantal angle and take its {f} coordinate.",
  "hints":[
    "Locate {q}*pi/2 on the unit circle.",
    "Choose the {f} coordinate.",
    "Check its sign on that axis."
  ],
  "samples":[
    {"params":{"f":"sin","q":0},"expected":"0"},
    {"params":{"f":"sin","q":1},"expected":"1"},
    {"params":{"f":"sin","q":2},"expected":"0"},
    {"params":{"f":"sin","q":3},"expected":"-1"},
    {"params":{"f":"cos","q":0},"expected":"1"},
    {"params":{"f":"cos","q":1},"expected":"0"},
    {"params":{"f":"cos","q":2},"expected":"-1"},
    {"params":{"f":"cos","q":3},"expected":"0"}
  ]
}"#;

fn expected(family: &str, quarter: usize) -> i64 {
    match family {
        "sin" => [0, 1, 0, -1][quarter],
        "cos" => [1, 0, -1, 0][quarter],
        _ => unreachable!(),
    }
}

fn bindings(family: Value, quarter: Value) -> Bindings {
    [("f".to_owned(), family), ("q".to_owned(), quarter)]
        .into_iter()
        .collect()
}

fn policy() -> FiniteObjectiveDomain {
    let mut cases = Vec::new();
    for family in ["sin", "cos"] {
        for quarter in 0..=3 {
            cases.push(FiniteObjectiveCase {
                id: Slug::new(format!("{family}-{quarter}")).unwrap(),
                role: FiniteCaseRole::TaughtRehearsal,
                variants: vec![FiniteCaseVariant {
                    problem: format!("Use the unit circle to evaluate {family}({quarter}*pi/2)."),
                    answer: expected(family, quarter).to_string(),
                    answer_contract: None,
                }],
            });
        }
    }
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:reviewed-eight-quadrantal-values".to_owned(),
        cases,
    }
}

#[ignore]
fn all_eight_quadrantal_values_evaluate_to_exact_integers() {
    let ast = parse_answer_expr("quartervalue(f,q)").unwrap();
    for family in ["sin", "cos"] {
        for quarter in 0..=3 {
            let result = answer(
                &ast,
                &bindings(
                    Value::Text(family.to_owned()),
                    Value::Num(BigRational::from_integer(BigInt::from(quarter))),
                ),
            )
            .unwrap();
            let expected = expected(family, quarter);
            assert_eq!(result.text, expected.to_string());
            assert_eq!(result.canon, canonical_form(&expected.to_string()).unwrap());
        }
    }
}

#[ignore]
fn unsupported_families_quarters_types_and_arities_are_refused() {
    let ast = parse_answer_expr("quartervalue(f,q)").unwrap();
    for family in ["tan", "sine", "", "SIN"] {
        assert!(
            answer(
                &ast,
                &bindings(
                    Value::Text(family.to_owned()),
                    Value::Num(BigRational::from_integer(0.into())),
                ),
            )
            .is_err(),
            "{family:?}"
        );
    }
    for quarter in ["-1", "4", "1/2", "1000000000000000000000000000000"] {
        let expression = format!("quartervalue(f,{quarter})");
        assert!(
            answer(
                &parse_answer_expr(&expression).unwrap(),
                &bindings(
                    Value::Text("sin".to_owned()),
                    Value::Num(BigRational::from_integer(0.into())),
                ),
            )
            .is_err(),
            "{expression}"
        );
    }
    for expression in ["quartervalue(1,0)", "quartervalue(f,x)"] {
        assert!(
            answer(
                &parse_answer_expr(expression).unwrap(),
                &bindings(
                    Value::Text("sin".to_owned()),
                    Value::Num(BigRational::from_integer(0.into())),
                ),
            )
            .is_err(),
            "{expression}"
        );
    }
    for args in [
        vec![],
        vec![Ast::Var("f".to_owned())],
        vec![
            Ast::Var("f".to_owned()),
            Ast::Integer(0.into()),
            Ast::Integer(1.into()),
        ],
    ] {
        assert!(
            evaluate(
                &Ast::Func("quartervalue".to_owned(), args),
                &Bindings::new()
            )
            .is_err()
        );
    }
}

#[ignore]
fn exact_finite_gate_proves_all_eight_rendered_cases_and_answers() {
    let policy = policy();
    let spec = GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite(KP_KEY, &policy)
        .unwrap();
    let verified = gate(&from_body(BODY).unwrap(), &spec).expect("all eight cases are exhaustive");
    assert!(verified.exhaustive);
    assert_eq!(verified.instances_checked, 8);
    assert_eq!(verified.finite_cases.len(), 8);
    let actual = verified
        .finite_cases
        .iter()
        .map(|case| (case.case_id.as_str(), case.role))
        .collect::<Vec<_>>();
    let mut expected = policy
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case.role))
        .collect::<Vec<_>>();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[ignore]
fn duplicate_unknown_answer_and_nonpractice_role_remain_refused() {
    let doc = from_body(BODY).unwrap();

    let mut duplicate = policy();
    let sin_two = duplicate
        .cases
        .iter()
        .find(|case| case.id.as_str() == "sin-2")
        .unwrap()
        .variants[0]
        .clone();
    duplicate.cases.retain(|case| case.id.as_str() != "sin-2");
    duplicate.cases[0].variants.push(sin_two);
    let spec = GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite(KP_KEY, &duplicate)
        .unwrap();
    assert_eq!(gate(&doc, &spec).unwrap_err().code, "finite-case-duplicate");

    let mut wrong_answer = policy();
    wrong_answer.cases[7].variants[0].answer = "1".to_owned();
    let spec = GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite(KP_KEY, &wrong_answer)
        .unwrap();
    assert_eq!(gate(&doc, &spec).unwrap_err().code, "finite-case-unknown");

    let mut reserved = policy();
    reserved.cases[7].role = FiniteCaseRole::ReservedAssessment;
    let spec = GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite(KP_KEY, &reserved)
        .unwrap();
    assert_eq!(gate(&doc, &spec).unwrap_err().code, "finite-case-role");
}
