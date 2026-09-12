//! M5 U8: the `next` problem of a grade reply (trap W5, trap W6), and the
//! `500 state_unavailable` refusals of a D-S6 row the arena cannot grade.
//!
//! The fixture topic `subtraction` authors no exemplar, so its pool cannot fall
//! back and the draw of its next problem fails on purpose.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_core::curriculum::review_context_digest;
use cadus_web::state::ServedProblem;
use common::{
    PROBLEM_ID, StatusCode, TestDb, addition_curriculum, answer_lesson, answer_lesson_ok, answer_task, events_of_type,
    learner_with_kp1, lesson_app as app, lesson_learner, lesson_problem, stored_state,
};
use serde_json::{Value, json};

/// The task id of the `subtraction` lesson of the fixture curriculum.
const SUBTRACTION: &str = "s_2026-01-01a-lesson-subtraction";

/// One live problem of the `subtraction` lesson, whose one knowledge point
/// authors no exemplar.
fn subtraction_problem() -> ServedProblem {
    let mut live = lesson_problem(5.0, "kp1", Vec::new());
    live.task_id = SUBTRACTION.to_string();
    live.topic = Some("subtraction".to_string());
    live.serve_topic = Some("subtraction".to_string());
    live
}

/// Trap W6. The attempt is recorded, and the draw of the next problem fails:
/// the pool of `subtraction/kp1` is empty and the knowledge point authors no
/// exemplar to fall back on. The reply reports `next_unavailable`, because a
/// bare `next: null` on an open task reads as "task over" (trap W5).
#[tokio::test]
async fn a_failed_draw_of_the_next_problem_is_reported_not_raised() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = lesson_learner(&db, "no-next@example.com", subtraction_problem()).await;

        let (status, body) = answer_task(
            &app,
            user,
            SUBTRACTION,
            json!({"problem_id": PROBLEM_ID, "answer": "14"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["attempt_id"], "s_2026-01-01a-lesson-subtraction-1");
        assert_eq!(body["task_status"], "continue");
        assert_eq!(body["next"], Value::Null);
        assert_eq!(body["next_unavailable"], true);

        // The attempt stands, and the task stays open with no live problem.
        assert_eq!(events_of_type(&db, user, "attempt").await.len(), 1);
        let scratch = stored_state(&db, user).await;
        assert!(!scratch.tasks[SUBTRACTION].done);
        assert!(!scratch.served.contains_key(SUBTRACTION));
    })
    .await;
}

/// A reply whose draw succeeded carries no `next_unavailable` key at all.
#[tokio::test]
async fn a_drawn_next_problem_carries_no_next_unavailable_key() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "has-next@example.com", 5.0).await;
        let curr_digest =
            review_context_digest(&addition_curriculum(Vec::new())).unwrap();
        common::seed_pool_row(
            &db, user, common::KEY, "Compute 4 + 5.", "9", "fresh-next",
            &curr_digest, cadus_core::review_engine::DIGEST,
        ).await;

        let body = answer_lesson_ok(&app, user, "14").await;
        assert_eq!(body["task_status"], "continue");
        assert_eq!(body["next"]["kp"], "kp1");
        assert_eq!(body.get("next_unavailable"), None);
    })
    .await;
}

/// A served problem whose topic is not in the arena has no authored solve
/// time, so `secs` is not capped and carries no timing tag; the lesson
/// advance has no knowledge points to read, so the task carries on.
#[tokio::test]
async fn a_topic_outside_the_arena_grades_with_no_cap_and_no_advance() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let mut live = lesson_problem(4000.0, "kp1", Vec::new());
        live.topic = Some("elsewhere".to_string());
        let user = lesson_learner(&db, "elsewhere@example.com", live).await;

        let body = answer_lesson_ok(&app, user, "13.5").await;
        assert_eq!(body["correct"], true);
        assert_eq!(body["secs"], 4000);
        assert_eq!(body["error_tags"], json!([]));
        assert_eq!(body["task_status"], "continue");

        let recorded = events_of_type(&db, user, "attempt").await;
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0]["topic"], "elsewhere");
        assert_eq!(recorded[0]["secs"], 4000);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// `500 state_unavailable`: the D-S6 row names what the arena cannot grade
// --------------------------------------------------------------------------- //

/// Seed `email` with `live`, answer it right, and assert the
/// `500 state_unavailable` envelope with nothing recorded.
async fn assert_state_unavailable(db: &TestDb, email: &str, live: ServedProblem) {
    let app = app(db);
    let user = lesson_learner(db, email, live).await;

    let (status, body) = answer_lesson(&app, user, "13.5").await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert_eq!(body["error"]["code"], "state_unavailable");
    assert_eq!(events_of_type(db, user, "attempt").await.len(), 0);
}

/// A served problem that names no answer kind cannot be graded honestly. The
/// route is `500 state_unavailable`, and nothing is recorded.
#[tokio::test]
async fn a_served_problem_with_no_answer_kind_is_500_state_unavailable() {
    TestDb::with(|db| async move {
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.answer_kind = None;
        assert_state_unavailable(&db, "no-kind@example.com", live).await;
    })
    .await;
}

/// A stashed H3 attempt that does not read as an attempt cannot be recorded.
/// The route is `500 state_unavailable`, and nothing is recorded.
#[tokio::test]
async fn a_stash_that_is_not_an_attempt_is_500_state_unavailable() {
    TestDb::with(|db| async move {
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.rework = Some(json!({"bogus": true}));
        assert_state_unavailable(&db, "bad-stash@example.com", live).await;
    })
    .await;
}

/// A served problem that names no topic cannot build the `attempt` event. The
/// route is `500 state_unavailable`, and nothing is recorded.
#[tokio::test]
async fn a_served_problem_with_no_topic_is_500_state_unavailable() {
    TestDb::with(|db| async move {
        let mut live = lesson_problem(5.0, "kp1", Vec::new());
        live.topic = None;
        assert_state_unavailable(&db, "no-topic@example.com", live).await;
    })
    .await;
}

/// A one-item pool retains the obligation until a fresh item becomes available.
#[tokio::test]
async fn feedback_never_reuses_the_studied_problem_and_resumes_after_refill() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_kp1(&db, "fresh-unavailable@example.com", 5.0).await;
        let reply = answer_lesson_ok(&app, user, "14").await;
        assert_eq!(reply["next_unavailable"], true);
        assert_eq!(reply["feedback_practice"], true);
        assert_eq!(reply["feedback_blocked"], true);
        let (blocked_status, blocked) = common::serve_task(&app, user, common::LESSON).await;
        assert_eq!(blocked_status, StatusCode::CONFLICT);
        assert_eq!(blocked["error"]["code"], "fresh_practice_unavailable");
        assert!(
            stored_state(&db, user)
                .await
                .feedback_practice
                .contains_key(common::LESSON)
        );
        let curr_digest =
            review_context_digest(&addition_curriculum(Vec::new())).unwrap();
        common::seed_pool_row(
            &db,
            user,
            common::KEY,
            "Compute 7 + 3.",
            "10",
            "after-refill",
            &curr_digest,
            cadus_core::review_engine::DIGEST,
        )
        .await;
        let (status, next) = common::serve_task(&app, user, common::LESSON).await;
        assert_eq!(status, StatusCode::OK, "{next}");
        assert_eq!(next["text"], "Compute 7 + 3.");
        assert_eq!(next["kp"], "kp1");
        assert!(
            stored_state(&db, user).await.served[common::LESSON]
                .rework
                .is_some()
        );
    })
    .await;
}
