//! The whole-quiz clock (QUIZ-budget) of fix unit FIX2-M6-G, finding V6 of
//! `docs/reviews/M6-review-2.md`, and the two refusals of the quiz on the
//! serve and hint routes.
//!
//! The clock is D-S6 state of the quiz task:
//!
//! 5. the first serve stamps the clock —
//!    [`a_quiz_serve_stamps_the_whole_quiz_clock_and_reports_no_time_gone`];
//! 6. a reload resumes it —
//!    [`a_reload_of_a_running_quiz_resumes_the_clock_and_keeps_the_first_stamp`];
//! 7. an open quiz written before the field —
//!    [`an_open_quiz_with_no_stamp_starts_its_clock_at_the_next_serve`].
//!
//! The header of `quiz_route.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use cadus_web::state::WebState;
use common::{
    PROBLEM_ID, QUIZ, SESSION, assert_refused, exhausted_quiz_progress, first_question, hint_task,
    now_secs, parse, put_doc, put_state, quiz_app as app, quiz_learner, seed_quiz_learner,
    serve_ok, serve_raw, serve_task, stored_state,
};
use serde_json::json;
use sqlx::types::Uuid;

/// Overwrite the whole-quiz stamp of `user`, and give back the value written.
///
/// It is how a test moves the clock: the route reads the stamp out of the D-S6
/// row, so a stamp `secs` in the past is a quiz that has run for `secs`.
async fn move_the_quiz_clock_back(db: &TestDb, user: Uuid, secs: f64) -> f64 {
    let started_at = now_secs() - secs;
    let mut scratch = stored_state(db, user).await;
    scratch
        .quizzes
        .get_mut(QUIZ)
        .unwrap()
        .started_at
        .replace(started_at);
    put_state(db, user, &scratch).await;
    started_at
}

/// The whole-quiz clock is SERVER state, and the serve payload carries it.
///
/// The first serve of the quiz stamps the clock and reports zero seconds gone.
/// Both values come from the one instant the handler takes, so the count is an
/// exact literal and not a measurement.
///
/// The payload gains this EIGHTH key and loses none:
/// `crates/web/tests/serve_routes.rs` pins the seven keys of a lesson serve, so
/// a key emitted off a quiz fails there.
#[tokio::test]
async fn a_quiz_serve_stamps_the_whole_quiz_clock_and_reports_no_time_gone() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_quiz_learner(&db, "quiz-clock-start@example.com").await;

        let (status, raw) = serve_raw(&app, user, QUIZ).await;
        assert_eq!(status, StatusCode::OK, "{raw}");

        let body = parse(&raw);
        let keys: Vec<&str> = body
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            vec![
                "countdown",
                "index",
                "kp",
                "problem_id",
                "quiz_elapsed_secs",
                "text",
                "time_budget_secs",
                "total",
            ]
        );
        assert_eq!(body["quiz_elapsed_secs"], 0);
        assert_eq!(body["index"], 1);
        assert_eq!(body["total"], 2);

        // The stamp went into the quiz buffer, which is where a later serve
        // reads it from. The buffer holds no answer yet.
        let scratch = stored_state(&db, user).await;
        let buffer = scratch.quizzes.get(QUIZ).unwrap();
        assert_eq!(buffer.answers.len(), 0);
        assert!(buffer.started_at.is_some(), "the serve stamped no clock");
    })
    .await;
}

