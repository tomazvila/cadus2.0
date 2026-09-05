//! The store faults of the serve, hint, and teach routes past the U7 set:
//! the pool reads and writes of the draw, the `content_store` reads, the
//! learner-model write, and the D-S6 commit. Each fault is `500
//! internal_error` and the transaction rolls back.
//!
//! The header of `serve_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::collections::BTreeMap;

use axum::http::StatusCode;
use cadus_store::test_support::TestDb;
use common::sessions::app_without_content;
use common::{
    KEY, LESSON, POOL_ANSWER, POOL_TEXT, TemplateRow, app_with_content, assert_refused, close_live,
    drill_app as app, fail_commit_after, fail_reads, fail_writes, hide_column, hint_task,
    learner_with_pool_row, one_unit_curriculum, problem_id_of, seed_content, seed_learner,
    seed_open_session, seed_template_row, serve_ok, serve_task, teach_task, topic,
};
use serde_json::json;
use sqlx::types::Uuid;

/// The text of the pop, and of no other read of the pool.
const POP_NEEDLE: &str = "sp.claimed_at IS NULL";

/// The text of the A6 rotation read, and of no other read of the pool.
const ROTATION_NEEDLE: &str = "sp.claimed_at IS NOT NULL";

/// A learner with an open session and an empty pool.
async fn learner(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    user
}

/// Serve `LESSON` to `user` and expect the `500` of a store fault.
async fn assert_serve_is_500(app: &axum::Router, user: Uuid) {
    assert_refused(
        &serve_task(app, user, LESSON).await,
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
    );
}

/// A learner with one live problem of `LESSON` and an approved one-rung
/// ladder for `KEY`; the answer is the live problem id.
async fn learner_with_live_problem(db: &TestDb, app: &axum::Router, email: &str) -> (Uuid, String) {
    let user = learner_with_pool_row(db, email).await;
    seed_content(
        db,
        KEY,
        "hint_ladder",
        "digest-hints",
        json!({"hints": ["One."]}),
    )
    .await;
    let problem_id = problem_id_of(&serve_ok(app, user, LESSON).await);
    (user, problem_id)
}

/// Expect the `500` of a store fault on the hint of `problem_id` and on the
/// teach of `LESSON`.
async fn assert_hint_and_teach_are_500(app: &axum::Router, user: Uuid, problem_id: &str) {
    for reply in [
        hint_task(app, user, LESSON, problem_id).await,
        teach_task(app, user, LESSON).await,
    ] {
        assert_refused(&reply, StatusCode::INTERNAL_SERVER_ERROR, "internal_error");
    }
}

/// The pop of the pool fails: the serve is `500`.
#[tokio::test]
async fn a_pool_read_that_fails_is_500_on_the_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_pool_row(&db, "fault-pop@example.com").await;
        fail_reads(&db, "serving_pool", POP_NEEDLE).await;

        assert_serve_is_500(&app, user).await;
    })
    .await;
}

/// The A6 refill cannot write its exemplar rows: the serve is `500`.
#[tokio::test]
async fn a_pool_insert_that_fails_is_500_on_the_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "fault-refill@example.com").await;
        fail_writes(&db, "serving_pool", "true").await;

        assert_serve_is_500(&app, user).await;
    })
    .await;
}

/// The pop after the A6 refill fails. The first pop reads an empty pool, so
/// the per-row policy fires on the rows the refill wrote.
#[tokio::test]
async fn a_pool_read_after_the_refill_that_fails_is_500_on_the_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "fault-repop@example.com").await;
        fail_reads(&db, "serving_pool", POP_NEEDLE).await;

        assert_serve_is_500(&app, user).await;
    })
    .await;
}

/// The A6 rotation read fails on a pair whose whole authored list is claimed:
/// the serve is `500`.
#[tokio::test]
async fn a_rotation_read_that_fails_is_500_on_the_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "fault-rotate@example.com").await;
        for _ in 0..2 {
            serve_ok(&app, user, LESSON).await;
            close_live(&db, user, LESSON).await;
        }
        fail_reads(&db, "serving_pool", ROTATION_NEEDLE).await;

        assert_serve_is_500(&app, user).await;
    })
    .await;
}

