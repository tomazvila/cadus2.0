//! Phase 1 contract acceptance and adversarial boundaries (D-F1, C4, D6).

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{
    AnswerContract, AnswerPart, Outcome, Quantity, canonical_form, check_contract,
};
use cadus_core::curriculum::model::Exemplar;
use cadus_core::pool::source::ExemplarSource;
use cadus_core::pool::{PoolAnswer, ProblemSource};

fn contract(doc: &str) -> AnswerContract {
    serde_json::from_str(doc).unwrap()
}

fn check(policy: &AnswerContract, expected: &str, learner: &str, correct: bool) {
    assert!(
        matches!(check_contract(expected, learner, policy.clone()), Outcome::Decided(verdict) if verdict.correct == correct),
        "{expected:?} vs {learner:?}"
    );
}

#[test]
fn units_require_a_dimension_and_accept_exact_conversions() {
    let policy = AnswerContract::Unit {
        quantity: Quantity::Volume,
        unit: "L".into(),
    };
    for learner in ["4200 ml", "4.2 L", "21/5 L"] {
        check(&policy, "4.2 L", learner, true);
    }
    for learner in ["4.2", "4200 g", "4.3 L", "4200", "4.2 m^3"] {
        check(&policy, "4.2 L", learner, false);
    }
    assert!(policy.validate_expected("4.2 kg").is_err());
    assert!(
        contract(r#"{"kind":"unit","quantity":"time","unit":"min"}"#)
            .validate_expected("90 s")
            .is_ok()
    );
}

#[test]
fn remainder_contract_enforces_integer_and_divisor_boundaries() {
    let policy = AnswerContract::QuotientRemainder { divisor: Some(3) };
    for learner in ["9 R2", "9 remainder 2", "(9, 2)"] {
        check(&policy, "9 R2", learner, true);
    }
    for learner in ["(2, 9)", "(9, -2)", "(9, 2.1)", "(9, 3)", "9", "{9,2}"] {
        check(&policy, "9 R2", learner, false);
    }
    assert!(policy.validate_expected("(9, 3)").is_err());
    assert!(policy.validate_expected("(9.5, 2)").is_err());
    check(&policy, "(-2, 0)", "(-2, 0)", true);
}

#[test]
fn coordinates_and_sets_select_distinct_shapes() {
    let coordinates = AnswerContract::Coordinates { arity: 2 };
    check(&coordinates, "(1/2, sqrt(2))", "(0.5, 2^(1/2))", true);
    for learner in ["(2, 1)", "{1,2}", "(1,2,3)", "[1,2]"] {
        check(&coordinates, "(1,2)", learner, false);
    }
    assert!(coordinates.validate_expected("(x, 2)").is_err());
    check(&AnswerContract::Set, "{2,4,6}", "{6,2,4,2}", true);
    for learner in ["{2,4}", "{2,4,6,8}", "(6,2,4)", "[2,4,6]"] {
        check(&AnswerContract::Set, "{2,4,6}", learner, false);
    }
}

#[test]
fn tolerances_are_inclusive_absolute_exact_rational_bounds() {
    let policy = contract(r#"{"kind":"approx","tolerance":"1/100"}"#);
    for learner in ["0.99", "1", "1.01", "100/100"] {
        check(&policy, "1", learner, true);
    }
    for learner in ["0.989999999999999999", "1.010000000000000001", "1.1"] {
        check(&policy, "1", learner, false);
    }
    check(&policy, "1000", "1001", false);
    check(&policy, "-1", "-1.01", true);
    assert!(policy.validate_expected("sqrt(2)").is_err());
    assert!(matches!(
        check_contract("sqrt(2)", "sqrt(2)", policy.clone()),
        Outcome::Undecidable(_)
    ));
    check(&policy, "1", "sqrt(2)", false);

    let rounded = AnswerContract::Approx { decimals: 2 };
    assert!(rounded.validate_expected("sqrt(2)").is_ok());
    check(&rounded, "sqrt(2)", "1.41", true);
    assert_eq!(
        serde_json::from_str::<AnswerContract>(&serde_json::to_string(&policy).unwrap()).unwrap(),
        policy
    );
}

fn choices() -> AnswerContract {
    contract(r#"{"kind":"label","options":[["yes","true"],["no","false"],["less than","<"]]}"#)
}

#[test]
fn closed_choices_accept_only_reviewed_aliases() {
    let policy = choices();
    check(&policy, "yes", " TRUE ", true);
    check(&policy, "less than", "LESS   THAN", true);
    check(&policy, "less than", "<", true);
    for learner in ["no", "yes, because", "yesterday", "true or false", "<"] {
        check(&policy, "yes", learner, false);
    }
    assert!(policy.validate_expected("sometimes").is_err());
    assert!(canonical_form("yes").is_err());
    assert!(matches!(
        check_contract("yes", "yes", AnswerContract::Exact),
        Outcome::Undecidable(_)
    ));
}

fn multipart() -> AnswerContract {
    AnswerContract::Multipart {
        parts: vec![
            AnswerPart {
                name: "x".into(),
                contract: AnswerContract::Exact,
            },
            AnswerPart {
                name: "estimate".into(),
                contract: AnswerContract::Approx { decimals: 2 },
            },
            AnswerPart {
                name: "feasible".into(),
                contract: choices(),
            },
        ],
    }
}

#[test]
fn named_parts_bind_each_value_to_its_authored_policy() {
    let policy = multipart();
    let expected = "x = 2; estimate = 1/3; feasible = yes";
    check(
        &policy,
        expected,
        "feasible=true; x=4/2; estimate=0.33",
        true,
    );
    for learner in [
        "x=y=2; estimate=0.33; feasible=yes",
        "x=2; estimate=0.3; feasible=yes",
        "x=0.33; estimate=2; feasible=yes",
        "x=2; estimate=0.33",
        "x=2; estimate=0.33; feasible=no",
        "x=2; x=0.33; feasible=yes",
        "x=2; estimate=0.33; feasible=yes; extra=1",
    ] {
        check(&policy, expected, learner, false);
    }
    assert!(matches!(
        check_contract(expected, "x=?; estimate=0.33; feasible=yes", policy.clone()),
        Outcome::Undecidable(_)
    ));
    assert!(
        policy
            .validate_expected("x=2; estimate=1/3; feasible=maybe")
            .is_err()
    );
}

#[test]
fn policy_schema_refuses_ambiguous_or_unbounded_contracts() {
    for doc in [
        r#"{"kind":"approx"}"#,
        r#"{"kind":"approx","decimals":2,"tolerance":"1/10"}"#,
        r#"{"kind":"approx","tolerance":"0"}"#,
        r#"{"kind":"approx","tolerance":"-1"}"#,
        r#"{"kind":"approx","tolerance":"1/0"}"#,
        r#"{"kind":"approx","tolerance":"sqrt(2)"}"#,
        r#"{"kind":"coordinates","arity":5}"#,
        r#"{"kind":"set","ordered":true}"#,
        r#"{"kind":"unit","quantity":"mass","unit":"m"}"#,
        r#"{"kind":"quotient_remainder","divisor":0}"#,
        r#"{"kind":"label","options":[["yes"],[" YES "]]}"#,
        r#"{"kind":"label","options":[[]]}"#,
        r#"{"kind":"multipart","parts":[]}"#,
        r#"{"kind":"multipart","parts":[{"name":"x","contract":{"kind":"none"}}]}"#,
        r#"{"kind":"multipart","parts":[{"name":"x","contract":{"kind":"exact"}},{"name":"x","contract":{"kind":"exact"}}]}"#,
    ] {
        assert!(
            serde_json::from_str::<AnswerContract>(doc).is_err(),
            "{doc}"
        );
    }
    assert!(
        AnswerContract::Label {
            options: vec![vec!["x".repeat(81)]]
        }
        .validate()
        .is_err()
    );
    assert!(
        AnswerContract::Multipart {
            parts: vec![AnswerPart {
                name: "nested".into(),
                contract: multipart()
            }]
        }
        .validate()
        .is_err()
    );
}

#[test]
fn structured_contracts_survive_exemplar_pool_storage() {
    for (policy, answer) in [
        (choices(), "yes"),
        (multipart(), "x=2; estimate=1/3; feasible=yes"),
    ] {
        let exemplar: Exemplar = serde_json::from_value(serde_json::json!({"problem":"Reviewed fixture", "answer":answer, "answer_contract":policy})).unwrap();
        let exemplars = [exemplar];
        let batch = ExemplarSource::new("contract/kp1", &exemplars)
            .fill("contract/kp1", 1, 0)
            .unwrap();
        let stored = PoolAnswer::from_instance(&batch.instances()[0]);
        let decoded = PoolAnswer::from_body(&stored.to_body().unwrap()).unwrap();
        assert_eq!(decoded.answer_contract, Some(policy));
    }
}

#[test]
fn owned_policies_preserve_attempt_problem_wire_documents() {
    use cadus_core::event::AttemptProblem;
    let doc = serde_json::json!({"text":"Reviewed choice", "expected":"yes", "answer_contract":choices()});
    let parsed: AttemptProblem = serde_json::from_value(doc.clone()).unwrap();
    assert_eq!(parsed.answer_contract.as_deref(), Some(&choices()));
    assert_eq!(serde_json::to_value(parsed).unwrap(), doc);
    let legacy = r#"{"text":"Old choice","expected":"yes"}"#;
    let parsed: AttemptProblem = serde_json::from_str(legacy).unwrap();
    assert_eq!(parsed.answer_contract, None);
    assert_eq!(serde_json::to_string(&parsed).unwrap(), legacy);
}
