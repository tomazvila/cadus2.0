//! Production preflight and document gates agree on pending typed contracts.
#![allow(clippy::unwrap_used)]

mod common;

use cadus_core::{answer::AnswerContract, curriculum::AnswerKind, pool::kp_key};
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::{
    job::{Outcome, preflight, verify_kind},
    prompt::Kind,
};
use common::{FakeModel, author_expect, content_rows, good_arguments, named_reply, squares_spec};
use serde_json::json;

#[test]
fn unsupported_kinds_have_the_same_offline_and_worker_preflight_refusal() {
    for answer_kind in [AnswerKind::Proof, AnswerKind::MultiStep] {
        let mut spec = squares_spec();
        spec.answer_kind = answer_kind;
        for kind in [Kind::Template, Kind::Diagnosis] {
            if answer_kind == AnswerKind::MultiStep && kind == Kind::Template {
                assert!(preflight(kind, &spec).is_ok());
                continue;
            }
            let early = preflight(kind, &spec).unwrap_err();
            let offline = verify_kind(kind, &spec, &good_arguments(), &[]).unwrap_err();
            assert_eq!(early.code, "answer-kind");
            assert_eq!(early.code, offline.code);
            assert_eq!(early.message, offline.message);
        }
        assert!(preflight(Kind::Teach, &spec).is_ok());
        assert!(preflight(Kind::HintLadder, &spec).is_ok());
    }
}

#[test]
fn pending_contracts_are_required_and_validated_after_preflight() {
    let mut spec = squares_spec();
    spec.answer_kind = AnswerKind::MultiStep;
    assert!(preflight(Kind::Template, &spec).is_ok());
    let mut arguments = good_arguments();
    assert!(verify_kind(Kind::Template, &spec, &arguments, &[]).is_err());
    for contract in [
        json!({"kind":"none"}),
        json!({"kind":"invented"}),
        json!({"kind":"label","labels":[]}),
        json!({"kind":"exact","unexpected":"policy"}),
        json!({"kind":"coordinates","arity":2}),
        json!({"kind":"multipart","parts":[]}),
        json!(null),
    ] {
        arguments["answer_contract"] = contract.clone();
        assert!(
            verify_kind(Kind::Template, &spec, &arguments, &[]).is_err(),
            "accepted malformed or wrong-shape policy {contract}"
        );
    }
}

#[test]
fn an_explicit_contract_cannot_replace_or_malform_the_reviewed_policy() {
    let mut spec = squares_spec();
    spec.answer_kind = AnswerKind::MultiStep;
    spec.exemplars[0].answer_contract = Some(AnswerContract::Exact);
    let mut arguments = good_arguments();
    for contract in [
        json!({"kind":"approx","decimals":0}),
        json!({"kind":"none"}),
        json!({"kind":"exact","unexpected":"policy"}),
        json!(null),
    ] {
        arguments["answer_contract"] = contract;
        assert_eq!(
            verify_kind(Kind::Template, &spec, &arguments, &[])
                .unwrap_err()
                .code,
            "answer-contract"
        );
    }
    arguments["answer_contract"] = json!({"kind":"exact"});
    assert!(verify_kind(Kind::Template, &spec, &arguments, &[]).is_ok());
}

#[test]
fn exact_policy_allows_a_validated_set_refinement_and_rejects_its_reverse() {
    let mut spec = squares_spec();
    spec.answer_kind = AnswerKind::MultiStep;
    spec.exemplars[0].answer_contract = Some(AnswerContract::Exact);
    let mut arguments = good_arguments();
    arguments["answer_contract"] = json!({"kind":"set"});
    // A scalar answer cannot use the collection-shaped refinement.
    assert!(verify_kind(Kind::Template, &spec, &arguments, &[]).is_err());
    arguments["answer_expr"] = json!("{a**2}");
    arguments["samples"][0]["expected"] = json!("{1}");
    arguments["samples"][1]["expected"] = json!("{144}");
    let body: serde_json::Value =
        serde_json::from_str(&verify_kind(Kind::Template, &spec, &arguments, &[]).unwrap())
            .unwrap();
    assert_eq!(body["answer_contract"], json!({"kind":"set"}));
    spec.exemplars[0].answer_contract = Some(AnswerContract::Set);
    arguments["answer_contract"] = json!({"kind":"exact"});
    assert_eq!(
        verify_kind(Kind::Template, &spec, &arguments, &[])
            .unwrap_err()
            .code,
        "answer-contract"
    );
}

#[tokio::test]
async fn pending_contracts_reach_the_real_worker_with_unshared_exemplar_policies() {
    TestDb::with(|db| async move {
        for mixed in [false, true] {
            let mut spec = squares_spec();
            spec.answer_kind = AnswerKind::MultiStep;
            spec.kp_id = format!("unshared-{mixed}");
            if mixed {
                spec.exemplars[0].answer_contract = Some(AnswerContract::Exact);
                let mut second = spec.exemplars[0].clone();
                second.answer_contract = Some(AnswerContract::Approx { decimals: 2 });
                spec.exemplars.push(second);
            }
            assert_eq!(spec.template_contract(), None);
            let mut arguments = good_arguments();
            arguments["answer_contract"] = json!({"kind":"exact"});
            let fake =
                FakeModel::start(vec![named_reply(Kind::Template.tool_name(), &arguments)]).await;
            author_expect(&db, &fake, Kind::Template, &spec, Outcome::Stored, 1).await;
            assert_eq!(fake.call_count(), 1);
            let rows = content_rows(&db.admin, &kp_key(&spec.topic_id, &spec.kp_id)).await;
            assert_eq!(
                (
                    rows.len(),
                    rows[0].status.as_str(),
                    &rows[0].body["answer_contract"]
                ),
                (1, "pending", &json!({"kind":"exact"}))
            );
        }
    })
    .await;
}

#[tokio::test]
async fn a_wrong_contract_reaches_the_gate_and_never_stores_a_row() {
    TestDb::with(|db| async move {
        let mut spec = squares_spec();
        spec.answer_kind = AnswerKind::MultiStep;
        let mut arguments = good_arguments();
        arguments["answer_contract"] = json!({"kind":"coordinates","arity":2});
        let fake =
            FakeModel::start(vec![named_reply(Kind::Template.tool_name(), &arguments); 5]).await;
        let result = author_expect(&db, &fake, Kind::Template, &spec, Outcome::Declined, 5).await;
        assert_eq!(fake.call_count(), 5);
        assert!(result.decline.unwrap().reasons.iter().all(|reason| {
            !reason.contains("answer kind multi-step is not symbolically decidable")
        }));
        assert!(
            content_rows(&db.admin, &kp_key(&spec.topic_id, &spec.kp_id))
                .await
                .is_empty()
        );
    })
    .await;
}
