#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cadus_core::curriculum::{
    AnswerKind, FiniteCaseRole, FiniteCaseVariant, FiniteObjectiveCase, FiniteObjectiveDomain, Slug,
};
use cadus_core::pool::{ProblemSource, TemplateSource};
use cadus_core::template::{FiniteGateSpec, GateSpec, TemplateDoc, from_body, gate};

const BODY: &str = r#"{
  "v":1,
  "topic_id":"finite-index",
  "answer_kind":"numeric",
  "statement":"Return the index ${a}$.",
  "params":{"a":{"kind":"int","low":1,"high":3}},
  "answer_expr":"a",
  "solution_sketch":"Read the displayed index.",
  "hints":["Use the displayed index."],
  "samples":[
    {"params":{"a":1},"expected":"1"},
    {"params":{"a":3},"expected":"3"}
  ]
}"#;

fn variant(a: i64) -> FiniteCaseVariant {
    FiniteCaseVariant {
        problem: format!("Return the index ${a}$."),
        answer: a.to_string(),
        answer_contract: None,
    }
}

fn case(a: i64, role: FiniteCaseRole) -> FiniteObjectiveCase {
    FiniteObjectiveCase {
        id: Slug::new(format!("index-{a}")).unwrap(),
        role,
        variants: vec![variant(a)],
    }
}

fn policy() -> FiniteObjectiveDomain {
    FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:reviewed-finite-index".to_owned(),
        cases: (1..=3)
            .map(|a| case(a, FiniteCaseRole::PracticeFresh))
            .collect(),
    }
}

fn doc() -> TemplateDoc {
    from_body(BODY).unwrap()
}

fn spec<'a>(policy: &'a FiniteObjectiveDomain) -> GateSpec<'a> {
    GateSpec::new(AnswerKind::Numeric, &[])
        .with_finite("finite-index/kp1", policy)
        .unwrap()
}

#[test]
fn ordinary_small_space_still_fails_the_floor() {
    let rejection = gate(&doc(), &GateSpec::new(AnswerKind::Numeric, &[])).unwrap_err();
    assert_eq!(rejection.code, "space-floor");
}

#[test]
fn reviewed_three_case_universe_passes_with_exact_case_evidence() {
    let policy = policy();
    let verified = gate(&doc(), &spec(&policy)).expect("the exact reviewed universe passes");
    assert!(verified.exhaustive);
    assert_eq!(verified.instances_checked, 3);
    assert_eq!(verified.finite_cases.len(), 3);
    assert_eq!(
        verified
            .finite_cases
            .iter()
            .map(|case| case.case_id.as_str())
            .collect::<Vec<_>>(),
        vec!["index-1", "index-2", "index-3"]
    );
    let expected = policy.fingerprint("finite-index/kp1").unwrap();
    assert_eq!(
        verified.finite_policy_fingerprint.as_deref(),
        Some(expected.as_str())
    );
    assert_eq!(expected.len(), 64);
}

#[test]
fn missing_unknown_and_nonpractice_cases_are_refused() {
    let mut missing = policy();
    missing.cases.push(case(4, FiniteCaseRole::PracticeFresh));
    assert_eq!(
        gate(&doc(), &spec(&missing)).unwrap_err().code,
        "finite-case-missing"
    );

    let mut unknown = policy();
    unknown.cases[1].variants[0].problem = "Give the index $2$.".to_owned();
    assert_eq!(
        gate(&doc(), &spec(&unknown)).unwrap_err().code,
        "finite-case-unknown"
    );

    let mut teach = policy();
    teach.cases[2].role = FiniteCaseRole::TeachOnly;
    assert_eq!(
        gate(&doc(), &spec(&teach)).unwrap_err().code,
        "finite-case-role"
    );
}

#[test]
fn two_renderings_cannot_inflate_one_semantic_case() {
    let body = BODY
        .replace(
            r#""statement":"Return the index ${a}$.""#,
            r#""statement":"{verb} the index ${a}$.""#,
        )
        .replace(
            r#""params":{"a":{"kind":"int","low":1,"high":3}}"#,
            r#""params":{"a":{"kind":"int","low":1,"high":1},"verb":{"kind":"choice","values":["Return","Give"]}}"#,
        )
        .replace(
            r#"{"params":{"a":1},"expected":"1"},
    {"params":{"a":3},"expected":"3"}"#,
            r#"{"params":{"a":1,"verb":"Return"},"expected":"1"},
    {"params":{"a":1,"verb":"Give"},"expected":"1"}"#,
        );
    let mut policy = FiniteObjectiveDomain {
        schema_version: 1,
        review_ref: "sha256:one-case".to_owned(),
        cases: vec![case(1, FiniteCaseRole::PracticeFresh)],
    };
    policy.cases[0].variants.push(FiniteCaseVariant {
        problem: "Give the index $1$.".to_owned(),
        answer: "1".to_owned(),
        answer_contract: None,
    });
    let rejection = gate(&from_body(&body).unwrap(), &spec(&policy)).unwrap_err();
    assert_eq!(rejection.code, "finite-case-duplicate");
}