/// A RELOAD resumes the running clock, because the clock is on the wire.
///
/// A reload builds a new client, so the client-side registry of unit FIX2-M6-D
/// is empty and every clock it holds is gone. The serve answers with the seconds
/// the quiz has run, and the second serve keeps the FIRST stamp: a reload that
/// re-stamped the clock hands the whole budget back, which is the defect
/// (M6-review-2, V6).
///
/// The elapsed count is a literal 90. The handler takes one instant and the
/// stamp is 90.0 seconds before it, so the floor reads 90 unless the request
/// takes a whole second; one serve against the test database takes tens of
/// milliseconds.
#[tokio::test]
async fn a_reload_of_a_running_quiz_resumes_the_clock_and_keeps_the_first_stamp() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_quiz_learner(&db, "quiz-clock-reload@example.com").await;

        let first = serve_ok(&app, user, QUIZ).await;
        assert_eq!(first["quiz_elapsed_secs"], 0);
        let problem_id = first["problem_id"].clone();

        // 90 seconds of the quiz are gone, and the learner reloads the page.
        let started_at = move_the_quiz_clock_back(&db, user, 90.0).await;
        let body = serve_ok(&app, user, QUIZ).await;
        assert_eq!(body["quiz_elapsed_secs"], 90);
        // The reload re-serves the SAME question (section 5.6), so the reload
        // spends no question either.
        assert_eq!(body["problem_id"], problem_id);
        assert_eq!(body["index"], 1);

        // The stamp did NOT move. Only the per-problem clock is re-stamped.
        let scratch = stored_state(&db, user).await;
        assert_eq!(
            scratch.quizzes.get(QUIZ).unwrap().started_at,
            Some(started_at)
        );
        assert!(scratch.served.get(QUIZ).unwrap().started_at > started_at);
    })
    .await;
}

/// An OPEN quiz written before this field keeps working, and its clock starts at
/// the next serve.
///
/// The D-S6 document is jsonb, so there is no migration: a row whose buffer
/// names no `started_at` reads as an unstamped clock. The serve stamps it and
/// reports no time gone, which is what the learner sees today.
#[tokio::test]
async fn an_open_quiz_with_no_stamp_starts_its_clock_at_the_next_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_quiz_learner(&db, "quiz-clock-legacy@example.com").await;

        // The row of an older build: one buffered answer, and NO clock key at
        // all. The key is removed from the raw document, not set to null.
        let mut scratch = WebState::for_session(SESSION);
        scratch
            .quizzes
            .entry(QUIZ.to_string())
            .or_default()
            .answers
            .push(json!({"problem_id": PROBLEM_ID, "correct": true}));
        let mut doc = scratch.to_doc();
        let removed = doc["quizzes"][QUIZ]
            .as_object_mut()
            .unwrap()
            .remove("started_at");
        assert!(removed.is_some(), "the buffer wrote no started_at key");
        assert!(!doc.to_string().contains("started_at"), "{doc}");
        put_doc(&db, user, doc).await;

        let body = serve_ok(&app, user, QUIZ).await;
        assert_eq!(body["quiz_elapsed_secs"], 0);

        // The stamp is in now, and the buffered answer survived it.
        let stored = stored_state(&db, user).await;
        let buffer = stored.quizzes.get(QUIZ).unwrap();
        assert!(buffer.started_at.is_some());
        assert_eq!(buffer.answers.len(), 1);
        assert_eq!(buffer.answers[0]["problem_id"], PROBLEM_ID);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The quiz refusals of the serve and hint routes
// --------------------------------------------------------------------------- //

/// A quiz reveals nothing until the batch reveal, so a hint inside one is
/// `409 no_hints_in_quiz` (`api.py:1813`), and the live question keeps no hint.
#[tokio::test]
async fn a_hint_inside_a_quiz_is_409_no_hints_in_quiz() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = quiz_learner(&db, "quiz-hint@example.com", first_question()).await;

        assert_refused(
            &hint_task(&app, user, QUIZ, PROBLEM_ID).await,
            StatusCode::CONFLICT,
            "no_hints_in_quiz",
        );
        let stored = stored_state(&db, user).await;
        assert_eq!(stored.served[QUIZ].hints_given.len(), 0);
    })
    .await;
}

/// A quiz whose every question is answered and whose task row is still open
/// has no further question to serve: `409 quiz_exhausted` (`api.py:244`).
#[tokio::test]
async fn a_serve_past_the_last_quiz_question_is_409_quiz_exhausted() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_quiz_learner(&db, "quiz-exhausted@example.com").await;
        let mut scratch = stored_state(&db, user).await;
        scratch
            .tasks
            .insert(QUIZ.to_string(), exhausted_quiz_progress());
        put_state(&db, user, &scratch).await;

        assert_refused(
            &serve_task(&app, user, QUIZ).await,
            StatusCode::CONFLICT,
            "quiz_exhausted",
        );
        // The refusal rolled the serve back: no question went live.
        assert!(!stored_state(&db, user).await.served.contains_key(QUIZ));
    })
    .await;
}