/// The read of the approved template behind a drawn template row fails: the
/// serve is `500`. The pop joins `content_store` on the digest and the status
/// only, so the pop still runs.
#[tokio::test]
async fn a_template_read_that_fails_is_500_on_the_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "fault-template@example.com").await;
        seed_content(&db, KEY, "template", "digest-t", json!({"v": 1})).await;
        seed_template_row(
            &db,
            user,
            TemplateRow {
                key: KEY,
                digest: Some("digest-t"),
                bindings: BTreeMap::new(),
                text: POOL_TEXT,
                answer: POOL_ANSWER,
                hash: "hash-t",
            },
        )
        .await;
        hide_column(&db, "content_store", "body").await;

        assert_serve_is_500(&app, user).await;
    })
    .await;
}

/// The learner-model write of the first serve fails: the serve is `500`.
#[tokio::test]
async fn a_model_write_that_fails_is_500_on_the_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_pool_row(&db, "fault-model-write@example.com").await;
        fail_writes(&db, "learner_models", "true").await;

        assert_serve_is_500(&app, user).await;
    })
    .await;
}

/// The D-S6 write fails on the serve: the serve is `500`.
#[tokio::test]
async fn a_state_write_that_fails_is_500_on_the_serve() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner_with_pool_row(&db, "fault-state-write@example.com").await;
        fail_writes(&db, "web_states", "true").await;

        assert_serve_is_500(&app, user).await;
    })
    .await;
}

/// Without a curriculum the hint and the teach are `503 curriculum_unavailable`.
#[tokio::test]
async fn the_hint_and_the_teach_without_content_are_503() {
    TestDb::with(|db| async move {
        let app = app_without_content(&db);
        let user = learner(&db, "no-content-hint@example.com").await;

        for reply in [
            hint_task(&app, user, LESSON, "p1").await,
            teach_task(&app, user, LESSON).await,
        ] {
            assert_refused(
                &reply,
                StatusCode::SERVICE_UNAVAILABLE,
                "curriculum_unavailable",
            );
        }
    })
    .await;
}

/// The D-S6 read fails: the hint and the teach are `500`.
#[tokio::test]
async fn a_state_read_that_fails_is_500_on_the_hint_and_the_teach() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let (user, problem_id) =
            learner_with_live_problem(&db, &app, "lock-hint@example.com").await;
        fail_reads(&db, "web_states", "FROM web_states WHERE").await;

        assert_hint_and_teach_are_500(&app, user, &problem_id).await;
    })
    .await;
}

/// A task the plan does not hold is `404 unknown_task` on the hint and on the
/// teach.
#[tokio::test]
async fn an_unknown_task_is_404_on_the_hint_and_the_teach() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "unknown-hint@example.com").await;

        for reply in [
            hint_task(&app, user, "s_nowhere", "p1").await,
            teach_task(&app, user, "s_nowhere").await,
        ] {
            assert_refused(&reply, StatusCode::NOT_FOUND, "unknown_task");
        }
    })
    .await;
}

/// The `content_store` read fails on the hint and on the teach: both are `500`.
#[tokio::test]
async fn a_document_read_that_fails_is_500_on_the_hint_and_the_teach() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let (user, problem_id) = learner_with_live_problem(&db, &app, "doc-hint@example.com").await;
        hide_column(&db, "content_store", "body").await;

        assert_hint_and_teach_are_500(&app, user, &problem_id).await;
    })
    .await;
}

/// The commit of the hint fails: the hint is `500`.
#[tokio::test]
async fn a_commit_that_fails_is_500_on_the_hint() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let (user, problem_id) =
            learner_with_live_problem(&db, &app, "commit-hint@example.com").await;
        fail_commit_after(&db, "web_states", "INSERT OR UPDATE", "true").await;

        assert_refused(
            &hint_task(&app, user, LESSON, &problem_id).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
        );
    })
    .await;
}

/// A lesson on a topic that authors no knowledge point has no page to teach:
/// the teach is `409 no_instruction`.
#[tokio::test]
async fn a_lesson_on_a_topic_with_no_knowledge_point_is_409_on_the_teach() {
    TestDb::with(|db| async move {
        let app = app_with_content(
            &db,
            one_unit_curriculum(vec![topic("addition", Vec::new())]),
        );
        let user = learner(&db, "no-kp-teach@example.com").await;

        assert_refused(
            &teach_task(&app, user, LESSON).await,
            StatusCode::CONFLICT,
            "no_instruction",
        );
    })
    .await;
}
