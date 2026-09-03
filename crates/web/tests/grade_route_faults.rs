//! M5 U8: the grade path under a store fault (D-O2, one transaction).
//!
//! Each test makes ONE statement of the transaction fail on purpose, from the
//! attempt INSERT to the COMMIT, and reads two things back: the
//! `500 internal_error` envelope, and a log that holds nothing of the grade.
//! The faults are triggers, a row policy and a REVOKE on the throwaway
//! database; no test double stands between the handler and Postgres.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use axum::http::StatusCode;
use cadus_core::learner::LearnerModel;
use cadus_store::test_support::TestDb;
use common::{
    Verdict, answer_lesson, events_of_type, fail_commit_after_insert, fail_reads, fail_writes,
    learner_at_the_fifth_miss, learner_with_kp1, lesson_app as app, revoke_reads, seed_attempt,
    seed_cached_model,
};
use serde_json::json;
use sqlx::types::Uuid;

/// Answer with `given`, and assert the `500 internal_error` envelope and an
/// attempt count of `attempts_after` in the log.
async fn assert_internal_error(
    db: &TestDb,
    app: &Router,
    user: Uuid,
    given: &str,
    attempts_after: usize,
) {
    let (status, body) = answer_lesson(app, user, given).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(body["error"]["code"], "internal_error");
    assert_eq!(
        events_of_type(db, user, "attempt").await.len(),
        attempts_after
    );
}

/// Step 6. The attempt INSERT fails: nothing is recorded and the D-S6 row is
/// not written.
#[tokio::test]
async fn an_attempt_append_that_fails_records_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fault-append@example.com", 5.0).await;
        fail_writes(&db, "events", "NEW.type = 'attempt'").await;

        assert_internal_error(&db, &app, user, "13.5", 0).await;
    })
    .await;
}

/// Step 7. The close event of a failed lesson does not append: the fifth miss
/// rolls back with it, so the log keeps the four misses and no close.
#[tokio::test]
async fn a_lesson_close_that_fails_rolls_the_attempt_back() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_at_the_fifth_miss(&db, "fault-close@example.com").await;
        fail_writes(&db, "events", "NEW.type = 'lesson_result'").await;

        assert_internal_error(&db, &app, user, "14", 4).await;
        assert_eq!(events_of_type(&db, user, "lesson_result").await.len(), 0);
    })
    .await;
}

/// Step 7. The history read of the repeat-fail rule fails: the grade rolls
/// back with the attempt it appended.
#[tokio::test]
async fn a_history_read_that_fails_rolls_the_grade_back() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fault-history@example.com", 5.0).await;
        // The policy runs per row, so the learner needs a cached row to read.
        seed_cached_model(&db, user, &LearnerModel::default(), 1).await;
        fail_reads(&db, "learner_models", "SELECT through_seq AS").await;

        assert_internal_error(&db, &app, user, "13.5", 0).await;
    })
    .await;
}

/// Step 7. The fold does not save: the grade rolls back.
#[tokio::test]
async fn a_fold_save_that_fails_rolls_the_grade_back() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fault-fold@example.com", 5.0).await;
        fail_writes(&db, "learner_models", "true").await;

        assert_internal_error(&db, &app, user, "13.5", 0).await;
    })
    .await;
}

/// Step 9. The pre-authored lookup of a miss fails: the grade rolls back.
#[tokio::test]
async fn a_diagnosis_lookup_that_fails_rolls_the_grade_back() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fault-lookup@example.com", 5.0).await;
        revoke_reads(&db, "content_store").await;

        assert_internal_error(&db, &app, user, "14", 0).await;
    })
    .await;
}

/// The replay of a standing miss looks the pre-authored answer up too, and a
/// lookup that fails is the same `500`; the standing attempt stays alone.
#[tokio::test]
async fn a_replayed_request_whose_lookup_fails_is_500() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fault-replay@example.com", 5.0).await;
        seed_attempt(
            &db,
            user,
            2,
            "s_2026-01-01a-lesson-addition-1",
            Verdict {
                given_answer: "14",
                correct: false,
                work_quality: "nearly_passable",
                error_tags: json!([]),
                secs: 20,
            },
        )
        .await;
        revoke_reads(&db, "content_store").await;

        assert_internal_error(&db, &app, user, "13.5", 1).await;
    })
    .await;
}

/// Step 10. The D-S6 write fails: the grade rolls back.
#[tokio::test]
async fn a_state_write_that_fails_rolls_the_grade_back() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fault-state@example.com", 5.0).await;
        fail_writes(&db, "web_states", "true").await;

        assert_internal_error(&db, &app, user, "13.5", 0).await;
    })
    .await;
}

/// Step 10. The COMMIT fails after every statement passed: the grade rolls
/// back, so the attempt the transaction inserted is gone.
#[tokio::test]
async fn a_commit_that_fails_records_nothing() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fault-commit@example.com", 5.0).await;
        fail_commit_after_insert(&db, "events", "NEW.type = 'attempt'").await;

        assert_internal_error(&db, &app, user, "13.5", 0).await;
    })
    .await;
}
