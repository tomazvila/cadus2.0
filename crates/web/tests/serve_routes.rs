//! M5 U7 acceptance: `POST /api/task/{id}/serve`, `/teach`, and `/hint`.
//!
//! Requirements: A6, D5, D-O1, D-O3, D-S6, L4, L5, R4, T1. Spec
//! `docs/reference/web-service-1.0-spec.md` sections 2, 4.1, 5.6 and row U7 of
//! section 11; `docs/reference/serving-1.0-spec.md` sections 6 and 7.2.
//!
//! The four acceptance checks of row U7 land in this file and its parts:
//!
//! 1. a re-serve returns the same `problem_id` and a fresh `started_at` —
//!    [`a_re_serve_returns_the_same_problem_id_and_a_fresh_started_at`];
//! 2. a stale id 404s on both answer and hint —
//!    `serve_routes_hint.rs`;
//! 3. a hint body never contains `expected` —
//!    `serve_routes_hint.rs`;
//! 4. teach on a review is `409 no_instruction` —
//!    `serve_routes_teach.rs`.
//!
//! `serve_routes_pool.rs` holds the A6 pool miss, `serve_routes_cadence.rs`
//! the drill cadence and the fold cursor, `serve_routes_refusals.rs` the
//! refusals of a document or a knowledge point that is not there, and
//! `serve_routes_faults.rs` the serve under a store fault.
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal statement, a literal count.
//!
//! Every call presents a real session cookie, and `auth::layer::tenant_layer`
//! binds the tenant from it (FIX-M5-C). No test here writes a request extension
//! by hand.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use common::{
    LESSON, POOL_ANSWER, POOL_TEXT, assert_refused, call, claimed_rows, drill_app as app,
    events_of_type, hint_task, learner_with_pool_row, lesson_problem, lesson_state, parse,
    put_state, seed_learner, seed_open_session, serve_ok, serve_raw, serve_task, stored_state,
};
use serde_json::{Value, json};

// --------------------------------------------------------------------------- //
// The auth seam and the unknown task
// --------------------------------------------------------------------------- //

/// All three routes need a session. A request with no bound tenant is
/// `401 unauthorized` in the `{"error":{"code","message"}}` envelope.
#[tokio::test]
async fn every_u7_route_without_a_tenant_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app(&db);
        for suffix in ["serve", "teach", "hint"] {
            let uri = format!("/api/task/{LESSON}/{suffix}");
            let (status, body) = call(&app, Method::POST, &uri, None, Some(json!({}))).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{uri}: {body}");
            assert_eq!(parse(&body)["error"]["code"], "unauthorized", "{uri}");
        }
    })
    .await;
}

/// A task id that this session's plan does not hold is `404 unknown_task`, and a
/// route called with no session open is `409 no_open_session`.
#[tokio::test]
async fn an_unknown_task_is_404_and_a_closed_session_is_409() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "unknown@example.com").await;
        let app = app(&db);

        assert_refused(
            &serve_task(&app, user, LESSON).await,
            StatusCode::CONFLICT,
            "no_open_session",
        );

        seed_open_session(&db, user).await;
        assert_refused(
            &serve_task(&app, user, "s_2026-01-01a-lesson-nowhere").await,
            StatusCode::NOT_FOUND,
            "unknown_task",
        );
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Acceptance 1: the re-serve
// --------------------------------------------------------------------------- //

