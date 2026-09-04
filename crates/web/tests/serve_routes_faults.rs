//! M5 U7: the serve under a store fault (D-O1, one transaction).
//!
//! Each test makes ONE statement of the serve transaction fail on purpose, and
//! reads two things back: the `500 internal_error` envelope, and a database
//! that holds nothing of the serve. The faults are triggers on the throwaway
//! database; no test double stands between the handler and Postgres.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::{
    DRILL, KEY, LESSON, POOL_ANSWER, POOL_TEXT, assert_refused, claimed_rows, drill_app as app,
    fail_commit_after_insert, fail_writes, hint_task, problem_id_of, seed_content, seed_drill_due,
    seed_learner, seed_open_session, seed_pool_row, serve_ok, serve_task, stored_state,
    task_served_rows,
};
use serde_json::json;
use sqlx::types::Uuid;

/// A learner with an open session and a due drill.
async fn drill_learner(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_drill_due(db, user).await;
    user
}

/// Serve the drill of `user` under a fault: the serve is `500 internal_error`,
/// no cadence row stands in the log, and no pool row is claimed.
async fn assert_serve_rolls_back(db: &TestDb, user: Uuid) {
    assert_refused(
        &serve_task(&app(db), user, DRILL).await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    );
    assert_eq!(task_served_rows(db, user, DRILL).await, 0);
    assert_eq!(claimed_rows(db, user).await, 0);
}

/// The `task_served` append fails: the serve records nothing.
#[tokio::test]
async fn a_task_served_append_that_fails_records_nothing() {
    TestDb::with(|db| async move {
        let user = drill_learner(&db, "fault-append@example.com").await;
        fail_writes(&db, "events", "NEW.type = 'task_served'").await;
        assert_serve_rolls_back(&db, user).await;
    })
    .await;
}

/// The COMMIT fails after the append: the whole serve rolls back with it.
#[tokio::test]
async fn a_commit_that_fails_rolls_the_whole_serve_back() {
    TestDb::with(|db| async move {
        let user = drill_learner(&db, "fault-commit@example.com").await;
        fail_commit_after_insert(&db, "events", "NEW.type = 'task_served'").await;
        assert_serve_rolls_back(&db, user).await;
    })
    .await;
}

/// The D-S6 write fails on the hint route: the hint is `500 internal_error`
/// and the live problem records no hint.
#[tokio::test]
async fn a_state_write_that_fails_records_no_hint() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_learner(&db, "fault-hint@example.com").await;
        seed_open_session(&db, user).await;
        seed_pool_row(&db, user, KEY, POOL_TEXT, POOL_ANSWER, "hash-a").await;
        seed_content(
            &db,
            KEY,
            "hint_ladder",
            "digest-hints",
            json!({"hints": ["One."]}),
        )
        .await;
        let problem_id = problem_id_of(&serve_ok(&app, user, LESSON).await);
        fail_writes(&db, "web_states", "true").await;

        assert_refused(
            &hint_task(&app, user, LESSON, &problem_id).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
        );
        assert_eq!(
            stored_state(&db, user).await.served[LESSON]
                .hints_given
                .len(),
            0
        );
    })
    .await;
}
