//! `POST /api/quiz/round` with `{kind: "paper", set}` and with the scope
//! `course_and_prereqs` (D-VBE3).
//!
//! A paper serves the form of one exam set in the fixed order of the paper. The
//! points of an item weigh the score, and a multipart item earns part of its
//! points for each named part it got right. Every expected value is a literal.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_core::config::Config;
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, ExamWeights, Exemplar, PrereqEdge, RawCurriculum,
    Slug, Topic,
};
use cadus_core::event::{Event, QuizResult, SchemaVersion, Timestamp};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::WebState;
use cadus_web::{AppState, create_app};
use common::{
    BASE_US, SESSION, answer_raw, assert_reveals_nothing, call, enroll_course, kp, now_secs, parse,
    passed_lesson, put_state, seed_event, seed_learner, seed_open_session, serve_task,
    stored_state, topic, unit,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// The pool sizes of one set: 10 short, 6 structured, 6 long, 3 "show that".
const POOLS: [(usize, u8, usize); 4] = [(10, 1, 1), (6, 4, 2), (6, 7, 3), (3, 4, 2)];

fn item(problem: String, parts: usize, points: u8) -> Exemplar {
    let names = ["a", "b", "c"];
    let (answer, contract) = if parts == 1 {
        ("5".to_owned(), Some(json!({"kind": "exact"})))
    } else {
        let answer = names[..parts]
            .iter()
            .map(|name| format!("{name} = 3"))
            .collect::<Vec<_>>()
            .join("; ");
        let listed: Vec<Value> = names[..parts]
            .iter()
            .map(|name| json!({"name": name, "contract": {"kind": "exact"}}))
            .collect();
        (answer, Some(json!({"kind": "multipart", "parts": listed})))
    };
    Exemplar {
        problem,
        answer_contract: contract.map(|value| serde_json::from_value(value).unwrap()),
        answer,
        solution_sketch: Some("Work it out.".to_owned()),
        visual: None,
        points: Some(points),
    }
}

fn set_topic(id: &str) -> Topic {
    let points = POOLS
        .iter()
        .enumerate()
        .map(|(number, (count, points, parts))| {
            let items = (0..*count)
                .map(|at| {
                    item(
                        format!("{id} pool {} item {at}", number + 1),
                        *parts,
                        *points,
                    )
                })
                .collect();
            kp(&format!("kp{}", number + 1), items)
        })
        .collect();
    let mut record = topic(id, points);
    record.answer_kind = AnswerKind::MultiStep;
    record.expected_time_secs = 14_400;
    record.area = Some("models".to_owned());
    record
}

fn skill(id: &str) -> Topic {
    topic(
        id,
        vec![kp("kp1", vec![item(format!("{id} question"), 1, 1)])],
    )
}

fn checkpoint(id: &str, needs: &str, area: &str) -> Topic {
    let mut record = topic(
        id,
        vec![kp("kp1", vec![item(format!("{id} question"), 1, 1)])],
    );
    record.prerequisites = vec![PrereqEdge {
        id: Slug::new(needs).unwrap(),
        weight: 0.5,
        key: true,
    }];
    record.area = Some(area.to_owned());
    record
}

fn course(id: &str, name: &str, order: i64, weights: Option<ExamWeights>) -> Course {
    Course {
        id: Slug::new(id).unwrap(),
        name: name.to_owned(),
        order,
        mastery_floor: Vec::new(),
        mastery_floor_course: None,
        reading: Vec::new(),
        exam_weights: weights,
    }
}

fn curriculum() -> Curriculum {
    let weights = ExamWeights {
        numbers: 15,
        models: 50,
        geometry: 20,
        data: 15,
    };
    let skills = vec![skill("sk-one"), skill("sk-two")];
    let exam = vec![
        checkpoint("cp-one", "sk-one", "numbers"),
        checkpoint("cp-two", "sk-two", "models"),
        set_topic("set1"),
        set_topic("set2"),
        topic(
            "lone",
            vec![kp("kp1", vec![item("lone question".to_owned(), 1, 1)])],
        ),
    ];
    Curriculum::build(RawCurriculum {
        catalog: Catalog {
            courses: vec![
                course("c0", "Skills", 0, None),
                course("c1", "Exam course", 1, Some(weights)),
            ],
        },
        units: vec![unit("c0", "M0", skills, 0), unit("c1", "M1", exam, 2)],
    })
    .unwrap()
}

fn app(db: &TestDb) -> Router {
    let content = common::open_content_with(curriculum(), Config::default());
    create_app(
        AppState::new(Db::new(db.app.clone(), DEFAULT_CLIENT_TIMEOUT_MS))
            .with_content(Arc::new(content)),
    )
}

async fn learner(db: &TestDb, email: &str, learned: &[&str]) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    for (index, id) in learned.iter().enumerate() {
        let seq = i64::try_from(index).unwrap() + 2;
        seed_event(db, user, seq, BASE_US, SESSION, passed_lesson(id)).await;
    }
    put_state(db, user, &WebState::for_session(SESSION)).await;
    user
}

async fn start(app: &Router, user: Uuid, body: Value) -> (StatusCode, Value) {
    let (status, raw) = call(app, Method::POST, "/api/quiz/round", Some(user), Some(body)).await;
    (status, parse(&raw))
}

#[tokio::test]
async fn a_paper_refuses_a_topic_that_is_no_exam_set() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "paper-refuse@example.com", &[]).await;
        for body in [
            json!({"kind": "paper"}),
            json!({"kind": "paper", "set": "ghost"}),
            json!({"kind": "paper", "set": "sk-one"}),
            json!({"kind": "paper", "set": "lone"}),
            json!({"kind": "tour", "set": "set1"}),
        ] {
            let (status, reply) = start(&app, user, body.clone()).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(reply["error"]["code"], "invalid_request");
            assert!(!reply["error"]["message"].as_str().unwrap().is_empty());
        }
    })
    .await;
}

