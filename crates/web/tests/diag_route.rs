//! The placement diagnostic over HTTP: `POST /api/diag/start`, `/api/diag/answer`
//! and `/api/diag/finish` (spec section 2).
//!
//! The spec table marks the three rows `keep`, and no M5 unit built them. This
//! file is the acceptance of the unit that did.
//!
//! The checks:
//!
//! 1. the loop walks probes and places the learner, and both events land;
//! 2. neither reply carries the expected answer or the solution;
//! 3. a first-run start enrolls the entry course, so placement needs no enroll;
//! 4. every refusal of the spec: `404 unknown_course`, `409 no_diagnostic`,
//!    `404 unknown_problem`, `422 invalid_request`.
//!
//! Every expected value is a literal.
//!
//! The tenant comes from a request extension, as `tests/quiz_route.rs` does: the
//! subject here is the handler and not the auth layer.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::config::Config;
use cadus_core::curriculum::model::{Exemplar, KnowledgePoint, PrereqEdge};
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, RawCurriculum, RawUnit, Slug, Topic, Unit,
};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::{Content, Tenant};
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use tower::ServiceExt;

/// The answer every exemplar of the fixture expects.
const ANSWER: &str = "7";

/// The solution sketch of every exemplar. No reply may carry it.
const SKETCH: &str = "add the two numbers and read the total";

/// The course of the fixture.
const COURSE: &str = "c1";

// --------------------------------------------------------------------------- //
// The fixture
// --------------------------------------------------------------------------- //

/// One topic with a diagnostic exemplar and a numeric answer kind.
fn topic(id: &str, prereq: Option<&str>) -> Topic {
    Topic {
        id: Slug::new(id).unwrap(),
        name: id.to_owned(),
        core: true,
        difficulty: 0.3,
        drill: false,
        answer_kind: AnswerKind::Numeric,
        expected_time_secs: 30,
        prerequisites: prereq
            .map(|parent| {
                vec![PrereqEdge {
                    id: Slug::new(parent).unwrap(),
                    weight: 1.0,
                    key: true,
                }]
            })
            .unwrap_or_default(),
        encompassings_extra: Vec::new(),
        knowledge_points: vec![KnowledgePoint {
            id: Slug::new("kp1").unwrap(),
            name: "kp1".to_owned(),
            key_prerequisites: Vec::new(),
            exemplars: Vec::new(),
            constraints: None,
        }],
        diagnostic_exemplar: Some(Exemplar {
            problem: format!("probe for {id}: what is 3 + 4?"),
            answer: ANSWER.to_owned(),
            solution_sketch: Some(SKETCH.to_owned()),
        }),
        anki_seeds: Vec::new(),
    }
}

/// One course of three topics in a chain.
fn graph() -> Curriculum {
    let catalog = Catalog {
        courses: vec![Course {
            id: Slug::new(COURSE).unwrap(),
            name: "Foundations".to_owned(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
        }],
    };
    Curriculum::build(RawCurriculum {
        catalog,
        units: vec![RawUnit {
            course_id: COURSE.to_owned(),
            file_name: "00-M1.yaml".to_owned(),
            unit: Unit {
                unit: "M1".to_owned(),
                course: Slug::new(COURSE).unwrap(),
                module: "M1".to_owned(),
                topics: vec![
                    topic("addition", None),
                    topic("subtraction", Some("addition")),
                    topic("word-problems", Some("subtraction")),
                ],
            },
            first_load_index: 0,
        }],
    })
    .unwrap()
}

/// The router of a test, with the fixture curriculum loaded.
fn app(db: &TestDb) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS)).with_content(Arc::new(
            Content {
                curriculum: graph(),
                cfg: Config::default(),
            },
        )),
    )
}

/// One POST as `user`, with the raw body of the answer.
async fn call(app: &Router, user: Uuid, path: &str, body: &Value) -> (StatusCode, String) {
    let mut request = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    request.extensions_mut().insert(Tenant(user));
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into())
}

/// The parsed body of a call.
fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|err| panic!("body is not JSON: {err}\n{body}"))
}

/// The `error.code` of an error envelope.
fn code(body: &str) -> String {
    parse(body)
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("no error.code")
        .to_owned()
}

/// The event type names of one learner's log, in order.
async fn event_types(db: &TestDb, user: Uuid) -> Vec<String> {
    sqlx::query_scalar!(
        r#"SELECT type AS "type!" FROM events WHERE user_id = $1 ORDER BY seq"#,
        user
    )
    .fetch_all(&db.admin)
    .await
    .unwrap()
}

/// A learner with no history.
async fn learner(db: &TestDb, email: &str) -> Uuid {
    db.seed_user(email).await
}

