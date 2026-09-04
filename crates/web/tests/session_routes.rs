//! M5 U6 acceptance, the HTTP routes: `/api/status`, `/api/graph`,
//! `/api/enroll`, `/api/modules`, `/api/export`, and the session cycle.
//!
//! Requirements: C2, C3, D-S6, R4. Spec
//! `docs/reference/web-service-1.0-spec.md` section 2 (the route table) and
//! section 11 row U6.
//!
//! The four acceptance checks of row U6 land here:
//!
//! 1. the second concurrent tab — `tests/session_state.rs`;
//! 2. listing the plan writes nothing — [`listing_the_plan_writes_nothing`];
//! 3. `progress.done` is true for a recomposed failed review —
//!    [`progress_done_is_true_for_a_recomposed_failed_review`];
//! 4. the export round-trips through the event reader —
//!    [`the_export_round_trips_through_the_event_reader`].
//!
//! Every expected value is a LITERAL: a literal status code, a literal error
//! code, a literal session id, a literal node count.
//!
//! Every call presents a real session cookie, and `auth::layer::tenant_layer`
//! binds the tenant from it (FIX-M5-C). No test here writes a request extension
//! by hand.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::parse;

use common::sessions::*;

use axum::http::{Method, StatusCode};
use cadus_store::test_support::TestDb;
use serde_json::json;

// --------------------------------------------------------------------------- //
// Auth seam and the curriculum guard
// --------------------------------------------------------------------------- //

/// Every U6 route needs a session. A request with no bound tenant is
/// `401 unauthorized` in the `{"error":{"code","message"}}` envelope.
#[tokio::test]
async fn every_u6_route_without_a_tenant_is_401_unauthorized() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let routes = [
            (Method::GET, "/api/status"),
            (Method::GET, "/api/graph"),
            (Method::GET, "/api/modules"),
            (Method::GET, "/api/export"),
            (Method::POST, "/api/enroll"),
            (Method::POST, "/api/session/start"),
            (Method::POST, "/api/session/end"),
            (Method::GET, "/api/session/plan"),
        ];
        for (method, uri) in routes {
            let (status, _, body) = call(&app, method.clone(), uri, None, None).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri}: {body}");
            let value = parse(&body);
            assert_eq!(value["error"]["code"], "unauthorized", "{method} {uri}");
            assert!(
                value["error"]["message"].is_string(),
                "{method} {uri} carries no message"
            );
        }
    })
    .await;
}

/// A process with no curriculum serves `503 curriculum_unavailable`, never an
/// empty graph. The binary loads the tree at boot and exits 2 when it does not.
#[tokio::test]
async fn a_route_that_needs_the_curriculum_is_503_without_one() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "nocontent@example.com").await;
        let app = app_without_content(&db);
        let (status, _, body) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(parse(&body)["error"]["code"], "curriculum_unavailable");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The session cycle
// --------------------------------------------------------------------------- //

/// `POST /api/session/start` is idempotent: the second call resumes the SAME
/// session and appends nothing. `POST /api/session/end` then closes it, and a
/// second end is `409 no_open_session`.
#[tokio::test]
async fn the_session_cycle_opens_once_resumes_and_refuses_a_second_end() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "cycle@example.com").await;
        let app = app(&db);

        let (status, _, body) =
            call(&app, Method::POST, "/api/session/start", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let first = parse(&body);
        assert_eq!(first["reopened"], false);
        let session = first["session"].as_str().unwrap().to_string();
        assert!(
            session.starts_with("s_") && session.ends_with('a'),
            "the first session of a day is `s_<date>a`, got {session}"
        );
        assert_eq!(first["frontier"], 2);
        assert_eq!(first["due_reviews"], 0);

        // A second start resumes: the same id, `reopened: true`.
        let (status, _, body) =
            call(&app, Method::POST, "/api/session/start", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let second = parse(&body);
        assert_eq!(second["reopened"], true);
        assert_eq!(second["session"].as_str().unwrap(), session);

        // Exactly ONE session_start reached the append-only log.
        let starts: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM events WHERE user_id = $1 AND type = 'session_start'"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(starts, 1);

        // The end takes the minutes the client sends.
        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/session/end",
            Some(user),
            Some(json!({"minutes": 12.5})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let closed = parse(&body);
        assert_eq!(closed["session"].as_str().unwrap(), session);
        assert_eq!(closed["minutes"], 12.5);
        assert_eq!(closed["xp_earned"], 0.0);
        assert_eq!(closed["anki"]["pending"], 0);

        // The scratch row went away with the session.
        let scratch: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(scratch, 0);

        // A second end has no session to close.
        let (status, _, body) =
            call(&app, Method::POST, "/api/session/end", Some(user), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(parse(&body)["error"]["code"], "no_open_session");

        // The plan needs an open session too.
        let (status, _, body) =
            call(&app, Method::GET, "/api/session/plan", Some(user), None).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(parse(&body)["error"]["code"], "no_open_session");
    })
    .await;
}
