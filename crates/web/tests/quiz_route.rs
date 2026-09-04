//! The quiz branch of `POST /api/task/{task_id}/answer`, over HTTP.
//!
//! Requirements: A3, C2, C3, C4, D-S6, R4. Spec
//! `docs/reference/web-service-1.0-spec.md` section 2.1, section 4.3, and trap
//! W7 of section 9: "Quiz batch reveal: no feedback and no `expected` in any
//! pre-reveal body — test by scanning raw JSON, not by reading fields."
//!
//! Fix unit FIX-M5-T, finding F13 of `docs/reviews/M5-review-1.md`. The quiz
//! branch of `crates/web/src/grade.rs` had no test at the route, so the rule
//! that a quiz reveals nothing before its batch reveal stood on one unexecuted
//! guard. This file drives it over HTTP:
//!
//! 1. the bare receipt of one quiz answer —
//!    [`a_quiz_answer_replies_with_a_bare_receipt`];
//! 2. a MISS that hides its verdict, while the log keeps it —
//!    [`a_wrong_quiz_answer_hides_its_verdict_before_the_reveal`];
//! 3. the `assisted` flag that reveals nothing either —
//!    [`an_assisted_flag_on_a_quiz_answer_reveals_nothing`];
//! 4. the last answer of the batch, and the buffer the reveal reads —
//!    [`the_last_quiz_answer_completes_the_batch_and_buffers_the_reveal`].
//!
//! `quiz_route_clock.rs` holds the whole-quiz clock of fix unit FIX2-M6-G.
//!
//! Every check scans the RAW body text, and every expected value is a LITERAL:
//! a literal status code, a literal key count, a literal remaining count, a
//! literal `correct` in the log. Nothing here is read back from the code under
//! test.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::{
    EXPECTED_ANSWER, PROBLEM_ID, PROBLEM_TEXT, PROBLEM_TWO, QUIZ, SOLUTION, SOLUTION_TWO, TEXT_TWO,
    answer_raw, assert_key_count, assert_reveals_nothing, events_of_type, first_question, parse,
    put_quiz_live, quiz_app as app, quiz_learner, second_question, stored_state,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// Answer the live quiz question with `body`, and give the RAW `200` reply
/// back after the scan of trap W7.
async fn answer_hidden(app: &axum::Router, user: Uuid, body: Value) -> String {
    let (status, raw) = answer_raw(app, user, QUIZ, body).await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    assert_reveals_nothing(&raw);
    raw
}

// --------------------------------------------------------------------------- //
// 1. The bare receipt
// --------------------------------------------------------------------------- //

/// One quiz answer gets the bare receipt of three fields and nothing else.
///
/// The answer is CORRECT. Every other task type replies to that with a verdict,
/// a solution sketch and the next problem. A quiz replies with `accepted`, the
/// remaining count, and the batch flag.
#[tokio::test]
async fn a_quiz_answer_replies_with_a_bare_receipt() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = quiz_learner(&db, "quiz-receipt@example.com", first_question()).await;

        let raw = answer_hidden(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
        )
        .await;

        let body = assert_key_count(&raw, 3);
        assert_eq!(body["accepted"], true);
        assert_eq!(body["remaining"], 1);
        assert_eq!(body["quiz_complete"], false);

        // The attempt IS recorded, and the verdict lives in the log alone.
        let log = events_of_type(&db, user, "attempt").await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["attempt_id"], "s_2026-01-01a-quiz-1");
        assert_eq!(log[0]["task_type"], "quiz");
        assert_eq!(log[0]["correct"], true);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 2. The hidden verdict
// --------------------------------------------------------------------------- //

