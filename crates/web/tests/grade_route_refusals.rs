//! M5 U8: the refusals of the grade path (spec section 10), the no-panic rule,
//! and the M5 U11 deterministic-grade counter (T6, spec section 7).
//!
//! No test here reaches the checker: every case is refused before the grade,
//! and the counter tests read one label per learner.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use common::{
    EXPECTED_ANSWER, LESSON, PROBLEM_ID, answer_lesson_ok, call, events_of_type, holds,
    learner_with_kp1, lesson_app as app, lesson_learner, lesson_problem, lesson_state, parse,
    put_state, scrape,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// One request of the answer route as `tenant`, with `body`.
async fn post_answer(
    app: &Router,
    tenant: Option<Uuid>,
    body: Option<Value>,
) -> (StatusCode, String) {
    call(
        app,
        Method::POST,
        &format!("/api/task/{LESSON}/answer"),
        tenant,
        body,
    )
    .await
}

/// One refused request of the answer route: the status and the `error.code`.
async fn refusal(app: &Router, tenant: Option<Uuid>, body: Value) -> (StatusCode, String) {
    let (status, raw) = post_answer(app, tenant, Some(body)).await;
    let code = parse(&raw)["error"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("the body carries no error.code: {raw}"))
        .to_string();
    (status, code)
}

/// A learner whose live lesson problem is a `proof`, which no checker decides.
async fn proof_learner(db: &TestDb, email: &str) -> Uuid {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.answer_kind = Some("proof".to_string());
    lesson_learner(db, email, live).await
}

// --------------------------------------------------------------------------- //
// The refusals
// --------------------------------------------------------------------------- //

/// Spec section 10. The caps are 4,000 answer characters and 20,000 work
/// characters; over either one the route is `413 answer_too_large`.
#[tokio::test]
async fn the_input_caps_are_413_answer_too_large() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "large@example.com", 5.0).await;

        let long_answer = json!({"problem_id": PROBLEM_ID, "answer": "1".repeat(4_001)});
        assert_eq!(
            refusal(&app, Some(user), long_answer).await,
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                "answer_too_large".to_string()
            )
        );

        let long_work = json!({
            "problem_id": PROBLEM_ID,
            "answer": "13.5",
            "work": "w".repeat(20_001),
        });
        assert_eq!(
            refusal(&app, Some(user), long_work).await,
            (
                StatusCode::PAYLOAD_TOO_LARGE,
                "answer_too_large".to_string()
            )
        );
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 0);
    })
    .await;
}

/// Spec section 10. A superseded `problem_id` is `404 unknown_problem` and a
/// closed task is `409 task_complete`. A body with no `problem_id` is
/// `422 invalid_request`, and a request with no tenant is `401 unauthorized`.
#[tokio::test]
async fn the_validate_refusals_are_the_pinned_literals() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "refuse@example.com", 5.0).await;
        let right = json!({"problem_id": PROBLEM_ID, "answer": "13.5"});

        assert_eq!(
            refusal(&app, None, right.clone()).await,
            (StatusCode::UNAUTHORIZED, "unauthorized".to_string())
        );
        assert_eq!(
            refusal(&app, Some(user), json!({"answer": "13.5"})).await,
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_request".to_string()
            )
        );
        let stale = json!({"problem_id": "p0000000000000000000000000000009"});
        assert_eq!(
            refusal(&app, Some(user), stale).await,
            (StatusCode::NOT_FOUND, "unknown_problem".to_string())
        );

        put_state(
            &db,
            user,
            &lesson_state(lesson_problem(5.0, "kp1", Vec::new()), 0, true),
        )
        .await;
        assert_eq!(
            refusal(&app, Some(user), right).await,
            (StatusCode::CONFLICT, "task_complete".to_string())
        );
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 0);
    })
    .await;
}