// --------------------------------------------------------------------------- //
// The loop
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn the_placement_loop_walks_probes_and_places_the_learner() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "loop@example.test").await;

        let (status, body) = call(&app, user, "/api/diag/start", &json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let start = parse(&body);
        assert_eq!(start["asked"], 0);
        assert_eq!(start["cap"], 40);
        let probe = start["probe"].clone();
        // The probe carries three fields and no fourth.
        let keys: Vec<&String> = probe.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["problem_id", "text", "topic"]);

        let mut problem_id = probe["problem_id"].as_str().unwrap().to_owned();
        let mut asked = 0;
        loop {
            asked += 1;
            assert!(asked < 20, "the probe list did not end");
            let (status, body) = call(
                &app,
                user,
                "/api/diag/answer",
                &json!({ "problem_id": problem_id, "answer": ANSWER }),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{body}");
            let reply = parse(&body);
            assert_eq!(reply["correct"], true);
            let next = reply["next_probe"].clone();
            if next["done"] == Value::Bool(true) {
                break;
            }
            problem_id = next["problem_id"].as_str().unwrap().to_owned();
        }

        let (status, body) = call(&app, user, "/api/diag/finish", &json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let placed = parse(&body);
        // One correct answer credits the topic and every ancestor, so the whole
        // chain places.
        assert_eq!(
            placed["placed"],
            json!(["addition", "subtraction", "word-problems"])
        );
        assert!(placed["conditional"].is_array());
        assert!(placed["frontier"].is_array());

        let types = event_types(&db, user).await;
        assert!(
            types.iter().any(|name| name == "diagnostic_answer"),
            "no diagnostic_answer in {types:?}"
        );
        assert_eq!(
            types.last().map(String::as_str),
            Some("diagnostic_placed"),
            "the placement is not the last event: {types:?}"
        );

        // The diagnostic is closed: a second finish has nothing to close.
        assert_finish_is_no_diagnostic(&app, user).await;
    })
    .await;
}

/// Fail the test when a finish for `user` is not `409 no_diagnostic`.
async fn assert_finish_is_no_diagnostic(app: &Router, user: Uuid) {
    let (status, body) = call(app, user, "/api/diag/finish", &json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(code(&body), "no_diagnostic");
}

#[tokio::test]
async fn no_reply_carries_the_expected_answer_or_the_solution() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "leak@example.test").await;

        let (_, start) = call(&app, user, "/api/diag/start", &json!({})).await;
        assert!(!start.contains(SKETCH), "the start leaked the solution");
        let problem_id = parse(&start)["probe"]["problem_id"]
            .as_str()
            .unwrap()
            .to_owned();

        // A wrong answer is the case that tempts a service to explain itself.
        let (_, reply) = call(
            &app,
            user,
            "/api/diag/answer",
            &json!({ "problem_id": problem_id, "answer": "12" }),
        )
        .await;
        assert_eq!(parse(&reply)["correct"], false);
        assert!(!reply.contains(SKETCH), "the answer leaked the solution");
        assert!(
            !reply.contains("\"expected\""),
            "the answer leaked the expected field: {reply}"
        );
    })
    .await;
}

#[tokio::test]
async fn a_blank_answer_is_the_honest_skip_and_grades_incorrect() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "skip@example.test").await;
        let (_, start) = call(&app, user, "/api/diag/start", &json!({})).await;
        let problem_id = parse(&start)["probe"]["problem_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let (status, body) = call(
            &app,
            user,
            "/api/diag/answer",
            &json!({ "problem_id": problem_id, "answer": "   " }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(parse(&body)["correct"], false);
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The first run
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn a_first_run_start_enrolls_the_entry_course() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "firstrun@example.test").await;

        let (status, body) = call(&app, user, "/api/diag/start", &json!({})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(parse(&body)["probe"]["problem_id"].is_string());

        let types = event_types(&db, user).await;
        assert_eq!(
            types.first().map(String::as_str),
            Some("enrolled"),
            "the entry course was not bound: {types:?}"
        );
    })
    .await;
}

#[tokio::test]
async fn a_second_start_replaces_the_diagnostic_in_progress() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "restart@example.test").await;

        let (_, first) = call(&app, user, "/api/diag/start", &json!({})).await;
        let stale = parse(&first)["probe"]["problem_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let (_, second) = call(&app, user, "/api/diag/start", &json!({})).await;
        assert_eq!(parse(&second)["asked"], 0);

        // The first probe is superseded, so its id no longer answers.
        let (status, body) = call(
            &app,
            user,
            "/api/diag/answer",
            &json!({ "problem_id": stale, "answer": ANSWER }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(code(&body), "unknown_problem");
    })
    .await;
}

// --------------------------------------------------------------------------- //
// The refusals
// --------------------------------------------------------------------------- //

#[tokio::test]
async fn an_unknown_course_is_404_unknown_course() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "nocourse@example.test").await;
        let (status, body) =
            call(&app, user, "/api/diag/start", &json!({ "course": "nope" })).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(code(&body), "unknown_course");
    })
    .await;
}

#[tokio::test]
async fn an_answer_with_no_diagnostic_is_409_no_diagnostic() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "noopen@example.test").await;
        let (status, body) = call(
            &app,
            user,
            "/api/diag/answer",
            &json!({ "problem_id": "whatever", "answer": ANSWER }),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(code(&body), "no_diagnostic");
    })
    .await;
}

#[tokio::test]
async fn a_finish_with_no_diagnostic_is_409_no_diagnostic() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "nofinish@example.test").await;
        assert_finish_is_no_diagnostic(&app, user).await;
    })
    .await;
}

#[tokio::test]
async fn an_answer_with_no_problem_id_is_422_invalid_request() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "nobody@example.test").await;
        call(&app, user, "/api/diag/start", &json!({})).await;
        let (status, body) =
            call(&app, user, "/api/diag/answer", &json!({ "answer": ANSWER })).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(code(&body), "invalid_request");
    })
    .await;
}

#[tokio::test]
async fn a_stale_problem_id_is_404_unknown_problem() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "stale@example.test").await;
        call(&app, user, "/api/diag/start", &json!({})).await;
        let (status, body) = call(
            &app,
            user,
            "/api/diag/answer",
            &json!({ "problem_id": "00000000000000000000000000000000", "answer": ANSWER }),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(code(&body), "unknown_problem");
    })
    .await;
}
