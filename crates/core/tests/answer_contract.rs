//! Per-item policies preserve exactness and authored precision (D-F1, C4).

#![allow(clippy::unwrap_used, clippy::panic)]

use cadus_core::answer::{AnswerContract, Outcome, check, check_contract};
use cadus_core::curriculum::AnswerKind;
use cadus_core::curriculum::model::Exemplar;
use cadus_core::pool::source::ExemplarSource;
use cadus_core::pool::{PoolAnswer, PoolProblem, ProblemSource};

fn verdict(expected: &str, given: &str, contract: AnswerContract) -> bool {
    let Outcome::Decided(result) = check_contract(expected, given, contract.clone()) else {
        panic!("the fixture must have a decided result");
    };
    result.correct
}

#[test]
fn exact_values_keep_equivalence_and_refuse_learner_selected_rounding() {
    for (expected, given) in [("1/3", "2/6"), ("sqrt(2)", "2^(1/2)"), ("100 cm", "1 m")] {
        assert!(verdict(expected, given, AnswerContract::Exact));
    }
    for given in ["0.3", "0.33", "0.333333", "2/3"] {
        assert!(!verdict("1/3", given, AnswerContract::Exact));
    }
    assert!(!verdict("1 m", "1", AnswerContract::Exact));
    assert!(matches!(check("1/3", "0.3", AnswerKind::Numeric), Outcome::Decided(v) if v.correct));
}

#[test]
fn approximate_values_use_the_authored_precision_without_truncation() {
    let contract = AnswerContract::Approx { decimals: 2 };
    for given in ["0.33", "0.3300", "33/100"] {
        assert!(verdict("1/3", given, contract.clone()));
    }
    for given in ["0.3", "0.339", "0.33001", "1/3"] {
        assert!(!verdict("1/3", given, contract.clone()));
    }
    assert!(verdict("-1/3", "-0.33", contract.clone()));
    assert!(!verdict("-1/3", "-0.339", contract.clone()));
    assert!(verdict("1.245", "1.24", contract.clone()));
    assert!(verdict("1.255", "1.26", contract.clone()));
    assert!(verdict("sqrt(2)", "1.41", contract.clone()));
    assert!(verdict("5/2", "2", AnswerContract::Approx { decimals: 0 }));
}

#[test]
fn invalid_and_unassessable_items_never_gain_a_string_match_verdict() {
    for (expected, given, contract) in [
        ("garbage", "garbage", AnswerContract::Exact),
        ("1", "?", AnswerContract::Exact),
        ("1", "1", AnswerContract::None),
        ("x", "x", AnswerContract::Approx { decimals: 2 }),
        ("1", "1", AnswerContract::Approx { decimals: 19 }),
    ] {
        assert!(matches!(
            check_contract(expected, given, contract.clone()),
            Outcome::Undecidable(_)
        ));
    }
    assert!(!verdict("1", "", AnswerContract::Exact));
    assert!(matches!(
        check_contract("1", &"1".repeat(10_000), AnswerContract::Exact),
        Outcome::Undecidable(_)
    ));
    assert!(serde_json::from_str::<AnswerContract>(r#"{"kind":"exact","tolerance":1}"#).is_err());
}

#[test]
fn the_exemplar_policy_survives_the_pool_document_round_trip() {
    let item: Exemplar = serde_json::from_str(
        r#"{"problem":"One third?","answer":"1/3","answer_contract":{"kind":"exact"}}"#,
    )
    .unwrap();
    let items = [item];
    let source = ExemplarSource::new("thirds/kp1", &items);
    let batch = source.fill("thirds/kp1", 1, 0).unwrap();
    let instance = &batch.instances()[0];
    let answer = PoolAnswer::from_instance(instance);
    let saved = PoolAnswer::from_body(&answer.to_body().unwrap()).unwrap();
    assert_eq!(saved.answer_contract, Some(AnswerContract::Exact));
    assert!(!verdict(
        &saved.answer,
        "0.3",
        saved.answer_contract.unwrap()
    ));
    assert!(
        !PoolProblem::from_instance(instance, 0)
            .to_body()
            .unwrap()
            .contains("answer_contract")
    );
    let old = r#"{"v":1,"answer":"1/3"}"#;
    let legacy = PoolAnswer::from_body(old).unwrap();
    assert_eq!(legacy.answer_contract, None);
    assert_eq!(legacy.to_body().unwrap(), old);
}
