//! The second probe of the placement diagnostic over HTTP.
//!
//! When the first phase ends and fewer than `diag.max_questions` answers stand,
//! the run asks a missed or thin topic one more time with another exemplar. The
//! checks:
//!
//! 1. a thin topic is asked again with another problem, once;
//! 2. a first miss then a correct second answer undoes the miss;
//! 3. a second miss keeps the miss, and a correct second answer clears the
//!    conditional flag of a thin topic;
//! 4. the cap holds, and no reply carries the answer or the sketch;
//! 5. a run stored before the second phase goes on into it, and a resume returns
//!    the same probe.
//!
//! Every expected value is a literal.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::placement::*;
use common::{events_of_type, open_content_with};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use cadus_core::config::Config;
use cadus_core::curriculum::model::{Catalog, Exemplar};
use cadus_core::curriculum::{Course, Curriculum, RawCurriculum, RawUnit, Slug, Topic, Unit};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::Tenant;
use cadus_web::{AppState, create_app};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::types::Uuid;
use tower::ServiceExt;

/// One POST as `user`.
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

/// A learner with no history.
async fn learner(db: &TestDb, email: &str) -> Uuid {
    db.seed_user(email).await
}

/// The topic with two exemplars in its knowledge point. Both expect `7`.
fn with_two(mut item: Topic) -> Topic {
    let id = item.id.as_str().to_owned();
    item.knowledge_points[0].exemplars = ["A", "B"]
        .iter()
        .map(|tag| Exemplar {
            answer_contract: None,
            problem: format!("second probe {tag} for {id}"),
            answer: ANSWER.to_owned(),
            solution_sketch: Some(SKETCH.to_owned()),
            visual: None,
            points: None,
        })
        .collect();
    item
}

/// The catalog of the fixture: one course.
fn catalog() -> Catalog {
    Catalog {
        courses: vec![Course {
            id: Slug::new(COURSE).unwrap(),
            name: "Foundations".to_owned(),
            order: 0,
            mastery_floor: Vec::new(),
            mastery_floor_course: None,
            reading: Vec::new(),
            exam_weights: None,
        }],
    }
}

/// One unit of the course, in the module `module`.
fn unit(module: &str, index: usize, topics: Vec<Topic>) -> RawUnit {
    RawUnit {
        course_id: COURSE.to_owned(),
        file_name: format!("0{index}-{module}.yaml"),
        unit: Unit {
            unit: module.to_owned(),
            course: Slug::new(COURSE).unwrap(),
            module: module.to_owned(),
            topics,
        },
        first_load_index: index,
    }
}

/// The chain of the placement fixture, each topic with two extra exemplars.
fn chain_graph() -> Curriculum {
    Curriculum::build(RawCurriculum {
        catalog: catalog(),
        units: vec![unit(
            "M1",
            0,
            vec![
                with_two(topic("addition", None)),
                with_two(topic("subtraction", Some("addition"))),
                with_two(topic("word-problems", Some("subtraction"))),
            ],
        )],
    })
    .unwrap()
}

/// One topic alone, so the first phase is one probe.
fn solo_graph() -> Curriculum {
    Curriculum::build(RawCurriculum {
        catalog: catalog(),
        units: vec![unit("M1", 0, vec![with_two(topic("alpha", None))])],
    })
    .unwrap()
}

/// Two topics with no edge, in two modules, so neither one is a sibling leaf of
/// the other.
fn pair_graph() -> Curriculum {
    Curriculum::build(RawCurriculum {
        catalog: catalog(),
        units: vec![
            unit("M1", 0, vec![with_two(topic("alpha", None))]),
            unit("M2", 1, vec![with_two(topic("beta", None))]),
        ],
    })
    .unwrap()
}

/// The router over `graph` with `cfg`.
fn app_over(db: &TestDb, graph: Curriculum, cfg: Config) -> Router {
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(std::sync::Arc::new(open_content_with(graph, cfg))),
    )
}

