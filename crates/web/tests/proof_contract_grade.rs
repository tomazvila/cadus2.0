//! A `proof` item with an answer contract gets a verdict (flow lane B1).
//!
//! The answer contract decides before the topic kind, on the grade route and on
//! the placement diagnostic.

#![allow(clippy::unwrap_used)]

mod common;

use cadus_core::answer::AnswerContract;
use cadus_core::curriculum::AnswerKind;
use cadus_store::test_support::TestDb;
use cadus_web::state::ServedProblem;
use common::placement::topic;
use common::{
    LESSON, Method, PROBLEM_ID, Verdict, answer_task, app_with_content, attempt_payload, call,
    events_of_type, lesson_app, lesson_learner, lesson_problem, lesson_state, one_unit_curriculum,
    parse, put_state, seed_attempt_row, seed_learner, seed_open_session,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The four `Step <n>` options of an item that asks for the wrong proof step.
fn steps() -> AnswerContract {
    AnswerContract::Label {
        options: (1..=4).map(|step| vec![format!("Step {step}")]).collect(),
    }
}

/// One live `proof` problem whose authored key is `Step 3`.
fn proof_problem() -> ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.answer_kind = Some("proof".to_owned());
    live.expected.answer = "Step 3".to_owned();
    live.expected.answer_contract = Some(steps());
    live
}

/// Give `given` as the answer to the live `proof` problem. The reply is a 200.
async fn answer(app: &axum::Router, user: Uuid, given: &str) -> Value {
    let submission = json!({"answer": given, "problem_id": PROBLEM_ID});
    let reply = answer_task(app, user, LESSON, submission).await;
    assert!(reply.0.is_success(), "{}", reply.1);
    reply.1
}

#[tokio::test]
async fn the_authored_key_of_a_proof_item_is_correct() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        for (index, given) in ["Step 3", "step  3"].into_iter().enumerate() {
            let email = format!("proof-label-key-{index}@example.com");
            let user = lesson_learner(&db, &email, proof_problem()).await;
            let body = answer(&app, user, given).await;
            assert_eq!(body["outcome"], "correct", "{body}");
            assert_eq!(body["correct"], true, "{body}");
            assert!(body.get("reason").is_none(), "{body}");
            let events = events_of_type(&db, user, "attempt").await;
            assert_eq!(events.len(), 1);
            assert_eq!(events[0]["correct"], true);
            assert_eq!(events[0]["problem"]["answer_contract"]["kind"], "label");
        }
    })
    .await;
}

#[tokio::test]
async fn a_different_option_of_a_proof_item_is_incorrect() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        for (index, given) in ["Step 2", "Step 9"].into_iter().enumerate() {
            let email = format!("proof-label-miss-{index}@example.com");
            let user = lesson_learner(&db, &email, proof_problem()).await;
            let body = answer(&app, user, given).await;
            assert_eq!(body["outcome"], "incorrect", "{body}");
            assert_eq!(body["correct"], false, "{body}");
            let events = events_of_type(&db, user, "attempt").await;
            assert_eq!(events.len(), 1);
            assert!(events[0].get("outcome").is_none(), "{}", events[0]);
        }
    })
    .await;
}

/// The attempt enters the pass sequence: one earlier correct answer at `kp1` is
/// in the log, so the correct `proof` answer passes the knowledge point.
#[tokio::test]
async fn a_correct_proof_answer_counts_in_the_pass_sequence() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = seed_learner(&db, "proof-label-pass@example.com").await;
        seed_open_session(&db, user).await;
        put_state(&db, user, &lesson_state(proof_problem(), 1, false)).await;
        let earlier = "s_2026-01-01a-lesson-addition-0";
        let prior = attempt_payload(
            LESSON,
            earlier,
            "kp1",
            ("Which step has the error?", "Step 3"),
            &Verdict {
                given_answer: "Step 3",
                correct: true,
                work_quality: "nearly_perfect",
                error_tags: json!([]),
                secs: 9,
            },
        );
        seed_attempt_row(&db, user, 2, earlier, &prior).await;

        let body = answer(&app, user, "Step 3").await;
        assert_eq!(body["outcome"], "correct", "{body}");
        assert_eq!(body["task_status"], "kp_advance", "{body}");
    })
    .await;
}

