//! Part of `tests/session_routes.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{BASE_US, SESSION, parse, seed_open_session};

use common::sessions::*;

use cadus_core::event::{Event, SchemaVersion, Timestamp};
use cadus_web::state::WebState;

// --------------------------------------------------------------------------- //
// enroll, status, graph, modules
// --------------------------------------------------------------------------- //

/// `POST /api/enroll` refuses a missing course with `422 invalid_request` and an
/// unknown one with `404 unknown_course`. A good one appends `enrolled`, reports
/// the mastery floor, and clears the scratch.
#[tokio::test]
async fn enroll_refuses_a_missing_or_unknown_course_and_clears_the_scratch() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "enroll@example.com").await;
        let app = app(&db);

        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/enroll",
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(parse(&body)["error"]["code"], "invalid_request");

        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/enroll",
            Some(user),
            Some(json!({"course": "no-such-course"})),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(parse(&body)["error"]["code"], "unknown_course");

        // A stale scratch row stands before the switch.
        sqlx::query!(
            "INSERT INTO web_states (user_id, doc) VALUES ($1, $2)",
            user,
            WebState::for_session(SESSION).to_doc()
        )
        .execute(&db.admin)
        .await
        .unwrap();

        let (status, _, body) = call(
            &app,
            Method::POST,
            "/api/enroll",
            Some(user),
            Some(json!({"course": "c1"})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let enrolled = parse(&body);
        assert_eq!(enrolled["enrolled"], "c1");
        assert_eq!(enrolled["mastery_floor"], json!(["addition"]));
        assert_eq!(enrolled["floor_size"], 1);

        let events: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM events WHERE user_id = $1 AND type = 'enrolled'"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(events, 1);

        let scratch: i64 = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM web_states WHERE user_id = $1"#,
            user
        )
        .fetch_one(&db.admin)
        .await
        .unwrap();
        assert_eq!(scratch, 0, "the enroll left a stale served problem behind");
    })
    .await;
}

/// `GET /api/status` reports the enrolled course, the journey, and the counts.
#[tokio::test]
async fn status_reports_the_enrolled_course_and_the_counts() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "status@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        seed_due_review(&db, user, 1).await;
        seed_event(&db, user, 2, &enrolled_c1()).await;

        let (status, _, body) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let value = parse(&body);

        assert_eq!(value["course"]["id"], "c1");
        assert_eq!(value["course"]["name"], "Foundations");
        assert_eq!(
            value["courses"],
            json!([
                {"id": "c1", "name": "Foundations", "current": true},
                {"id": "c2", "name": "Proofs", "current": false},
            ])
        );
        assert_eq!(value["test_prep"], Value::Null);
        assert_eq!(value["placed"], true);
        assert_eq!(value["due_reviews"], 1);
        // `subtraction` is the only unmastered `c1` topic with no prerequisite.
        assert_eq!(value["frontier"], 1);
        assert_eq!(value["quiz_due"], false);
        assert_eq!(value["drill_due"], false);
        assert_eq!(value["xp"]["goal"], 40);
        assert_eq!(value["pending_remediation"], json!([]));
    })
    .await;
}

/// `GET /api/graph` filters by scope, `404`s an unknown course, and never
/// leaves the request tenant.
#[tokio::test]
async fn graph_filters_by_scope_and_404s_an_unknown_course() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "graph@example.com").await;
        let app = app(&db);

        let (status, _, body) =
            call(&app, Method::GET, "/api/graph?scope=all", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let all = parse(&body);
        assert_eq!(all["scope"], "all");
        assert_eq!(all["counts"]["nodes"], 3);
        assert_eq!(all["counts"]["edges"], 1);
        assert_eq!(all["counts"]["mastered"], 0);
        assert_eq!(all["modules"], json!(["M1", "M2"]));
        assert_eq!(
            all["edges"],
            json!([{"from": "addition", "to": "fractions"}])
        );
        assert_eq!(all["nodes"][0]["id"], "addition");
        assert_eq!(all["nodes"][0]["name"], "The addition topic");
        assert_eq!(all["nodes"][0]["status"], "untouched");
        assert!(all["now"].is_string());

        let (status, _, body) =
            call(&app, Method::GET, "/api/graph?scope=c1", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let one = parse(&body);
        assert_eq!(one["counts"]["nodes"], 2);
        // `fractions` is out of scope, so its prerequisite edge is out too.
        assert_eq!(one["counts"]["edges"], 0);
        assert_eq!(one["modules"], json!(["M1"]));

        let (status, _, body) = call(
            &app,
            Method::GET,
            "/api/graph?scope=no-such-course",
            Some(user),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(parse(&body)["error"]["code"], "unknown_course");
    })
    .await;
}

/// `GET /api/modules` lists the enrolled course's modules in curriculum order.
#[tokio::test]
async fn modules_lists_the_enrolled_course_modules() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "modules@example.com").await;
        let app = app(&db);

        // With no enrollment the whole curriculum is in scope.
        let (status, _, body) = call(&app, Method::GET, "/api/modules", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let every = parse(&body);
        assert_eq!(every["course"]["id"], Value::Null);
        assert_eq!(every["modules"], json!(["M1", "M2"]));

        seed_event(
            &db,
            user,
            1,
            &Event::Enrolled(cadus_core::event::Enrolled {
                ts: Timestamp::from_micros(BASE_US),
                session: None,
                v: SchemaVersion,
                course: cadus_core::event::Slug::new("c2").unwrap(),
                reason: None,
                return_to: None,
            }),
        )
        .await;

        let (status, _, body) = call(&app, Method::GET, "/api/modules", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let scoped = parse(&body);
        assert_eq!(scoped["course"]["id"], "c2");
        assert_eq!(scoped["course"]["name"], "Proofs");
        assert_eq!(scoped["modules"], json!(["M2"]));
    })
    .await;
}
