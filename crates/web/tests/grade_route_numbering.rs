//! M5 U8: the attempt number. An `attempt_id` is `{task_id}-{n}`, and `n` is
//! read from the LOG (M5 review 1, findings F1 and F12).
//!
//! The file reads the `attempt_id` column of the log beside every reply, so a
//! number that only the reply carries cannot pass.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use cadus_web::state::WebState;
use common::{
    KEY, LESSON, SESSION, a_miss, answer_lesson_ok, answer_task, enroll_course, learner_with_kp1,
    lesson_app as app, put_state, seed_attempt, seed_learner, seed_open_session, seed_pool_row,
    seed_task_attempt, serve_task,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// Serve the lesson's problem over the route that owns the draw, and read the
/// `problem_id` it hands out.
async fn serve(app: &Router, user: Uuid) -> String {
    let (status, drawn) = serve_task(app, user, LESSON).await;
    assert_eq!(status, StatusCode::OK, "{drawn}");
    drawn["problem_id"].as_str().unwrap().to_string()
}

/// Answer the lesson's problem `problem_id` with `0`, and read the `200` body.
async fn answer_zero(app: &Router, user: Uuid, problem_id: &str) -> Value {
    let (status, body) = answer_task(
        app,
        user,
        LESSON,
        json!({"problem_id": problem_id, "answer": "0"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// The `attempt_id` column of every attempt row of `user`, oldest first.
async fn attempt_ids(db: &TestDb, user: Uuid) -> Vec<String> {
    sqlx::query_scalar!(
        r#"
        SELECT attempt_id AS "attempt_id!" FROM events
        WHERE user_id = $1 AND type = 'attempt' ORDER BY seq
        "#,
        user
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// Two pool rows of the lesson, so two serves have a problem to draw.
async fn seed_two_pool_rows(db: &TestDb, user: Uuid) {
    seed_pool_row(db, user, KEY, "Compute 2 + 2.", "4", "hash-a").await;
    seed_pool_row(db, user, KEY, "Compute 3 + 3.", "6", "hash-b").await;
}

/// F1. `POST /api/enroll` clears the D-S6 row and leaves the session open, so
/// the serve counter restarts while the task ids stay. `n` comes from the LOG,
/// so the answer after the enroll takes `-2` and stands in the log.
///
/// The 1-based attempt index of the task is `docs/plans/M3.md` trap T12 and spec
/// section 4.3 step 6. A counter that lives in the deletable scratch repeats
/// `-1`, and the partial unique index then discards the whole second attempt.
#[tokio::test]
#[ignore]
async fn an_enroll_between_two_answers_numbers_the_second_attempt_from_the_log() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "enroll-clear@example.com", 5.0).await;
        seed_two_pool_rows(&db, user).await;

        let first = answer_lesson_ok(&app, user, "14").await;
        assert_eq!(first["attempt_id"], "s_2026-01-01a-lesson-addition-1");

        // The enroll clears the D-S6 row and leaves the session open
        // (`api.py:833`).
        let (status, switched) = enroll_course(&app, user, "c1").await;
        assert_eq!(status, StatusCode::OK, "{switched}");
        assert_eq!(switched["enrolled"], "c1");

        let problem_id = serve(&app, user).await;
        let second = answer_zero(&app, user, &problem_id).await;
        assert_eq!(second["attempt_id"], "s_2026-01-01a-lesson-addition-2");
        assert_eq!(second["task_status"], "continue");
        assert_eq!(
            attempt_ids(&db, user).await,
            vec![
                "s_2026-01-01a-lesson-addition-1".to_string(),
                "s_2026-01-01a-lesson-addition-2".to_string(),
            ]
        );
    })
    .await;
}

/// A topic id may hold a hyphen, so one task id can be the prefix of another.
/// The number of a task counts ITS OWN attempts: an attempt of the sibling task
/// `{LESSON}-extra` carries the `{LESSON}-` prefix, and it must not move this
/// lesson's number.
///
/// `assign_ids` builds `{session}-{type}-{topic}`, so the two task ids below are
/// the topics `addition` and `addition-extra` of one session.
#[tokio::test]
async fn an_attempt_of_a_sibling_task_does_not_move_this_number() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "sibling@example.com", 5.0).await;
        seed_task_attempt(
            &db,
            user,
            2,
            "s_2026-01-01a-lesson-addition-extra",
            "s_2026-01-01a-lesson-addition-extra-1",
            a_miss(),
        )
        .await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
        assert_eq!(body["attempt_id"], "s_2026-01-01a-lesson-addition-1");
        assert_eq!(body["task_status"], "continue");
    })
    .await;
}

/// F12. The chain serve -> answer -> serve -> answer, over the real routes, on a
/// log that already holds one attempt of the task and a D-S6 row that holds no
/// counter at all. The two answers take `-2` and `-3`: the number is the
/// position in the LOG, never the position in the scratch.
#[tokio::test]
#[ignore]
async fn a_serve_answer_chain_numbers_the_attempts_from_the_log() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_learner(&db, "chain@example.com").await;
        seed_open_session(&db, user).await;
        // The learner answered one problem of this lesson already. The scratch
        // was cleared after it, so the D-S6 row carries no serve counter.
        seed_attempt(&db, user, 2, "s_2026-01-01a-lesson-addition-1", a_miss()).await;
        put_state(&db, user, &WebState::for_session(SESSION)).await;
        seed_two_pool_rows(&db, user).await;

        let first_id = serve(&app, user).await;
        let first = answer_zero(&app, user, &first_id).await;
        assert_eq!(first["attempt_id"], "s_2026-01-01a-lesson-addition-2");

        let second_id = serve(&app, user).await;
        // The chain answers two DIFFERENT problems, so the two attempts are two
        // attempts and not one request sent twice.
        assert_ne!(first_id, second_id);
        let second = answer_zero(&app, user, &second_id).await;
        assert_eq!(second["attempt_id"], "s_2026-01-01a-lesson-addition-3");

        assert_eq!(
            attempt_ids(&db, user).await,
            vec![
                "s_2026-01-01a-lesson-addition-1".to_string(),
                "s_2026-01-01a-lesson-addition-2".to_string(),
                "s_2026-01-01a-lesson-addition-3".to_string(),
            ]
        );
    })
    .await;
}