/// A `proof` item with no contract stays ungraded, as before this change.
#[tokio::test]
async fn a_proof_item_with_no_contract_stays_ungraded() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let mut live = proof_problem();
        live.expected.answer_contract = None;
        let user = lesson_learner(&db, "proof-no-contract@example.com", live).await;
        let body = answer(&app, user, "Step 3").await;
        assert_eq!(body["outcome"], "ungraded", "{body}");
        assert_eq!(body["reason"], "no deterministic verdict for a proof");
    })
    .await;
}

/// A `proof` item served with the contract `none` stays ungraded with the proof
/// reason. The serve path records this contract for each free-text proof
/// exemplar. The key as the learner text and a blank answer get no verdict.
#[tokio::test]
async fn a_proof_item_with_the_none_contract_stays_ungraded() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        for (index, given) in ["Step 3", "Step 2", ""].into_iter().enumerate() {
            let mut live = proof_problem();
            live.expected.answer_contract = Some(AnswerContract::None);
            let email = format!("proof-none-contract-{index}@example.com");
            let user = lesson_learner(&db, &email, live).await;
            let body = answer(&app, user, given).await;
            assert_eq!(body["outcome"], "ungraded", "{body}");
            assert_eq!(body["reason"], "no deterministic verdict for a proof");
            assert!(body.get("correct").is_none(), "{body}");
            assert!(body.get("solution").is_none(), "{body}");
            let events = events_of_type(&db, user, "attempt").await;
            assert_eq!(events.len(), 1);
            assert_eq!(events[0]["problem"]["answer_contract"]["kind"], "none");
        }
    })
    .await;
}

/// Start the diagnostic of course `c1` and read the probe object.
async fn start_probe(app: &axum::Router, user: Uuid) -> Value {
    let course = json!({"course": "c1"});
    let (_, start) = call(
        app,
        Method::POST,
        "/api/diag/start",
        Some(user),
        Some(course),
    )
    .await;
    parse(&start)["probe"].clone()
}

/// A `proof` probe with a contract gets a mark, not `409 no_diagnostic`.
#[tokio::test]
async fn a_proof_probe_with_a_contract_gets_a_mark() {
    TestDb::with(|db| async move {
        let mut item = topic("induction", None);
        item.answer_kind = AnswerKind::Proof;
        let exemplar = item.diagnostic_exemplar.as_mut().unwrap();
        exemplar.answer = "Step 3".to_owned();
        exemplar.answer_contract = Some(steps());
        let app = app_with_content(&db, one_unit_curriculum(vec![item]));
        for (index, given, correct) in [(0, "step 3", true), (1, "Step 2", false)] {
            let email = format!("diag-proof-contract-{index}@example.test");
            let user = seed_learner(&db, &email).await;
            let probe = start_probe(&app, user).await;
            assert!(probe["problem_id"].is_string(), "{probe}");
            let reply = json!({"problem_id": probe["problem_id"], "answer": given});
            let (status, raw) = call(
                &app,
                Method::POST,
                "/api/diag/answer",
                Some(user),
                Some(reply),
            )
            .await;
            assert_eq!(status.as_u16(), 200, "{raw}");
            assert_eq!(parse(&raw)["correct"], correct, "{raw}");
            let events = events_of_type(&db, user, "diagnostic_answer").await;
            assert_eq!(events.len(), 1);
            assert_eq!(events[0]["correct"], correct);
            assert!(events[0].get("outcome").is_none(), "{}", events[0]);
        }
    })
    .await;
}

/// A `proof` topic with no contract gets no probe, as before this change.
#[tokio::test]
async fn a_proof_topic_with_no_contract_gets_no_probe() {
    TestDb::with(|db| async move {
        let mut item = topic("induction", None);
        item.answer_kind = AnswerKind::Proof;
        let app = app_with_content(&db, one_unit_curriculum(vec![item]));
        let user = seed_learner(&db, "diag-proof-no-contract@example.test").await;
        let probe = start_probe(&app, user).await;
        assert!(probe.get("problem_id").is_none(), "{probe}");
    })
    .await;
}
