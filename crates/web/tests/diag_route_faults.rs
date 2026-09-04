//! The placement diagnostic under a store fault.
//!
//! Each test makes ONE statement of a diagnostic call fail on purpose, and
//! reads the `500 internal_error` envelope back. The faults are triggers, row
//! policies and a closed pool on the throwaway database; no test double stands
//! between the handler and Postgres.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::learner::LearnerModel;
use cadus_web::state::WebState;
use common::placement::app;
use common::{
    Method, Router, SESSION, TestDb, Uuid, Value, assert_internal, call, events_of_type,
    fail_deletes, fail_reads, fail_writes, hold_state_lock, json, parse, put_state,
    seed_cached_model, seed_learner, seed_open_session, seed_unreadable_diagnostic,
};

/// A learner with an open session, a cached model at the head of the log, and
/// an empty D-S6 row.
async fn learner(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_cached_model(db, user, &LearnerModel::default(), 1).await;
    put_state(db, user, &WebState::for_session(SESSION)).await;
    user
}

/// Open a diagnostic of course `c1` for `user`, and read the first probe id.
async fn started(app: &Router, user: Uuid) -> String {
    let (status, body) = call(
        app,
        Method::POST,
        "/api/diag/start",
        Some(user),
        Some(json!({"course": "c1"})),
    )
    .await;
    assert_eq!(status.as_u16(), 200, "{body}");
    parse(&body)["probe"]["problem_id"]
        .as_str()
        .expect("the start dealt no probe")
        .to_owned()
}

/// The body of an answer to `problem_id`.
fn answer_body(problem_id: &str) -> Option<Value> {
    Some(json!({"problem_id": problem_id, "answer": "3"}))
}

/// Fail the test when the answer and the finish of `user` are not `500`.
async fn assert_answer_and_finish_fail(app: &Router, user: Uuid, problem_id: &str) {
    assert_internal(
        app,
        Method::POST,
        "/api/diag/answer",
        user,
        answer_body(problem_id),
    )
    .await;
    assert_internal(app, Method::POST, "/api/diag/finish", user, None).await;
}

/// A held advisory lock fails the answer and the finish at the lock wait.
#[tokio::test]
async fn a_held_lock_is_500_on_the_answer_and_the_finish() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-lock@example.com").await;
        let problem_id = started(&app, user).await;
        let held = hold_state_lock(&db, user).await;
        assert_answer_and_finish_fail(&app, user, &problem_id).await;
        drop(held);
    })
    .await;
}

/// A diagnostic read that fails stops the answer and the finish.
#[tokio::test]
async fn a_diagnostic_read_that_fails_is_500_on_the_answer_and_the_finish() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-read@example.com").await;
        let problem_id = started(&app, user).await;
        fail_reads(&db, "diag_states", "FROM diag_states WHERE").await;
        assert_answer_and_finish_fail(&app, user, &problem_id).await;
    })
    .await;
}

/// A diagnostic document that does not read is `409 no_diagnostic`.
#[tokio::test]
async fn a_diagnostic_document_that_does_not_read_is_409_no_diagnostic() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-doc@example.com").await;
        seed_unreadable_diagnostic(&db, user).await;
        let (status, body) = call(&app, Method::POST, "/api/diag/finish", Some(user), None).await;
        assert_eq!(status.as_u16(), 409, "{body}");
        assert_eq!(parse(&body)["error"]["code"], "no_diagnostic");
    })
    .await;
}

/// A D-S6 read that fails stops the three calls.
#[tokio::test]
async fn a_state_read_that_fails_is_500_on_the_three_calls() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-state@example.com").await;
        let problem_id = started(&app, user).await;
        fail_reads(&db, "web_states", "FROM web_states WHERE").await;
        assert_internal(&app, Method::POST, "/api/diag/start", user, None).await;
        assert_answer_and_finish_fail(&app, user, &problem_id).await;
    })
    .await;
}

/// A model read that fails stops the three calls.
#[tokio::test]
async fn a_model_read_that_fails_is_500_on_the_three_calls() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-model@example.com").await;
        let problem_id = started(&app, user).await;
        fail_reads(&db, "learner_models", "SELECT model AS").await;
        assert_internal(&app, Method::POST, "/api/diag/start", user, None).await;
        assert_answer_and_finish_fail(&app, user, &problem_id).await;
    })
    .await;
}

/// An event append that fails rolls each of the three calls back: the
/// `enrolled` of a first run, the `diagnostic_answer`, and the
/// `diagnostic_placed`.
#[tokio::test]
async fn an_event_append_that_fails_is_500_on_the_three_calls() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-append@example.com").await;
        let problem_id = started(&app, user).await;
        // A first run names no course and has none enrolled, so the start
        // appends `enrolled` first.
        let fresh = learner(&db, "diag-fault-append-first@example.com").await;
        fail_writes(&db, "events", "true").await;
        assert_answer_and_finish_fail(&app, user, &problem_id).await;
        assert_eq!(
            events_of_type(&db, user, "diagnostic_answer").await.len(),
            0
        );
        assert_eq!(
            events_of_type(&db, user, "diagnostic_placed").await.len(),
            0
        );
        assert_internal(&app, Method::POST, "/api/diag/start", fresh, None).await;
        assert_eq!(events_of_type(&db, fresh, "enrolled").await.len(), 0);
    })
    .await;
}

/// A diagnostic write that fails stops the start and the answer.
#[tokio::test]
async fn a_diagnostic_write_that_fails_is_500_on_the_start_and_the_answer() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-write@example.com").await;
        let problem_id = started(&app, user).await;
        fail_writes(&db, "diag_states", "true").await;
        assert_internal(&app, Method::POST, "/api/diag/start", user, None).await;
        assert_internal(
            &app,
            Method::POST,
            "/api/diag/answer",
            user,
            answer_body(&problem_id),
        )
        .await;
    })
    .await;
}

/// A diagnostic clear that fails stops the finish.
#[tokio::test]
async fn a_diagnostic_clear_that_fails_is_500_on_the_finish() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-clear@example.com").await;
        started(&app, user).await;
        fail_deletes(&db, "diag_states", "true").await;
        assert_internal(&app, Method::POST, "/api/diag/finish", user, None).await;
        assert_eq!(
            events_of_type(&db, user, "diagnostic_placed").await.len(),
            0
        );
    })
    .await;
}

/// A D-S6 write that fails stops the finish.
#[tokio::test]
async fn a_state_write_that_fails_is_500_on_the_finish() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "diag-fault-state-write@example.com").await;
        started(&app, user).await;
        fail_writes(&db, "web_states", "true").await;
        assert_internal(&app, Method::POST, "/api/diag/finish", user, None).await;
    })
    .await;
}
