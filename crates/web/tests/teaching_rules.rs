//! The teaching rules of D-L5 over HTTP: a non-answer and a give-up are decided
//! misses, a second miss names the lesson page, and a corrective loop on a review
//! ends after two failed fresh items.
#![allow(clippy::unwrap_used)]
mod common;

use axum::Router;
use axum::http::StatusCode;
use cadus_core::curriculum::review_context_digest;
use cadus_store::test_support::TestDb;
use cadus_web::state::WebState;
use common::*;
use serde_json::{Value, json};
use sqlx::types::Uuid;

async fn learner(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_due_review(db, user).await;
    put_state(db, user, &WebState::for_session(SESSION)).await;
    let curriculum = addition_curriculum(Vec::new());
    let digest = review_context_digest(&curriculum).unwrap();
    for kp in ["kp1", "kp2"] {
        for index in 0..8 {
            seed_pool_row(
                db,
                user,
                &format!("addition/{kp}"),
                &format!("Give {kp} value {index}."),
                &index.to_string(),
                &format!("rules-{kp}-{index}"),
                (&digest, cadus_core::review_engine::DIGEST),
            )
            .await;
        }
    }
    user
}

/// Answer the live problem of the review with `body` merged over its problem id.
async fn answer_with(app: &Router, db: &TestDb, user: Uuid, extra: Value) -> Value {
    let live = stored_state(db, user).await.served[REVIEW].clone();
    let mut body = json!({ "problem_id": live.problem_id });
    for (key, value) in extra.as_object().unwrap() {
        body[key] = value.clone();
    }
    let reply = answer_task(app, user, REVIEW, body).await;
    assert_eq!(reply.0, StatusCode::OK, "{}", reply.1);
    reply.1
}

/// Rule 1: a typed word is a decided miss with the tag `non_answer`. The reply
/// asks for the worked solution (`re_solve`; the fixture items author no sketch)
/// and carries the line that names what was asked for.
#[tokio::test]
async fn a_typed_word_is_a_decided_miss_with_the_solution() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner(&db, "rules-word@example.com").await;
        serve_ok(&app, user, REVIEW).await;
        let reply = answer_with(&app, &db, user, json!({ "answer": "idk" })).await;
        assert_eq!(reply["outcome"], "incorrect");
        assert_eq!(reply["correct"], false);
        assert_eq!(reply["error_tags"], json!(["non_answer"]));
        assert_eq!(reply["notation"], "That is not a number.");
        assert!(reply["re_solve"].is_string());
        assert_eq!(reply["feedback_practice"], true);
        assert!(reply["diagnosis"]["status"] == "not_offered");
        let attempts = events_of_type(&db, user, "attempt").await;
        assert_eq!(attempts.len(), 1);
        assert_eq!(attempts[0]["correct"], false);
        assert_eq!(attempts[0]["error_tags"], json!(["non_answer"]));
    })
    .await;
}

/// Rule 6: the give-up records a miss with the tag `gave_up` and shows the solution.
#[tokio::test]
async fn the_give_up_records_a_miss_and_shows_the_solution() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner(&db, "rules-giveup@example.com").await;
        serve_ok(&app, user, REVIEW).await;
        let reply = answer_with(&app, &db, user, json!({ "answer": "", "gave_up": true })).await;
        assert_eq!(reply["outcome"], "incorrect");
        assert_eq!(reply["error_tags"], json!(["gave_up"]));
        assert!(reply["re_solve"].is_string());
        assert_eq!(reply["feedback_practice"], true);
        assert!(reply.get("notation").is_none());
        let attempts = events_of_type(&db, user, "attempt").await;
        assert_eq!(attempts[0]["error_tags"], json!(["gave_up"]));
        assert_eq!(attempts[0]["given_answer"], "");
    })
    .await;
}

/// Rules 3 and 4: the second miss in a row names the lesson page, and the third
/// miss (the second failed fresh item) closes the review with a failed result.
#[tokio::test]
async fn two_failed_fresh_items_end_the_loop_with_a_failed_result() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner(&db, "rules-loop@example.com").await;
        serve_ok(&app, user, REVIEW).await;
        let first = answer_with(&app, &db, user, json!({ "answer": "99999" })).await;
        assert!(first.get("teach_first").is_none());
        assert_eq!(first["task_status"], "continue");
        let second = answer_with(&app, &db, user, json!({ "answer": "99999" })).await;
        assert_eq!(second["teach_first"]["topic"], "addition");
        assert_eq!(second["teach_first"]["kp"], "kp1");
        assert_eq!(second["task_status"], "continue");
        assert!(events_of_type(&db, user, "review_result").await.is_empty());
        let third = answer_with(&app, &db, user, json!({ "answer": "99999" })).await;
        assert!(third.get("teach_first").is_none());
        assert_eq!(third["task_status"], "task_failed");
        assert!(third.get("feedback_practice").is_none());
        let closes = events_of_type(&db, user, "review_result").await;
        assert_eq!(closes.len(), 1);
        assert_eq!(closes[0]["passed"], false);
        assert_eq!(closes[0]["task_id"], REVIEW);
        let scratch = stored_state(&db, user).await;
        assert!(scratch.tasks[REVIEW].done);
        assert!(!scratch.feedback_practice.contains_key(REVIEW));
        assert_refused(
            &serve_task(&app, user, REVIEW).await,
            StatusCode::CONFLICT,
            "task_complete",
        );
    })
    .await;
}

/// A right fresh item keeps the loop open: misses that a right answer separates
/// do not end the review.
#[tokio::test]
async fn a_right_answer_keeps_the_loop_open() {
    TestDb::with(|db| async move {
        let app = lesson_app(&db);
        let user = learner(&db, "rules-open@example.com").await;
        serve_ok(&app, user, REVIEW).await;
        answer_with(&app, &db, user, json!({ "answer": "99999" })).await;
        let live = stored_state(&db, user).await.served[REVIEW].clone();
        let right = answer_with(&app, &db, user, json!({ "answer": live.expected.answer })).await;
        assert_eq!(right["correct"], true);
        assert_eq!(right["task_status"], "continue");
        let wrong = answer_with(&app, &db, user, json!({ "answer": "99999" })).await;
        assert_eq!(wrong["task_status"], "continue");
        assert!(wrong.get("teach_first").is_none());
        assert!(events_of_type(&db, user, "review_result").await.is_empty());
    })
    .await;
}