/// Start the run and return the body.
async fn start(app: &Router, user: Uuid) -> Value {
    let (status, body) = call(app, user, "/api/diag/start", &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

/// Answer `probe` with `given` and return the whole reply text.
async fn answer(app: &Router, user: Uuid, probe: &Value, given: &str) -> String {
    let (status, body) = call(
        app,
        user,
        "/api/diag/answer",
        &json!({ "problem_id": probe["problem_id"], "answer": given }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
}

/// The next probe of a reply text.
fn next_of(reply: &str) -> Value {
    parse(reply)["next_probe"].clone()
}

/// Whether a probe is the done marker.
fn is_done(probe: &Value) -> bool {
    probe["done"] == Value::Bool(true)
}

/// Close the run and read the placement.
async fn finish(app: &Router, user: Uuid) -> Value {
    let (status, body) = call(app, user, "/api/diag/finish", &json!({})).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    parse(&body)
}

/// The text of a probe.
fn text_of(probe: &Value) -> &str {
    probe["text"].as_str().unwrap()
}

#[tokio::test]
async fn a_thin_topic_is_asked_again_with_another_problem() {
    TestDb::with(|db| async move {
        let app = app_over(&db, pair_graph(), Config::default());
        let user = learner(&db, "second-thin@example.test").await;
        let mut probe = start(&app, user).await["probe"].clone();
        let mut firsts: Vec<(String, String, String)> = Vec::new();
        let mut seconds: Vec<(String, String, String)> = Vec::new();
        let mut answers = 0;
        while !is_done(&probe) {
            let row = (
                probe["topic"].as_str().unwrap().to_owned(),
                text_of(&probe).to_owned(),
                probe["problem_id"].as_str().unwrap().to_owned(),
            );
            if text_of(&probe).starts_with("second probe") {
                seconds.push(row);
            } else {
                firsts.push(row);
            }
            answers += 1;
            assert!(answers < 10, "the run did not end");
            probe = next_of(&answer(&app, user, &probe, ANSWER).await);
        }
        assert_eq!(firsts.len(), 2);
        assert_eq!(seconds.len(), 2);
        for (topic, text, problem_id) in &seconds {
            let first = firsts.iter().find(|row| &row.0 == topic).unwrap();
            assert_ne!(&first.1, text);
            assert_ne!(&first.2, problem_id);
            assert!(text.starts_with(&format!("second probe A for {topic}")));
        }
        // The first phase ends before the second starts.
        let events = events_of_type(&db, user, "diagnostic_answer").await;
        assert_eq!(events.len(), 4);
        assert!(events.len() < 40);
        let topics: Vec<&str> = events
            .iter()
            .map(|event| event["topic"].as_str().unwrap())
            .collect();
        // Both first probes come before both second probes.
        assert_ne!(topics[0], topics[1]);
        assert_eq!(topics.iter().filter(|name| **name == "alpha").count(), 2);
        assert_eq!(topics.iter().filter(|name| **name == "beta").count(), 2);
    })
    .await;
}

#[tokio::test]
async fn the_second_probe_counts_in_the_progress_line() {
    TestDb::with(|db| async move {
        let app = app_over(&db, solo_graph(), Config::default());
        let user = learner(&db, "second-count@example.test").await;
        let first = start(&app, user).await;
        assert_eq!(first["asked"], 0);
        let second = next_of(&answer(&app, user, &first["probe"], ANSWER).await);
        assert!(text_of(&second).starts_with("second probe"));
        let again = start(&app, user).await;
        assert_eq!(again["asked"], 1);
        assert_eq!(again["cap"], 40);
    })
    .await;
}

#[tokio::test]
async fn a_second_miss_keeps_the_miss() {
    TestDb::with(|db| async move {
        let app = app_over(&db, solo_graph(), Config::default());
        let user = learner(&db, "second-miss@example.test").await;
        let first = start(&app, user).await["probe"].clone();
        let topic = first["topic"].as_str().unwrap().to_owned();
        let second = next_of(&answer(&app, user, &first, ANSWER).await);
        assert_eq!(second["topic"], topic.as_str());
        let reply = answer(&app, user, &second, "0").await;
        assert!(is_done(&next_of(&reply)));
        let placed = finish(&app, user).await;
        assert!(!placed["placed"].as_array().unwrap().contains(&json!(topic)));
        assert!(
            !placed["conditional"]
                .as_array()
                .unwrap()
                .contains(&json!(topic))
        );
    })
    .await;
}

#[tokio::test]
async fn a_correct_second_answer_clears_the_conditional() {
    TestDb::with(|db| async move {
        let app = app_over(&db, pair_graph(), Config::default());
        let user = learner(&db, "second-clear@example.test").await;
        let mut probe = start(&app, user).await["probe"].clone();
        while !is_done(&probe) {
            probe = next_of(&answer(&app, user, &probe, ANSWER).await);
        }
        let placed = finish(&app, user).await;
        assert_eq!(placed["placed"], json!(["alpha", "beta"]));
        assert_eq!(placed["conditional"], json!([]));
        let events = events_of_type(&db, user, "diagnostic_placed").await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["conditional"], json!([]));
    })
    .await;
}

#[tokio::test]
async fn a_slip_is_undone_by_a_correct_second_answer() {
    TestDb::with(|db| async move {
        let app = app_over(&db, solo_graph(), Config::default());
        let user = learner(&db, "second-slip@example.test").await;
        let first = start(&app, user).await["probe"].clone();
        let topic = first["topic"].as_str().unwrap().to_owned();
        let second = next_of(&answer(&app, user, &first, "0").await);
        assert_eq!(second["topic"], topic.as_str());
        assert!(text_of(&second).starts_with("second probe"));
        assert!(is_done(&next_of(
            &answer(&app, user, &second, ANSWER).await
        )));
        let placed = finish(&app, user).await;
        assert_eq!(placed["placed"], json!([topic]));
        assert_eq!(placed["conditional"], json!([]));
        assert_eq!(
            events_of_type(&db, user, "diagnostic_answer").await.len(),
            2
        );
    })
    .await;
}

#[tokio::test]
async fn a_miss_then_a_second_miss_places_nothing() {
    TestDb::with(|db| async move {
        let app = app_over(&db, solo_graph(), Config::default());
        let user = learner(&db, "second-twice@example.test").await;
        let first = start(&app, user).await["probe"].clone();
        let second = next_of(&answer(&app, user, &first, "0").await);
        assert!(is_done(&next_of(&answer(&app, user, &second, "0").await)));
        let placed = finish(&app, user).await;
        assert_eq!(placed["placed"], json!([]));
    })
    .await;
}

#[tokio::test]
async fn the_second_phase_stops_at_the_cap() {
    TestDb::with(|db| async move {
        // Two first probes, then a cap of three leaves room for one second probe.
        let mut cfg = Config::default();
        cfg.diag.max_questions = 3;
        let app = app_over(&db, pair_graph(), cfg);
        let user = learner(&db, "second-cap@example.test").await;
        let began = start(&app, user).await;
        assert_eq!(began["cap"], 3);
        let mut probe = began["probe"].clone();
        let mut asked = 0;
        while !is_done(&probe) {
            asked += 1;
            probe = next_of(&answer(&app, user, &probe, ANSWER).await);
        }
        assert_eq!(asked, 3);
        assert_eq!(
            events_of_type(&db, user, "diagnostic_answer").await.len(),
            3
        );

        // A cap of one ends the run after the first answer, with no second phase.
        let mut one = Config::default();
        one.diag.max_questions = 1;
        let app = app_over(&db, pair_graph(), one);
        let user = learner(&db, "second-cap-one@example.test").await;
        let probe = start(&app, user).await["probe"].clone();
        assert!(is_done(&next_of(&answer(&app, user, &probe, ANSWER).await)));
    })
    .await;
}

#[tokio::test]
async fn no_second_probe_reply_carries_the_answer_or_the_sketch() {
    TestDb::with(|db| async move {
        let app = app_over(&db, solo_graph(), Config::default());
        let user = learner(&db, "second-leak@example.test").await;
        let first = start(&app, user).await["probe"].clone();
        let reply = answer(&app, user, &first, ANSWER).await;
        let second = next_of(&reply);
        let keys: Vec<&String> = second.as_object().unwrap().keys().collect();
        assert_eq!(keys, vec!["problem_id", "text", "topic"]);
        assert!(!reply.contains(SKETCH), "{reply}");
        assert!(!reply.contains("\"answer\""), "{reply}");
        assert!(!reply.contains("\"expected\""), "{reply}");
        let (_, again) = call(&app, user, "/api/diag/start", &json!({})).await;
        assert!(!again.contains(SKETCH), "{again}");
    })
    .await;
}

#[tokio::test]
async fn a_run_stored_before_the_change_goes_on_into_the_second_phase() {
    TestDb::with(|db| async move {
        let app = app_over(&db, chain_graph(), Config::default());
        let user = learner(&db, "second-old@example.test").await;
        // A document of the earlier shape: no `second`, no `missed`.
        sqlx::query("INSERT INTO diag_states (user_id, state) VALUES ($1, $2::text::jsonb)")
            .bind(user)
            .bind(
                json!({
                    "course": COURSE,
                    "probe_set": ["word-problems"],
                    "balances": {
                        "addition": 1.0, "subtraction": 1.0, "word-problems": 1.0
                    },
                    "answered": ["word-problems"],
                    "supplemental": false,
                })
                .to_string(),
            )
            .execute(&db.admin)
            .await
            .unwrap();
        let body = start(&app, user).await;
        assert_eq!(body["asked"], 1);
        assert_eq!(body["probe"]["topic"], "word-problems");
        assert!(text_of(&body["probe"]).starts_with("second probe"));
    })
    .await;
}

#[tokio::test]
async fn a_resume_during_the_second_phase_returns_the_same_probe() {
    TestDb::with(|db| async move {
        let app = app_over(&db, chain_graph(), Config::default());
        let user = learner(&db, "second-resume@example.test").await;
        let first = start(&app, user).await["probe"].clone();
        let second = next_of(&answer(&app, user, &first, ANSWER).await);
        let once = start(&app, user).await;
        let twice = start(&app, user).await;
        assert_eq!(once["probe"]["problem_id"], second["problem_id"]);
        assert_eq!(twice["probe"]["problem_id"], second["problem_id"]);
        assert_eq!(once["asked"], twice["asked"]);
        assert_eq!(once["asked"], 1);
    })
    .await;
}
