//! The two parts of an exam paper (D-VBE4).
//!
//! A part I set serves 24 one-point items and 8 two-point items. A part II set
//! serves its form as before. The result of a paper names the exam points once
//! both parts of one form are done. Every expected value is a literal.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::sync::Arc;

use axum::Router;
use axum::http::{Method, StatusCode};
use cadus_core::config::Config;
use cadus_core::curriculum::{
    AnswerKind, Catalog, Course, Curriculum, ExamWeights, Exemplar, RawCurriculum, Slug, Topic,
};
use cadus_store::test_support::TestDb;
use cadus_store::{DEFAULT_CLIENT_TIMEOUT_MS, Db};
use cadus_web::state::WebState;
use cadus_web::{AppState, create_app};
use common::{
    SESSION, answer_raw, call, kp, parse, put_state, seed_learner, seed_open_session, serve_task,
    stored_state, topic, unit,
};
use serde_json::{Value, json};
use sqlx::types::Uuid;

/// Part II pools: 10 one-point items, 5 of 4 points, 3 of 7 points, 2 of 4 points = 59.
const POOLS_II: [(usize, u8); 4] = [(10, 1), (5, 4), (3, 7), (2, 4)];

fn item(problem: String, points: u8) -> Exemplar {
    Exemplar {
        problem,
        answer_contract: Some(serde_json::from_value(json!({"kind": "exact"})).unwrap()),
        answer: "5".to_owned(),
        solution_sketch: Some("Work it out.".to_owned()),
        visual: None,
        points: Some(points),
    }
}

fn pools(id: &str, pools: &[(usize, u8)]) -> Topic {
    let points = pools
        .iter()
        .enumerate()
        .map(|(number, (count, points))| {
            let items = (0..*count)
                .map(|at| item(format!("{id} pool {} item {at}", number + 1), *points))
                .collect();
            kp(&format!("kp{}", number + 1), items)
        })
        .collect();
    let mut record = topic(id, points);
    record.answer_kind = AnswerKind::MultiStep;
    record.area = Some("models".to_owned());
    record
}

/// Part I of the form `fm1`: 30 one-point items and 12 two-point items stand in the pools.
fn part_one(id: &str) -> Topic {
    let mut record = pools(id, &[(30, 1), (12, 2)]);
    record.expected_time_secs = 7_200;
    record
}

fn part_two(id: &str) -> Topic {
    let mut record = pools(id, &POOLS_II);
    record.expected_time_secs = 14_400;
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
    let exam = vec![part_two("fm1"), part_one("fm1-i"), part_two("fm2")];
    Curriculum::build(RawCurriculum {
        catalog: Catalog {
            courses: vec![course("c1", "Exam course", 1, Some(weights))],
        },
        units: vec![unit("c1", "M1", exam, 0)],
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

async fn learner(db: &TestDb, email: &str) -> Uuid {
    let user = seed_learner(db, email).await;
    seed_open_session(db, user).await;
    put_state(db, user, &WebState::for_session(SESSION)).await;
    user
}

async fn start(app: &Router, user: Uuid, set: &str) -> (StatusCode, Value) {
    let body = json!({"kind": "paper", "set": set});
    let (status, raw) = call(app, Method::POST, "/api/quiz/round", Some(user), Some(body)).await;
    (status, parse(&raw))
}

/// Start the paper of `set`, answer every item, and read the result. `right`
/// says whether the item of pool `pool` at position `at` gets a right answer.
async fn play(
    app: &Router,
    db: &TestDb,
    user: Uuid,
    set: &str,
    right: impl Fn(usize, usize) -> bool,
) -> Value {
    let (status, body) = start(app, user, set).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let task = body["task"]["task_id"].as_str().unwrap().to_owned();
    let count = body["round"]["n"].as_u64().unwrap();
    for _ in 0..count {
        let (status, served) = serve_task(app, user, &task).await;
        assert_eq!(status, StatusCode::OK, "{served}");
        let scratch = stored_state(db, user).await;
        let live = scratch.served.get(&task).unwrap();
        let (pool, at) = position_of(&live.text);
        let given = if right(pool, at) { "5" } else { "0" };
        let (status, raw) = answer_raw(
            app,
            user,
            &task,
            json!({"problem_id": live.problem_id, "answer": given}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{raw}");
    }
    let (status, raw) = call(
        app,
        Method::POST,
        &format!("/api/task/{task}/quiz-result"),
        Some(user),
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{raw}");
    parse(&raw)
}

/// The pool and position that an item text names: `<id> pool <n> item <at>`.
fn position_of(text: &str) -> (usize, usize) {
    let words: Vec<&str> = text.split_whitespace().collect();
    (words[2].parse().unwrap(), words[4].parse().unwrap())
}

#[tokio::test]
async fn a_part_one_paper_serves_32_items_in_the_part_one_time() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "part-one@example.com").await;
        let (status, body) = start(&app, user, "fm1-i").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["round"]["kind"], "paper");
        assert_eq!(body["round"]["set"], "fm1-i");
        assert_eq!(body["round"]["n"], 32);
        assert_eq!(body["round"]["target_secs"], 7_200);
        assert_eq!(
            body["task"]["task_id"],
            format!("{SESSION}-round-paper-fm1-i-1")
        );
    })
    .await;
}

#[tokio::test]
async fn a_part_one_paper_reaches_the_line_at_38_of_40_and_names_the_open_part() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "part-one-line@example.com").await;
        // 24 + 7 * 2 = 38 points.
        let reply = play(&app, &db, user, "fm1-i", |pool, at| pool == 1 || at < 7).await;
        assert_eq!(reply["points_earned"], 38.0);
        assert_eq!(reply["points_total"], 40.0);
        assert_eq!(reply["exam"]["part"], 1);
        assert_eq!(reply["exam"]["part_points"], 38);
        assert_eq!(reply["exam"]["open_part"], 2);
        assert_eq!(
            reply["exam"]["line"],
            "Part I: 38 of 40 points. Part II is still open."
        );
        let results = common::events_of_type(&db, user, "quiz_result").await;
        assert_eq!(results[0]["per_topic"][0]["topic"], "fm1-i");
        assert_eq!(results[0]["per_topic"][0]["correct"], true);
        let (_, raw) = call(&app, Method::GET, "/api/status", Some(user), None).await;
        let sets = parse(&raw)["courses"][0]["exam_sets"].clone();
        assert_eq!(sets[1]["id"], "fm1-i");
        assert_eq!(sets[1]["part"], 1);
        assert_eq!(sets[1]["form"], "fm1");
        assert_eq!(sets[1]["points"], 38);
        assert_eq!(sets[1]["target_points"], 38);
        assert_eq!(sets[0]["points"], Value::Null);
    })
    .await;
}

