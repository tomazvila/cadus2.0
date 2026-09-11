//! Required simplest radicals retain an exact reduced rational coefficient and squarefree root.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use cadus_core::answer::{AnswerContract, AnswerPart, Outcome, check_contract};
use cadus_core::curriculum::{
    AnswerKind, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
};
use cadus_core::template::{GateSpec, from_body, gate};

const KP_KEY: &str = "tangent-special-angles/kp1";
const BODY: &str = r#"{
  "v":1,
  "topic_id":"tangent-special-angles",
  "answer_kind":"numeric",
  "answer_contract":{"kind":"required_simplest_radical"},
  "statement":"Give the exact tangent value for reviewed case {a}. Write an integer or a reduced rational times one simplified square root.",
  "params":{"a":{"kind":"choice","values":[-1,0,1]}},
  "constraints":[],
  "answer_expr":"signcase(a,[sqrt(3)/3,1,sqrt(3)])",
  "solution_sketch":"Use the reviewed special-angle ratio and rationalize any denominator.",
  "hints":["Recall sine divided by cosine.","Use the reviewed exact values.","Write a squarefree root with a reduced rational coefficient."],
  "samples":[
    {"params":{"a":-1},"expected":"sqrt(3)/3"},
    {"params":{"a":0},"expected":"1"},
    {"params":{"a":1},"expected":"sqrt(3)"}
  ]
}"#;

fn correct(expected: &str, learner: &str) {
    let outcome = check_contract(expected, learner, AnswerContract::RequiredSimplestRadical);
    assert!(
        matches!(outcome, Outcome::Decided(verdict) if verdict.correct),
        "{expected:?} vs {learner:?}: {outcome:?}"
    );
}

fn refused_or_wrong(expected: &str, learner: &str) {
    let outcome = check_contract(expected, learner, AnswerContract::RequiredSimplestRadical);
    assert!(
        !matches!(outcome, Outcome::Decided(verdict) if verdict.correct),
        "{expected:?} vs {learner:?}: {outcome:?}"
    );
}

fn policy() -> FiniteObjectiveDomain {
    let values = [
        ("tan-pi-6", -1, "sqrt(3)/3"),
        ("tan-pi-4", 0, "1"),
        ("tan-pi-3", 1, "sqrt(3)"),
    ];
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:reviewed-three-tangent-values".to_owned(),
        cases: values
            .into_iter()
            .map(|(id, a, answer)| FiniteObjectiveCase {
                id: Slug::new(id).unwrap(),
                role: FiniteCaseRole::PracticeFresh,
                variants: vec![FiniteCaseVariant {
                    problem: format!(
                        "Give the exact tangent value for reviewed case {}. Write an integer or a reduced rational times one simplified square root.",
                        if a < 0 { format!("({a})") } else { a.to_string() }
                    ),
                    answer: answer.to_owned(),
                    answer_contract: Some(AnswerContract::RequiredSimplestRadical),
                }],
            })
            .collect(),
    }
}

#[test]
fn public_checker_accepts_only_exact_simplified_numeric_radicals() {
    let policy = AnswerContract::RequiredSimplestRadical;
    assert_eq!(
        serde_json::to_string(&policy).unwrap(),
        r#"{"kind":"required_simplest_radical"}"#
    );
    assert!(
        serde_json::from_str::<AnswerContract>(r#"{"kind":"required_simplest_radical","index":2}"#)
            .is_err()
    );

    for learner in [
        "sqrt(3)/3",
        "(1/3)*sqrt(3)",
        "sqrt(3)*(1/3)",
        r"\frac{\sqrt{3}}{3}",
        r"\frac{1}{3}\sqrt{3}",
    ] {
        correct("sqrt(3)/3", learner);
    }
    for learner in ["sqrt(3)", r"\sqrt{3}", "1*sqrt(3)"] {
        if learner == "1*sqrt(3)" {
            refused_or_wrong("sqrt(3)", learner);
        } else {
            correct("sqrt(3)", learner);
        }
    }
    correct("-2*sqrt(3)/3", r"-\frac{2\sqrt{3}}{3}");
    correct("1", "1");
    correct("-3/2", r"-\frac{3}{2}");

    for learner in [
        "sqrt(12)/2",
        "2*sqrt(3)/2",
        "sqrt(3)/1",
        "sqrt(1+2)",
        "3^(1/2)",
        "sqrt(3)^1",
        "1.7320508075688772",
        "sqrt(2)",
    ] {
        refused_or_wrong("sqrt(3)", learner);
    }
    for learner in ["1/sqrt(3)", "sqrt(1/3)"] {
        refused_or_wrong("sqrt(3)/3", learner);
    }
    for learner in ["sqrt(3)+sqrt(3)", "sqrt(2)*sqrt(6)", "(1+1)*sqrt(3)"] {
        refused_or_wrong("2*sqrt(3)", learner);
    }
    refused_or_wrong("-sqrt(3)/3", "sqrt(3)/-3");
    refused_or_wrong("0", "0*sqrt(3)");

    refused_or_wrong("-1/2", "1/-2");
    refused_or_wrong("-1/2", r"\frac{1}{-2}");
    refused_or_wrong("-sqrt(3)/2", "sqrt(3)/(-2)");
    refused_or_wrong("-sqrt(3)/2", "sqrt(3)/++-2");
    refused_or_wrong("-sqrt(3)/2", "sqrt(3)/(+(+-2))");
    refused_or_wrong("-1/2", "1/++-2");
    refused_or_wrong("-1/2", "1/(+(+-2))");
    refused_or_wrong("-3*sqrt(3)/2", "sqrt(3)/(-2)*3");
    refused_or_wrong("1", "2/2");
    refused_or_wrong("1", "1.0");

    assert!(policy.validate_expected("sqrt(12)/2").is_err());
    assert!(policy.validate_expected("1/sqrt(3)").is_err());
    assert!(policy.validate_expected("2/2").is_err());
    assert!(policy.validate_expected("1.0").is_err());
    assert!(matches!(
        check_contract("sqrt(3)", "sqrt(", policy.clone()),
        Outcome::Undecidable(_)
    ));
    let huge = format!("sqrt({})", "9".repeat(5_000));
    assert!(policy.validate_expected(&huge).is_err());
}

#[test]
fn finite_three_case_template_emits_each_reviewed_exact_value() {
    let policy = policy();
    let spec = GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite(KP_KEY, &policy)
        .unwrap();
    let verified = gate(&from_body(BODY).unwrap(), &spec)
        .expect("the three reviewed tangent cases are exhaustive");
    assert!(verified.exhaustive);
    assert_eq!(verified.instances_checked, 3);
    assert_eq!(verified.finite_cases.len(), 3);
}

#[test]
fn syntax_sensitive_radical_policy_is_top_level_only_and_exact_is_unchanged() {
    let list = AnswerContract::List {
        ordered: true,
        member: Box::new(AnswerContract::RequiredSimplestRadical),
    };
    assert!(list.validate().is_err());
    let multipart = AnswerContract::Multipart {
        parts: vec![AnswerPart {
            name: "value".to_owned(),
            contract: AnswerContract::RequiredSimplestRadical,
        }],
    };
    assert!(multipart.validate().is_err());

    let exact = check_contract("sqrt(12)", "2*sqrt(3)", AnswerContract::Exact);
    assert!(matches!(exact, Outcome::Decided(verdict) if verdict.correct));
}
