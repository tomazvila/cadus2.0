//! FIX-M5-B acceptance: the authored solution and the serve topic of one serve.
//!
//! Requirements: A4, A6, D-M5-3, D-O3, D-S6, L5, C6. Spec
//! `docs/reference/web-service-1.0-spec.md` sections 2.1, 5.5 and 6.2;
//! `docs/reviews/M5-review-1.md`, unit FIX-M5-B.
//!
//! Two defects of the M5 review land here and in `serve_solution_component.rs`:
//!
//! 1. F2 and F11 — `install_next` set `solution_sketch: None` for every serve,
//!    so no graded reply carried the worked solution the stock re-solve text
//!    tells the learner to study:
//!    [`a_graded_miss_carries_the_authored_exemplar_solution`] and
//!    [`a_graded_miss_carries_the_rendered_template_solution`];
//! 2. F10 and F16 — a review that micro-interleaves a component skill drew the
//!    statement from the component and then read the hint ladder and the
//!    pre-authored diagnosis of the PARENT topic: `serve_solution_component.rs`.
//!
//! Every expected value is a LITERAL: a literal status code, a literal statement,
//! a literal hint, a literal solution.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::Router;
use cadus_store::test_support::TestDb;
use common::{
    COMPONENT_KEY, COMPONENT_SOLUTION, COMPONENT_TEXT, COUNTING_LESSON as LESSON, TemplateRow,
    answer_task_ok, component_app as app, problem_id_of, seed_content, seed_content_aged,
    seed_learner, seed_open_session, seed_template_row, seeded_bindings, serve_ok, template_body,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// Put the seeded `counting/kp1` template row, drawn from `digest`, into the
/// pool of `user`: `Count on from 8 by -3.`, whose answer is 5.
async fn seed_counting_row(db: &TestDb, user: Uuid, digest: &str) {
    seed_template_row(
        db,
        user,
        TemplateRow {
            key: COMPONENT_KEY,
            digest: Some(digest),
            bindings: seeded_bindings(),
            text: "Count on from 8 by -3.",
            answer: "5",
            hash: "hash-template",
        },
    )
    .await;
}

/// Serve the lesson, check the statement, and answer it wrong with 11.
async fn miss_the_counting_row(app: &Router, user: Uuid) -> Value {
    let served = serve_ok(app, user, LESSON).await;
    assert_eq!(served["text"], "Count on from 8 by -3.");
    let problem_id = problem_id_of(&served);
    let graded = answer_task_ok(
        app,
        user,
        LESSON,
        json!({"problem_id": problem_id, "answer": "11"}),
    )
    .await;
    assert_eq!(graded["correct"], false);
    graded
}

// --------------------------------------------------------------------------- //
// F2 and F11: the graded reply carries the authored solution
// --------------------------------------------------------------------------- //

/// The A6 exemplar path. An empty pool falls back to the authored exemplars, and
/// the row the fallback serves carries the exemplar's own `solution_sketch`, so
/// the graded miss reveals it beside the stock re-solve instruction (A4, A6,
/// D-M5-3, spec section 5.5).
#[tokio::test]
async fn a_graded_miss_carries_the_authored_exemplar_solution() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "sketch-exemplar@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let served = serve_ok(&app, user, LESSON).await;
        assert_eq!(served["text"], COMPONENT_TEXT);
        // Hard Rule 1: the serve itself reveals nothing.
        assert_eq!(served.get("solution"), None);

        let problem_id = problem_id_of(&served);
        let graded = answer_task_ok(
            &app,
            user,
            LESSON,
            json!({"problem_id": problem_id, "answer": "6"}),
        )
        .await;
        assert_eq!(graded["correct"], false);
        assert_eq!(
            graded["solution"], COMPONENT_SOLUTION,
            "the graded miss carried no authored solution: {graded}"
        );
    })
    .await;
}

/// The A1 template path. A pool row names its `content_store` document by
/// digest, and the document's `solution_sketch` is a statement over the same
/// parameters, so the row's own bindings render it. `b` binds `-3`, and the
/// renderer brackets a negative value exactly as it brackets one in a statement.
#[tokio::test]
async fn a_graded_miss_carries_the_rendered_template_solution() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "sketch-template@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "template",
            "digest-template",
            template_body("Start at {a} and step {b} to reach 5."),
        )
        .await;
        seed_counting_row(&db, user, "digest-template").await;

        let graded = miss_the_counting_row(&app, user).await;
        assert_eq!(
            graded["solution"], "Start at 8 and step (-3) to reach 5.",
            "the graded miss carried no rendered solution: {graded}"
        );
    })
    .await;
}

/// C6: the row was drawn from ONE digest. A document approved after the draw is
/// a different problem's solution, so the reply carries none at all.
#[tokio::test]
async fn a_row_that_a_later_approval_superseded_carries_no_solution() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "sketch-stale@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_content_aged(
            &db,
            COMPONENT_KEY,
            "template",
            "digest-superseded",
            template_body("The first author wrote this."),
            1,
        )
        .await;
        seed_content(
            &db,
            COMPONENT_KEY,
            "template",
            "digest-newer",
            template_body("A later author wrote this."),
        )
        .await;
        seed_counting_row(&db, user, "digest-superseded").await;

        let graded = miss_the_counting_row(&app, user).await;
        assert_eq!(
            graded.get("solution"),
            None,
            "a superseded digest revealed another document's solution: {graded}"
        );
    })
    .await;
}