#[test]
fn fingerprint_is_scoped_and_policy_validation_rejects_ambiguous_or_roleless_sets() {
    let policy = policy();
    assert_eq!(
        policy.fingerprint("finite-index/kp1").unwrap(),
        policy.fingerprint("finite-index/kp1").unwrap()
    );
    assert_ne!(
        policy.fingerprint("finite-index/kp1").unwrap(),
        policy.fingerprint("finite-index/kp2").unwrap()
    );
    let mut reordered = policy.clone();
    reordered.cases[0].variants.push(FiniteCaseVariant {
        problem: "Give the index $1$.".to_owned(),
        answer: "1".to_owned(),
        answer_contract: None,
    });
    let mut reordered_variants = reordered.clone();
    reordered_variants.cases[0].variants.reverse();
    assert_eq!(
        reordered.fingerprint("finite-index/kp1").unwrap(),
        reordered_variants.fingerprint("finite-index/kp1").unwrap()
    );
    let mut cases_only = policy.clone();
    cases_only.cases.reverse();
    assert_eq!(
        policy.fingerprint("finite-index/kp1").unwrap(),
        cases_only.fingerprint("finite-index/kp1").unwrap()
    );
    let mut changed_review = policy.clone();
    changed_review.review_ref.push_str("-v2");
    assert_ne!(
        policy.fingerprint("finite-index/kp1").unwrap(),
        changed_review.fingerprint("finite-index/kp1").unwrap()
    );

    let mut duplicate = policy.clone();
    duplicate.cases[1].variants[0] = duplicate.cases[0].variants[0].clone();
    assert!(
        duplicate
            .validate()
            .unwrap_err()
            .contains("more than one semantic case")
    );

    let mut blank = policy.clone();
    blank.cases[0].variants[0].problem = "   ".to_owned();
    assert!(
        blank
            .validate()
            .unwrap_err()
            .contains("blank problem or answer")
    );

    let mut roleless = policy;
    for case in &mut roleless.cases {
        case.role = FiniteCaseRole::ReservedAssessment;
    }
    assert!(
        roleless
            .validate()
            .unwrap_err()
            .contains("needs a practice_fresh")
    );
}

#[test]
fn template_source_rechecks_each_instance_against_current_finite_policy() {
    let doc = doc();
    let policy = policy();
    let source = TemplateSource::new("finite-index/kp1", &doc)
        .unwrap()
        .with_finite_policy("finite-index/kp1", &policy)
        .unwrap();
    let batch = source.fill("finite-index/kp1", 3, 0).unwrap();
    assert_eq!(batch.instances().len(), 3);

    let mut changed = policy;
    changed.cases[0].role = FiniteCaseRole::ReservedAssessment;
    let source = TemplateSource::new("finite-index/kp1", &doc)
        .unwrap()
        .with_finite_policy("finite-index/kp1", &changed)
        .unwrap();
    let batch = source.fill("finite-index/kp1", 3, 0).unwrap();
    assert_eq!(batch.instances().len(), 2);
    assert_eq!(batch.refusals().len(), 1);
}

#[test]
fn public_match_api_returns_case_role_and_hash() {
    let doc = doc();
    let compiled = cadus_core::template::Compiled::new(&doc).unwrap();
    let instance = compiled
        .instantiate(
            [(
                "a".to_owned(),
                cadus_core::template::Value::Num(num_rational::BigRational::from_integer(2.into())),
            )]
            .into(),
        )
        .unwrap();
    let policy = policy();
    let finite = FiniteGateSpec::new("finite-index/kp1", &policy).unwrap();
    let matched = finite.match_practice_instance(&instance).unwrap();
    assert_eq!(matched.case_id, "index-2");
    assert_eq!(matched.role, FiniteCaseRole::PracticeFresh);
    assert_eq!(matched.instance_hash, instance.instance_hash);
}
