//! A reviewed per-item contract reaches pending template content (C6, D-F1).

#![allow(clippy::unwrap_used)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_core::pool::kp_key;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::Outcome;
use cadus_worker::authoring::job::verify;
use cadus_worker::authoring::prompt::Kind;
use common::{FakeModel, author_expect, content_rows, good_arguments, named_reply, squares_spec};
use serde_json::json;

#[tokio::test]
async fn an_exact_multi_step_template_passes_authoring_and_stays_pending() {
    TestDb::with(|db| async move {
        let mut spec = squares_spec();
        spec.answer_kind = AnswerKind::MultiStep;
        for item in &mut spec.exemplars {
            item.answer_contract = Some(AnswerContract::Exact);
        }
        let prompt = cadus_worker::authoring::prompt::render_exemplars(&spec.exemplars);
        assert!(prompt.contains(r#"Answer contract: {"kind":"exact"}"#));
        let arguments = good_arguments();
        let fake =
            FakeModel::start(vec![named_reply(Kind::Template.tool_name(), &arguments)]).await;
        author_expect(&db, &fake, Kind::Template, &spec, Outcome::Stored, 1).await;
        let rows = content_rows(&db.admin, &kp_key(&spec.topic_id, &spec.kp_id)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "pending");
        assert_eq!(rows[0].body["answer_contract"], json!({"kind": "exact"}));
        assert_eq!(rows[0].body["answer_kind"], "multi-step");
    })
    .await;
}

#[test]
fn mixed_item_policies_require_an_explicit_authoring_decision() {
    let mut spec = squares_spec();
    assert_eq!(spec.template_contract(), None);
    spec.exemplars[0].answer_contract = Some(AnswerContract::Exact);
    assert_eq!(spec.template_contract(), Some(AnswerContract::Exact));
    let mut second = spec.exemplars[0].clone();
    second.answer_contract = Some(AnswerContract::Approx { decimals: 2 });
    spec.exemplars.push(second);
    assert_eq!(spec.template_contract(), None);
}

#[test]
fn an_uncontracted_multi_step_spec_accepts_a_validated_pending_contract() {
    let mut spec = squares_spec();
    spec.answer_kind = AnswerKind::MultiStep;
    let mut arguments = good_arguments();
    arguments["answer_contract"] = json!({"kind":"exact"});
    let body: serde_json::Value =
        serde_json::from_str(&verify(&spec, &arguments).unwrap()).unwrap();
    assert_eq!(body["answer_contract"], json!({"kind":"exact"}));
    assert_eq!(body["answer_kind"], "multi-step");

    arguments["answer_contract"] = json!({"kind":"none"});
    assert_eq!(
        verify(&spec, &arguments).unwrap_err().code,
        "answer-contract"
    );
}
