//! M5 U7, the refusals of a document or a knowledge point that is not there:
//! the serve of a knowledge point with no problem, a hint on a live problem
//! that names no knowledge point, a ladder with no rung, and a page the route
//! cannot read.
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use cadus_web::state::ServedProblem;
use common::{
    KEY, LESSON, PROBLEM_ID, StatusCode, TestDb, assert_refused, drill_app as app, hint_task,
    lesson_learner, lesson_problem, lesson_state, put_state, seed_content, seed_learner,
    seed_open_session, serve_task, stored_state, teach_task,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The task id of the `subtraction` lesson, whose one knowledge point authors
/// no exemplar.
const SUBTRACTION: &str = "s_2026-01-01a-lesson-subtraction";

/// Seed a learner with a live lesson problem, and ask for a hint on it.
async fn hint_on(db: &TestDb, email: &str, live: ServedProblem) -> (StatusCode, Value) {
    let user = lesson_learner(db, email, live).await;
    hint_task(&app(db), user, LESSON, PROBLEM_ID).await
}

/// A learner whose lesson progress row stands at `kp`, with no live problem.
async fn learner_at_kp(db: &TestDb, email: &str, kp: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    let mut scratch = lesson_state(lesson_problem(0.0, kp, Vec::new()), 0, false);
    scratch.served.clear();
    put_state(db, user, &scratch).await;
    user
}

/// A knowledge point that authors no exemplar and has no pool row can produce
/// no problem: the serve is `409 pool_unavailable`, and it never generates
/// (A6, T1).
#[tokio::test]
async fn a_knowledge_point_with_no_exemplar_and_no_pool_row_is_409_pool_unavailable() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "no-exemplar@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        assert_refused(
            &serve_task(&app, user, SUBTRACTION).await,
            StatusCode::CONFLICT,
            "pool_unavailable",
        );
    })
    .await;
}

/// A progress row that names a knowledge point the arena no longer holds takes
/// the same refusal: the pool is empty and there is no author to fall back to.
#[tokio::test]
async fn a_progress_row_at_an_unknown_knowledge_point_is_409_pool_unavailable() {
    TestDb::with(|db| async move {
        let user = learner_at_kp(&db, "unknown-kp@example.com", "kp9").await;
        let app = app(&db);

        assert_refused(
            &serve_task(&app, user, LESSON).await,
            StatusCode::CONFLICT,
            "pool_unavailable",
        );
        assert!(!stored_state(&db, user).await.served.contains_key(LESSON));
    })
    .await;
}

/// A live problem that names no knowledge point, or no topic, has no ladder to
/// read: `409 no_hint_ladder`.
#[tokio::test]
async fn a_live_problem_that_names_no_knowledge_point_or_topic_has_no_ladder() {
    TestDb::with(|db| async move {
        let mut no_kp = lesson_problem(5.0, "kp1", Vec::new());
        no_kp.kp = None;
        assert_refused(
            &hint_on(&db, "no-kp@example.com", no_kp).await,
            StatusCode::CONFLICT,
            "no_hint_ladder",
        );

        let mut no_topic = lesson_problem(5.0, "kp1", Vec::new());
        no_topic.topic = None;
        no_topic.serve_topic = None;
        assert_refused(
            &hint_on(&db, "no-topic@example.com", no_topic).await,
            StatusCode::CONFLICT,
            "no_hint_ladder",
        );
    })
    .await;
}

/// An approved ladder with no rung gives no hint: `409 no_hint_ladder`.
#[tokio::test]
#[ignore]
async fn an_approved_ladder_with_no_rung_is_409_no_hint_ladder() {
    TestDb::with(|db| async move {
        seed_content(
            &db,
            KEY,
            "hint_ladder",
            "digest-empty",
            json!({"hints": []}),
        )
        .await;
        let live = lesson_problem(5.0, "kp1", Vec::new());
        assert_refused(
            &hint_on(&db, "empty-ladder@example.com", live).await,
            StatusCode::CONFLICT,
            "no_hint_ladder",
        );
    })
    .await;
}

/// An approved ladder whose body is not a ladder is `500 internal_error`: the
/// route reads one shape, and a body the gate never wrote fails there.
#[tokio::test]
#[ignore]
async fn an_approved_ladder_that_does_not_read_is_500_internal_error() {
    TestDb::with(|db| async move {
        seed_content(
            &db,
            KEY,
            "hint_ladder",
            "digest-broken",
            json!({"rungs": 3}),
        )
        .await;
        let live = lesson_problem(5.0, "kp1", Vec::new());
        assert_refused(
            &hint_on(&db, "broken-ladder@example.com", live).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
        );
    })
    .await;
}

/// An approved teach page whose body is not a page takes the same `500`.
#[tokio::test]
async fn an_approved_teach_page_that_does_not_read_is_500_internal_error() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "broken-page@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content(&db, KEY, "teach", "digest-broken", json!({"concept": 1})).await;

        assert_refused(
            &teach_task(&app, user, LESSON).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
        );
    })
    .await;
}

/// The teach page follows the progress row: a lesson that moved on to `kp2`
/// teaches `kp2`, not the first knowledge point of the topic.
#[tokio::test]
async fn the_teach_page_follows_the_knowledge_point_of_the_progress_row() {
    TestDb::with(|db| async move {
        let user = learner_at_kp(&db, "teach-kp2@example.com", "kp2").await;
        let app = app(&db);
        seed_content(
            &db,
            "addition/kp2",
            "teach",
            "digest-kp2",
            json!({
                "concept": "Carry the ten.",
                "worked_example": {"problem": "Compute 40 + 2.5.", "steps": ["40 + 2.5 = 42.5."]}
            }),
        )
        .await;

        let (status, page) = teach_task(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{page}");
        assert_eq!(page["kp"], "kp2");
        assert_eq!(page["concept"], "Carry the ten.");
    })
    .await;
}