/// A WRONG quiz answer gets the same bare receipt: no verdict, no re-solve text,
/// no solution sketch. The log records the miss, and the buffer holds the
/// hidden sketch for the batch reveal.
#[tokio::test]
async fn a_wrong_quiz_answer_hides_its_verdict_before_the_reveal() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = quiz_learner(&db, "quiz-miss@example.com", first_question()).await;

        let raw = answer_hidden(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;

        let body = assert_key_count(&raw, 3);
        assert_eq!(body["accepted"], true);
        assert_eq!(body["quiz_complete"], false);

        let log = events_of_type(&db, user, "attempt").await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["correct"], false);
        assert_eq!(log[0]["given_answer"], "14");

        // The question left the live slot, and the buffer took the answer with
        // the sketch the reveal hands out later.
        let scratch = stored_state(&db, user).await;
        assert!(!scratch.served.contains_key(QUIZ));
        let buffered = &scratch.quizzes.get(QUIZ).unwrap().answers;
        assert_eq!(buffered.len(), 1);
        assert_eq!(buffered[0]["problem_id"], PROBLEM_ID);
        assert_eq!(buffered[0]["correct"], false);
        assert_eq!(buffered[0]["solution_sketch"], SOLUTION);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 3. The `assisted` flag
// --------------------------------------------------------------------------- //

/// A client that sends `"assisted": true` with a CORRECT quiz answer gets the
/// bare receipt too.
///
/// On every other task type that pair is the H3 first branch, and its reply
/// names `expected` and `solution` (spec section 5.4). A quiz is never assisted,
/// so the branch does not run and the authored answer stays hidden.
#[tokio::test]
async fn an_assisted_flag_on_a_quiz_answer_reveals_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = quiz_learner(&db, "quiz-assisted@example.com", first_question()).await;

        let raw = answer_hidden(
            &app,
            user,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER, "assisted": true}),
        )
        .await;

        let body = assert_key_count(&raw, 3);
        assert_eq!(body["remaining"], 1);

        // The attempt is recorded once, and it is NOT flagged as assisted.
        let log = events_of_type(&db, user, "attempt").await;
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["assisted"], false);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// 4. The last answer of the batch
// --------------------------------------------------------------------------- //

/// The last answer closes the batch: `remaining` reaches 0 and `quiz_complete`
/// turns true, still with no verdict in the body.
///
/// The buffer then holds both answers, each with its outcome and its hidden
/// sketch. That buffer is what the batch reveal reads, so it is the reveal that
/// the pre-reveal bodies withheld.
#[tokio::test]
async fn the_last_quiz_answer_completes_the_batch_and_buffers_the_reveal() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = quiz_learner(&db, "quiz-last@example.com", first_question()).await;

        let (status, first) = answer_raw(
            &app,
            user,
            QUIZ,
            json!({"problem_id": PROBLEM_ID, "answer": EXPECTED_ANSWER}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(parse(&first)["remaining"], 1);
        assert_eq!(parse(&first)["quiz_complete"], false);

        put_quiz_live(&db, user, second_question()).await;
        let last = answer_hidden(
            &app,
            user,
            json!({"problem_id": PROBLEM_TWO, "answer": "12"}),
        )
        .await;

        let body = assert_key_count(&last, 3);
        assert_eq!(body["accepted"], true);
        assert_eq!(body["remaining"], 0);
        assert_eq!(body["quiz_complete"], true);

        // Two attempts stand in the log, in serve order.
        let log = events_of_type(&db, user, "attempt").await;
        assert_eq!(log.len(), 2);
        assert_eq!(log[0]["attempt_id"], "s_2026-01-01a-quiz-1");
        assert_eq!(log[1]["attempt_id"], "s_2026-01-01a-quiz-2");

        // The task is closed, and the buffer outlives the close: it carries the
        // whole reveal.
        let scratch = stored_state(&db, user).await;
        assert!(scratch.tasks.get(QUIZ).unwrap().done);
        let buffered = &scratch.quizzes.get(QUIZ).unwrap().answers;
        assert_eq!(buffered.len(), 2);
        assert_eq!(buffered[0]["text"], PROBLEM_TEXT);
        assert_eq!(buffered[0]["correct"], true);
        assert_eq!(buffered[0]["solution_sketch"], SOLUTION);
        assert_eq!(buffered[1]["text"], TEXT_TWO);
        assert_eq!(buffered[1]["given_answer"], "12");
        assert_eq!(buffered[1]["correct"], false);
        assert_eq!(buffered[1]["solution_sketch"], SOLUTION_TWO);
    })
    .await;
}
