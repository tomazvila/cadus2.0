use serde_norway::Value;

use super::{Checker, validate};

/// The schema walk accepts only the typed trusted finite-policy shape and
/// applies its role/catalog invariants before a unit enters the arena.
#[test]
fn finite_objective_policy_is_typed_and_semantically_checked() {
    let source = r#"
id: kp1
name: Finite index
finite_objective_domain:
  schema_version: 1
  review_ref: "sha256:review"
  cases:
    - id: one
      role: practice_fresh
      variants:
        - problem: "Return 1."
          answer: "1"
"#;
    let document: Value = serde_norway::from_str(source).unwrap();
    let point = validate::<crate::curriculum::KnowledgePoint, _>(
        &document,
        "unit.yaml",
        Checker::check_knowledge_point,
    )
    .expect("the trusted finite policy reads");
    assert_eq!(
        point
            .finite_objective_domain
            .as_ref()
            .expect("the policy is retained")
            .cases[0]
            .role,
        crate::curriculum::FiniteCaseRole::PracticeFresh
    );

    let roleless = source.replace("practice_fresh", "reserved_assessment");
    let document: Value = serde_norway::from_str(&roleless).unwrap();
    let findings = validate::<crate::curriculum::KnowledgePoint, _>(
        &document,
        "unit.yaml",
        Checker::check_knowledge_point,
    )
    .unwrap_err();
    assert_eq!(findings.len(), 1);
    assert!(findings[0].message.contains("needs a practice_fresh"));
}

/// Canonical exemplars must be explicit members of the reviewed semantic
/// universe; rewording cannot silently manufacture another case.
#[test]
fn finite_policy_binds_every_canonical_exemplar() {
    let source = r#"
id: kp1
name: Finite index
exemplars:
  - problem: "Return 2."
    answer: "2"
finite_objective_domain:
  schema_version: 1
  review_ref: "sha256:review"
  cases:
    - id: one
      role: practice_fresh
      variants:
        - problem: "Return 1."
          answer: "1"
"#;
    let document: Value = serde_norway::from_str(source).unwrap();
    let findings = validate::<crate::curriculum::KnowledgePoint, _>(
        &document,
        "unit.yaml",
        Checker::check_knowledge_point,
    )
    .unwrap_err();
    assert_eq!(findings.len(), 1);
    assert!(
        findings[0]
            .message
            .contains("does not match any reviewed finite case variant")
    );
}
