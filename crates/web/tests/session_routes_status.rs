//! Part of `tests/session_routes.rs`: the header of that file gives the
//! requirements and the rules.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{BASE_US, SESSION, parse, seed_open_session};

use common::sessions::*;

use std::collections::BTreeMap;

use cadus_core::event::{Event, SchemaVersion, Timestamp, TopicStatus};
use cadus_core::learner::{LearnerModel, TopicState};
use cadus_web::state::WebState;
use sqlx::types::chrono::Utc;

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

/// A learner with an open session, a due review of `addition`, and an
/// enrollment in `c1`.
async fn enrolled_learner(db: &TestDb, email: &str) -> Uuid {
    let user = common::seed_learner(db, email).await;
    seed_open_session(db, user).await;
    seed_due_review(db, user, 1).await;
    seed_event(db, user, 2, &enrolled_c1()).await;
    user
}

/// One GET of `path` as `user`, which must succeed, read as JSON.
async fn get_json(app: &Router, user: Uuid, path: &str) -> Value {
    let (status, _, body) = call(app, Method::GET, path, Some(user), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

/// Cache a model of `user` holding one topic in `state`, at `seq`.
async fn seed_one_topic(db: &TestDb, user: Uuid, topic: &str, state: TopicState, seq: i64) {
    let mut topics: BTreeMap<String, TopicState> = BTreeMap::new();
    topics.insert(topic.to_string(), state);
    let model = LearnerModel {
        topics,
        ..LearnerModel::default()
    };
    common::seed_cached_model(db, user, &model, seq).await;
}

/// `GET /api/status` reports the enrolled course, the journey, and the counts.
#[tokio::test]
async fn status_reports_the_enrolled_course_and_the_counts() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = enrolled_learner(&db, "status@example.com").await;

        let value = get_json(&app, user, "/api/status").await;

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

// --------------------------------------------------------------------------- //
// The dashboard of a learner with no session, and the review that is nearly due
// --------------------------------------------------------------------------- //

/// A model whose one topic stands on the frontier is not placed: the
/// placement check reads its false arm on a cached model. The fold drops a
/// topic that equals the default state, so the state must differ from it.
#[tokio::test]
async fn status_reads_a_frontier_model_as_not_placed() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "frontier@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        let frontier = TopicState {
            status: TopicStatus::Frontier,
            ..TopicState::default()
        };
        seed_one_topic(&db, user, "addition", frontier, 1).await;

        let value = get_json(&app, user, "/api/status").await;
        assert_eq!(value["placed"], false);
        assert_eq!(value["due_reviews"], 0);
        assert_eq!(value["nearly_due"], 0);
    })
    .await;
}

/// A review whose memory stands between the due threshold and the nearly-due
/// threshold counts as nearly due, and not as due.
#[tokio::test]
async fn status_counts_a_review_that_is_nearly_due() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "nearly@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        // 0.9 days into a 1-day interval: the memory is 0.5^0.9, about 0.54.
        let t0 = Utc::now().timestamp_micros() - 9 * 8_640_000_000;
        let nearly = TopicState {
            status: TopicStatus::Learning,
            rep_num: 1.0,
            memory_base: 1.0,
            t0: Some(Timestamp::from_micros(t0)),
            interval_days: 1.0,
            ability: 0.6,
            ..TopicState::default()
        };
        seed_one_topic(&db, user, "addition", nearly, 1).await;

        let value = get_json(&app, user, "/api/status").await;
        assert_eq!(value["due_reviews"], 0);
        assert_eq!(value["nearly_due"], 1);
    })
    .await;
}

/// Without a scope the graph reads the enrolled course, counts the mastered
/// topics, and a scope whose prerequisite lies outside it draws no edge to it.
#[tokio::test]
async fn graph_without_a_scope_reads_the_enrolled_course() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = enrolled_learner(&db, "graph-enrolled@example.com").await;

        let enrolled = get_json(&app, user, "/api/graph").await;
        // `c1` holds two of the three topics.
        assert_eq!(enrolled["counts"]["nodes"], 2);
        assert_eq!(enrolled["counts"]["mastered"], 1);

        let other = get_json(&app, user, "/api/graph?scope=c2").await;
        assert_eq!(other["counts"]["nodes"], 1);
        // `addition` is out of scope, so the edge into `fractions` is out too.
        assert_eq!(other["counts"]["edges"], 0);
    })
    .await;
}

/// A topic in the placed state counts as placed on the dashboard: the Placed
/// arm of the placement check.
#[tokio::test]
async fn status_reads_a_placed_topic_as_placed() {
    TestDb::with(|db| async move {
        let user = common::seed_learner(&db, "placed@example.com").await;
        let app = app(&db);
        seed_open_session(&db, user).await;
        let placed = TopicState {
            status: TopicStatus::Placed,
            ..TopicState::default()
        };
        seed_one_topic(&db, user, "addition", placed, 1).await;

        let value = get_json(&app, user, "/api/status").await;
        assert_eq!(value["placed"], true);
    })
    .await;
}
