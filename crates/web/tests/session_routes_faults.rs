//! M5 U6: the session and dashboard routes under a store fault.
//!
//! Each test makes ONE statement of a route fail on purpose, and reads the
//! `500 internal_error` envelope back. The faults are triggers, row policies,
//! a held advisory lock and a closed pool on the throwaway database; no test
//! double stands between the handler and Postgres.
//!
//! The header of `session_routes.rs` gives the requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::sessions::{U6_ROUTES, app, cached_learner};
use common::{
    Method, Router, TestDb, Uuid, assert_internal, events_of_type, fail_deletes, fail_reads,
    fail_rows, fail_tenant_bind, fail_writes, hold_state_lock, json, seed_learner,
    seed_open_session, seed_unreadable_state,
};

/// Fail the test when the enroll and the session end of `user` are not `500`,
/// or when the enroll left an `enrolled` event behind.
async fn assert_enroll_and_end_fail(db: &TestDb, app: &Router, user: Uuid) {
    assert_internal(
        app,
        Method::POST,
        "/api/enroll",
        user,
        body_of("/api/enroll"),
    )
    .await;
    assert_internal(app, Method::POST, "/api/session/end", user, None).await;
    assert_eq!(events_of_type(db, user, "enrolled").await.len(), 0);
}

/// The body of `path`, when the route reads one.
fn body_of(path: &str) -> Option<serde_json::Value> {
    (path == "/api/enroll").then(|| json!({"course": "c1"}))
}

/// A tenant bind that fails stops every route at its first statement.
#[tokio::test]
async fn a_tenant_bind_that_fails_is_500_on_every_route() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-bind@example.com").await;
        fail_tenant_bind(&db).await;
        for (method, path) in U6_ROUTES {
            assert_internal(&app, method, path, user, body_of(path)).await;
        }
    })
    .await;
}

/// A model read that fails stops every route that folds the model.
#[tokio::test]
async fn a_model_read_that_fails_is_500_on_every_folding_route() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-model@example.com").await;
        fail_reads(&db, "learner_models", "SELECT model AS").await;
        for (method, path) in U6_ROUTES
            .into_iter()
            .filter(|(_, path)| *path != "/api/export")
        {
            assert_internal(&app, method, path, user, body_of(path)).await;
        }
    })
    .await;
}

/// A held advisory lock fails the locked routes at the lock wait.
#[tokio::test]
async fn a_held_lock_is_500_on_the_session_start() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-lock@example.com").await;
        let held = hold_state_lock(&db, user).await;
        assert_internal(&app, Method::POST, "/api/session/start", user, None).await;
        drop(held);
    })
    .await;
}

/// A read of the open session's window that fails stops the dashboard, the
/// plan and the export.
#[tokio::test]
async fn an_event_read_that_fails_is_500_on_the_window_readers() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-events@example.com").await;
        // The enroll appends line 2 and folds the model to it, so the fold of
        // the next request reads line 2 alone and the window read of the open
        // session is the first to touch line 1.
        let (status, body) = common::call(
            &app,
            Method::POST,
            "/api/enroll",
            Some(user),
            body_of("/api/enroll"),
        )
        .await;
        assert_eq!(status.as_u16(), 200, "{body}");
        fail_rows(&db, "events", "seq = 1").await;
        for path in ["/api/status", "/api/session/plan", "/api/export"] {
            assert_internal(&app, Method::GET, path, user, None).await;
        }
    })
    .await;
}

/// A D-S6 read that fails stops the routes that read the document.
#[tokio::test]
async fn a_state_read_that_fails_is_500_on_the_state_readers() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-state-read@example.com").await;
        fail_reads(&db, "web_states", "FROM web_states WHERE").await;
        assert_internal(&app, Method::POST, "/api/session/start", user, None).await;
        assert_internal(&app, Method::POST, "/api/session/end", user, None).await;
        assert_internal(&app, Method::GET, "/api/session/plan", user, None).await;
    })
    .await;
}

/// A D-S6 row that does not read is `500 state_unavailable`.
#[tokio::test]
async fn a_state_row_that_does_not_read_is_500_state_unavailable() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = seed_learner(&db, "fault-state-doc@example.com").await;
        seed_open_session(&db, user).await;
        seed_unreadable_state(&db, user).await;
        let (status, body) =
            common::call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status.as_u16(), 500, "{body}");
        assert_eq!(common::parse(&body)["error"]["code"], "state_unavailable");
    })
    .await;
}

/// An event append that fails rolls each write back.
#[tokio::test]
async fn an_event_append_that_fails_is_500_on_every_write() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-append@example.com").await;
        fail_writes(&db, "events", "true").await;
        assert_enroll_and_end_fail(&db, &app, user).await;
        assert_eq!(events_of_type(&db, user, "session_end").await.len(), 0);

        // A start with no open session appends `session_start`.
        let fresh = seed_learner(&db, "fault-append-start@example.com").await;
        assert_internal(&app, Method::POST, "/api/session/start", fresh, None).await;
        assert_eq!(events_of_type(&db, fresh, "session_start").await.len(), 0);
    })
    .await;
}

/// A fold save that fails stops the session start.
#[tokio::test]
async fn a_fold_save_that_fails_is_500_on_the_session_start() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-fold@example.com").await;
        fail_writes(&db, "learner_models", "true").await;
        assert_internal(&app, Method::POST, "/api/session/start", user, None).await;
    })
    .await;
}

/// A D-S6 write that fails stops the session start.
#[tokio::test]
async fn a_state_write_that_fails_is_500_on_the_session_start() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-state-write@example.com").await;
        fail_writes(&db, "web_states", "true").await;
        assert_internal(&app, Method::POST, "/api/session/start", user, None).await;
    })
    .await;
}

/// A D-S6 clear that fails stops the enroll and the session end.
#[tokio::test]
async fn a_state_clear_that_fails_is_500_on_the_enroll_and_the_end() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = cached_learner(&db, "fault-clear@example.com").await;
        fail_deletes(&db, "web_states", "true").await;
        assert_enroll_and_end_fail(&db, &app, user).await;
    })
    .await;
}

/// A lock wait that runs past the client bound is `500` at the bound, before
/// the server-side lock timeout.
#[tokio::test]
async fn a_lock_wait_past_the_client_bound_is_500_on_the_session_start() {
    TestDb::with(|db| async move {
        let app = cadus_web::create_app(
            cadus_web::AppState::new(cadus_store::Db::new(db.app.clone(), 500)).with_content(
                std::sync::Arc::new(cadus_web::state::Content::new(common::sessions::graph())),
            ),
        );
        let user = cached_learner(&db, "fault-bound@example.com").await;
        let held = hold_state_lock(&db, user).await;
        assert_internal(&app, Method::POST, "/api/session/start", user, None).await;
        drop(held);
    })
    .await;
}
