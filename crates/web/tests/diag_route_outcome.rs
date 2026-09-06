//! Ungraded diagnostic responses preserve placement and private evidence.
#![allow(clippy::unwrap_used)]
mod common;
use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_store::state::load_diag_state;
use common::placement::{app, topic};
use common::{
    Method, TestDb, Value, app_with_content, call, events_of_type, json, one_unit_curriculum, parse,
};

#[tokio::test]
async fn an_unmarked_answer_is_recorded_without_penalty_or_disclosure() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = common::seed_learner(&db, "diag-ungraded@example.test").await;
        let (_, start) = call(
            &app,
            Method::POST,
            "/api/diag/start",
            Some(user),
            Some(json!({"course":"c1"})),
        )
        .await;
        let id = parse(&start)["probe"]["problem_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut tx = db.admin.begin().await.unwrap();
        let before = load_diag_state(&mut tx, user).await.unwrap().unwrap();
        tx.rollback().await.unwrap();
        let (status, raw) = call(
            &app,
            Method::POST,
            "/api/diag/answer",
            Some(user),
            Some(json!({"problem_id":id,"answer":"1/0"})),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{raw}");
        let reply = parse(&raw);
        assert_eq!(reply["outcome"], "ungraded");
        for secret in ["correct", "expected", "solution"] {
            assert!(reply.get(secret).is_none(), "{raw}");
        }
        let mut tx = db.admin.begin().await.unwrap();
        let after = load_diag_state(&mut tx, user).await.unwrap().unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(after["balances"], before["balances"]);
        assert_eq!(after["answered"].as_array().unwrap().len(), 1);
        let events = events_of_type(&db, user, "diagnostic_answer").await;
        assert_eq!(events.len(), 1);
        assert!(events[0]["outcome"]["ungraded"]["reason"].is_string());
        assert_eq!(events[0]["weight"], 0.0);
        assert_eq!(events[0]["submitted"], "1/0");
        assert_eq!(events[0]["problem_id"], id);
        assert_eq!(events[0]["problem"]["expected"], "7");
        let (status, _) = call(
            &app,
            Method::POST,
            "/api/diag/answer",
            Some(user),
            Some(json!({"problem_id":id,"answer":"1/0"})),
        )
        .await;
        assert_eq!(status.as_u16(), 404);
        assert_eq!(
            events_of_type(&db, user, "diagnostic_answer").await.len(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn a_multi_step_diagnostic_uses_its_explicit_exact_policy() {
    TestDb::with(|db| async move {
        let mut item = topic("application", None);
        item.answer_kind = AnswerKind::MultiStep;
        item.diagnostic_exemplar.as_mut().unwrap().answer_contract = Some(AnswerContract::Exact);
        let app = app_with_content(&db, one_unit_curriculum(vec![item]));
        let user = common::seed_learner(&db, "diag-contract@example.test").await;
        let (_, start) = call(
            &app,
            Method::POST,
            "/api/diag/start",
            Some(user),
            Some(json!({"course":"c1"})),
        )
        .await;
        let probe = parse(&start)["probe"].clone();
        assert!(probe["problem_id"].is_string(), "{start}");
        let (status, raw) = call(
            &app,
            Method::POST,
            "/api/diag/answer",
            Some(user),
            Some(json!({"problem_id":probe["problem_id"],"answer":"14/2"})),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{raw}");
        let reply: Value = parse(&raw);
        assert_eq!(reply["correct"], true);
        assert_eq!(
            events_of_type(&db, user, "diagnostic_answer").await[0]["problem"]["answer_contract"],
            json!({"kind":"exact"})
        );
    })
    .await;
}

#[tokio::test]
async fn a_recognizable_required_form_violation_is_recorded_as_incorrect() {
    TestDb::with(|db| async move {
        let mut item = topic("ratio", None);
        let exemplar = item.diagnostic_exemplar.as_mut().unwrap();
        exemplar.answer = "2:3".to_owned();
        exemplar.answer_contract = Some(AnswerContract::ReducedRatio);
        let app = app_with_content(&db, one_unit_curriculum(vec![item]));
        let user = common::seed_learner(&db, "diag-required-form@example.test").await;
        let (_, start) = call(
            &app,
            Method::POST,
            "/api/diag/start",
            Some(user),
            Some(json!({"course":"c1"})),
        )
        .await;
        let problem_id = parse(&start)["probe"]["problem_id"].clone();
        let (status, raw) = call(
            &app,
            Method::POST,
            "/api/diag/answer",
            Some(user),
            Some(json!({"problem_id":problem_id,"answer":"4:6"})),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{raw}");
        assert_eq!(parse(&raw)["correct"], false, "{raw}");
        let events = events_of_type(&db, user, "diagnostic_answer").await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["correct"], false);
        assert!(events[0].get("outcome").is_none(), "{}", events[0]);
        assert!(events[0]["weight"].as_f64().unwrap() > 0.0, "{}", events[0]);
    })
    .await;
}
