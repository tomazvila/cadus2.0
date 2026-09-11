//! Required assignments retain their target label and exact right-hand value.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use cadus_core::answer::{AnswerContract, Outcome, check_contract};
use cadus_core::curriculum::{
    AnswerKind, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
};
use cadus_core::template::{GateSpec, from_body, gate};

const KP_KEY: &str = "trig-graphs-midline/kp1";
const BODY: &str = r#"{
  "v":1,
  "topic_id":"trig-graphs-midline",
  "answer_kind":"numeric",
  "answer_contract":{"kind":"required_assignment"},
  "statement":"State the midline of $y=3\\sin x+{d}$. Write the midline in the form y = value.",
  "params":{"d":{"kind":"choice","values":[-4,-3,-2,0,1,5]}},
  "constraints":[],
  "answer_expr":"y=d",
  "solution_sketch":"The vertical shift is {d}, so the full midline equation has y on the left.",
  "hints":["Identify the vertical shift {d}.","The midline is horizontal.","Write the result as y = value."],
  "samples":[
    {"params":{"d":-4},"expected":"y = -4"},
    {"params":{"d":-3},"expected":"y = -3"},
    {"params":{"d":-2},"expected":"y = -2"},
    {"params":{"d":0},"expected":"y = 0"},
    {"params":{"d":1},"expected":"y = 1"},
    {"params":{"d":5},"expected":"y = 5"}
  ]
}"#;

fn decided(expected: &str, learner: &str, correct: bool) {
    let outcome = check_contract(expected, learner, AnswerContract::RequiredAssignment);
    assert!(
        matches!(outcome, Outcome::Decided(verdict) if verdict.correct == correct),
        "{expected:?} vs {learner:?}: {outcome:?}"
    );
}

fn policy() -> FiniteObjectiveDomain {
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:reviewed-midline-practice-centres".to_owned(),
        cases: [-4, -3, -2, 0, 1, 5]
            .into_iter()
            .map(|d| FiniteObjectiveCase {
                id: Slug::new(if d < 0 {
                    format!("shift-neg{}", -d)
                } else {
                    format!("shift-{d}")
                })
                .unwrap(),
                role: FiniteCaseRole::PracticeFresh,
                variants: vec![FiniteCaseVariant {
                    problem: format!(
                        "State the midline of $y=3\\sin x+{}$. Write the midline in the form y = value.",
                        if d < 0 {
                            format!("({d})")
                        } else {
                            d.to_string()
                        }
                    ),
                    answer: format!("y = {d}"),
                    answer_contract: Some(AnswerContract::RequiredAssignment),
                }],
            })
            .collect(),
    }
}

#[test]
fn public_checker_requires_the_authored_target_and_exact_value() {
    let policy = AnswerContract::RequiredAssignment;
    assert_eq!(
        serde_json::to_string(&policy).unwrap(),
        r#"{"kind":"required_assignment"}"#
    );
    assert!(
        serde_json::from_str::<AnswerContract>(r#"{"kind":"required_assignment","target":"y"}"#)
            .is_err()
    );
    for learner in ["y=-4", "Y = -4", "y = -8/2", r"y=\frac{-4}{1}"] {
        decided("y = -4", learner, true);
    }
    for learner in ["-4", "x = -4", "y = 4", "y = -3-0"] {
        decided("y = -4", learner, false);
    }
    assert!(policy.validate_expected("-4").is_err());
    assert!(matches!(
        check_contract("y = -4", "-4 = y", policy.clone()),
        Outcome::Undecidable(_)
    ));
    assert!(matches!(
        check_contract("y = -4", "y =", policy),
        Outcome::Undecidable(_)
    ));

    let exact = check_contract("y = -4", "-4", AnswerContract::Exact);
    assert!(matches!(exact, Outcome::Decided(verdict) if verdict.correct));
}

#[test]
fn finite_numeric_template_emits_six_exact_full_midline_equations() {
    let policy = policy();
    let spec = GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite(KP_KEY, &policy)
        .unwrap();
    let verified = gate(&from_body(BODY).unwrap(), &spec)
        .expect("the six reviewed equation cases are exhaustive");
    assert!(verified.exhaustive);
    assert_eq!(verified.instances_checked, 6);
    assert_eq!(verified.finite_cases.len(), 6);
}

#[test]
fn assignment_target_is_metadata_only_under_the_required_contract() {
    let policy = policy();
    let spec = GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite(KP_KEY, &policy)
        .unwrap();

    let exact = BODY.replace(
        r#""answer_contract":{"kind":"required_assignment"}"#,
        r#""answer_contract":{"kind":"exact"}"#,
    );
    assert_eq!(
        gate(&from_body(&exact).unwrap(), &spec).unwrap_err().code,
        "unknown-names"
    );

    let self_reference = BODY.replace(r#""answer_expr":"y=d""#, r#""answer_expr":"y=y""#);
    assert_eq!(
        gate(&from_body(&self_reference).unwrap(), &spec)
            .unwrap_err()
            .code,
        "unknown-names"
    );
}

#[test]
fn assignment_contract_is_top_level_only() {
    let list = AnswerContract::List {
        ordered: true,
        member: Box::new(AnswerContract::RequiredAssignment),
    };
    assert!(list.validate().is_err());

    let multipart = AnswerContract::Multipart {
        parts: vec![cadus_core::answer::AnswerPart {
            name: "equation".to_owned(),
            contract: AnswerContract::RequiredAssignment,
        }],
    };
    assert!(multipart.validate().is_err());
}