#[tokio::test]
async fn a_part_one_paper_under_the_line_waits_and_leaves_part_two_open() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "part-one-wait@example.com").await;
        // 24 + 6 * 2 = 36 points: under 38.
        let reply = play(&app, &db, user, "fm1-i", |pool, at| pool == 1 || at < 6).await;
        assert_eq!(
            reply["exam"]["line"],
            "Part I: 36 of 40 points. Part II is still open."
        );
        let results = common::events_of_type(&db, user, "quiz_result").await;
        assert_eq!(results[0]["per_topic"][0]["correct"], false);
        let (status, body) = start(&app, user, "fm1-i").await;
        assert_eq!(status, StatusCode::CONFLICT, "{body}");
        assert_eq!(body["error"]["code"], "quiz_retake_wait");
        // The set `fm1` has an id that starts the id of `fm1-i`. Its wait is its own.
        let (status, body) = start(&app, user, "fm1").await;
        assert_eq!(status, StatusCode::OK, "{body}");
    })
    .await;
}

#[tokio::test]
async fn the_second_part_reports_the_exam_score() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "both-parts@example.com").await;
        // Part I: 38 points.
        play(&app, &db, user, "fm1-i", |pool, at| pool == 1 || at < 7).await;
        // Part II: all right, 60 points. 98 points give the score 100.
        let reply = play(&app, &db, user, "fm1", |_, _| true).await;
        assert_eq!(reply["exam"]["exam_points"], 98);
        assert_eq!(reply["exam"]["score"], 100);
        assert_eq!(reply["exam"]["passed"], true);
        assert_eq!(reply["exam"]["line"], "Exam points 98 of 100. Score 100.");
    })
    .await;
}

#[tokio::test]
async fn a_score_under_25_points_is_not_passed() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let other = learner(&db, "not-passed@example.com").await;
        play(&app, &db, other, "fm1-i", |pool, at| pool == 1 && at < 14).await;
        let reply = play(&app, &db, other, "fm1", |pool, at| pool == 1 && at < 7).await;
        // 14 + floor(7 / 59 * 60) = 14 + 7 = 21 points.
        assert_eq!(reply["exam"]["exam_points"], 21);
        assert_eq!(reply["exam"]["score"], Value::Null);
        assert_eq!(reply["exam"]["passed"], false);
        assert_eq!(
            reply["exam"]["line"],
            "Exam points 21 of 100. Not passed (25 points pass)."
        );
    })
    .await;
}

#[tokio::test]
async fn a_set_with_no_other_part_has_no_exam_line() {
    TestDb::with(|db| async move {
        let app = app(&db);
        let user = learner(&db, "no-sibling@example.com").await;
        let reply = play(&app, &db, user, "fm2", |_, _| true).await;
        assert_eq!(reply["exam"], Value::Null);
        assert_eq!(reply["percent"], 100.0);
    })
    .await;
}
