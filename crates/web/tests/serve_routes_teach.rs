//! M5 U7 acceptance, the teach page: acceptance check 4 of row U7.
//!
//! 4. teach on a review is `409 no_instruction` —
//!    [`teach_on_a_review_is_409_no_instruction`].
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_core::instruction::{InstructionSpec, gate_teach};
use cadus_store::test_support::TestDb;
use common::{
    EXEMPLAR_TEXT_2, EXPECTED_ANSWER, KEY, LESSON, PROBLEM_TEXT, REVIEW, assert_refused,
    drill_app as app, exemplar, model_calls, seed_content, seed_due_review, seed_learner,
    seed_open_session, teach_task,
};
use serde_json::json;

/// Approve the one-step teach page of `KEY`.
async fn seed_teach_page(db: &TestDb) {
    seed_content(
        db,
        KEY,
        "teach",
        "digest-teach",
        json!({
            "concept": "Addition combines two counts.",
            "worked_example": {"problem": "Compute 2 + 3.", "steps": ["2 + 3 = 5."]}
        }),
    )
    .await;
}

/// Only a lesson teaches. A review is `409 no_instruction`, and so is a lesson
/// whose knowledge point has no APPROVED teach page: in both cases the server
/// has no worked example, and it never asks a model for one (L4, T1).
#[tokio::test]
async fn teach_on_a_review_is_409_no_instruction() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "teachreview@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_due_review(&db, user).await;
        seed_teach_page(&db).await;

        assert_refused(
            &teach_task(&app, user, REVIEW).await,
            StatusCode::CONFLICT,
            "no_instruction",
        );
    })
    .await;
}

/// A lesson serves the authored page verbatim, and a lesson with no approved
/// page takes the same `409 no_instruction`.
#[tokio::test]
async fn teach_on_a_lesson_serves_the_authored_page() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "teach@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        assert_refused(
            &teach_task(&app, user, LESSON).await,
            StatusCode::CONFLICT,
            "no_instruction",
        );

        seed_teach_page(&db).await;

        let (status, page) = teach_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["kp"], "kp1");
        assert_eq!(page["concept"], "Addition combines two counts.");
        assert_eq!(page["worked_example"]["problem"], "Compute 2 + 3.");
        assert_eq!(page["worked_example"]["steps"][0], "2 + 3 = 5.");

        // D-O3: the teach read writes nothing.
        let rows = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(rows, 0, "the teach read installed a state row");
    })
    .await;
}

/// M6 R6 acceptance, the third check: an APPROVED teach body serves through the
/// M5 teach route with no model call (L4, L5, T1).
///
/// The page the route serves is the output of the M6 authoring gate
/// (`cadus_core::instruction::gate_teach`), so the two halves of the unit meet
/// here: what the gate accepts is what the route reads, field for field.
///
/// "No model call" is a LITERAL count. Every model call of 2.0 writes one
/// `model_call_log` row per HTTP attempt (T6, `docs/plans/M5.md:31-33`), so a
/// request tier that spent a token leaves a row. The count is 0 before the
/// request and 0 after it. `crates/web/tests/purity.rs` holds the other half of
/// the rule: `cadus-web` declares no dependency on the model client (L6).
#[tokio::test]
async fn an_approved_teach_page_from_the_gate_serves_with_no_model_call() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "authoredteach@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        // The tool arguments of one authoring attempt, as the model emits them.
        let arguments = r#"{
            "concept": "To add a whole number and a decimal, line up the decimal points.",
            "worked_example": {
                "problem": "Compute 6 + 2.25.",
                "steps": [
                    "Write 6 as 6.00, so both numbers carry two decimal places.",
                    "Add the hundredths, the tenths, then the ones: $6.00 + 2.25 = 8.25$."
                ]
            }
        }"#;
        let exemplars = vec![
            exemplar(PROBLEM_TEXT, EXPECTED_ANSWER),
            exemplar(EXEMPLAR_TEXT_2, "13.25"),
        ];
        let page = gate_teach(
            arguments,
            &InstructionSpec {
                exemplars: &exemplars,
                instance_answers: Vec::new(),
            },
        )
        .expect("the gate accepts the page");
        let body = serde_json::to_value(&page).unwrap();
        seed_content(&db, KEY, "teach", "sha256:authored-teach", body).await;

        assert_eq!(model_calls(&db).await, 0);

        let (status, served) = teach_task(&app, user, LESSON).await;

        assert_eq!(status, StatusCode::OK, "{served}");
        assert_eq!(served["kp"], "kp1");
        assert_eq!(
            served["concept"],
            "To add a whole number and a decimal, line up the decimal points."
        );
        assert_eq!(served["worked_example"]["problem"], "Compute 6 + 2.25.");
        assert_eq!(
            served["worked_example"]["steps"][0],
            "Write 6 as 6.00, so both numbers carry two decimal places."
        );
        assert_eq!(
            served["worked_example"]["steps"][1],
            "Add the hundredths, the tenths, then the ones: $6.00 + 2.25 = 8.25$."
        );
        assert_eq!(
            served["worked_example"]["steps"].as_array().unwrap().len(),
            2
        );

        // T1: the route spent no model token.
        assert_eq!(model_calls(&db).await, 0);
    })
    .await;
}
