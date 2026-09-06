//! A reviewed per-item contract reaches pending template content (C6, D-F1).

#![allow(clippy::unwrap_used)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_core::pool::kp_key;
use cadus_store::test_support::TestDb;
use cadus_worker::authoring::job::Outcome;
use cadus_worker::authoring::prompt::Kind;
use common::{FakeModel, author, content_rows, good_arguments, named_reply, squares_spec};
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
        let mut arguments = good_arguments();
        // A model cannot replace the policy the reviewed curriculum owns.
        arguments["answer_contract"] = json!({"kind": "approx", "decimals": 0});
        let fake =
            FakeModel::start(vec![named_reply(Kind::Template.tool_name(), &arguments)]).await;
        let result = author(&db, &fake, Kind::Template, &spec).await;
        assert_eq!(result.outcome, Outcome::Stored, "{result:?}");
        assert_eq!(result.attempts, 1);
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