/// Spec section 5.1. A `multi-step` or a `proof` answer gets NO synchronous
/// verdict: the route refuses it, records nothing, and asks no model.
#[tokio::test]
async fn an_undecidable_kind_gets_no_verdict_and_records_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = proof_learner(&db, "proof@example.com").await;

        let proof = json!({"problem_id": PROBLEM_ID, "answer": "Assume the contrary."});
        assert_eq!(
            refusal(&app, Some(user), proof).await,
            (StatusCode::CONFLICT, "undecidable_kind".to_string())
        );
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 0);
    })
    .await;
}

/// The handler never panics on any body. Each one of these is an envelope, never
/// a 500 and never a dropped connection.
#[tokio::test]
async fn a_hostile_body_never_panics() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "hostile@example.com", 5.0).await;

        let bodies = [
            json!([]),
            json!("text"),
            json!(7),
            json!({"problem_id": 5}),
            json!({"problem_id": PROBLEM_ID, "answer": 5}),
            json!({"problem_id": PROBLEM_ID, "answer": "13.5", "work": []}),
            json!({"problem_id": PROBLEM_ID, "answer": "13.5", "assisted": "yes"}),
        ];
        for body in bodies {
            let (status, raw) = post_answer(&app, Some(user), Some(body.clone())).await;
            assert!(
                status == StatusCode::OK || status.is_client_error(),
                "{body} gave {status}: {raw}"
            );
        }

        // A request with no body at all is the same refusal, not a panic.
        let (status, raw) = post_answer(&app, Some(user), None).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{raw}");
        assert_eq!(parse(&raw)["error"]["code"], "invalid_request");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// M5 U11: the deterministic-grade counter (T6, spec section 7)
// --------------------------------------------------------------------------- //

/// Four verdicts, four `result` labels, read from `GET /metrics`.
///
/// The counter is the T6 half that costs no token: it says how many decisions
/// this service took with no model call at all. Each learner below answers once,
/// so each label carries the count 1.
#[tokio::test]
async fn every_deterministic_verdict_counts_its_own_grade_label() {
    TestDb::with(|db| async move {
        let app = app(&db);

        let right = learner_with_kp1(&db, "count-correct@example.com", 5.0).await;
        answer_lesson_ok(&app, right, EXPECTED_ANSWER).await;

        let wrong = learner_with_kp1(&db, "count-wrong@example.com", 5.0).await;
        answer_lesson_ok(&app, wrong, "14").await;

        let empty = learner_with_kp1(&db, "count-blank@example.com", 5.0).await;
        answer_lesson_ok(&app, empty, "   ").await;

        let mut grouped = lesson_problem(5.0, "kp1", Vec::new());
        grouped.expected.answer = "7329".to_string();
        let form = lesson_learner(&db, "count-notation@example.com", grouped).await;
        answer_lesson_ok(&app, form, "7.329").await;

        let text = scrape(&app).await;
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"correct\"} 1",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"incorrect\"} 1",
        );
        holds(&text, "cadus_deterministic_grade_total{result=\"blank\"} 1");
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"notation\"} 1",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"undecidable\"} 0",
        );
    })
    .await;
}

/// An answer kind with no deterministic verdict counts `undecidable`.
///
/// The route refuses the kind with `409 undecidable_kind` and asks no model, so
/// the refusal is the decision and the counter records it. Nothing is graded and
/// nothing is recorded, so no other label moves.
#[tokio::test]
async fn an_undecidable_kind_counts_one_undecidable_grade() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = proof_learner(&db, "count-proof@example.com").await;

        let proof = json!({"problem_id": PROBLEM_ID, "answer": "Assume the contrary."});
        let (status, body) = post_answer(&app, Some(user), Some(proof)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");

        let text = scrape(&app).await;
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"undecidable\"} 1",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"correct\"} 0",
        );
        holds(
            &text,
            "cadus_deterministic_grade_total{result=\"incorrect\"} 0",
        );
    })
    .await;
}