#[tokio::test]
async fn a_paper_serves_the_form_in_order_and_scores_by_points() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "paper-play@example.com", &[]).await;
        let (status, body) = start(&app, user, json!({"kind": "paper", "set": "set1"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["kind"], "paper");
        assert_eq!(body["round"]["set"], "set1");
        assert_eq!(body["round"]["n"], 20);
        assert_eq!(body["round"]["target_secs"], 14_400);
        let task = format!("{SESSION}-round-paper-set1-1");
        assert_eq!(body["task"]["task_id"], task);

        // The form: 10 short, 5 structured, 3 long, 2 "show that" items.
        let form: Vec<(usize, usize)> = [(1, 10), (2, 5), (3, 3), (4, 2)]
            .iter()
            .flat_map(|&(pool, count)| (0..count).map(move |at| (pool, at)))
            .collect();
        for (position, (pool, at)) in form.iter().enumerate() {
            let (status, served) = serve_task(&app, user, &task).await;
            assert_eq!(status, StatusCode::OK, "{served}");
            let scratch = stored_state(&db, user).await;
            let live = scratch.served.get(&task).unwrap();
            assert_eq!(live.text, format!("set1 pool {pool} item {at}"));
            // Pool 1 all right; pool 2 items 0 and 1 right, item 2 half right;
            // pool 3 item 0 right, item 1 two parts of three; the rest wrong.
            let right = &live.expected.answer;
            let given = match (*pool, *at) {
                (1, _) | (2, 0 | 1) | (3, 0) => right.clone(),
                (2, 2) => "a = 3; b = 0".to_owned(),
                (3, 1) => "a = 3; b = 3; c = 0".to_owned(),
                _ => "a = 0; b = 0; c = 0".to_owned(),
            };
            let (status, raw) = answer_raw(
                &app,
                user,
                &task,
                json!({"problem_id": live.problem_id, "answer": given}),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{position}: {raw}");
            assert_reveals_nothing(&raw);
        }

        let results = common::events_of_type(&db, user, "quiz_result").await;
        assert_eq!(results.len(), 1);
        // One row for the set topic, wrong against the retake line.
        assert_eq!(results[0]["per_topic"].as_array().unwrap().len(), 1);
        assert_eq!(results[0]["per_topic"][0]["topic"], "set1");
        assert_eq!(results[0]["per_topic"][0]["correct"], false);
        let score = results[0]["score"].as_f64().unwrap();
        assert!(
            (score - 31.666_666_666_666_664 / 59.0).abs() < 1e-9,
            "{score}"
        );

        let (status, raw) = call(
            &app,
            Method::POST,
            &format!("/api/task/{task}/quiz-result"),
            Some(user),
            Some(json!({})),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let reply = parse(&raw);
        // 10 + (4 + 4 + 2) + (7 + 4.67) + 0 = 31.67 of 10 + 20 + 21 + 8 = 59.
        assert_eq!(reply["points_earned"], 31.67);
        assert_eq!(reply["points_total"], 59.0);
        assert_eq!(reply["percent"], 53.67);
        assert_eq!(reply["round"]["kind"], "paper");
        assert_eq!(reply["round"]["form"], "The set1 topic");
    })
    .await;
}

#[tokio::test]
async fn the_retake_wait_of_a_paper_counts_per_exam_set() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "paper-wait@example.com", &[]).await;
        let now_us = (now_secs() * 1_000_000.0) as i64;
        let result = Event::QuizResult(QuizResult {
            inconclusive: false,
            ts: Timestamp::from_micros(now_us),
            session: Some(SESSION.to_owned()),
            v: SchemaVersion::current(),
            quiz_id: format!("{SESSION}-round-paper-set1-1"),
            score: 0.5,
            per_topic: Vec::new(),
            xp: 0.0,
        });
        seed_event(
            &db,
            user,
            100,
            now_us,
            SESSION,
            serde_json::to_value(result).unwrap(),
        )
        .await;
        let (status, body) = start(&app, user, json!({"kind": "paper", "set": "set1"})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "quiz_retake_wait");
        let (status, body) = start(&app, user, json!({"kind": "paper", "set": "set2"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["set"], "set2");
    })
    .await;
}

#[tokio::test]
async fn the_ancestor_scope_adds_the_skills_below_the_course() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(
            &db,
            "paper-scope@example.com",
            &["sk-one", "sk-two", "cp-one", "cp-two"],
        )
        .await;
        let (status, _) = enroll_course(&app, user, "c1").await;
        assert_eq!(status, StatusCode::OK);
        let (status, body) = start(&app, user, json!({"length": 20, "scope": "course"})).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["n"], 2);
        let (status, body) = start(
            &app,
            user,
            json!({"length": 20, "scope": "course_and_prereqs"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["scope"], "course_and_prereqs");
        assert_eq!(body["round"]["n"], 4);
        assert_eq!(body["round"]["course_name"], "Exam course");
        let (status, body) = start(&app, user, json!({"length": 20, "scope": "nearby"})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    })
    .await;
}

#[tokio::test]
async fn only_a_course_with_exam_weights_lists_its_exam_sets() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "paper-status@example.com", &[]).await;
        let (status, raw) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        assert_eq!(status, StatusCode::OK, "{raw}");
        let courses = parse(&raw)["courses"].clone();
        assert!(courses[0].get("exam_sets").is_none(), "{courses}");
        assert_eq!(
            courses[1]["exam_sets"],
            json!([
                {"id": "set1", "name": "The set1 topic"},
                {"id": "set2", "name": "The set2 topic"},
            ])
        );
    })
    .await;
}