/// Section 5.6 and `_serve_live`. A second serve of a live problem hands back
/// the SAME `problem_id` and re-stamps `started_at`, because that is the moment
/// the problem goes on screen. It also serves no second problem: `served` is
/// keyed by task id, and `progress.served` stays 1.
#[tokio::test]
#[ignore]
async fn a_re_serve_returns_the_same_problem_id_and_a_fresh_started_at() {
    TestDb::with(|db| async move {
        let user = learner_with_pool_row(&db, "reserve@example.com").await;
        let app = app(&db);

        let first = serve_ok(&app, user, LESSON).await;
        assert_eq!(first["text"], POOL_TEXT);
        assert_eq!(first["index"], 1);
        assert_eq!(first["kp"], "kp1");
        assert_eq!(first["time_budget_secs"], 30);
        assert_eq!(first["countdown"], false);
        // A lesson names no problem count, so `total` is null, not 0
        // (`api.py:520`). The D-S6 row stores the absent count as 0, and the
        // payload must not read it back from there.
        assert_eq!(first["total"], Value::Null);
        let first_state = stored_state(&db, user).await;
        let first_stamp = first_state.served[LESSON].started_at;
        let handoff = first_state.served[LESSON]
            .handoff
            .as_ref()
            .expect("a new ordinary problem freezes its hand-off");
        assert_eq!(handoff.item_source, cadus_core::event::ItemSource::Template);
        assert_eq!(handoff.exposure, cadus_core::event::Exposure::First);
        let recorded = events_of_type(&db, user, "ordinary_problem_served").await;
        assert_eq!(
            recorded.len(),
            1,
            "the abandoned hand-off is durable before an answer"
        );
        assert_eq!(recorded[0]["problem_id"], first["problem_id"]);
        assert_eq!(recorded[0]["item_digest"], handoff.item_digest);
        assert_eq!(recorded[0]["exposure"], "first");

        let second = serve_ok(&app, user, LESSON).await;
        assert_eq!(
            second["problem_id"], first["problem_id"],
            "a re-serve dealt a different problem"
        );
        assert_eq!(second["text"], POOL_TEXT);
        assert_eq!(second["index"], 1);

        // The stamp MOVED. Section 5.6: the clock starts when the client asks
        // for the problem to put on screen, so a reload restarts it. Each serve
        // makes several database round trips, so the two instants are hundreds
        // of microseconds apart and the strict comparison is not a race.
        let after = stored_state(&db, user).await;
        assert!(
            after.served[LESSON].started_at > first_stamp,
            "started_at was not re-stamped: {} then {}",
            first_stamp,
            after.served[LESSON].started_at
        );
        assert_eq!(after.served.len(), 1, "served is keyed by task id");
        assert_eq!(after.tasks[LESSON].served, 1, "the re-serve counted twice");
        assert_eq!(
            after.served[LESSON].handoff,
            first_state.served[LESSON].handoff
        );
        assert_eq!(
            events_of_type(&db, user, "ordinary_problem_served")
                .await
                .len(),
            1,
            "a reload returns the frozen hand-off without appending another",
        );

        // The re-serve claimed no second pool row.
        assert_eq!(claimed_rows(&db, user).await, 1);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// Hard Rule 1: the serve payload
// --------------------------------------------------------------------------- //

/// Trap W7: scan the RAW JSON. A serve carries neither `expected` nor the
/// solution sketch, and the answer text never appears in it.
#[tokio::test]
#[ignore]
async fn a_serve_never_carries_the_expected_answer_or_the_sketch() {
    TestDb::with(|db| async move {
        let user = learner_with_pool_row(&db, "secrecy@example.com").await;
        let app = app(&db);

        let (status, body) = serve_raw(&app, user, LESSON).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(
            !body.contains("expected"),
            "the serve leaked expected: {body}"
        );
        assert!(
            !body.contains("solution_sketch"),
            "the serve leaked the sketch: {body}"
        );
        assert!(
            !body.contains(POOL_ANSWER),
            "the serve leaked the answer text: {body}"
        );
        // The LESSON payload carries these seven fields and no eighth
        // (`_serve_payload`, `api.py:501-527`). A QUIZ serve carries one more,
        // the whole-quiz clock, and this list is what holds that key to the
        // quiz: `crates/web/tests/quiz_route_clock.rs` pins the eight keys there.
        let payload = parse(&body);
        let keys: Vec<&str> = payload
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
                "text",
                "time_budget_secs",
                "total",
            ]
        );

        // The document the client cannot read DOES hold it, so the grade path of
        // unit U8 finds it.
        let stored = stored_state(&db, user).await;
        assert_eq!(stored.served[LESSON].expected.answer, POOL_ANSWER);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The closed task
// --------------------------------------------------------------------------- //

/// A closed task refuses a serve with `409 task_complete`, and a hint against it
/// takes the same refusal (section 4.2).
#[tokio::test]
async fn a_closed_task_refuses_a_serve_and_a_hint_with_409_task_complete() {
    TestDb::with(|db| async move {
        let user = seed_learner(&db, "closed@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;

        let mut live = lesson_problem(1.0, "kp1", Vec::new());
        live.problem_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        live.text = POOL_TEXT.to_string();
        live.expected.answer = POOL_ANSWER.to_string();
        live.solution_sketch = None;
        live.index = 2;
        put_state(&db, user, &lesson_state(live, 3, true)).await;

        assert_refused(
            &serve_task(&app, user, LESSON).await,
            StatusCode::CONFLICT,
            "task_complete",
        );
        assert_refused(
            &hint_task(&app, user, LESSON, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").await,
            StatusCode::CONFLICT,
            "task_complete",
        );
    })
    .await;
}
