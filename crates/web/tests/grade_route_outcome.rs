//! Unit f4-outcome, step 2: the grade route grades EVERY answer kind, and the
//! kind the checker does not decide gives the third outcome (D-F1, D-F2, D-F4).
//!
//! Audit finding (a): the route accepted `numeric` and `expression` only, and a
//! `multi-step` or a `proof` problem answered `409 undecidable_kind`, so the
//! learner got no outcome at all. Audit finding (c): the verdict folded an
//! undecidable answer into `correct: false`, so a learner read "wrong".

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_store::test_support::TestDb;
use common::{
    LESSON, PROBLEM_ID, answer_task, events_of_type, lesson_app as app, lesson_learner,
    lesson_problem,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// A learner whose live lesson problem carries `kind` and expects `expected`.
async fn learner_of(db: &TestDb, email: &str, kind: &str, expected: &str) -> Uuid {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.answer_kind = Some(kind.to_string());
    live.expected.answer = expected.to_string();
    lesson_learner(db, email, live).await
}

/// Answer the live lesson problem and read the reply body.
async fn answer(app: &axum::Router, user: Uuid, given: &str) -> Value {
    let body = json!({"problem_id": PROBLEM_ID, "answer": given});
    let (status, reply) = answer_task(app, user, LESSON, body).await;
    assert_eq!(status.as_u16(), 200, "{reply}");
    reply
}

/// Every reply names the outcome, and every task hands back a next step.
fn holds_an_outcome(body: &Value) {
    assert!(body["outcome"].is_string(), "{body}");
    let has_next = body["next"].is_object() || body["next_unavailable"] == json!(true);
    assert!(has_next, "the learner must get the next task: {body}");
}

// --------------------------------------------------------------------------- //
// One test per answer kind (audit finding a)
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn a_numeric_answer_is_decided_correct() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_of(&db, "kind-numeric@example.com", "numeric", "13.5").await;
        let body = answer(&app, user, "13.5").await;
        assert_eq!(body["outcome"], "correct");
        assert_eq!(body["correct"], json!(true));
        assert!(body.get("reason").is_none(), "{body}");
        holds_an_outcome(&body);
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);
    })
    .await;
}

#[tokio::test]
async fn an_expression_answer_is_decided_incorrect() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_of(&db, "kind-expression@example.com", "expression", "2*x").await;
        let body = answer(&app, user, "3*x").await;
        assert_eq!(body["outcome"], "incorrect");
        assert_eq!(body["correct"], json!(false));
        // A decided miss still reveals the worked solution and the re-solve step.
        assert!(body["solution"].is_string(), "{body}");
        assert!(body["re_solve"].is_string(), "{body}");
        holds_an_outcome(&body);
    })
    .await;
}

#[tokio::test]
async fn a_multi_step_answer_reaches_the_checker_and_is_ungraded() {
    TestDb::with(|db| async move {
        let app = app(&db);
        // An ordinary integer final answer. The route no longer refuses the kind;
        // the checker itself says it has no verdict for it yet (unit f3 adds the
        // per-item contract that decides this one).
        let user = learner_of(&db, "kind-multistep@example.com", "multi-step", "12").await;
        let body = answer(&app, user, "12").await;
        assert_eq!(body["outcome"], "ungraded");
        assert_eq!(body["reason"], "the answer kind is not decidable");
        assert!(body.get("correct").is_none(), "{body}");
        holds_an_outcome(&body);
        // The attempt is recorded, so the recovery path can regrade it.
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);
    })
    .await;
}

#[tokio::test]
async fn a_proof_answer_is_ungraded_without_a_checker_call() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_of(&db, "kind-proof@example.com", "proof", "13.5").await;
        // The answer equals the authored answer. A checker call would decide it
        // correct; the route never makes one for a proof (V2, D-F1).
        let body = answer(&app, user, "13.5").await;
        assert_eq!(body["outcome"], "ungraded");
        assert_eq!(body["reason"], "no deterministic verdict for a proof");
        assert!(body.get("correct").is_none(), "{body}");
        holds_an_outcome(&body);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The ungraded reply reveals nothing and diagnoses nothing
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn an_ungraded_reply_reveals_no_solution_and_fires_no_diagnosis() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_of(&db, "ungraded-quiet@example.com", "proof", "13.5").await;
        let body = answer(&app, user, "Assume the contrary.").await;
        // Hard Rule 1 holds: an ungraded attempt is not a miss, so no answer and
        // no re-solve instruction leave the route.
        assert!(body.get("solution").is_none(), "{body}");
        assert!(body.get("re_solve").is_none(), "{body}");
        // D-F4: no model-assisted diagnosis on an ungraded attempt.
        assert_eq!(body["diagnosis"]["status"], "not_offered");
        // The lesson does not advance on an ungraded attempt.
        assert_eq!(body["task_status"], "continue");
        assert!(body.get("xp").is_none(), "{body}");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Audit finding (d): a rational exponent (unit f2 makes this one correct)
// --------------------------------------------------------------------------- //

/// The f2 grammar recognizes a rational exponent as an equivalent radical.
/// The HTTP outcome must expose the successful grade.
#[tokio::test]
async fn a_rational_exponent_is_never_told_it_is_wrong() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_of(
            &db,
            "rational-exponent@example.com",
            "expression",
            "sqrt(2)",
        )
        .await;
        let body = answer(&app, user, "2^(1/2)").await;
        assert_ne!(body["outcome"], "incorrect", "{body}");
        assert_ne!(body["correct"], json!(false), "{body}");
        assert_eq!(body["outcome"], "correct");
        assert_eq!(body["correct"], json!(true));
        assert!(body.get("reason").is_none(), "{body}");
    })
    .await;
}
